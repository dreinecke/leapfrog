use std::{
    error::Error,
    ffi::CString,
    fmt, fs,
    io::{Error as IoError, ErrorKind},
    mem::{self, MaybeUninit},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt},
        },
    },
    path::Path,
    ptr::NonNull,
    slice,
    time::Instant,
};

use super::wire::MAX_PACKET_LENGTH;

const SOCKET_PATH: &str = "/run/t1bridge/touchbar.sock";
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportError {
    Endpoint,
    Socket,
    Peer,
    WouldBlock,
    Interrupted,
    Closed,
    Truncated,
    Ancillary,
    Send,
    Receive,
    Timeout,
    Frame,
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Endpoint => "invalid T1Bridge hardware endpoint",
            Self::Socket => "T1Bridge hardware socket is unavailable",
            Self::Peer => "T1Bridge hardware peer is not root",
            Self::WouldBlock => "T1Bridge transport would block",
            Self::Interrupted => "T1Bridge transport was interrupted",
            Self::Closed => "T1Bridge hardware connection closed",
            Self::Truncated => "T1Bridge packet was truncated",
            Self::Ancillary => "T1Bridge packet carried unexpected ancillary data",
            Self::Send => "T1Bridge packet send failed",
            Self::Receive => "T1Bridge packet receive failed",
            Self::Timeout => "T1Bridge transport timed out",
            Self::Frame => "T1Bridge frame allocation failed",
        })
    }
}

impl Error for TransportError {}

pub struct SeqPacket {
    descriptor: OwnedFd,
}

impl SeqPacket {
    pub fn connect_root() -> Result<Self, TransportError> {
        Self::connect_at(Path::new(SOCKET_PATH), 0)
    }

    fn connect_at(path: &Path, expected_uid: u32) -> Result<Self, TransportError> {
        let metadata = fs::symlink_metadata(path).map_err(|_| TransportError::Endpoint)?;
        if !metadata.file_type().is_socket() || metadata.uid() != expected_uid {
            return Err(TransportError::Endpoint);
        }
        // SAFETY: socket returns one new descriptor or a negative errno sentinel.
        let descriptor =
            unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
        if descriptor < 0 {
            return Err(TransportError::Socket);
        }
        // SAFETY: descriptor is freshly owned and transferred exactly once.
        let descriptor = unsafe { OwnedFd::from_raw_fd(descriptor) };
        let (address, address_length) = unix_address(path)?;
        // SAFETY: address and length describe a fully initialized sockaddr_un.
        if unsafe {
            libc::connect(
                descriptor.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                address_length,
            )
        } < 0
        {
            return Err(TransportError::Socket);
        }
        require_peer_uid(descriptor.as_raw_fd(), expected_uid)?;
        set_nonblocking(descriptor.as_raw_fd())?;
        Ok(Self { descriptor })
    }

    pub fn descriptor(&self) -> BorrowedFd<'_> {
        self.descriptor.as_fd()
    }

    pub fn send(
        &self,
        packet: &[u8],
        descriptor: Option<BorrowedFd<'_>>,
    ) -> Result<(), TransportError> {
        if packet.is_empty() || packet.len() > MAX_PACKET_LENGTH {
            return Err(TransportError::Send);
        }
        let mut vector = libc::iovec {
            iov_base: packet.as_ptr().cast_mut().cast(),
            iov_len: packet.len(),
        };
        #[repr(C, align(8))]
        struct Control([u8; 64]);
        let mut control = Control([0; 64]);
        let mut message = unsafe { MaybeUninit::<libc::msghdr>::zeroed().assume_init() };
        message.msg_iov = &mut vector;
        message.msg_iovlen = 1;
        if let Some(descriptor) = descriptor {
            let descriptor_size = mem::size_of::<RawFd>() as u32;
            message.msg_control = control.0.as_mut_ptr().cast();
            message.msg_controllen = unsafe { libc::CMSG_SPACE(descriptor_size) as usize };
            // SAFETY: msg_control is aligned and sized for one cmsghdr plus one RawFd.
            let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
            if header.is_null() {
                return Err(TransportError::Ancillary);
            }
            // SAFETY: header points into the live control buffer.
            unsafe {
                (*header).cmsg_level = libc::SOL_SOCKET;
                (*header).cmsg_type = libc::SCM_RIGHTS;
                (*header).cmsg_len = libc::CMSG_LEN(descriptor_size) as usize;
                std::ptr::write_unaligned(
                    libc::CMSG_DATA(header).cast::<RawFd>(),
                    descriptor.as_raw_fd(),
                );
            }
        }
        // SAFETY: message references live packet and optional control storage.
        let sent =
            unsafe { libc::sendmsg(self.descriptor.as_raw_fd(), &message, libc::MSG_NOSIGNAL) };
        if sent == packet.len() as isize {
            Ok(())
        } else if sent < 0 {
            Err(operation_error(TransportError::Send))
        } else {
            Err(TransportError::Send)
        }
    }

    pub fn receive(&self, buffer: &mut [u8]) -> Result<usize, TransportError> {
        if buffer.len() < MAX_PACKET_LENGTH {
            return Err(TransportError::Receive);
        }
        let mut vector = libc::iovec {
            iov_base: buffer.as_mut_ptr().cast(),
            iov_len: buffer.len(),
        };
        #[repr(C, align(8))]
        struct Control([u8; 128]);
        let mut control = Control([0; 128]);
        let mut message = unsafe { MaybeUninit::<libc::msghdr>::zeroed().assume_init() };
        message.msg_iov = &mut vector;
        message.msg_iovlen = 1;
        message.msg_control = control.0.as_mut_ptr().cast();
        message.msg_controllen = control.0.len();
        // SAFETY: message references live writable packet and control buffers.
        let received = unsafe {
            libc::recvmsg(
                self.descriptor.as_raw_fd(),
                &mut message,
                libc::MSG_TRUNC | libc::MSG_CMSG_CLOEXEC,
            )
        };
        if received < 0 {
            return Err(operation_error(TransportError::Receive));
        }
        if received == 0 {
            return Err(TransportError::Closed);
        }
        let ancillary = close_received_descriptors(&message);
        if message.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0
            || received as usize > buffer.len()
        {
            return Err(TransportError::Truncated);
        }
        if ancillary {
            return Err(TransportError::Ancillary);
        }
        Ok(received as usize)
    }

    pub fn receive_until(
        &self,
        buffer: &mut [u8],
        deadline: Instant,
    ) -> Result<usize, TransportError> {
        loop {
            match self.receive(buffer) {
                Ok(length) => return Ok(length),
                Err(TransportError::WouldBlock | TransportError::Interrupted) => {
                    poll_until(self.descriptor.as_raw_fd(), libc::POLLIN, deadline)?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn send_until(
        &self,
        packet: &[u8],
        descriptor: Option<BorrowedFd<'_>>,
        deadline: Instant,
    ) -> Result<(), TransportError> {
        loop {
            match self.send(packet, descriptor) {
                Ok(()) => return Ok(()),
                Err(TransportError::WouldBlock | TransportError::Interrupted) => {
                    poll_until(self.descriptor.as_raw_fd(), libc::POLLOUT, deadline)?;
                }
                Err(error) => return Err(error),
            }
        }
    }
}

pub struct FrameMemory {
    descriptor: OwnedFd,
    mapping: NonNull<u8>,
    length: usize,
}

impl FrameMemory {
    pub fn new(length: usize) -> Result<Self, TransportError> {
        if length == 0 || length > MAX_FRAME_BYTES {
            return Err(TransportError::Frame);
        }
        let name = CString::new("touchbar-rs-frame").expect("static memfd name");
        // SAFETY: name is a valid C string and flags request a private close-on-exec memfd.
        let descriptor = unsafe {
            libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING)
        };
        if descriptor < 0 {
            return Err(TransportError::Frame);
        }
        // SAFETY: descriptor is freshly owned and transferred exactly once.
        let descriptor = unsafe { OwnedFd::from_raw_fd(descriptor) };
        let exact_length = i64::try_from(length).map_err(|_| TransportError::Frame)?;
        // SAFETY: descriptor is a writable memfd and exact_length is positive.
        if unsafe { libc::ftruncate(descriptor.as_raw_fd(), exact_length) } < 0 {
            return Err(TransportError::Frame);
        }
        // SAFETY: descriptor has exact length and remains owned for the mapping lifetime.
        let mapping = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                descriptor.as_raw_fd(),
                0,
            )
        };
        let mapping = NonNull::new(mapping.cast::<u8>()).filter(|_| mapping != libc::MAP_FAILED);
        let Some(mapping) = mapping else {
            return Err(TransportError::Frame);
        };
        let seals = libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL;
        // SAFETY: descriptor is a seal-capable memfd; seals preserve writable content.
        if unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_ADD_SEALS, seals) } < 0 {
            // SAFETY: mapping was created above and has exact length.
            unsafe { libc::munmap(mapping.as_ptr().cast(), length) };
            return Err(TransportError::Frame);
        }
        Ok(Self {
            descriptor,
            mapping,
            length,
        })
    }

    pub fn descriptor(&self) -> BorrowedFd<'_> {
        self.descriptor.as_fd()
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: the mapping is uniquely borrowed through &mut self and has exact length.
        unsafe { slice::from_raw_parts_mut(self.mapping.as_ptr(), self.length) }
    }
}

impl Drop for FrameMemory {
    fn drop(&mut self) {
        // SAFETY: mapping and length came from one successful mmap and are unmapped once.
        unsafe {
            libc::munmap(self.mapping.as_ptr().cast(), self.length);
        }
    }
}

fn unix_address(path: &Path) -> Result<(libc::sockaddr_un, libc::socklen_t), TransportError> {
    let bytes = path.as_os_str().as_bytes();
    let mut address = unsafe { MaybeUninit::<libc::sockaddr_un>::zeroed().assume_init() };
    if bytes.len() >= address.sun_path.len() {
        return Err(TransportError::Endpoint);
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, source) in address.sun_path.iter_mut().zip(bytes) {
        *target = *source as libc::c_char;
    }
    let length = mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1;
    Ok((address, length as libc::socklen_t))
}

fn require_peer_uid(descriptor: RawFd, expected_uid: u32) -> Result<(), TransportError> {
    let mut credentials = MaybeUninit::<libc::ucred>::zeroed();
    let mut length = mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: credentials and length are valid output storage.
    if unsafe {
        libc::getsockopt(
            descriptor,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            credentials.as_mut_ptr().cast(),
            &mut length,
        )
    } < 0
    {
        return Err(TransportError::Peer);
    }
    let credentials = unsafe { credentials.assume_init() };
    if length as usize != mem::size_of::<libc::ucred>() || credentials.uid != expected_uid {
        return Err(TransportError::Peer);
    }
    Ok(())
}

fn set_nonblocking(descriptor: RawFd) -> Result<(), TransportError> {
    // SAFETY: descriptor is a live socket.
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(TransportError::Socket);
    }
    Ok(())
}

fn poll_until(descriptor: RawFd, events: i16, deadline: Instant) -> Result<(), TransportError> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(TransportError::Timeout);
        }
        let timeout = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        let mut poll = libc::pollfd {
            fd: descriptor,
            events,
            revents: 0,
        };
        // SAFETY: poll references one initialized descriptor.
        let ready = unsafe { libc::poll(&mut poll, 1, timeout) };
        if ready > 0 {
            if poll.revents & events != 0 {
                return Ok(());
            }
            return Err(TransportError::Closed);
        }
        if ready == 0 {
            return Err(TransportError::Timeout);
        }
        if IoError::last_os_error().kind() != ErrorKind::Interrupted {
            return Err(TransportError::Socket);
        }
    }
}

fn operation_error(other: TransportError) -> TransportError {
    match IoError::last_os_error().kind() {
        ErrorKind::WouldBlock => TransportError::WouldBlock,
        ErrorKind::Interrupted => TransportError::Interrupted,
        _ => other,
    }
}

fn close_received_descriptors(message: &libc::msghdr) -> bool {
    let mut found = false;
    // SAFETY: message's control buffer remains live for this traversal.
    let mut header = unsafe { libc::CMSG_FIRSTHDR(message) };
    while !header.is_null() {
        found = true;
        // SAFETY: header is one validated control record in the kernel-filled buffer.
        unsafe {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let header_length = libc::CMSG_LEN(0) as usize;
                let data_length = (*header).cmsg_len.saturating_sub(header_length);
                let count = data_length / mem::size_of::<RawFd>();
                for index in 0..count {
                    let descriptor = std::ptr::read_unaligned(
                        libc::CMSG_DATA(header).cast::<RawFd>().add(index),
                    );
                    libc::close(descriptor);
                }
            }
            header = libc::CMSG_NXTHDR(message, header);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_exactly_sized_and_sealed() {
        let mut frame = FrameMemory::new(4096).unwrap();
        frame.bytes_mut()[0] = 0x5a;
        // SAFETY: descriptor is a live memfd and F_GET_SEALS has no third argument.
        let seals = unsafe { libc::fcntl(frame.descriptor().as_raw_fd(), libc::F_GET_SEALS) };
        assert_eq!(
            seals & (libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL),
            libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL
        );
        // SAFETY: descriptor is live; prohibited resizing must fail.
        assert!(unsafe { libc::ftruncate(frame.descriptor().as_raw_fd(), 8192) } < 0);
    }

    #[test]
    fn seqpacket_transport_preserves_packet_boundaries() {
        let mut descriptors = [0; 2];
        // SAFETY: descriptors is valid output storage for a socket pair.
        assert_eq!(
            unsafe {
                libc::socketpair(
                    libc::AF_UNIX,
                    libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                    0,
                    descriptors.as_mut_ptr(),
                )
            },
            0
        );
        // SAFETY: both descriptors are freshly owned and transferred once.
        let left = unsafe { OwnedFd::from_raw_fd(descriptors[0]) };
        let right = unsafe { OwnedFd::from_raw_fd(descriptors[1]) };
        set_nonblocking(left.as_raw_fd()).unwrap();
        set_nonblocking(right.as_raw_fd()).unwrap();
        let left = SeqPacket { descriptor: left };
        let right = SeqPacket { descriptor: right };
        left.send(b"first", None).unwrap();
        left.send(b"second", None).unwrap();
        let mut buffer = vec![0; MAX_PACKET_LENGTH];
        let first = right.receive(&mut buffer).unwrap();
        assert_eq!(&buffer[..first], b"first");
        let second = right.receive(&mut buffer).unwrap();
        assert_eq!(&buffer[..second], b"second");
    }
}

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    os::fd::{AsRawFd, RawFd},
    time::{Duration, Instant},
};

use super::{
    transport::{FrameMemory, SeqPacket, TransportError},
    wire::{
        self, Damage, Dimensions, Features, InputFrame, Key, ServiceError, ServiceMessage,
        WireError,
    },
};

const BUFFER_ID: u32 = 1;
const FIRST_FRAME_ID: u64 = 1;
const MAX_PENDING_REQUESTS: usize = 64;
const CONTROL_DEADLINE: Duration = Duration::from_secs(1);
const ACTION_SEND_DEADLINE: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    pub display_brightness: bool,
    pub keyboard_backlight: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Brightness {
    Display,
    Keyboard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionKind {
    Keys,
    Brightness(Brightness),
    CancelTouchId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Input(InputFrame),
    FrameAvailable,
    ActionRejected {
        action: ActionKind,
        error: ServiceError,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientError {
    Unavailable,
    Transport,
    Protocol,
    Negotiation,
    Frame,
    Service,
    Busy,
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "T1Bridge hardware service is unavailable",
            Self::Transport => "T1Bridge hardware connection was lost",
            Self::Protocol => "T1Bridge renderer protocol failed",
            Self::Negotiation => "T1Bridge renderer negotiation failed",
            Self::Frame => "T1Bridge renderer frame failed",
            Self::Service => "T1Bridge hardware service rejected renderer state",
            Self::Busy => "T1Bridge renderer request capacity is full",
        })
    }
}

impl Error for ClientError {}

impl From<TransportError> for ClientError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::Endpoint | TransportError::Socket => Self::Unavailable,
            TransportError::Frame => Self::Frame,
            _ => Self::Transport,
        }
    }
}

impl From<WireError> for ClientError {
    fn from(_: WireError) -> Self {
        Self::Protocol
    }
}

struct Negotiated {
    socket: SeqPacket,
    dimensions: Dimensions,
    features: Features,
    next_request_id: u32,
    receive_buffer: Vec<u8>,
}

enum HelloOutcome {
    Accepted(Negotiated),
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pending {
    Submit { frame_id: u64 },
    Keys,
    CancelTouchId,
    Brightness { kind: Brightness, value: u8 },
}

#[derive(Default)]
struct BrightnessLane {
    in_flight: Option<u32>,
    trailing: Option<u8>,
}

impl BrightnessLane {
    fn offer(&mut self, value: u8) -> Option<u8> {
        if self.in_flight.is_some() {
            self.trailing = Some(value);
            None
        } else {
            Some(value)
        }
    }

    fn mark_sent(&mut self, request_id: u32) -> Result<(), ClientError> {
        if self.in_flight.replace(request_id).is_some() {
            return Err(ClientError::Protocol);
        }
        Ok(())
    }

    fn complete(
        &mut self,
        request_id: u32,
        rejected: Option<u8>,
    ) -> Result<Option<u8>, ClientError> {
        if self.in_flight != Some(request_id) {
            return Err(ClientError::Protocol);
        }
        self.in_flight = None;
        Ok(self.trailing.take().or(rejected))
    }
}

#[derive(Clone, Copy)]
struct InFlightFrame {
    frame_id: u64,
    acknowledged: bool,
}

pub struct Client {
    socket: SeqPacket,
    dimensions: Dimensions,
    capabilities: Capabilities,
    frame: FrameMemory,
    receive_buffer: Vec<u8>,
    next_request_id: u32,
    next_frame_id: u64,
    in_flight: Option<InFlightFrame>,
    pending: BTreeMap<u32, Pending>,
    display_brightness: BrightnessLane,
    keyboard_backlight: BrightnessLane,
}

impl Client {
    pub fn connect() -> Result<Self, ClientError> {
        let Negotiated {
            socket,
            dimensions,
            features,
            mut next_request_id,
            mut receive_buffer,
        } = negotiate()?;
        let mut frame = FrameMemory::new(dimensions.frame_length()?)?;
        frame.bytes_mut().fill(0);
        let register_id = take_request_id(&mut next_request_id, &BTreeMap::new())?;
        let packet = wire::register_buffer(register_id, BUFFER_ID, dimensions)?;
        let deadline = Instant::now() + CONTROL_DEADLINE;
        socket.send_until(&packet, Some(frame.descriptor()), deadline)?;
        let length = socket.receive_until(&mut receive_buffer, deadline)?;
        let response = wire::decode_service(&receive_buffer[..length], Some(dimensions))?;
        if response.request_id != register_id || response.message != ServiceMessage::Ack {
            return Err(ClientError::Negotiation);
        }
        Ok(Self {
            socket,
            dimensions,
            capabilities: Capabilities {
                display_brightness: features.contains(Features::DISPLAY_BRIGHTNESS),
                keyboard_backlight: features.contains(Features::KEYBOARD_BACKLIGHT),
            },
            frame,
            receive_buffer,
            next_request_id,
            next_frame_id: FIRST_FRAME_ID,
            in_flight: None,
            pending: BTreeMap::new(),
            display_brightness: BrightnessLane::default(),
            keyboard_backlight: BrightnessLane::default(),
        })
    }

    pub fn raw_fd(&self) -> RawFd {
        self.socket.descriptor().as_raw_fd()
    }

    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    pub const fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    pub const fn frame_available(&self) -> bool {
        self.in_flight.is_none()
    }

    pub fn submit_rgba(
        &mut self,
        source: &[u8],
        source_width: u32,
        source_height: u32,
    ) -> Result<bool, ClientError> {
        self.submit_rgba_damage(source, source_width, source_height, &[])
    }

    pub fn submit_rgba_damage(
        &mut self,
        source: &[u8],
        source_width: u32,
        source_height: u32,
        source_damage: &[Damage],
    ) -> Result<bool, ClientError> {
        if self.in_flight.is_some() {
            return Ok(false);
        }
        copy_rgba_to_xrgb(
            source,
            source_width,
            source_height,
            self.frame.bytes_mut(),
            self.dimensions,
        )?;
        let damage = scale_damage(source_damage, source_width, source_height, self.dimensions)?;
        let frame_id = self.next_frame_id;
        self.next_frame_id = self
            .next_frame_id
            .checked_add(1)
            .ok_or(ClientError::Protocol)?;
        let request_id = self.allocate(Pending::Submit { frame_id })?;
        let packet = wire::submit_frame(request_id, BUFFER_ID, frame_id, &damage)?;
        if let Err(error) = self.send_action_packet(&packet) {
            self.pending.remove(&request_id);
            return Err(error);
        }
        self.in_flight = Some(InFlightFrame {
            frame_id,
            acknowledged: false,
        });
        Ok(true)
    }

    pub fn tap_keys(&mut self, keys: &[Key]) -> Result<(), ClientError> {
        let request_id = self.allocate(Pending::Keys)?;
        let packet = wire::tap_keys(request_id, keys)?;
        if let Err(error) = self.send_action_packet(&packet) {
            self.pending.remove(&request_id);
            return Err(error);
        }
        Ok(())
    }

    pub fn cancel_touch_id(&mut self) -> Result<(), ClientError> {
        let request_id = self.allocate(Pending::CancelTouchId)?;
        let packet = wire::cancel_touch_id(request_id)?;
        if let Err(error) = self.send_action_packet(&packet) {
            self.pending.remove(&request_id);
            return Err(error);
        }
        Ok(())
    }

    pub fn set_brightness(&mut self, kind: Brightness, value: u8) -> Result<(), ClientError> {
        if value > 100 || !self.supports(kind) {
            return Err(ClientError::Negotiation);
        }
        let Some(value) = self.lane_mut(kind).offer(value) else {
            return Ok(());
        };
        self.send_brightness(kind, value)
    }

    pub fn receive(&mut self) -> Result<Option<Event>, ClientError> {
        let length = match self.socket.receive(&mut self.receive_buffer) {
            Ok(length) => length,
            Err(TransportError::WouldBlock | TransportError::Interrupted) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let envelope = wire::decode_service(&self.receive_buffer[..length], Some(self.dimensions))?;
        match envelope.message {
            ServiceMessage::Ack => self.handle_ack(envelope.request_id),
            ServiceMessage::Error(error) => self.handle_error(envelope.request_id, error),
            ServiceMessage::FrameReleased {
                buffer_id,
                frame_id,
            } => {
                if envelope.request_id != 0 || buffer_id != BUFFER_ID {
                    return Err(ClientError::Protocol);
                }
                let Some(in_flight) = self.in_flight else {
                    return Err(ClientError::Protocol);
                };
                if !in_flight.acknowledged || in_flight.frame_id != frame_id {
                    return Err(ClientError::Protocol);
                }
                self.in_flight = None;
                Ok(Some(Event::FrameAvailable))
            }
            ServiceMessage::InputFrame(input) => Ok(Some(Event::Input(input))),
            ServiceMessage::HelloAck(_) => Err(ClientError::Protocol),
        }
    }

    fn handle_ack(&mut self, request_id: u32) -> Result<Option<Event>, ClientError> {
        let pending = self
            .pending
            .remove(&request_id)
            .ok_or(ClientError::Protocol)?;
        match pending {
            Pending::Submit { frame_id } => {
                let Some(in_flight) = self.in_flight.as_mut() else {
                    return Err(ClientError::Protocol);
                };
                if in_flight.frame_id != frame_id || in_flight.acknowledged {
                    return Err(ClientError::Protocol);
                }
                in_flight.acknowledged = true;
            }
            Pending::Brightness { kind, .. } => {
                self.complete_brightness(request_id, kind, None)?;
            }
            Pending::Keys | Pending::CancelTouchId => {}
        }
        Ok(None)
    }

    fn handle_error(
        &mut self,
        request_id: u32,
        error: ServiceError,
    ) -> Result<Option<Event>, ClientError> {
        let pending = self
            .pending
            .remove(&request_id)
            .ok_or(ClientError::Protocol)?;
        match pending {
            Pending::Brightness { kind, value } if error == ServiceError::ResourceLimit => {
                self.complete_brightness(request_id, kind, Some(value))?;
                Ok(None)
            }
            Pending::Brightness { kind, .. } => {
                self.complete_brightness(request_id, kind, None)?;
                Err(ClientError::Service)
            }
            Pending::Keys => Ok(Some(Event::ActionRejected {
                action: ActionKind::Keys,
                error,
            })),
            Pending::CancelTouchId => Ok(Some(Event::ActionRejected {
                action: ActionKind::CancelTouchId,
                error,
            })),
            Pending::Submit { .. } => Err(ClientError::Service),
        }
    }

    fn complete_brightness(
        &mut self,
        request_id: u32,
        kind: Brightness,
        rejected: Option<u8>,
    ) -> Result<(), ClientError> {
        if let Some(value) = self.lane_mut(kind).complete(request_id, rejected)? {
            self.send_brightness(kind, value)?;
        }
        Ok(())
    }

    fn send_brightness(&mut self, kind: Brightness, value: u8) -> Result<(), ClientError> {
        let request_id = self.allocate(Pending::Brightness { kind, value })?;
        let packet = match kind {
            Brightness::Display => wire::set_display_brightness(request_id, value)?,
            Brightness::Keyboard => wire::set_keyboard_backlight(request_id, value)?,
        };
        if let Err(error) = self.send_action_packet(&packet) {
            self.pending.remove(&request_id);
            return Err(error);
        }
        if let Err(error) = self.lane_mut(kind).mark_sent(request_id) {
            self.pending.remove(&request_id);
            return Err(error);
        }
        Ok(())
    }

    fn send_action_packet(&self, packet: &[u8]) -> Result<(), ClientError> {
        self.socket
            .send_until(packet, None, Instant::now() + ACTION_SEND_DEADLINE)
            .map_err(Into::into)
    }

    fn allocate(&mut self, pending: Pending) -> Result<u32, ClientError> {
        if self.pending.len() >= MAX_PENDING_REQUESTS {
            return Err(ClientError::Busy);
        }
        let request_id = take_request_id(&mut self.next_request_id, &self.pending)?;
        self.pending.insert(request_id, pending);
        Ok(request_id)
    }

    fn lane_mut(&mut self, kind: Brightness) -> &mut BrightnessLane {
        match kind {
            Brightness::Display => &mut self.display_brightness,
            Brightness::Keyboard => &mut self.keyboard_backlight,
        }
    }

    fn supports(&self, kind: Brightness) -> bool {
        match kind {
            Brightness::Display => self.capabilities.display_brightness,
            Brightness::Keyboard => self.capabilities.keyboard_backlight,
        }
    }
}

fn scale_damage(
    source: &[Damage],
    source_width: u32,
    source_height: u32,
    target: Dimensions,
) -> Result<Vec<Damage>, ClientError> {
    if source_width == 0 || source_height == 0 {
        return Err(ClientError::Frame);
    }
    source
        .iter()
        .map(|rect| {
            let source_right = rect
                .x
                .checked_add(rect.width)
                .filter(|right| rect.width > 0 && *right <= source_width)
                .ok_or(ClientError::Frame)?;
            let source_bottom = rect
                .y
                .checked_add(rect.height)
                .filter(|bottom| rect.height > 0 && *bottom <= source_height)
                .ok_or(ClientError::Frame)?;
            let x = u64::from(rect.x) * u64::from(target.width) / u64::from(source_width);
            let y = u64::from(rect.y) * u64::from(target.height) / u64::from(source_height);
            let right = (u64::from(source_right) * u64::from(target.width))
                .div_ceil(u64::from(source_width));
            let bottom = (u64::from(source_bottom) * u64::from(target.height))
                .div_ceil(u64::from(source_height));
            Ok(Damage {
                x: x as u32,
                y: y as u32,
                width: (right - x) as u32,
                height: (bottom - y) as u32,
            })
        })
        .collect()
}

fn negotiate() -> Result<Negotiated, ClientError> {
    let optional = Features::DISPLAY_BRIGHTNESS | Features::KEYBOARD_BACKLIGHT;
    if let HelloOutcome::Accepted(connection) = connect_and_hello(Features::BASE | optional)? {
        return Ok(connection);
    }
    let mut supported = Features::BASE;
    for feature in [Features::DISPLAY_BRIGHTNESS, Features::KEYBOARD_BACKLIGHT] {
        if matches!(
            connect_and_hello(Features::BASE | feature)?,
            HelloOutcome::Accepted(_)
        ) {
            supported = supported | feature;
        }
    }
    match connect_and_hello(supported)? {
        HelloOutcome::Accepted(connection) => Ok(connection),
        HelloOutcome::Unsupported => Err(ClientError::Negotiation),
    }
}

fn connect_and_hello(features: Features) -> Result<HelloOutcome, ClientError> {
    let socket = SeqPacket::connect_root()?;
    let mut receive_buffer = vec![0; wire::MAX_PACKET_LENGTH];
    let request_id = 1;
    let packet = wire::hello(request_id, features)?;
    let deadline = Instant::now() + CONTROL_DEADLINE;
    socket.send_until(&packet, None, deadline)?;
    let length = socket.receive_until(&mut receive_buffer, deadline)?;
    let response = wire::decode_service(&receive_buffer[..length], None)?;
    if response.request_id != request_id {
        return Err(ClientError::Protocol);
    }
    match response.message {
        ServiceMessage::HelloAck(ack) => Ok(HelloOutcome::Accepted(Negotiated {
            socket,
            dimensions: ack.dimensions,
            features,
            next_request_id: 2,
            receive_buffer,
        })),
        ServiceMessage::Error(ServiceError::UnsupportedFeature) => Ok(HelloOutcome::Unsupported),
        ServiceMessage::Error(_) => Err(ClientError::Service),
        _ => Err(ClientError::Protocol),
    }
}

fn take_request_id(next: &mut u32, pending: &BTreeMap<u32, Pending>) -> Result<u32, ClientError> {
    for _ in 0..=pending.len() {
        let candidate = *next;
        *next = (*next).checked_add(1).unwrap_or(1);
        if candidate != 0 && !pending.contains_key(&candidate) {
            return Ok(candidate);
        }
    }
    Err(ClientError::Busy)
}

fn copy_rgba_to_xrgb(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    destination: &mut [u8],
    dimensions: Dimensions,
) -> Result<(), ClientError> {
    let source_length = u64::from(source_width)
        .checked_mul(u64::from(source_height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|length| usize::try_from(length).ok())
        .ok_or(ClientError::Frame)?;
    if source_width == 0
        || source_height == 0
        || source.len() != source_length
        || destination.len() != dimensions.frame_length()?
    {
        return Err(ClientError::Frame);
    }
    for y in 0..dimensions.height {
        let source_y = y as u64 * u64::from(source_height) / u64::from(dimensions.height);
        for x in 0..dimensions.width {
            let source_x = x as u64 * u64::from(source_width) / u64::from(dimensions.width);
            let source_offset = ((source_y * u64::from(source_width) + source_x) * 4) as usize;
            let destination_offset =
                ((u64::from(y) * u64::from(dimensions.width) + u64::from(x)) * 4) as usize;
            destination[destination_offset] = source[source_offset + 2];
            destination[destination_offset + 1] = source[source_offset + 1];
            destination[destination_offset + 2] = source[source_offset];
            destination[destination_offset + 3] = 0;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_conversion_swaps_channels_and_scales_coordinates() {
        let source = [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255];
        let mut destination = [0; 8];
        copy_rgba_to_xrgb(
            &source,
            2,
            2,
            &mut destination,
            Dimensions {
                width: 2,
                height: 1,
            },
        )
        .unwrap();
        assert_eq!(destination, [3, 2, 1, 0, 6, 5, 4, 0]);
    }

    #[test]
    fn damage_scales_outward_and_rejects_source_overflow() {
        let scaled = scale_damage(
            &[Damage {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }],
            4,
            4,
            Dimensions {
                width: 10,
                height: 10,
            },
        )
        .unwrap();
        assert_eq!(
            scaled,
            [Damage {
                x: 2,
                y: 2,
                width: 6,
                height: 6,
            }]
        );
        assert!(
            scale_damage(
                &[Damage {
                    x: 3,
                    y: 0,
                    width: 2,
                    height: 1,
                }],
                4,
                4,
                Dimensions {
                    width: 10,
                    height: 10,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn brightness_lane_is_bounded_and_preserves_latest_value() {
        let mut lane = BrightnessLane::default();
        assert_eq!(lane.offer(20), Some(20));
        lane.mark_sent(4).unwrap();
        for value in 21..=100 {
            assert_eq!(lane.offer(value), None);
        }
        assert_eq!(lane.complete(4, Some(20)).unwrap(), Some(100));
        assert_eq!(lane.in_flight, None);
        assert_eq!(lane.trailing, None);
    }

    #[test]
    fn request_ids_skip_live_values_and_wrap_without_zero() {
        let mut pending = BTreeMap::new();
        pending.insert(u32::MAX, Pending::Keys);
        pending.insert(1, Pending::Keys);
        let mut next = u32::MAX;
        assert_eq!(take_request_id(&mut next, &pending).unwrap(), 2);
        assert_eq!(next, 3);
    }
}

use std::{collections::HashSet, error::Error, fmt};

pub const PROTOCOL_MINOR: u16 = 0;
pub const MAX_PACKET_LENGTH: usize = 64 * 1024;
pub const MAX_FRAME_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_KEYS: usize = 4;
pub const MAX_DAMAGE_RECTS: usize = 64;

const HEADER_LENGTH: usize = 16;
const MAGIC: &[u8; 4] = b"T1HW";
const PROTOCOL_MAJOR: u16 = 1;

const HELLO: u16 = 0x0001;
const REGISTER_BUFFER: u16 = 0x0002;
const SUBMIT_FRAME: u16 = 0x0003;
const TAP_KEYS: u16 = 0x0004;
const SET_DISPLAY_BRIGHTNESS: u16 = 0x0005;
const SET_KEYBOARD_BACKLIGHT: u16 = 0x0006;
const CANCEL_TOUCH_ID: u16 = 0x0007;
const HELLO_ACK: u16 = 0x8001;
const ACK: u16 = 0x8002;
const ERROR: u16 = 0x8003;
const FRAME_RELEASED: u16 = 0x9001;
const INPUT_FRAME: u16 = 0x9002;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Features(u64);

impl Features {
    pub const DISPLAY_BRIGHTNESS: Self = Self(0x08);
    pub const KEYBOARD_BACKLIGHT: Self = Self(0x10);
    pub const BASE: Self = Self(0x27);

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Features {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Key {
    Escape = 1,
    F1 = 59,
    F2 = 60,
    F3 = 61,
    F4 = 62,
    F5 = 63,
    F6 = 64,
    F7 = 65,
    F8 = 66,
    F9 = 67,
    F10 = 68,
    F11 = 87,
    F12 = 88,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ServiceError {
    Unsupported = 1,
    UnsupportedFeature = 2,
    ResourceLimit = 3,
    InvalidBuffer = 4,
    UnknownBuffer = 5,
    BufferBusy = 6,
    ActionDenied = 7,
    DeviceUnavailable = 8,
    IoFailure = 9,
    InternalFailure = 10,
}

impl ServiceError {
    fn decode(value: u32) -> Result<Self, WireError> {
        Ok(match value {
            1 => Self::Unsupported,
            2 => Self::UnsupportedFeature,
            3 => Self::ResourceLimit,
            4 => Self::InvalidBuffer,
            5 => Self::UnknownBuffer,
            6 => Self::BufferBusy,
            7 => Self::ActionDenied,
            8 => Self::DeviceUnavailable,
            9 => Self::IoFailure,
            10 => Self::InternalFailure,
            _ => return Err(WireError::InvalidField),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Damage {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    pub fn frame_length(self) -> Result<usize, WireError> {
        let length = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|length| *length > 0 && *length <= MAX_FRAME_BYTES)
            .ok_or(WireError::InvalidField)?;
        usize::try_from(length).map_err(|_| WireError::InvalidField)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelloAck {
    pub dimensions: Dimensions,
    pub max_buffers: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contact {
    pub id: u8,
    pub tip: bool,
    pub in_range: bool,
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputFrame {
    pub monotonic_ns: u64,
    pub fn_pressed: bool,
    pub contacts: Vec<Contact>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceMessage {
    HelloAck(HelloAck),
    Ack,
    Error(ServiceError),
    FrameReleased { buffer_id: u32, frame_id: u64 },
    InputFrame(InputFrame),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Envelope {
    pub request_id: u32,
    pub message: ServiceMessage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireError {
    PacketLength,
    Magic,
    Major,
    Direction,
    Phase,
    Identifier,
    InvalidField,
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PacketLength => "invalid T1Bridge packet length",
            Self::Magic => "invalid T1Bridge packet magic",
            Self::Major => "unsupported T1Bridge protocol major",
            Self::Direction => "invalid T1Bridge message direction",
            Self::Phase => "invalid T1Bridge message phase",
            Self::Identifier => "invalid T1Bridge request identifier",
            Self::InvalidField => "invalid T1Bridge message field",
        })
    }
}

impl Error for WireError {}

pub fn hello(request_id: u32, features: Features) -> Result<Vec<u8>, WireError> {
    let mut payload = Vec::with_capacity(12);
    push_u16(&mut payload, PROTOCOL_MINOR);
    push_u16(&mut payload, 0);
    push_u64(&mut payload, features.bits());
    packet(HELLO, request_id, &payload)
}

pub fn register_buffer(
    request_id: u32,
    buffer_id: u32,
    dimensions: Dimensions,
) -> Result<Vec<u8>, WireError> {
    if buffer_id == 0 {
        return Err(WireError::InvalidField);
    }
    let length = dimensions.frame_length()?;
    let stride = dimensions
        .width
        .checked_mul(4)
        .ok_or(WireError::InvalidField)?;
    let mut payload = Vec::with_capacity(16);
    push_u32(&mut payload, buffer_id);
    push_u32(&mut payload, stride);
    push_u64(
        &mut payload,
        u64::try_from(length).map_err(|_| WireError::InvalidField)?,
    );
    packet(REGISTER_BUFFER, request_id, &payload)
}

pub fn submit_frame(
    request_id: u32,
    buffer_id: u32,
    frame_id: u64,
    damage: &[Damage],
) -> Result<Vec<u8>, WireError> {
    if buffer_id == 0
        || frame_id == 0
        || damage.len() > MAX_DAMAGE_RECTS
        || damage.iter().any(|rect| {
            rect.width == 0
                || rect.height == 0
                || rect.x.checked_add(rect.width).is_none()
                || rect.y.checked_add(rect.height).is_none()
        })
    {
        return Err(WireError::InvalidField);
    }
    let mut payload = Vec::with_capacity(16 + damage.len() * 16);
    push_u32(&mut payload, buffer_id);
    push_u64(&mut payload, frame_id);
    push_u32(&mut payload, damage.len() as u32);
    for rect in damage {
        push_u32(&mut payload, rect.x);
        push_u32(&mut payload, rect.y);
        push_u32(&mut payload, rect.width);
        push_u32(&mut payload, rect.height);
    }
    packet(SUBMIT_FRAME, request_id, &payload)
}

pub fn tap_keys(request_id: u32, keys: &[Key]) -> Result<Vec<u8>, WireError> {
    if keys.is_empty() || keys.len() > MAX_KEYS {
        return Err(WireError::InvalidField);
    }
    let mut unique = HashSet::new();
    if !keys.iter().all(|key| unique.insert(*key as u16)) {
        return Err(WireError::InvalidField);
    }
    let mut payload = vec![0; 12];
    payload[0] = keys.len() as u8;
    for (index, key) in keys.iter().enumerate() {
        payload[4 + index * 2..6 + index * 2].copy_from_slice(&(*key as u16).to_le_bytes());
    }
    packet(TAP_KEYS, request_id, &payload)
}

pub fn set_display_brightness(request_id: u32, value: u8) -> Result<Vec<u8>, WireError> {
    percentage_packet(SET_DISPLAY_BRIGHTNESS, request_id, value)
}

pub fn set_keyboard_backlight(request_id: u32, value: u8) -> Result<Vec<u8>, WireError> {
    percentage_packet(SET_KEYBOARD_BACKLIGHT, request_id, value)
}

pub fn cancel_touch_id(request_id: u32) -> Result<Vec<u8>, WireError> {
    packet(CANCEL_TOUCH_ID, request_id, &[])
}

pub fn decode_service(bytes: &[u8], dimensions: Option<Dimensions>) -> Result<Envelope, WireError> {
    if bytes.len() < HEADER_LENGTH || bytes.len() > MAX_PACKET_LENGTH {
        return Err(WireError::PacketLength);
    }
    if &bytes[..4] != MAGIC {
        return Err(WireError::Magic);
    }
    if read_u16(bytes, 4)? != PROTOCOL_MAJOR {
        return Err(WireError::Major);
    }
    let message_type = read_u16(bytes, 6)?;
    let payload_length = read_u32(bytes, 8)? as usize;
    if payload_length != bytes.len() - HEADER_LENGTH {
        return Err(WireError::PacketLength);
    }
    let request_id = read_u32(bytes, 12)?;
    let payload = &bytes[HEADER_LENGTH..];
    let event = matches!(message_type, FRAME_RELEASED | INPUT_FRAME);
    if (event && request_id != 0) || (!event && request_id == 0) {
        return Err(WireError::Identifier);
    }
    match message_type {
        HELLO_ACK if dimensions.is_some() => return Err(WireError::Phase),
        ACK | FRAME_RELEASED | INPUT_FRAME if dimensions.is_none() => {
            return Err(WireError::Phase);
        }
        _ => {}
    }
    let message = match message_type {
        HELLO_ACK => ServiceMessage::HelloAck(decode_hello_ack(payload)?),
        ACK => {
            require_length(payload, 0)?;
            ServiceMessage::Ack
        }
        ERROR => {
            require_length(payload, 4)?;
            ServiceMessage::Error(ServiceError::decode(read_u32(payload, 0)?)?)
        }
        FRAME_RELEASED => {
            require_length(payload, 12)?;
            let buffer_id = read_u32(payload, 0)?;
            let frame_id = read_u64(payload, 4)?;
            if buffer_id == 0 || frame_id == 0 {
                return Err(WireError::InvalidField);
            }
            ServiceMessage::FrameReleased {
                buffer_id,
                frame_id,
            }
        }
        INPUT_FRAME => ServiceMessage::InputFrame(decode_input_frame(
            payload,
            dimensions.ok_or(WireError::Phase)?,
        )?),
        HELLO
        | REGISTER_BUFFER
        | SUBMIT_FRAME
        | TAP_KEYS
        | SET_DISPLAY_BRIGHTNESS
        | SET_KEYBOARD_BACKLIGHT
        | CANCEL_TOUCH_ID => return Err(WireError::Direction),
        _ => return Err(WireError::Direction),
    };
    Ok(Envelope {
        request_id,
        message,
    })
}

fn decode_hello_ack(payload: &[u8]) -> Result<HelloAck, WireError> {
    require_length(payload, 20)?;
    if read_u16(payload, 0)? != PROTOCOL_MINOR
        || read_u16(payload, 2)? != 0
        || read_u32(payload, 12)? != 1
    {
        return Err(WireError::InvalidField);
    }
    let dimensions = Dimensions {
        width: read_u32(payload, 4)?,
        height: read_u32(payload, 8)?,
    };
    dimensions.frame_length()?;
    let max_buffers = read_u32(payload, 16)?;
    if !(1..=3).contains(&max_buffers) {
        return Err(WireError::InvalidField);
    }
    Ok(HelloAck {
        dimensions,
        max_buffers,
    })
}

fn decode_input_frame(payload: &[u8], dimensions: Dimensions) -> Result<InputFrame, WireError> {
    if payload.len() < 12 {
        return Err(WireError::PacketLength);
    }
    let fn_pressed = decode_bool(payload[8])?;
    let count = payload[9] as usize;
    if count > 10 || read_u16(payload, 10)? != 0 || payload.len() != 12 + count * 12 {
        return Err(WireError::InvalidField);
    }
    let mut contacts = Vec::with_capacity(count);
    let mut unique = HashSet::new();
    for index in 0..count {
        let offset = 12 + index * 12;
        let id = payload[offset];
        let tip = decode_bool(payload[offset + 1])?;
        let in_range = decode_bool(payload[offset + 2])?;
        let x = read_u32(payload, offset + 4)?;
        let y = read_u32(payload, offset + 8)?;
        if payload[offset + 3] != 0
            || id > 15
            || !unique.insert(id)
            || x >= dimensions.width
            || y >= dimensions.height
        {
            return Err(WireError::InvalidField);
        }
        contacts.push(Contact {
            id,
            tip,
            in_range,
            x,
            y,
        });
    }
    Ok(InputFrame {
        monotonic_ns: read_u64(payload, 0)?,
        fn_pressed,
        contacts,
    })
}

fn percentage_packet(message_type: u16, request_id: u32, value: u8) -> Result<Vec<u8>, WireError> {
    if value > 100 {
        return Err(WireError::InvalidField);
    }
    packet(message_type, request_id, &[value])
}

fn packet(message_type: u16, request_id: u32, payload: &[u8]) -> Result<Vec<u8>, WireError> {
    if request_id == 0 || payload.len() > MAX_PACKET_LENGTH - HEADER_LENGTH {
        return Err(WireError::Identifier);
    }
    let mut packet = Vec::with_capacity(HEADER_LENGTH + payload.len());
    packet.extend_from_slice(MAGIC);
    push_u16(&mut packet, PROTOCOL_MAJOR);
    push_u16(&mut packet, message_type);
    push_u32(
        &mut packet,
        u32::try_from(payload.len()).map_err(|_| WireError::PacketLength)?,
    );
    push_u32(&mut packet, request_id);
    packet.extend_from_slice(payload);
    Ok(packet)
}

fn decode_bool(value: u8) -> Result<bool, WireError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(WireError::InvalidField),
    }
}

fn require_length(bytes: &[u8], expected: usize) -> Result<(), WireError> {
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(WireError::PacketLength)
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, WireError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(WireError::PacketLength)?
            .try_into()
            .map_err(|_| WireError::PacketLength)?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, WireError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(WireError::PacketLength)?
            .try_into()
            .map_err(|_| WireError::PacketLength)?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, WireError> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(WireError::PacketLength)?
            .try_into()
            .map_err(|_| WireError::PacketLength)?,
    ))
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_and_actions_match_minor_zero_layouts() {
        assert_eq!(
            hello(7, Features::BASE | Features::DISPLAY_BRIGHTNESS).unwrap(),
            [
                b'T', b'1', b'H', b'W', 1, 0, 1, 0, 12, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0x2f, 0,
                0, 0, 0, 0, 0, 0,
            ]
        );
        assert_eq!(
            set_display_brightness(8, 73).unwrap(),
            [
                b'T', b'1', b'H', b'W', 1, 0, 5, 0, 1, 0, 0, 0, 8, 0, 0, 0, 73
            ]
        );
        assert_eq!(
            cancel_touch_id(9).unwrap(),
            [b'T', b'1', b'H', b'W', 1, 0, 7, 0, 0, 0, 0, 0, 9, 0, 0, 0]
        );
        let frame = submit_frame(
            10,
            1,
            2,
            &[Damage {
                x: 20,
                y: 0,
                width: 50,
                height: 60,
            }],
        )
        .unwrap();
        assert_eq!(
            &frame[28..],
            &[
                1, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 50, 0, 0, 0, 60, 0, 0, 0
            ]
        );
    }

    #[test]
    fn submit_rejects_invalid_damage() {
        assert!(
            submit_frame(
                1,
                1,
                1,
                &[Damage {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 1
                }]
            )
            .is_err()
        );
        assert!(
            submit_frame(
                1,
                1,
                1,
                &vec![
                    Damage {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1
                    };
                    MAX_DAMAGE_RECTS + 1
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn decodes_acknowledgement_and_bounded_input() {
        let mut hello = vec![
            b'T', b'1', b'H', b'W', 1, 0, 1, 0x80, 20, 0, 0, 0, 5, 0, 0, 0,
        ];
        hello.extend_from_slice(&[
            0, 0, 0, 0, 122, 8, 0, 0, 60, 0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0,
        ]);
        assert_eq!(
            decode_service(&hello, None).unwrap(),
            Envelope {
                request_id: 5,
                message: ServiceMessage::HelloAck(HelloAck {
                    dimensions: Dimensions {
                        width: 2170,
                        height: 60,
                    },
                    max_buffers: 3,
                }),
            }
        );

        let mut input = vec![
            b'T', b'1', b'H', b'W', 1, 0, 2, 0x90, 24, 0, 0, 0, 0, 0, 0, 0,
        ];
        input.extend_from_slice(&99_u64.to_le_bytes());
        input.extend_from_slice(&[1, 1, 0, 0, 3, 1, 1, 0]);
        input.extend_from_slice(&400_u32.to_le_bytes());
        input.extend_from_slice(&20_u32.to_le_bytes());
        let decoded = decode_service(
            &input,
            Some(Dimensions {
                width: 2170,
                height: 60,
            }),
        )
        .unwrap();
        assert!(matches!(
            decoded.message,
            ServiceMessage::InputFrame(InputFrame { fn_pressed: true, ref contacts, .. })
                if contacts.len() == 1 && contacts[0].id == 3
        ));
    }

    #[test]
    fn rejects_noncanonical_fields_and_ranges() {
        assert_eq!(set_keyboard_backlight(1, 101), Err(WireError::InvalidField));
        assert_eq!(tap_keys(1, &[]), Err(WireError::InvalidField));
        assert_eq!(
            tap_keys(1, &[Key::F1, Key::F1]),
            Err(WireError::InvalidField)
        );

        let mut response = vec![
            b'T', b'1', b'H', b'W', 1, 0, 2, 0x80, 0, 0, 0, 0, 1, 0, 0, 0,
        ];
        response.push(0);
        assert_eq!(
            decode_service(
                &response,
                Some(Dimensions {
                    width: 1,
                    height: 1
                })
            ),
            Err(WireError::PacketLength)
        );
    }

    #[test]
    fn error_replies_are_valid_during_negotiation_and_steady_state() {
        let mut response = vec![
            b'T', b'1', b'H', b'W', 1, 0, 3, 0x80, 4, 0, 0, 0, 9, 0, 0, 0,
        ];
        response.extend_from_slice(&(ServiceError::UnsupportedFeature as u32).to_le_bytes());
        assert!(matches!(
            decode_service(&response, None).unwrap().message,
            ServiceMessage::Error(ServiceError::UnsupportedFeature)
        ));
        assert!(matches!(
            decode_service(
                &response,
                Some(Dimensions {
                    width: 2170,
                    height: 60,
                })
            )
            .unwrap()
            .message,
            ServiceMessage::Error(ServiceError::UnsupportedFeature)
        ));
    }
}

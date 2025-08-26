use num_derive::FromPrimitive;
// use num_traits::FromPrimitive;
use serde_derive::Deserialize;
use thiserror::Error;

pub const MARKER: [u8; 16] = [0xff; 16];
pub const VERSION: u8 = 4;
pub const MIN_MESSAGE_LENGTH: usize = 19;
pub const MAX_MESSAGE_LENGTH: usize = 4096;

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Deserialize, Hash, Eq)]
#[repr(u16)]
pub enum Afi {
    Ipv4 = 1,
    Ipv6,
}

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Deserialize, Hash, Eq)]
#[repr(u8)]
pub enum Safi {
    NLRIUnicast = 1,
    NLRIMulticast,
}

#[derive(Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct AddressFamily {
    pub afi: Afi,
    pub safi: Safi,
}

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Default)]
#[repr(u8)]
pub enum MessageType {
    Open = 1,
    #[default]
    Update,
    Notification,
    Keepalive,
}

#[derive(Debug, Clone, FromPrimitive, Copy, PartialEq)]
#[repr(u8)]
pub enum ErrorCode {
    MessageHeader = 1,
    OpenMessage,
    UpdateMessage,
    HoldTimerExpired,
    FSMError,
    Cease,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum HeaderSubCode {
    ConnectionNotSynchronized = 1,
    BadMessageLength = 2,
    BadMessageType = 3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum OpenSubCode {
    UnsupportedVersionNumber = 1,
    BadPeerAS = 2,
    BadBGPIdentifier = 3,
    UnsupportedOptionalParameter = 4,
    Deprecated = 5,
    UnacceptableHoldTime = 6,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum UpdateSubCode {
    MalformedAttributeList = 1,
    UnrecognizedWellKnownAttribute = 2,
    MissingWellKnownAttribute = 3,
    AttributeFlagsError = 4,
    AttributeLengthError = 5,
    InvalidORIGINAttribute = 6,
    Deprecated = 7,
    InvalidNEXTHOPAttribute = 8,
    OptionalAttributeError = 9,
    InvalidNetworkField = 10,
    MalformedASPATH = 11,
}

#[derive(Error, Debug)]
pub enum BgpValidationError {
    #[error("Message too short: got {actual}, minimum {minimum}")]
    MessageTooShort { actual: usize, minimum: usize },

    #[error("Message too long: got {actual}, maximum {maximum}")]
    MessageTooLong { actual: usize, maximum: usize },

    #[error("Invalid marker: expected all 0xFF")]
    InvalidMarker,

    #[error("Invalid message type: {0}")]
    InvalidMessageType(u8),

    #[error("Invalid BGP version: got {actual}, expected {expected}")]
    InvalidVersion { actual: u8, expected: u8 },

    #[error("Invalid AS number: {0}")]
    InvalidAsn(u16),

    #[error("Invalid hold time: {0}")]
    InvalidHoldTime(u16),

    #[error("Invalid router ID: {0}")]
    InvalidRouterId(u32),

    #[error("Invalid optional parameter length: {0}")]
    InvalidOptionalParameterLength(u8),

    #[error("Invalid path attribute length: {0}")]
    InvalidPathAttributeLength(usize),

    #[error("Invalid NLRI prefix length: {0}")]
    InvalidNlriPrefixLength(u8),

    #[error("Invalid buffer bounds: offset {offset}, length {length}, buffer size {buffer_size}")]
    InvalidBufferBounds {
        offset: usize,
        length: usize,
        buffer_size: usize,
    },

    #[error("Missing required path attribute: {0}")]
    MissingRequiredAttribute(String),

    #[error("Malformed AS_PATH: {0}")]
    MalformedAsPath(String),

    #[error("Invalid next hop: {0}")]
    InvalidNextHop(String),

    #[error("Invalid capability: {0}")]
    InvalidCapability(String),
}

impl BgpValidationError {
    pub fn to_notification_codes(&self) -> (ErrorCode, u8) {
        match self {
            BgpValidationError::MessageTooShort { .. }
            | BgpValidationError::MessageTooLong { .. } => (
                ErrorCode::MessageHeader,
                HeaderSubCode::BadMessageLength as u8,
            ),
            BgpValidationError::InvalidMarker => (
                ErrorCode::MessageHeader,
                HeaderSubCode::ConnectionNotSynchronized as u8,
            ),
            BgpValidationError::InvalidMessageType(_) => (
                ErrorCode::MessageHeader,
                HeaderSubCode::BadMessageType as u8,
            ),
            BgpValidationError::InvalidVersion { .. } => (
                ErrorCode::OpenMessage,
                OpenSubCode::UnsupportedVersionNumber as u8,
            ),
            BgpValidationError::InvalidAsn(_) => {
                (ErrorCode::OpenMessage, OpenSubCode::BadPeerAS as u8)
            }
            BgpValidationError::InvalidHoldTime(_) => (
                ErrorCode::OpenMessage,
                OpenSubCode::UnacceptableHoldTime as u8,
            ),
            BgpValidationError::InvalidRouterId(_) => {
                (ErrorCode::OpenMessage, OpenSubCode::BadBGPIdentifier as u8)
            }
            BgpValidationError::InvalidOptionalParameterLength(_)
            | BgpValidationError::InvalidCapability(_) => (
                ErrorCode::OpenMessage,
                OpenSubCode::UnsupportedOptionalParameter as u8,
            ),
            BgpValidationError::InvalidPathAttributeLength(_)
            | BgpValidationError::InvalidBufferBounds { .. } => (
                ErrorCode::UpdateMessage,
                UpdateSubCode::AttributeLengthError as u8,
            ),
            BgpValidationError::InvalidNlriPrefixLength(_) => (
                ErrorCode::UpdateMessage,
                UpdateSubCode::InvalidNetworkField as u8,
            ),
            BgpValidationError::MissingRequiredAttribute(_) => (
                ErrorCode::UpdateMessage,
                UpdateSubCode::MissingWellKnownAttribute as u8,
            ),
            BgpValidationError::MalformedAsPath(_) => (
                ErrorCode::UpdateMessage,
                UpdateSubCode::MalformedASPATH as u8,
            ),
            BgpValidationError::InvalidNextHop(_) => (
                ErrorCode::UpdateMessage,
                UpdateSubCode::InvalidNEXTHOPAttribute as u8,
            ),
        }
    }
}

pub fn validate_message_length(length: usize) -> Result<(), BgpValidationError> {
    if length < MIN_MESSAGE_LENGTH {
        return Err(BgpValidationError::MessageTooShort {
            actual: length,
            minimum: MIN_MESSAGE_LENGTH,
        });
    }
    if length > MAX_MESSAGE_LENGTH {
        return Err(BgpValidationError::MessageTooLong {
            actual: length,
            maximum: MAX_MESSAGE_LENGTH,
        });
    }
    Ok(())
}

pub fn validate_marker(marker: &[u8; 16]) -> Result<(), BgpValidationError> {
    if *marker != MARKER {
        return Err(BgpValidationError::InvalidMarker);
    }
    Ok(())
}

pub fn is_extended_len(mask: u8) -> bool {
    let mask = mask >> 4;
    !matches!(mask & 0b0001, 0)
}

pub fn prefix_bytes(plen: u8, afi: &Afi) -> Result<usize, BgpValidationError> {
    let max_len = match afi {
        Afi::Ipv4 => 32,
        Afi::Ipv6 => 128,
    };
    
    if plen > max_len {
        return Err(BgpValidationError::InvalidNlriPrefixLength(plen));
    }
    
    Ok((plen as usize).div_ceil(8))
}

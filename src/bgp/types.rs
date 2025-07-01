use num_derive::FromPrimitive;
// use num_traits::FromPrimitive;
use serde_derive::Deserialize;
use thiserror::Error;

// Constants
pub const MARKER: [u8; 16] = [0xff; 16];
pub const VERSION: u8 = 4;
pub const MAX: usize = 4096;
pub const MIN_MESSAGE_LENGTH: usize = 19;
pub const MAX_MESSAGE_LENGTH: usize = 4096;

// Basic enums
#[derive(Debug, Clone, FromPrimitive, PartialEq, Deserialize, Hash, Eq)]
#[repr(u16)]
pub enum Afi {
    Ipv4 = 1,
    Ipv6,
}

#[derive(Debug, Clone, FromPrimitive, PartialEq, Deserialize, Hash, Eq)]
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

#[derive(Debug, Clone, FromPrimitive, PartialEq, Default)]
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

// BGP-specific validation errors
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
    InvalidBufferBounds { offset: usize, length: usize, buffer_size: usize },
    
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
            BgpValidationError::MessageTooShort { .. } |
            BgpValidationError::MessageTooLong { .. } => {
                (ErrorCode::MessageHeader, HeaderSubCode::BadMessageLength as u8)
            }
            BgpValidationError::InvalidMarker => {
                (ErrorCode::MessageHeader, HeaderSubCode::ConnectionNotSynchronized as u8)
            }
            BgpValidationError::InvalidMessageType(_) => {
                (ErrorCode::MessageHeader, HeaderSubCode::BadMessageType as u8)
            }
            BgpValidationError::InvalidVersion { .. } => {
                (ErrorCode::OpenMessage, OpenSubCode::UnsupportedVersionNumber as u8)
            }
            BgpValidationError::InvalidAsn(_) => {
                (ErrorCode::OpenMessage, OpenSubCode::BadPeerAS as u8)
            }
            BgpValidationError::InvalidHoldTime(_) => {
                (ErrorCode::OpenMessage, OpenSubCode::UnacceptableHoldTime as u8)
            }
            BgpValidationError::InvalidRouterId(_) => {
                (ErrorCode::OpenMessage, OpenSubCode::BadBGPIdentifier as u8)
            }
            BgpValidationError::InvalidOptionalParameterLength(_) |
            BgpValidationError::InvalidCapability(_) => {
                (ErrorCode::OpenMessage, OpenSubCode::UnsupportedOptionalParameter as u8)
            }
            BgpValidationError::InvalidPathAttributeLength(_) |
            BgpValidationError::InvalidBufferBounds { .. } => {
                (ErrorCode::UpdateMessage, UpdateSubCode::AttributeLengthError as u8)
            }
            BgpValidationError::InvalidNlriPrefixLength(_) => {
                (ErrorCode::UpdateMessage, UpdateSubCode::InvalidNetworkField as u8)
            }
            BgpValidationError::MissingRequiredAttribute(_) => {
                (ErrorCode::UpdateMessage, UpdateSubCode::MissingWellKnownAttribute as u8)
            }
            BgpValidationError::MalformedAsPath(_) => {
                (ErrorCode::UpdateMessage, UpdateSubCode::MalformedASPATH as u8)
            }
            BgpValidationError::InvalidNextHop(_) => {
                (ErrorCode::UpdateMessage, UpdateSubCode::InvalidNEXTHOPAttribute as u8)
            }
        }
    }
}

// Validation helper functions
pub fn validate_buffer_bounds(buffer: &[u8], offset: usize, length: usize) -> Result<(), BgpValidationError> {
    if offset + length > buffer.len() {
        return Err(BgpValidationError::InvalidBufferBounds {
            offset,
            length,
            buffer_size: buffer.len(),
        });
    }
    Ok(())
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

pub fn validate_bgp_version(version: u8) -> Result<(), BgpValidationError> {
    if version != VERSION {
        return Err(BgpValidationError::InvalidVersion {
            actual: version,
            expected: VERSION,
        });
    }
    Ok(())
}

pub fn validate_asn(asn: u16) -> Result<(), BgpValidationError> {
    if asn == 0 {
        return Err(BgpValidationError::InvalidAsn(asn));
    }
    Ok(())
}

pub fn validate_hold_time(hold_time: u16) -> Result<(), BgpValidationError> {
    // Hold time must be 0 or >= 3 seconds per RFC 4271
    if hold_time != 0 && hold_time < 3 {
        return Err(BgpValidationError::InvalidHoldTime(hold_time));
    }
    Ok(())
}

pub fn validate_router_id(router_id: u32) -> Result<(), BgpValidationError> {
    if router_id == 0 {
        return Err(BgpValidationError::InvalidRouterId(router_id));
    }
    Ok(())
}

pub fn validate_nlri_prefix_length(prefix_len: u8, afi: &Afi) -> Result<(), BgpValidationError> {
    let max_prefix_len = match afi {
        Afi::Ipv4 => 32,
        Afi::Ipv6 => 128,
    };
    
    if prefix_len > max_prefix_len {
        return Err(BgpValidationError::InvalidNlriPrefixLength(prefix_len));
    }
    Ok(())
}

// Utility function for checking extended length bit in path attribute flags
pub fn is_extended_len(mask: u8) -> bool {
    let mask = mask >> 4;
    !matches!(mask & 0b0001, 0)
}

// Safe slice extraction with validation
pub fn safe_slice(buffer: &[u8], start: usize, end: usize) -> Result<&[u8], BgpValidationError> {
    if start > end || end > buffer.len() {
        return Err(BgpValidationError::InvalidBufferBounds {
            offset: start,
            length: end - start,
            buffer_size: buffer.len(),
        });
    }
    Ok(&buffer[start..end])
}

// Safe array extraction with validation
pub fn safe_array<const N: usize>(buffer: &[u8], offset: usize) -> Result<[u8; N], BgpValidationError> {
    validate_buffer_bounds(buffer, offset, N)?;
    let mut array = [0u8; N];
    array.copy_from_slice(&buffer[offset..offset + N]);
    Ok(array)
}
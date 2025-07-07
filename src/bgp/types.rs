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

// Validation helper functions
pub fn validate_buffer_bounds(
    buffer: &[u8],
    offset: usize,
    length: usize,
) -> Result<(), BgpValidationError> {
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
            length: buffer.len().saturating_sub(start), // or just 0
            buffer_size: buffer.len(),
        });
    }
    Ok(&buffer[start..end])
}

// Safe array extraction with validation
pub fn safe_array<const N: usize>(
    buffer: &[u8],
    offset: usize,
) -> Result<[u8; N], BgpValidationError> {
    validate_buffer_bounds(buffer, offset, N)?;
    let mut array = [0u8; N];
    array.copy_from_slice(&buffer[offset..offset + N]);
    Ok(array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(MARKER.len(), 16);
        assert!(MARKER.iter().all(|&b| b == 0xff));
        assert_eq!(VERSION, 4);
        assert_eq!(MIN_MESSAGE_LENGTH, 19);
        assert_eq!(MAX_MESSAGE_LENGTH, 4096);
    }

    #[test]
    fn test_validate_message_length() {
        assert!(validate_message_length(18).is_err());
        assert!(validate_message_length(19).is_ok());
        assert!(validate_message_length(1000).is_ok());
        assert!(validate_message_length(4096).is_ok());
        assert!(validate_message_length(4097).is_err());
    }

    #[test]
    fn test_validate_marker() {
        let good_marker = MARKER;
        assert!(validate_marker(&good_marker).is_ok());

        let mut bad_marker = MARKER;
        bad_marker[0] = 0xfe;
        assert!(validate_marker(&bad_marker).is_err());
    }

    #[test]
    fn test_validate_bgp_version() {
        assert!(validate_bgp_version(4).is_ok());
        assert!(validate_bgp_version(3).is_err());
        assert!(validate_bgp_version(5).is_err());
    }

    #[test]
    fn test_validate_asn() {
        assert!(validate_asn(0).is_err());
        assert!(validate_asn(1).is_ok());
        assert!(validate_asn(65535).is_ok());
    }

    #[test]
    fn test_validate_hold_time() {
        assert!(validate_hold_time(0).is_ok()); // 0 is valid (means don't send keepalives)
        assert!(validate_hold_time(1).is_err());
        assert!(validate_hold_time(2).is_err());
        assert!(validate_hold_time(3).is_ok());
        assert!(validate_hold_time(180).is_ok());
        assert!(validate_hold_time(65535).is_ok());
    }

    #[test]
    fn test_validate_router_id() {
        assert!(validate_router_id(0).is_err());
        assert!(validate_router_id(1).is_ok());
        assert!(validate_router_id(0xFFFFFFFF).is_ok());
    }

    #[test]
    fn test_validate_nlri_prefix_length() {
        assert!(validate_nlri_prefix_length(0, &Afi::Ipv4).is_ok());
        assert!(validate_nlri_prefix_length(32, &Afi::Ipv4).is_ok());
        assert!(validate_nlri_prefix_length(33, &Afi::Ipv4).is_err());

        assert!(validate_nlri_prefix_length(0, &Afi::Ipv6).is_ok());
        assert!(validate_nlri_prefix_length(128, &Afi::Ipv6).is_ok());
        assert!(validate_nlri_prefix_length(129, &Afi::Ipv6).is_err());
    }

    #[test]
    fn test_is_extended_len() {
        assert!(!is_extended_len(0b00000000)); // Extended bit not set
        assert!(is_extended_len(0b00010000)); // Extended bit set
        assert!(!is_extended_len(0b11100000)); // Extended bit not set
        assert!(is_extended_len(0b11110000)); // Extended bit set
    }

    #[test]
    fn test_safe_slice() {
        let buffer = vec![1, 2, 3, 4, 5];

        assert!(safe_slice(&buffer, 0, 3).is_ok());
        assert_eq!(safe_slice(&buffer, 0, 3).unwrap(), &[1, 2, 3]);

        assert!(safe_slice(&buffer, 2, 5).is_ok());
        assert_eq!(safe_slice(&buffer, 2, 5).unwrap(), &[3, 4, 5]);

        assert!(safe_slice(&buffer, 0, 6).is_err()); // End beyond buffer
        assert!(safe_slice(&buffer, 3, 2).is_err()); // Start > end
        assert!(safe_slice(&buffer, 6, 7).is_err()); // Start beyond buffer
    }

    #[test]
    fn test_safe_array() {
        let buffer = vec![1, 2, 3, 4, 5];

        let arr: Result<[u8; 2], _> = safe_array(&buffer, 0);
        assert!(arr.is_ok());
        assert_eq!(arr.unwrap(), [1, 2]);

        let arr: Result<[u8; 3], _> = safe_array(&buffer, 2);
        assert!(arr.is_ok());
        assert_eq!(arr.unwrap(), [3, 4, 5]);

        let arr: Result<[u8; 3], _> = safe_array(&buffer, 3);
        assert!(arr.is_err()); // Not enough bytes
    }

    #[test]
    fn test_error_to_notification_codes() {
        let err = BgpValidationError::InvalidMarker;
        let (code, subcode) = err.to_notification_codes();
        assert_eq!(code, ErrorCode::MessageHeader);
        assert_eq!(subcode, HeaderSubCode::ConnectionNotSynchronized as u8);

        let err = BgpValidationError::InvalidAsn(0);
        let (code, subcode) = err.to_notification_codes();
        assert_eq!(code, ErrorCode::OpenMessage);
        assert_eq!(subcode, OpenSubCode::BadPeerAS as u8);

        let err = BgpValidationError::MalformedAsPath("test".to_string());
        let (code, subcode) = err.to_notification_codes();
        assert_eq!(code, ErrorCode::UpdateMessage);
        assert_eq!(subcode, UpdateSubCode::MalformedASPATH as u8);
    }

    #[test]
    fn test_address_family_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        let af1 = AddressFamily {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        };
        let af2 = AddressFamily {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        };
        let af3 = AddressFamily {
            afi: Afi::Ipv6,
            safi: Safi::NLRIUnicast,
        };

        set.insert(af1.clone());
        assert!(!set.insert(af2)); // Should return false (already exists)
        assert!(set.insert(af3)); // Should return true (new)

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_message_type_default() {
        let msg_type = MessageType::default();
        assert_eq!(msg_type, MessageType::Update);
    }

    #[test]
    fn test_enum_from_primitive() {
        use num_traits::FromPrimitive;

        assert_eq!(Afi::from_u16(1), Some(Afi::Ipv4));
        assert_eq!(Afi::from_u16(2), Some(Afi::Ipv6));
        assert_eq!(Afi::from_u16(3), None);

        assert_eq!(Safi::from_u8(1), Some(Safi::NLRIUnicast));
        assert_eq!(Safi::from_u8(2), Some(Safi::NLRIMulticast));
        assert_eq!(Safi::from_u8(3), None);

        assert_eq!(MessageType::from_u8(1), Some(MessageType::Open));
        assert_eq!(MessageType::from_u8(2), Some(MessageType::Update));
        assert_eq!(MessageType::from_u8(3), Some(MessageType::Notification));
        assert_eq!(MessageType::from_u8(4), Some(MessageType::Keepalive));
        assert_eq!(MessageType::from_u8(5), None);
    }

    #[test]
    fn test_validate_buffer_bounds() {
        let buffer = vec![1, 2, 3, 4, 5];

        assert!(validate_buffer_bounds(&buffer, 0, 5).is_ok());
        assert!(validate_buffer_bounds(&buffer, 0, 6).is_err());
        assert!(validate_buffer_bounds(&buffer, 5, 1).is_err());
        assert!(validate_buffer_bounds(&buffer, 2, 3).is_ok());
    }
}

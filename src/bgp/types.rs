use num_derive::FromPrimitive;
// use num_traits::FromPrimitive;
use serde_derive::Deserialize;
// use std::error::Error;
// use std::fmt;
// use thiserror::Error;

// Constants
pub const MARKER: [u8; 16] = [0xff; 16];
pub const VERSION: u8 = 4;
pub const MAX: usize = 4096;

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

#[derive(Debug, Clone, FromPrimitive)]
#[repr(u8)]
pub enum ErrorCode {
    MessageHeader = 1,
    OpenMessage,
    UpdateMessage,
    HoldTimerExpired,
    FSMError,
    Cease,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
#[repr(u8)]
pub enum HeaderSubCode {
    ConnectionNotSynchronized = 1,
    BadMessageLength = 2,
    BadMessageType = 3,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
#[repr(u8)]
pub enum OpenSubCode {
    UnsupportedVersionNumber = 1,
    BadPeerAS = 2,
    BadBGPIdentifier = 3,
    UnsupportedOptionalParameter = 4,
    Deprecated = 5,
    UnacceptableHoldTime = 6,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
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

// Utility function for checking extended length bit in path attribute flags
pub fn is_extended_len(mask: u8) -> bool {
    let mask = mask >> 4;
    !matches!(mask & 0b0001, 0)
}

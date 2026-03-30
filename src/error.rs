use std::io;
use std::net::AddrParseError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BgpError {
    #[error("BGP protocol error: {0}")]
    Protocol(String),

    #[error("BGP message error: {0}")]
    Message(String),

    #[error("BGP session error: {0}")]
    Session(String),

    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Invalid address: {0}")]
    Address(#[from] AddrParseError),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Channel send error")]
    ChannelSend,

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid state transition: {0}")]
    InvalidState(String),

    #[error("Validation error: {0}")]
    Validation(#[from] crate::bgp::BgpValidationError),
}

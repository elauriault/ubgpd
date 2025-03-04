use thiserror::Error;
use std::io;
use std::net::AddrParseError;

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
}

#[derive(Error, Debug)]
pub enum RouteError {
    #[error("Failed to add route: {0}")]
    Add(String),
    
    #[error("Failed to delete route: {0}")]
    Delete(String),
    
    #[error("Failed to find route: {0}")]
    NotFound(String),
    
    #[error("Failed to retrieve routes: {0}")]
    Retrieval(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to parse config: {0}")]
    Parse(String),
    
    #[error("Invalid configuration: {0}")]
    Invalid(String),
    
    #[error("Missing required field: {0}")]
    Missing(String),
}

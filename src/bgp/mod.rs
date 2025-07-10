// Re-export types from submodules
pub use self::attributes::*;
pub use self::capabilities::*;
pub use self::codec::*;
pub use self::messages::*;
pub use self::nlri::*;
pub use self::types::*;

// Declare submodules
mod attributes;
mod capabilities;
mod codec;
mod messages;
mod nlri;
mod types;

// Include tests
#[cfg(test)]
mod tests;

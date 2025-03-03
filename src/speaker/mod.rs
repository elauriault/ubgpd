// File: src/speaker/mod.rs
//
// This file serves as the main module entry point that re-exports
// the public API from the submodules.

// mod builder;
mod connection;
mod events;
mod manager;
mod types;

// Re-export the public types and functions
// pub use builder::BGPSpeakerBuilder;
pub use events::{RibEvent, Update};
pub use types::BGPSpeaker;

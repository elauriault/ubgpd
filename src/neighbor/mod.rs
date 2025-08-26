// src/neighbor/mod.rs

mod capabilities;
mod connection;
mod fsm;
mod message_handler;
mod session;
mod timers;
mod types;

#[cfg(test)]
mod fsm_tests;

// Public exports
pub use capabilities::Capabilities;
pub use fsm::{connect, fsm_tcp};
pub use session::BGPNeighbor;
pub use types::{BGPState, Event};

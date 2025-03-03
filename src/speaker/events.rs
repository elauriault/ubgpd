// File: src/speaker/events.rs
//
// This file contains event types used for communication between components.

use crate::rib;

/// Event for updates to the RIB.
#[derive(Debug)]
pub enum RibEvent {
    UpdateRoutes(Update),
}

/// Event for updates to the FIB.
#[derive(Debug)]
pub enum FibEvent {
    RibUpdated,
}

/// Represents an update to routes (additions and withdrawals).
#[derive(Debug)]
pub struct Update {
    pub added: Option<rib::RibUpdate>,
    pub withdrawn: Option<rib::RibUpdate>,
    pub rid: u32,
}

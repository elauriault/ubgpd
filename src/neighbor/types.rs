// src/neighbor/types.rs

use crate::bgp::{self};
use crate::rib;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BGPState {
    Idle,
    Connect,
    #[default]
    Active,
    OpenSent,
    OpenConfirm,
    Established,
}

#[derive(Debug)]
pub enum Event {
    ManualStart,
    ManualStop,
    AutomaticStart,
    ManualStartWithPassiveTcpEstablishment,
    AutomaticStartWithPassiveTcpEstablishment,
    AutomaticStartWithDampPeerOscillations,
    AutomaticStartWithDampPeerOscillationsAndPassiveTcpEstablishment,
    AutomaticStop,
    ConnectRetryTimerExpires,
    HoldTimerExpires,
    KeepaliveTimerExpires,
    DelayOpenTimerExpires,
    IdleHoldTimerExpires,
    TcpConnectionValid,
    TcpCRInvalid,
    TcpCRAcked,
    TcpConnectionConfirmed,
    TcpConnectionFails,
    BGPOpen,
    BGPOpenWithDelayOpenTimerRunning,
    BGPHeaderErr,
    BGPOpenMsgErr,
    OpenCollisionDump,
    NotifMsgVerErr,
    NotifMsg,
    KeepAliveMsg,
    UpdateMsg,
    UpdateMsgErr,
    RibUpdate(Vec<(bgp::Nlri, Option<rib::RouteAttributes>)>),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Ord, Hash)]
pub enum PeeringType {
    Ibgp,
    Ebgp,
}

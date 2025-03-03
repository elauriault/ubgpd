use serde_derive::Deserialize;
use std::net::Ipv4Addr;

use crate::bgp;

/// Configuration for a BGP neighbor.
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    /// Autonomous System Number (ASN) of the router.
    pub asn: u16,
    /// Router ID (RID) of the router.
    pub rid: Ipv4Addr,
    /// Local IP address of the router.
    #[serde(default)]
    pub localip: Option<Ipv4Addr>,
    /// Port number for BGP connections.
    #[serde(default)]
    pub port: Option<u16>,
    /// Hold time for BGP connections.
    #[serde(default)]
    pub hold_time: Option<u16>,
    /// Address families supported by the router.
    #[serde(default)]
    pub families: Option<Vec<bgp::AddressFamily>>,
    /// List of neighbors for the router.
    #[serde(default)]
    pub neighbors: Option<Vec<Neighbor>>,
}

fn default_connect_retry() -> Option<u16> {
    Some(120)
}

fn default_keepalive_interval() -> Option<u16> {
    Some(60)
}

#[derive(Deserialize, Debug, Clone)]
pub struct Neighbor {
    /// Autonomous System Number (ASN) of the neighbor.
    pub asn: u16,
    /// IP address of the neighbor.
    pub ip: String,
    /// Port number for BGP connections to the neighbor.
    pub port: u16,
    /// Hold time for BGP connections to the neighbor.
    #[serde(default)]
    pub hold_time: Option<u16>,
    /// Address families supported by the neighbor.
    #[serde(default)]
    pub families: Option<Vec<bgp::AddressFamily>>,
    /// Connect retry interval for BGP connections to the neighbor.
    #[serde(default = "default_connect_retry")]
    pub connect_retry: Option<u16>,
    /// Keepalive interval for BGP connections to the neighbor.
    #[serde(default = "default_keepalive_interval")]
    pub keepalive_interval: Option<u16>,
}

// impl Default for Neighbor {
//     fn default() -> Self {
//         Neighbor {
//             connect_retry: Some(120),
//         }
//     }
// }

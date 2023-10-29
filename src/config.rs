use serde_derive::Deserialize;
use std::net::Ipv4Addr;

use crate::bgp;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub asn: u16,
    pub rid: Ipv4Addr,
    pub localip: Option<Ipv4Addr>,
    pub port: Option<u16>,
    pub hold_time: Option<u16>,
    pub families: Option<Vec<bgp::AddressFamily>>,
    pub neighbors: Option<Vec<Neighbor>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Neighbor {
    pub asn: u16,
    pub ip: String,
    pub port: u16,
    pub hold_time: Option<u16>,
    pub families: Option<Vec<bgp::AddressFamily>>,
    pub connect_retry: Option<u16>,
    pub keepalive_interval: Option<u16>,
}

// impl Default for Neighbor {
//     fn default() -> Self {
//         Neighbor {
//             connect_retry: Some(120),
//         }
//     }
// }

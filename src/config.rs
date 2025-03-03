use serde_derive::Deserialize;
use std::net::Ipv4Addr;

use crate::bgp;
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub asn: u16,
    pub rid: Ipv4Addr,
    #[serde(default)]
    pub localip: Option<Ipv4Addr>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub hold_time: Option<u16>,
    #[serde(default)]
    pub families: Option<Vec<bgp::AddressFamily>>,
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
    pub asn: u16,
    pub ip: String,
    pub port: u16,
    #[serde(default)]
    pub hold_time: Option<u16>,
    #[serde(default)]
    pub families: Option<Vec<bgp::AddressFamily>>,
    #[serde(default = "default_connect_retry")]
    pub connect_retry: Option<u16>,
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

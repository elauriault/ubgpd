use anyhow::{Context, Result};
use std::io::prelude::*;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use crate::bgp;
use serde_derive::Deserialize;

pub const BGP_DEFAULT_PORT: u16 = 179;
pub const BGP_DEFAULT_HOLD_TIME: u16 = 3;
pub const BGP_DEFAULT_LOCAL_IP: &str = "[::]:0";

/// Configuration for a BGP neighbor.
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    /// Autonomous System Number (ASN) of the router.
    pub asn: u16,
    /// Router ID (RID) of the router.
    pub rid: Ipv4Addr,
    /// Local IP address of the router.
    #[serde(default)]
    pub localips: Option<Vec<IpAddr>>,
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

pub fn read_config(path: &PathBuf) -> Result<Config> {
    let mut f = std::fs::File::open(path)
        .with_context(|| format!("Failed to open config file {}", path.display()))?;

    let mut c = String::new();
    f.read_to_string(&mut c)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;

    let config: Config = toml::from_str(&c)
        .with_context(|| format!("Failed to parse config file {}", path.display()))?;

    Ok(config)
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

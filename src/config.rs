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
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub asn: u16,
    pub rid: Ipv4Addr,
    #[serde(default)]
    pub localips: Option<Vec<IpAddr>>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub hold_time: Option<u16>,
    #[serde(default)]
    pub families: Option<Vec<bgp::AddressFamily>>,
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

fn default_max_retry_count() -> Option<u16> {
    None
}

fn default_exponential_backoff() -> bool {
    false
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
    #[serde(default = "default_max_retry_count")]
    pub max_retry_count: Option<u16>,
    #[serde(default = "default_exponential_backoff")]
    pub exponential_backoff: bool,
}

// impl Default for Neighbor {
//     fn default() -> Self {
//         Neighbor {
//             connect_retry: Some(120),
//         }
//     }
// }

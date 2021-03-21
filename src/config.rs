use serde_derive::Deserialize;
use std::net::Ipv4Addr;
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub asn: u16,
    pub rid: Ipv4Addr,
    pub localip: Option<Ipv4Addr>,
    pub holdtime: Option<u16>,
    pub port: Option<u16>,
    pub neighbors: Option<Vec<Neighbor>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Neighbor {
    pub asn: u16,
    pub ip: String,
    pub port: u16,
    connect_retry: Option<u16>,
    holdtime: Option<u16>,
    keepalive_interval: Option<u16>,
}

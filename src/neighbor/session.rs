// src/neighbor/session.rs

use super::capabilities::Capabilities;
use super::types::BGPState;
use crate::bgp::{self, AddressFamily};
use crate::rib::{self, RibUpdate};
use crate::speaker;
use derive_builder::Builder;
use std::collections::HashMap;
use std::net::IpAddr;

#[derive(Default, Builder, Debug, Clone, Copy)]
#[builder(default)]
pub struct BGPSessionAttributes {
    pub state: BGPState,
    pub hold_time: u16,
    pub hold_timer: usize,
    pub keepalive_time: usize,
    pub keepalive_timer: usize,
    pub connect_retry_time: u16,
    pub connect_retry_timer: usize,
    pub connect_retry_counter: usize,
    // pub accept_connections_unconfigured_peers: bool,
    pub allow_automatic_start: bool,
    // pub allow_automatic_stop: bool,
    // pub collision_detect_established_state: bool,
    // pub damp_peer_oscillations: bool,
    // pub delay_open: bool,
    // pub delay_open_time: usize,
    // pub delay_open_timer: usize,
    // pub idle_hold_time: usize,
    // pub idle_hold_timer: usize,
    pub passive_tcp_establishment: bool,
    // pub send_notification_without_open: bool,
    // pub track_tcp_state: bool,
}

#[derive(Debug, Clone)]
pub struct BGPNeighbor {
    pub local_ip: Option<IpAddr>,
    pub local_port: Option<u16>,
    pub local_asn: u16,
    pub local_rid: u32,
    pub remote_ip: Option<IpAddr>,
    pub remote_port: Option<u16>,
    pub remote_asn: Option<u16>,
    pub remote_rid: Option<u32>,
    // connect_retry_time: Option<u16>,
    pub capabilities_advertised: Capabilities,
    pub capabilities_received: Capabilities,
    pub adjrib: HashMap<bgp::AddressFamily, rib::Rib>,
    pub tx: Option<tokio::sync::mpsc::Sender<super::types::Event>>,
    pub ribtx: HashMap<bgp::AddressFamily, tokio::sync::mpsc::Sender<speaker::RibEvent>>,
    pub attributes: BGPSessionAttributes,
}

impl BGPNeighbor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        local_ip: Option<IpAddr>,
        local_port: Option<u16>,
        local_asn: u16,
        local_rid: u32,
        remote_ip: Option<IpAddr>,
        remote_port: Option<u16>,
        remote_asn: Option<u16>,
        hold_time: u16,
        connect_retry_time: u16,
        state: BGPState,
        families: Option<Vec<bgp::AddressFamily>>,
        ribtx: HashMap<bgp::AddressFamily, tokio::sync::mpsc::Sender<speaker::RibEvent>>,
    ) -> Self {
        let tx = None;
        let attributes = BGPSessionAttributesBuilder::default()
            .connect_retry_time(connect_retry_time)
            .hold_time(hold_time)
            .state(state)
            .allow_automatic_start(true)
            .build()
            .unwrap();
        let capabilities_advertised = Capabilities {
            multiprotocol: families,
            ..Default::default()
        };
        BGPNeighbor {
            local_ip,
            local_port,
            local_asn,
            local_rid,
            remote_ip,
            remote_port,
            remote_asn,
            remote_rid: None,
            capabilities_advertised,
            capabilities_received: Capabilities::default(),
            adjrib: HashMap::default(),
            tx,
            ribtx,
            attributes,
        }
    }

    pub async fn is_established(&self) -> bool {
        matches!(self.attributes.state, BGPState::Established)
    }

    pub async fn adjrib_add(&mut self, af: AddressFamily, routes: RibUpdate) {
        println!("Adding routes to ajdrib {:?} : {:?}", af, routes);
        match self.adjrib.get_mut(&af) {
            None => {
                let mut rib = rib::Rib::default();
                for nlri in routes.nlris {
                    rib.insert(nlri, vec![routes.attributes.clone()]);
                }
                self.adjrib.insert(af.clone(), rib);
            }
            Some(rib) => {
                for nlri in routes.nlris {
                    match rib.get_mut(&nlri) {
                        None => {
                            rib.insert(nlri, vec![routes.attributes.clone()]);
                        }
                        Some(attributes) => {
                            attributes.clear();
                            attributes.push(routes.attributes.clone());
                        }
                    }
                }
            }
        }
    }

    pub async fn adjrib_withdraw(&mut self, af: AddressFamily, routes: RibUpdate) {
        println!("Removing routes from adjrib {:?} : {:?}", af, routes);
        match self.adjrib.get_mut(&af) {
            None => {}
            Some(rib) => {
                for nlri in routes.nlris {
                    rib.remove(&nlri);
                }
            }
        }
    }
}

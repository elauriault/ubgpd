// File: src/speaker/types.rs
//
// This file contains the main BGPSpeaker struct and its associated types.

use async_std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::net::Ipv4Addr;

use crate::bgp;
use crate::config;
use crate::fib;
use crate::neighbor;
use crate::rib;

use super::events::{FibEvent, RibEvent};

/// The main BGP speaker struct that coordinates BGP operations.
#[derive(Builder, Debug)]
#[builder(setter(into))]
pub struct BGPSpeaker {
    pub local_asn: u16,
    pub router_id: u32,
    pub hold_time: u16,
    pub local_ip: Ipv4Addr,
    pub local_port: u16,
    pub families: Vec<bgp::AddressFamily>,
    pub rib: HashMap<bgp::AddressFamily, Arc<Mutex<rib::Rib>>>,
    pub fib: HashMap<bgp::AddressFamily, Arc<Mutex<fib::Fib>>>,
    pub ribtx: HashMap<bgp::AddressFamily, tokio::sync::mpsc::Sender<RibEvent>>,
    pub neighbors: Vec<Arc<Mutex<neighbor::BGPNeighbor>>>,
}

impl BGPSpeaker {
    /// Create a new BGP speaker instance.
    pub fn new(
        local_asn: u16,
        router_id: u32,
        hold_time: u16,
        local_ip: Ipv4Addr,
        local_port: u16,
        families: Vec<bgp::AddressFamily>,
    ) -> Self {
        BGPSpeakerBuilder::default()
            .local_asn(local_asn)
            .router_id(router_id)
            .hold_time(hold_time)
            .local_ip(local_ip)
            .local_port(local_port)
            .families(families)
            .rib(HashMap::new())
            .fib(HashMap::new())
            .ribtx(HashMap::new())
            .neighbors(vec![])
            .build()
            .unwrap()
    }

    /// Add a neighbor to the BGP speaker.
    pub async fn add_neighbor(
        &mut self,
        config: config::Neighbor,
        ribtx: HashMap<bgp::AddressFamily, tokio::sync::mpsc::Sender<RibEvent>>,
    ) {
        let n = Arc::new(Mutex::new(neighbor::BGPNeighbor::new(
            None,
            None,
            self.local_asn,
            self.router_id,
            Some(config.ip.parse().unwrap()),
            Some(config.port),
            Some(config.asn),
            config.hold_time.unwrap(),
            config.connect_retry.unwrap(),
            neighbor::BGPState::Idle,
            config.families,
            ribtx,
        )));
        self.neighbors.push(n);
    }

    /// Start the BGP speaker with all its associated processes.
    pub async fn start(speaker: Arc<Mutex<BGPSpeaker>>) {
        use super::connection;
        use super::manager;
        use tokio::sync::mpsc;

        // Initialize RIB and FIB for each address family
        {
            let mut speaker = speaker.lock().await;
            for af in speaker.families.clone() {
                let rib = Arc::new(Mutex::new(HashMap::new()));
                let fib = Arc::new(Mutex::new(fib::Fib::new(af.clone()).await));
                let (rib_tx, rib_rx) = mpsc::channel::<RibEvent>(100);
                let (fib_tx, fib_rx) = mpsc::channel::<FibEvent>(100);
                speaker.rib.insert(af.clone(), rib.clone());
                speaker.ribtx.insert(af.clone(), rib_tx);
                speaker.fib.insert(af, fib.clone());
                let r1 = rib.clone();
                let f1 = fib.clone();
                let asn = speaker.local_asn;
                let neighbors = speaker.neighbors.clone();
                tokio::spawn(async move {
                    manager::rib_mgr(r1, f1, neighbors, asn, rib_rx, fib_tx).await
                });
                tokio::spawn(async move { manager::fib_mgr(fib, rib, fib_rx).await });
            }
        }

        let s1 = speaker.clone();
        let s2 = speaker.clone();

        tokio::spawn(async move { connection::connect_mgr(s1).await });
        tokio::spawn(async move { connection::listen(s2).await });
    }
}

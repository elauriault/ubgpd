use async_std::sync::{Arc, Mutex};
// use futures::prelude::sink::SinkExt;
// use futures::TryFutureExt;
use std::collections::HashMap;
// use std::error::Error;
// use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
// use std::task::Wake;
// use std::sync::mpsc::{channel, Receiver};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
// use tokio_stream::StreamExt;
// use tokio_util::codec::Framed;

use crate::bgp;
// use crate::bgp::AddressFamily;
use crate::config;
use crate::fib;
use crate::neighbor;
use crate::rib;

#[derive(Debug)]
pub struct Update {
    pub added: Option<rib::RibUpdate>,
    pub withdrawn: Option<rib::RibUpdate>,
    pub rid: u32,
}

#[derive(Debug)]
pub enum RibEvent {
    // AddRoutes(rib::RibUpdate),
    // WithdrawRoutes(rib::RibUpdate),
    UpdateRoutes(Update),
}

#[derive(Debug)]
pub enum FibEvent {
    RibUpdated,
}

#[derive(Builder, Debug)]
#[builder(setter(into))]
pub struct BGPSpeaker {
    pub local_asn: u16,
    pub router_id: u32,
    pub hold_time: u16,
    local_ip: Ipv4Addr,
    local_port: u16,
    families: Vec<bgp::AddressFamily>,
    pub rib: HashMap<bgp::AddressFamily, Arc<Mutex<rib::Rib>>>,
    pub fib: HashMap<bgp::AddressFamily, Arc<Mutex<fib::Fib>>>,
    pub ribtx: HashMap<bgp::AddressFamily, tokio::sync::mpsc::Sender<RibEvent>>,
    pub neighbors: Vec<Arc<Mutex<neighbor::BGPNeighbor>>>,
}

impl BGPSpeaker {
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

    async fn add_incoming(speaker: Arc<Mutex<BGPSpeaker>>, socket: TcpStream, addr: SocketAddr) {
        println!("A new connection!");
        let n;
        {
            let mut s = speaker.lock().await;
            let remote_asn = None;
            let local_addr = socket.local_addr().unwrap();
            let local_ip = local_addr.ip();
            let local_port = local_addr.port();
            let local_asn = s.local_asn;
            let local_rid = s.router_id;
            let remote_ip = addr.ip();
            let remote_port = addr.port();
            let hold_time = s.hold_time;
            let connect_retry_time = 120; // This is a default value
                                          // for neighbor in s.neighbors.clone() {
                                          //     let n = neighbor.lock().await;
                                          //     if addr.ip() == n.remote_ip {
                                          //         asn = n.remote_asn;
                                          //         port = n.remote_port;
                                          //         hold_time = n.attributes.hold_time;
                                          //     }
                                          // }
            {
                let speaker = speaker.lock().await;
                // let ribtx = speaker.ribtx.clone();
                n = Arc::new(Mutex::new(neighbor::BGPNeighbor::new(
                    Some(local_ip),
                    Some(local_port),
                    local_asn,
                    local_rid,
                    Some(remote_ip),
                    Some(remote_port),
                    remote_asn,
                    hold_time,
                    connect_retry_time,
                    neighbor::BGPState::Active,
                    Some(speaker.families.clone()),
                    speaker.ribtx.clone(),
                )));
            }
            s.neighbors.push(n.clone());
        }
        tokio::spawn(
            async move { neighbor::BGPNeighbor::fsm_tcp(n, socket, speaker.clone()).await },
        );
    }

    pub async fn start(speaker: Arc<Mutex<BGPSpeaker>>) {
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
                    BGPSpeaker::rib_mgr(r1, f1, neighbors, asn, rib_rx, fib_tx).await
                });
                tokio::spawn(async move { BGPSpeaker::fib_mgr(fib, rib, fib_rx).await });
            }
        }

        let s1 = speaker.clone();
        let s2 = speaker.clone();

        tokio::spawn(async move { BGPSpeaker::connection_mgr(s1).await });
        tokio::spawn(async move { BGPSpeaker::listen(s2).await });
    }

    async fn connection_mgr(speaker: Arc<Mutex<BGPSpeaker>>) {
        let neighbors;
        {
            let s = speaker.lock().await;
            neighbors = s.neighbors.clone();
        }
        for neighbor in neighbors {
            let speaker = speaker.clone();
            tokio::spawn(async move { neighbor::BGPNeighbor::connect(speaker, neighbor).await });
        }
    }

    async fn listen(speaker: Arc<Mutex<BGPSpeaker>>) {
        let local_ip;
        let local_port;
        {
            let s = speaker.lock().await;
            local_ip = s.local_ip;
            local_port = s.local_port;
        }

        let listener = TcpListener::bind(local_ip.to_string() + ":" + &local_port.to_string())
            .await
            .unwrap();

        loop {
            let (socket, addr) = listener.accept().await.unwrap();
            BGPSpeaker::add_incoming(speaker.clone(), socket, addr).await;
        }
    }

    pub async fn best_reachable(
        fib: Arc<Mutex<fib::Fib>>,
        attributes: Vec<rib::RouteAttributes>,
    ) -> Option<rib::RouteAttributes> {
        let fib = fib.lock().await;
        attributes
            .iter()
            .find(|a| fib.has_route(a.next_hop))
            .cloned()
    }

    async fn loc_rib_added(
        rib: Arc<Mutex<rib::Rib>>,
        fib: Arc<Mutex<fib::Fib>>,
        asn: u16,
        routes: rib::RibUpdate,
    ) -> Vec<(bgp::Nlri, Option<rib::RouteAttributes>)> {
        let mut modified = vec![];
        // println!("Adding routes {:?} from {:?}", routes, msg.rid);
        let mut rib = rib.lock().await;
        for nlri in routes.nlris {
            match rib.get_mut(&nlri) {
                None => {
                    if routes.attributes.is_valid(asn).await {
                        rib.insert(nlri, vec![routes.attributes.clone()]);
                        {
                            let fib = fib.lock().await;
                            if fib.has_route(routes.attributes.clone().next_hop) {
                                modified.push((nlri, Some(routes.attributes.clone())));
                            }
                        }
                    }
                }
                Some(all_attributes) => {
                    if routes.attributes.is_valid(asn).await {
                        let previous_best =
                            Self::best_reachable(fib.clone(), all_attributes.to_vec()).await;

                        all_attributes.push(routes.attributes.clone());
                        all_attributes.sort();
                        all_attributes.reverse();

                        {
                            let fib = fib.lock().await;
                            if fib.has_route(routes.attributes.clone().next_hop) {
                                match previous_best {
                                    None => {
                                        modified.push((nlri, Some(routes.attributes.clone())));
                                    }
                                    Some(best) => {
                                        if routes.attributes > best {
                                            modified.push((nlri, Some(routes.attributes.clone())));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        modified
    }

    async fn loc_rib_withdraw(
        rib: Arc<Mutex<rib::Rib>>,
        fib: Arc<Mutex<fib::Fib>>,
        routes: rib::RibUpdate,
    ) -> Vec<(bgp::Nlri, Option<rib::RouteAttributes>)> {
        let mut modified = vec![];
        let mut rib = rib.lock().await;
        for nlri in routes.nlris {
            match rib.get_mut(&nlri) {
                None => {}
                Some(all_attributes) => {
                    let previous_best =
                        Self::best_reachable(fib.clone(), all_attributes.to_vec()).await;

                    all_attributes.retain(|a| !a.is_from_neighbor(routes.attributes.peer_rid));
                    if all_attributes.is_empty() {
                        rib.remove(&nlri);
                    }

                    match previous_best {
                        None => {}
                        Some(best) => {
                            if best.peer_rid == routes.attributes.peer_rid {
                                modified.push((nlri, None));
                            }
                        }
                    }
                }
            }
        }
        modified
    }

    async fn rib_mgr(
        rib: Arc<Mutex<rib::Rib>>,
        fib: Arc<Mutex<fib::Fib>>,
        neighbors: Vec<Arc<Mutex<neighbor::BGPNeighbor>>>,
        asn: u16,
        mut rx: tokio::sync::mpsc::Receiver<RibEvent>,
        tx: tokio::sync::mpsc::Sender<FibEvent>,
    ) {
        // println!("starting rib manager for {:?}", family);

        loop {
            let e = rx.recv().await.unwrap();
            println!("Rib Manager got {:?}", e);
            match e {
                RibEvent::UpdateRoutes(msg) => {
                    let mut modified = vec![];
                    match msg.added {
                        None => {}
                        Some(routes) => {
                            println!("Adding routes {:?} from {:?}", routes, msg.rid);
                            let mut added =
                                Self::loc_rib_added(rib.clone(), fib.clone(), asn, routes.clone())
                                    .await;
                            modified.append(&mut added);
                        }
                    };
                    match msg.withdrawn {
                        None => {}
                        Some(routes) => {
                            println!("Withdrawing routes {:?} from {:?}", routes, msg.rid);
                            let mut withdraw =
                                Self::loc_rib_withdraw(rib.clone(), fib.clone(), routes.clone())
                                    .await;
                            modified.append(&mut withdraw);
                        }
                    };
                    if !modified.is_empty() {
                        let _ = tx.send(FibEvent::RibUpdated).await;
                        for n in &neighbors {
                            let n = n.lock().await;
                            if n.is_established().await {
                                let tx = n.tx.clone();
                                tx.unwrap()
                                    .send(neighbor::Event::RibUpdate(modified.clone()))
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                }
            }
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn fib_mgr(
        fib: Arc<Mutex<fib::Fib>>,
        rib: Arc<Mutex<rib::Rib>>,
        mut rx: tokio::sync::mpsc::Receiver<FibEvent>,
    ) {
        println!("starting fib manager");

        loop {
            let e = rx.recv().await.unwrap();
            match e {
                FibEvent::RibUpdated => {
                    println!("Fib Manager : Got {:?}", e);
                    let mut fib = fib.lock().await;
                    fib.refresh().await;
                    fib.sync(rib.clone()).await;
                }
            }
            sleep(Duration::from_secs(1)).await;
            let mut fib = fib.lock().await;
            fib.refresh().await;
        }
    }
}

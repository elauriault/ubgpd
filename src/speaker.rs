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
use crate::bgp::AddressFamily;
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
            config.ip.parse().unwrap(),
            config.port,
            config.asn,
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
            let asn = 0;
            let port = addr.port();
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
                    addr.ip(),
                    port,
                    asn,
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
                speaker.fib.insert(af.clone(), fib.clone());
                let r1 = rib.clone();
                let f1 = fib.clone();
                let asn = speaker.local_asn.clone();
                tokio::spawn(async move { BGPSpeaker::rib_mgr(r1, f1, asn, rib_rx, fib_tx).await });
                tokio::spawn(async move { BGPSpeaker::fib_mgr(fib, rib, af, fib_rx).await });
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

    async fn rib_mgr(
        rib: Arc<Mutex<rib::Rib>>,
        fib: Arc<Mutex<fib::Fib>>,
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
                            let mut rib = rib.lock().await;
                            for nlri in routes.nlris {
                                match rib.get_mut(&nlri) {
                                    None => {
                                        if routes.attributes.is_valid(asn, fib.clone()).await {
                                            rib.insert(
                                                nlri.clone(),
                                                vec![routes.attributes.clone()],
                                            );
                                            modified.push(nlri.clone());
                                        }
                                    }
                                    Some(attributes) => {
                                        if routes.attributes > *attributes.first().unwrap() {
                                            attributes.clear();
                                            if routes.attributes.is_valid(asn, fib.clone()).await {
                                                attributes.push(routes.attributes.clone());
                                                modified.push(nlri.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    };
                    match msg.withdrawn {
                        None => {}
                        Some(routes) => {
                            println!("Withdrawing routes {:?} from {:?}", routes, msg.rid);
                            let mut rib = rib.lock().await;
                            let n = routes.attributes.peer_rid;
                            for nlri in routes.nlris {
                                match rib.get_mut(&nlri) {
                                    None => {}
                                    Some(attributes) => {
                                        let initial_best = attributes.first().unwrap().clone();
                                        attributes.retain(|x| !x.from_neighbor(n));
                                        let new_best = attributes.first().unwrap();
                                        if initial_best != *new_best {
                                            modified.push(nlri.clone());
                                        }
                                    }
                                }
                            }
                        }
                    };
                } // RibEvent::AddRoutes(routes) => {
                  //     println!("Adding routes {:?}", routes);
                  //     let mut rib = rib.lock().await;
                  //     for nlri in routes.nlris {
                  //         match rib.get_mut(&nlri) {
                  //             None => {
                  //                 rib.insert(nlri, vec![routes.attributes.clone()]);
                  //             }
                  //             Some(attributes) => {
                  //                 attributes.push(routes.attributes.clone());
                  //             }
                  //         }
                  //     }
                  // }
                  // RibEvent::WithdrawRoutes(routes) => {
                  //     println!("Withdrawing routes {:?}", routes);
                  //     let mut rib = rib.lock().await;
                  //     let n = routes.attributes.peer_rid;
                  //     for nlri in routes.nlris {
                  //         match rib.get_mut(&nlri) {
                  //             None => {}
                  //             Some(attributes) => {
                  //                 attributes.retain(|x| !x.from_neighbor(n));
                  //             }
                  //         }
                  //     }
                  // }
            }
            //
            // HERE WE MEED TO UPDATE THE NEIGHBORS WITH THE CONTENT of modified
            //
            let _ = tx.send(FibEvent::RibUpdated).await;
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn fib_mgr(
        fib: Arc<Mutex<fib::Fib>>,
        rib: Arc<Mutex<rib::Rib>>,
        family: AddressFamily,
        mut rx: tokio::sync::mpsc::Receiver<FibEvent>,
    ) {
        println!("starting fib manager for {:?}", family);

        loop {
            let e = rx.recv().await.unwrap();
            match e {
                FibEvent::RibUpdated => {
                    println!("Fib Manager {:?} : Got {:?}", family, e);
                    let mut fib = fib.lock().await;
                    fib.refresh(family.clone()).await;
                    fib.sync(rib.clone()).await;
                }
            }
            sleep(Duration::from_secs(1)).await;
            let mut fib = fib.lock().await;
            fib.refresh(family.clone()).await;
        }
    }
}

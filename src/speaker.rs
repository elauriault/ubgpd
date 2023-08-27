use async_std::sync::{Arc, Mutex};
// use futures::prelude::sink::SinkExt;
// use futures::TryFutureExt;
use std::collections::HashMap;
// use std::error::Error;
// use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
// use std::sync::mpsc::{channel, Receiver};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
// use tokio_stream::StreamExt;
// use tokio_util::codec::Framed;

// use crate::bgp;
use crate::config;
use crate::fib;
use crate::neighbor;
use crate::rib;

#[derive(Debug)]
pub enum RibEvent {
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
    pub rib: rib::Rib,
    pub ribtx: Option<tokio::sync::mpsc::Sender<RibEvent>>,
    pub neighbors: Vec<Arc<Mutex<neighbor::BGPNeighbor>>>,
}

impl BGPSpeaker {
    pub fn new(
        local_asn: u16,
        router_id: u32,
        hold_time: u16,
        local_ip: Ipv4Addr,
        local_port: u16,
    ) -> Self {
        BGPSpeakerBuilder::default()
            .local_asn(local_asn)
            .router_id(router_id)
            .hold_time(hold_time)
            .local_ip(local_ip)
            .local_port(local_port)
            .rib(HashMap::new())
            .ribtx(None)
            .neighbors(vec![])
            .build()
            .unwrap()
    }

    pub async fn add_neighbor(
        &mut self,
        config: config::Neighbor,
        ribtx: Option<tokio::sync::mpsc::Sender<RibEvent>>,
    ) {
        let n = Arc::new(Mutex::new(neighbor::BGPNeighbor::new(
            config.ip.parse().unwrap(),
            config.port,
            config.asn,
            config.holdtime.unwrap(),
            config.connect_retry,
            neighbor::BGPState::Idle,
            ribtx,
        )));
        self.neighbors.push(n);
    }

    async fn add_incoming(
        speaker: Arc<Mutex<BGPSpeaker>>,
        socket: TcpStream,
        addr: SocketAddr,
        // ribtx: tokio::sync::mpsc::Sender<RibEvent>,
    ) {
        println!("A new connection!");
        let n;
        {
            let mut s = speaker.lock().await;
            let asn = 0;
            let port = addr.port();
            let hold_time = s.hold_time;
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
                    None,
                    neighbor::BGPState::Active,
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
        let (tx, rx) = mpsc::channel::<RibEvent>(100);
        // let t1 = tx.clone();
        // let tx = Arc::new(Mutex::new(tx));

        {
            let mut speaker = speaker.lock().await;
            speaker.ribtx = Some(tx);
        }

        let s1 = speaker.clone();
        let s2 = speaker.clone();

        tokio::spawn(async move { BGPSpeaker::fib_mgr(speaker, rx).await });
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

    // async fn listen(speaker: Arc<Mutex<BGPSpeaker>>, ribtx: tokio::sync::mpsc::Sender<RibEvent>) {
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
            // BGPSpeaker::add_incoming(speaker.clone(), socket, addr, ribtx.clone()).await;
            BGPSpeaker::add_incoming(speaker.clone(), socket, addr).await;
        }
    }

    async fn fib_mgr(
        speaker: Arc<Mutex<BGPSpeaker>>,
        mut rx: tokio::sync::mpsc::Receiver<RibEvent>,
    ) {
        println!("starting fib manager");

        loop {
            let e = rx.recv().await;
            println!("Fib Manager : Got {:?}", e);
            let mut f = fib::Fib::new().await;
            {
                let s = speaker.lock().await;
                f.sync(s.rib.clone()).await;
            }
            sleep(Duration::from_secs(1)).await;
        }
    }
}

use async_std::sync::{Arc, Mutex};
use futures::prelude::sink::SinkExt;
use std::error::Error;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::bgp;

#[derive(Default, Builder, Debug)]
#[builder(setter(into))]
pub struct BGPSpeaker {
    local_asn: u16,
    router_id: u32,
    hold_time: u16,

    neighbors: Vec<Arc<Mutex<BGPNeighbor>>>,
}

impl BGPSpeaker {
    pub fn new(local_asn: u16, router_id: u32, hold_time: u16) -> Self {
        BGPSpeakerBuilder::default()
            .local_asn(local_asn)
            .router_id(router_id)
            .hold_time(hold_time)
            .neighbors(vec![])
            .build()
            .unwrap()
    }
    pub fn add_neighbor(&mut self, socket: TcpStream) {
        let n = Arc::new(Mutex::new(BGPNeighbor::new(self.hold_time)));
        self.neighbors.push(n.clone());
        tokio::spawn(BGPNeighbor::fsm(
            n,
            socket,
            self.local_asn,
            self.router_id,
            self.hold_time,
        ));
    }
}

#[derive(Default, Builder, Debug, Clone, Copy)]
#[builder(default)]
pub struct BGPNeighbor {
    remote_asn: u16,
    router_id: u32,
    attributes: BGPSessionAttributes,
}

#[derive(Default, Builder, Debug, Clone, Copy)]
#[builder(default)]
pub struct BGPSessionAttributes {
    state: usize,
    hold_time: u16,
    hold_timer: usize,
    keepalive_time: usize,
    keepalive_timer: usize,
    connect_retry_time: usize,
    connect_retry_timer: usize,
    connect_retry_counter: usize,
    // accept_connections_unconfigured_peers: bool,
    // allow_automatic_start: bool,
    // allow_automatic_stop: bool,
    // collision_detect_established_state: bool,
    // damp_peer_oscillations: bool,
    // delay_open: bool,
    // delay_open_time: usize,
    // delay_open_timer: usize,
    // idle_hold_time: usize,
    // idle_hold_timer: usize,
    // passive_tcp_establishment: bool,
    // send_notification_without_open: bool,
    // track_tcp_state: bool,
}

#[derive(Debug)]
enum Event {
    // HoldTimerExpired,
    SendKeepalive,
    KeepaliveExpired,
    ReceivedOpen,
    ReceivedKeepalive,
    ReceivedUpdate,
    ReceivedNotification,
}

impl BGPNeighbor {
    pub fn new(hold: u16) -> Self {
        let attr = BGPSessionAttributesBuilder::default()
            .hold_time(hold)
            .build()
            .unwrap();
        BGPNeighborBuilder::default()
            .attributes(attr)
            .build()
            .unwrap()
    }

    async fn fsm(n: Arc<Mutex<BGPNeighbor>>, s: TcpStream, asn: u16, rid: u32, hold: u16) {
        println!("starting fsm for {:?} with {:?}", n, s);

        let (tx, mut rx) = mpsc::channel::<Event>(100);

        let mut server = Framed::new(s, bgp::BGPMessageCodec);
        let _ = BGPNeighbor::send_open(&mut server, asn, rid, hold)
            .await
            .unwrap();

        let na = n.clone();
        let ta = tx.clone();
        tokio::spawn(async {
            BGPNeighbor::timer_hold(na, ta).await;
        });

        loop {
            tokio::select! {
                Some(m) = BGPNeighbor::read_message(&mut server) => {
                    BGPNeighbor::process_message(m,n.clone(),tx.clone()).await;
                }
                Some(e) = rx.recv() => {
                    BGPNeighbor::process_event(e,&mut server).await;
                }
            }
        }
    }

    async fn timer_hold(n: Arc<Mutex<BGPNeighbor>>, tx: mpsc::Sender<Event>) {
        loop {
            let s;
            {
                s = n.lock().await.attributes.hold_time;
            }
            println!("Sleeping for {} seconds", s as u64 / 3);
            sleep(Duration::from_secs(s as u64 / 3)).await;
            tx.send(Event::SendKeepalive).await.unwrap();
        }
    }

    async fn timer_keepalive(n: Arc<Mutex<BGPNeighbor>>, tx: mpsc::Sender<Event>) {
        println!("Starting keepalive timer");
        loop {
            sleep(Duration::from_secs(1)).await;
            let k;
            let h;
            {
                let mut n = n.lock().await;
                n.attributes.keepalive_timer += 1;
                k = n.attributes.keepalive_timer;
                h = n.attributes.hold_time as usize;
            }
            println!("Keepalive incremented");
            if k > h {
                tx.send(Event::KeepaliveExpired).await.unwrap()
            }
        }
    }

    async fn process_event(
        e: Event,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        match e {
            Event::SendKeepalive => {
                let _ = BGPNeighbor::send_keepalive(server).await.unwrap();
            }
            _ => {
                println!("{:?}", e);
            }
        }
    }

    async fn process_message(
        m: bgp::Message,
        nb: Arc<Mutex<BGPNeighbor>>,
        tb: mpsc::Sender<Event>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(body) => {
                {
                    let mut n = nb.lock().await;
                    n.attributes.keepalive_timer = 0;
                    println!("Keepalive reset");
                }
                tb.send(Event::ReceivedKeepalive).await.unwrap();
            }
            bgp::BGPMessageBody::Open(body) => {
                println!("{}", body);
                {
                    let mut n = nb.lock().await;
                    n.attributes.hold_time = body.hold_time;
                    n.router_id = body.router_id;
                    n.remote_asn = body.local_asn;
                    println!("Neighbor updated : {:?}", n);
                }
                let na = nb.clone();
                let ta = tb.clone();
                tokio::spawn(async {
                    BGPNeighbor::timer_keepalive(na, ta).await;
                });
                tb.send(Event::ReceivedOpen).await.unwrap();
            }
            bgp::BGPMessageBody::Notification(body) => {
                tb.send(Event::ReceivedNotification).await.unwrap();
            }
            bgp::BGPMessageBody::Update(body) => {
                tb.send(Event::ReceivedUpdate).await.unwrap();
            }
        };
    }

    async fn send_open(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
        asn: u16,
        rid: u32,
        hold: u16,
    ) -> Result<(), Box<dyn Error>> {
        let body = bgp::BGPOpenMessage::new(asn, rid, hold).unwrap();
        println!("open :{:?}", body);
        let message: Vec<u8> =
            bgp::Message::new(bgp::MessageType::OPEN, bgp::BGPMessageBody::Open(body))
                .unwrap()
                .into();
        println!("message :{:?}", message);
        let r = server.send(message).await;
        match r {
            Ok(_) => Ok(()),
            Err(e) => {
                println!("{:?}", e);
                // Ok(())
                Err(Box::new(e))
            }
        }
    }

    async fn send_keepalive(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) -> Result<(), Box<dyn Error>> {
        let body = bgp::BGPKeepaliveMessage::new().unwrap();
        // println!("keepalive :{:?}", body);
        let message: Vec<u8> = bgp::Message::new(
            bgp::MessageType::KEEPALIVE,
            bgp::BGPMessageBody::Keepalive(body),
        )
        .unwrap()
        .into();
        println!("Sending keepalive");
        let r = server.send(message).await;
        match r {
            Ok(_) => Ok(()),
            Err(e) => {
                println!("{:?}", e);
                // Err(e)
                Err(Box::new(e))
                // Ok(())
            }
        }
    }

    async fn read_message(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) -> Option<bgp::Message> {
        let message = server.next().await;
        match message {
            Some(bytes) => {
                let bytes: bgp::Message = bgp::Message::from(bytes.unwrap());
                Some(bytes)
            }
            None => None,
        }
    }
}

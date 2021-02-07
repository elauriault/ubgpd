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
        let _ = tokio::spawn(BGPNeighbor::fsm(
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
        tokio::spawn(async move {
            loop {
                let s;
                {
                    s = na.lock().await.attributes.hold_time;
                }
                println!("Sleeping for {} seconds", s as u64 / 3);
                sleep(Duration::from_secs(s as u64 / 3)).await;
                ta.send(Event::SendKeepalive).await.unwrap();
            }
        });

        let nb = n.clone();
        let tb = tx.clone();
        tokio::spawn(async move {
            loop {
                let m = BGPNeighbor::read_message(&mut server).await.unwrap();
                match m.header.message_type {
                    bgp::MessageType::KEEPALIVE => {
                        tb.send(Event::ReceivedKeepalive).await.unwrap();
                    }
                    bgp::MessageType::OPEN => {
                        let o: bgp::BGPOpenMessage = bgp::BGPOpenMessage::from(m.body);
                        println!("{}", o);
                        {
                            let mut n = nb.lock().await;
                            n.attributes.hold_time = o.hold_time;
                            n.router_id = o.router_id;
                            n.remote_asn = o.local_asn;
                            println!("Neighbor updated : {:?}", n);
                        }
                        tb.send(Event::ReceivedOpen).await.unwrap();
                    }
                    bgp::MessageType::NOTIFICATION => {
                        tb.send(Event::ReceivedNotification).await.unwrap();
                    }
                    bgp::MessageType::UPDATE => {
                        tb.send(Event::ReceivedUpdate).await.unwrap();
                    }
                };
                let _ = BGPNeighbor::send_keepalive(&mut server).await.unwrap();
            }
        });
        drop(tx);

        while let Some(res) = rx.recv().await {
            match res {
                // SendKeepalive => {
                //     let _ = BGPNeighbor::send_keepalive(&mut server).await.unwrap();
                // }
                _ => {
                    println!("{:?}", res);
                }
            }
        }
        // loop {

        // let m = BGPNeighbor::read_message(&mut server).await.unwrap();
        // match m.header.message_type {
        //     bgp::MessageType::KEEPALIVE => {
        //         println!("KEEPALIVE")
        //     }
        //     bgp::MessageType::OPEN => {
        //         println!("{}", o);
        //         let o: bgp::BGPOpenMessage = bgp::BGPOpenMessage::from(m.body);
        //         {
        //             let mut n = n.lock().await;
        //             n.attributes.hold_time = o.hold_time;
        //             n.router_id = o.router_id;
        //             n.remote_asn = o.local_asn;
        //             println!("Neighbor updated : {:?}", n);
        //         }
        //     }
        //     bgp::MessageType::NOTIFICATION => {
        //         println!("NOTIFICATION")
        //     }
        //     bgp::MessageType::UPDATE => {
        //         println!("UPDATE")
        //     }
        // };
        // println!("Sending keepalive...");
        // let _ = BGPNeighbor::send_keepalive(&mut server).await.unwrap();
        // let s;
        // {
        //     s = n.lock().await.attributes.hold_time;
        // }
        // println!("Sleeping for {} seconds", s as u64 / 3);
        // sleep(Duration::from_secs(s as u64 / 3)).await;
        // }
    }

    async fn send_open(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
        asn: u16,
        rid: u32,
        hold: u16,
    ) -> Result<bgp::Message, Box<dyn Error>> {
        let body: Vec<u8> = bgp::BGPOpenMessage::new(asn, rid, hold).unwrap().into();
        println!("open :{:?}", body);
        let message: Vec<u8> = bgp::Message::new(bgp::MessageType::OPEN, body)
            .unwrap()
            .into();
        println!("message :{:?}", message);
        let r = server.send(message).await;
        match r {
            Ok(_) => Ok(bgp::Message::new(bgp::MessageType::KEEPALIVE, vec![]).unwrap()),
            Err(e) => {
                println!("{:?}", e);
                Ok(bgp::Message::new(bgp::MessageType::KEEPALIVE, vec![]).unwrap())
            }
        }
    }

    async fn send_keepalive(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) -> Result<bgp::Message, Box<dyn Error>> {
        let body: Vec<u8> = bgp::BGPKeepaliveMessage::new().unwrap().into();
        // println!("keepalive :{:?}", body);
        let message: Vec<u8> = bgp::Message::new(bgp::MessageType::KEEPALIVE, body)
            .unwrap()
            .into();
        println!("Sending keepalive");
        let r = server.send(message).await;
        match r {
            Ok(_) => Ok(bgp::Message::new(bgp::MessageType::KEEPALIVE, vec![]).unwrap()),
            Err(e) => {
                println!("{:?}", e);
                Ok(bgp::Message::new(bgp::MessageType::KEEPALIVE, vec![]).unwrap())
            }
        }
    }

    async fn read_message(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) -> Result<bgp::Message, Box<dyn Error>> {
        let message = server.next().await;
        match message {
            Some(bytes) => {
                let bytes: bgp::Message = bgp::Message::from(bytes.unwrap());
                // println!("Message received : {:?}", bytes.header.message_type);
                Ok(bytes)
            }
            None => {
                println!("Empty");
                Ok(bgp::Message::new(bgp::MessageType::KEEPALIVE, vec![]).unwrap())
            }
        }
    }
}

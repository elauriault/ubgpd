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
        let n = Arc::new(Mutex::new(BGPNeighbor::new(
            self.local_asn,
            self.router_id,
            self.hold_time,
        )));
        self.neighbors.push(n.clone());
        tokio::spawn(BGPNeighbor::fsm(
            n,
            socket,
            // self.local_asn,
            // self.router_id,
            // self.hold_time,
        ));
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BGPState {
    Idle,
    Connect,
    Active,
    OpenSent,
    OpenConfirm,
    Established,
}

impl Default for BGPState {
    fn default() -> Self {
        BGPState::Active
    }
}

#[derive(Default, Builder, Debug, Clone, Copy)]
#[builder(default)]
pub struct BGPSessionAttributes {
    state: BGPState,
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
    passive_tcp_establishment: bool,
    // send_notification_without_open: bool,
    // track_tcp_state: bool,
}

#[derive(Debug)]
enum Event {
    ManualStart,
    ManualStop,
    AutomaticStart,
    ManualStartWithPassiveTcpEstablishment,
    AutomaticStartWithPassiveTcpEstablishment,
    AutomaticStartWithDampPeerOscillations,
    AutomaticStartWithDampPeerOscillationsAndPassiveTcpEstablishment,
    AutomaticStop,
    ConnectRetryTimerExpires,
    HoldTimerExpires,
    KeepaliveTimerExpires,
    DelayOpenTimerExpires,
    IdleHoldTimerExpires,
    TcpConnectionValid,
    TcpCRInvalid,
    TcpCRAcked,
    TcpConnectionConfirmed,
    TcpConnectionFails,
    BGPOpen,
    BGPOpenWithDelayOpenTimerRunning,
    BGPHeaderErr,
    BGPOpenMsgErr,
    OpenCollisionDump,
    NotifMsgVerErr,
    NotifMsg,
    KeepAliveMsg,
    UpdateMsg,
    UpdateMsgErr,
}

#[derive(Default, Builder, Debug, Clone, Copy)]
#[builder(default)]
pub struct BGPNeighbor {
    remote_asn: u16,
    router_id: u32,
    attributes: BGPSessionAttributes,
    local_asn: u16,
    local_router_id: u32,
    local_hold_time: u16,
}

impl BGPNeighbor {
    pub fn new(local_asn: u16, local_router_id: u32, local_hold_time: u16) -> Self {
        let attr = BGPSessionAttributesBuilder::default()
            .hold_time(local_hold_time)
            .build()
            .unwrap();
        BGPNeighborBuilder::default()
            .local_hold_time(local_hold_time)
            .local_router_id(local_router_id)
            .local_asn(local_asn)
            .attributes(attr)
            .build()
            .unwrap()
    }

    async fn fsm(n: Arc<Mutex<BGPNeighbor>>, s: TcpStream) {
        println!("starting fsm for {:?} with {:?}", n, s);

        let (tx, mut rx) = mpsc::channel::<Event>(100);

        // let _ = tx.send(Event::AutomaticStart).await;
        // let _ = tx.send(Event::TcpConnectionConfirmed).await;

        let mut server = Framed::new(s, bgp::BGPMessageCodec);

        // let _ = tx.send(Event::TcpConnectionConfirmed).await;

        let na = n.clone();
        let ta = tx.clone();

        tokio::spawn(async {
            BGPNeighbor::timer_hold(na, ta).await;
        });

        loop {
            tokio::select! {
                Some(e) = rx.recv() => {
                    BGPNeighbor::process_event(e,n.clone(),&mut server).await;
                }
                Some(m) = BGPNeighbor::read_message(&mut server) => {
                    BGPNeighbor::process_message(m,n.clone(),tx.clone()).await;
                }
            }
        }
    }

    async fn init_peer(n: Arc<Mutex<BGPNeighbor>>) {
        {
            let mut n = n.lock().await;
            n.attributes.connect_retry_counter = 0;
            n.attributes.state = BGPState::Active;
        }
        println!("FSM init_peer: Idle to Active");
    }

    async fn timer_hold(n: Arc<Mutex<BGPNeighbor>>, tx: mpsc::Sender<Event>) {
        loop {
            let s;
            {
                s = n.lock().await.attributes.hold_time;
            }
            // println!("Sleeping for {} seconds", s as u64 / 3);
            sleep(Duration::from_secs(s as u64 / 3)).await;
            tx.send(Event::KeepaliveTimerExpires).await.unwrap();
        }
    }

    async fn timer_keepalive(n: Arc<Mutex<BGPNeighbor>>, tx: mpsc::Sender<Event>) {
        println!("FSM: Starting TimerKeepalive");
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
            println!("FSM: TimerKeepalive incremented");
            if k > h {
                tx.send(Event::KeepaliveTimerExpires).await.unwrap()
            }
        }
    }

    async fn process_event(
        e: Event,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        let state;
        {
            let nb = nb.lock().await;
            state = nb.attributes.state;
        }
        match state {
            BGPState::Active => {
                println!("FSM ACTIVE: received {:?}", e);
                BGPNeighbor::process_event_active(e, nb, server).await;
            }
            BGPState::Connect => {
                println!("FSM CONNECT: received {:?}", e);
                BGPNeighbor::process_event_connect(e, nb, server).await;
            }
            BGPState::OpenConfirm => {
                println!("FSM OPENCONFIRM: received {:?}", e);
                BGPNeighbor::process_event_openconfirm(e, nb, server).await;
            }
            BGPState::OpenSent => {
                println!("FSM OPENSENT: received {:?}", e);
                BGPNeighbor::process_event_opensent(e, nb, server).await;
            }
            BGPState::Established => {
                println!("FSM ESTABLISHED: received {:?}", e);
                BGPNeighbor::process_event_established(e, nb, server).await;
            }
            BGPState::Idle => {
                println!("FSM IDLE: received {:?}", e);
                BGPNeighbor::process_event_idle(e, nb, server).await;
            }
        }
    }

    async fn process_event_idle(
        e: Event,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        match e {
            Event::ManualStartWithPassiveTcpEstablishment => {
                println!("FSM IDLE: {:?} to be implemented", e);
            }
            Event::AutomaticStartWithPassiveTcpEstablishment => {
                println!("FSM IDLE: {:?} to be implemented", e);
            }
            Event::ManualStart => {
                println!("FSM IDLE: {:?} to be implemented", e);
            }
            Event::AutomaticStart => {
                BGPNeighbor::init_peer(nb).await;
            }
            _ => {
                println!("{:?}", e);
            }
        }
    }

    async fn process_event_connect(
        e: Event,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        match e {
            Event::KeepaliveTimerExpires => {
                let _ = BGPNeighbor::send_keepalive(server).await.unwrap();
            }
            Event::ManualStart => {
                let _ = BGPNeighbor::send_keepalive(server).await.unwrap();
            }
            Event::AutomaticStart => {
                BGPNeighbor::init_peer(nb).await;
            }
            _ => {
                println!("FSM CONNECT: {:?} looks like an error", e);
            }
        }
    }

    async fn process_event_active(
        e: Event,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        match e {
            Event::ManualStop => {
                println!("FSM ACTIVE: {:?} to be implemented", e);
            }
            Event::ConnectRetryTimerExpires => {
                println!("FSM ACTIVE: {:?} to be implemented", e);
            }
            Event::DelayOpenTimerExpires => {
                println!("FSM ACTIVE: {:?} to be implemented", e);
            }
            Event::TcpConnectionConfirmed => {
                let asn;
                let rid;
                let hold;
                {
                    let n = nb.lock().await;
                    asn = n.local_asn;
                    rid = n.local_router_id;
                    hold = n.local_hold_time;
                }
                let _ = BGPNeighbor::send_open(server, asn, rid, hold)
                    .await
                    .unwrap();
                {
                    let mut n = nb.lock().await;
                    n.attributes.state = BGPState::OpenSent;
                }
                println!("FSM: Active to OpenSent");
            }
            Event::NotifMsg => {
                println!("FSM ACTIVE: {:?} to be implemented", e);
            }
            _ => {
                println!("FSM: Looks {:?} like an error", e);
            }
        }
    }

    async fn process_event_opensent(
        e: Event,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        match e {
            Event::HoldTimerExpires => {
                println!("FSM OPENSENT: {:?} to be implemented", e);
            }
            Event::ManualStop => {
                println!("FSM OPENSENT: {:?} to be implemented", e);
            }
            Event::AutomaticStop => {
                println!("FSM OPENSENT: {:?} to be implemented", e);
            }
            Event::TcpConnectionValid => {
                println!("FSM OPENSENT: {:?} to be implemented", e);
            }
            Event::TcpConnectionConfirmed => {
                println!("FSM OPENSENT: {:?} to be implemented", e);
            }
            Event::TcpConnectionFails => {
                println!("FSM OPENSENT: {:?} to be implemented", e);
            }
            Event::NotifMsg => {
                println!("FSM OPENSENT: {:?} to be implemented", e);
            }
            _ => {
                println!("FSM OPENSENT: {:?} looks like an error", e);
            }
        }
    }

    async fn process_event_openconfirm(
        e: Event,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        match e {
            Event::KeepaliveTimerExpires => {
                let _ = BGPNeighbor::send_keepalive(server).await.unwrap();
            }
            Event::ManualStart => {
                println!("FSM OPENCONFIRM: {:?} to be implemented", e);
            }
            Event::AutomaticStart => {
                BGPNeighbor::init_peer(nb).await;
            }
            Event::NotifMsg => {
                println!("FSM OPENCONFIRM: {:?} to be implemented", e);
            }
            Event::BGPOpen => {
                let asn;
                let rid;
                let hold;
                {
                    let n = nb.lock().await;
                    asn = n.local_asn;
                    rid = n.local_router_id;
                    hold = n.local_hold_time;
                }
                let _ = BGPNeighbor::send_open(server, asn, rid, hold)
                    .await
                    .unwrap();
            }
            _ => {
                println!("FSM OPENCONFIRM: {:?} looks like an error", e);
            }
        }
    }

    async fn process_event_established(
        e: Event,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        match e {
            Event::HoldTimerExpires => {
                println!("FSM ESTABLISHED: {:?} to be implemented", e);
            }
            Event::AutomaticStop => {
                println!("FSM ESTABLISHED: {:?} to be implemented", e);
            }
            Event::ManualStop => {
                println!("FSM ESTABLISHED: {:?} to be implemented", e);
            }
            Event::TcpConnectionValid => {
                println!("FSM ESTABLISHED: {:?} to be implemented", e);
            }
            Event::NotifMsg => {
                println!("FSM ESTABLISHED: {:?} to be implemented", e);
            }
            Event::KeepaliveTimerExpires => {
                let _ = BGPNeighbor::send_keepalive(server).await.unwrap();
            }
            _ => {
                println!("FSM ESTABLISHED: {:?} looks like an error", e);
            }
        }
    }

    async fn process_message(
        m: bgp::Message,
        nb: Arc<Mutex<BGPNeighbor>>,
        tx: mpsc::Sender<Event>,
    ) {
        let state;
        {
            let nb = nb.lock().await;
            state = nb.attributes.state;
        }
        match state {
            BGPState::Active => {
                println!("FSM ACTIVE: received {:?}", m.body);
                BGPNeighbor::process_message_active(m, nb, tx).await;
            }
            BGPState::Connect => {
                println!("FSM CONNECT: received {:?}", m.body);
                BGPNeighbor::process_message_connect(m, nb, tx).await;
            }
            BGPState::OpenConfirm => {
                println!("FSM OPENCONFIRM: received {:?}", m.body);
                BGPNeighbor::process_message_openconfirm(m, nb, tx).await;
            }
            BGPState::OpenSent => {
                println!("FSM OPENSENT: received {:?}", m.body);
                BGPNeighbor::process_message_opensent(m, nb, tx).await;
            }
            BGPState::Established => {
                println!("FSM ESTABLISHED: received {:?}", m.body);
                BGPNeighbor::process_message_established(m, nb, tx).await;
            }
            BGPState::Idle => {
                println!("FSM IDLE: received {:?}", m.body);
                BGPNeighbor::process_message_idle(m, nb, tx).await;
            }
        }
    }

    async fn process_message_opensent(
        m: bgp::Message,
        nb: Arc<Mutex<BGPNeighbor>>,
        tb: mpsc::Sender<Event>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(_body) => {
                BGPNeighbor::process_message_keepalive(nb).await;
                // {
                //     let mut n = nb.lock().await;
                //     n.attributes.keepalive_timer = 0;
                //     // println!("Keepalive reset");
                // }
                // tb.send(Event::KeepAliveMsg).await.unwrap();
            }
            bgp::BGPMessageBody::Open(body) => {
                println!("FSM OPENSENT: Open {}", body);
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
                {
                    let mut n = nb.lock().await;
                    n.attributes.state = BGPState::OpenConfirm
                }
                println!("FSM OPENSENT: OpenSent to OpenConfirm");
            }
            bgp::BGPMessageBody::Notification(_body) => {
                println!("FSM OPENSENT: Notification unimplemented");
            }
            _ => {
                println!("FSM OPENSENT: Unimplemented");
                tb.send(Event::NotifMsg).await.unwrap();
            }
        };
    }

    async fn process_message_active(
        m: bgp::Message,
        nb: Arc<Mutex<BGPNeighbor>>,
        tb: mpsc::Sender<Event>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Open(body) => {
                println!("FSM ACTIVE: Open {}", body);
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
                {
                    let mut n = nb.lock().await;
                    n.attributes.state = BGPState::OpenConfirm
                }
                println!("FSM ACTIVE: Active to OpenConfirm");
                tb.send(Event::BGPOpen).await.unwrap();
            }
            bgp::BGPMessageBody::Notification(_body) => {
                tb.send(Event::NotifMsg).await.unwrap();
            }
            _ => {
                println!("Unimplemented");
            }
        };
    }

    async fn process_message_connect(
        m: bgp::Message,
        nb: Arc<Mutex<BGPNeighbor>>,
        tb: mpsc::Sender<Event>,
    ) {
        match m.body {
            _ => {
                println!("FSM: Shouldn't receive messages in Connect state");
            }
        };
    }

    async fn process_message_openconfirm(
        m: bgp::Message,
        nb: Arc<Mutex<BGPNeighbor>>,
        tb: mpsc::Sender<Event>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(_body) => {
                BGPNeighbor::process_message_keepalive(nb.clone()).await;
                {
                    let mut n = nb.lock().await;
                    // n.attributes.keepalive_timer = 0;
                    n.attributes.state = BGPState::Established
                }
                println!("FSM: OpenConfirm to Established");
                // tb.send(Event::KeepAliveMsg).await.unwrap();
            }
            bgp::BGPMessageBody::Notification(_body) => {
                tb.send(Event::NotifMsg).await.unwrap();
            }
            _ => {
                println!("Unimplemented");
            }
        };
    }

    async fn process_message_established(
        m: bgp::Message,
        nb: Arc<Mutex<BGPNeighbor>>,
        tb: mpsc::Sender<Event>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(_body) => {
                BGPNeighbor::process_message_keepalive(nb).await;
            }
            bgp::BGPMessageBody::Notification(_body) => {
                tb.send(Event::NotifMsg).await.unwrap();
            }
            bgp::BGPMessageBody::Update(body) => {
                tb.send(Event::UpdateMsg).await.unwrap();
            }
            _ => {
                println!("Unimplemented");
            }
        };
    }

    async fn process_message_keepalive(nb: Arc<Mutex<BGPNeighbor>>) {
        let mut n = nb.lock().await;
        n.attributes.keepalive_timer = 0;
    }

    async fn process_message_idle(
        m: bgp::Message,
        nb: Arc<Mutex<BGPNeighbor>>,
        tb: mpsc::Sender<Event>,
    ) {
        match m.body {
            _ => {
                println!("Unimplemented");
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
                Err(Box::new(e))
            }
        }
    }

    async fn send_keepalive(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) -> Result<(), Box<dyn Error>> {
        let body = bgp::BGPKeepaliveMessage::new().unwrap();
        let message: Vec<u8> = bgp::Message::new(
            bgp::MessageType::KEEPALIVE,
            bgp::BGPMessageBody::Keepalive(body.clone()),
        )
        .unwrap()
        .into();
        println!("FSM KeepaliveTimerExpires: Sending {:?}", body);
        let r = server.send(message).await;
        match r {
            Ok(_) => Ok(()),
            Err(e) => {
                println!("{:?}", e);
                Err(Box::new(e))
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

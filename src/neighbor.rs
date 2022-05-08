use async_std::sync::{Arc, Mutex};
use futures::prelude::sink::SinkExt;
use futures::TryFutureExt;
use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::mpsc::{channel, Receiver};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::bgp;
use crate::config;
use crate::fib;
use crate::rib;
use crate::speaker;

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
    allow_automatic_start: bool,
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

#[derive(Debug, Clone)]
pub struct BGPNeighbor {
    pub remote_ip: IpAddr,
    remote_port: u16,
    pub remote_asn: u16,
    pub router_id: u32,
    tx: Option<tokio::sync::mpsc::Sender<Event>>,
    ribtx: Option<tokio::sync::mpsc::Sender<speaker::RibEvent>>,
    attributes: BGPSessionAttributes,
}

impl BGPNeighbor {
    pub fn new(
        remote_ip: IpAddr,
        remote_port: u16,
        remote_asn: u16,
        hold_time: u16,
        state: BGPState,
        ribtx: Option<tokio::sync::mpsc::Sender<speaker::RibEvent>>,
    ) -> Self {
        let tx = None;
        // let ribtx = None;
        let attributes = BGPSessionAttributesBuilder::default()
            .hold_time(hold_time)
            .state(state)
            .allow_automatic_start(true)
            .build()
            .unwrap();
        BGPNeighbor {
            remote_ip,
            remote_port,
            remote_asn,
            router_id: 0,
            tx,
            ribtx,
            attributes,
        }
    }

    async fn fsm(neighbor: Arc<Mutex<BGPNeighbor>>, speaker: Arc<Mutex<speaker::BGPSpeaker>>) {
        println!("starting fsm for {:?}", neighbor);

        let (tx, mut rx) = mpsc::channel::<Event>(100);

        let state;
        let automatic_start;
        {
            let mut n = neighbor.lock().await;
            state = n.attributes.state;
            automatic_start = n.attributes.allow_automatic_start;
            n.tx = Some(tx.clone());
            // let _ = n.ribtx.as_ref().unwrap().send(RibEvent::RibUpdated).await;
        }
        match state {
            BGPState::Idle => match automatic_start {
                true => {
                    BGPNeighbor::process_event(
                        Event::AutomaticStart,
                        speaker.clone(),
                        neighbor.clone(),
                        None,
                    )
                    .await;
                }
                false => {}
            },
            _ => {}
        }

        loop {
            match rx.recv().await {
                Some(e) => {
                    BGPNeighbor::process_event(e, speaker.clone(), neighbor.clone(), None).await;
                }
                None => {
                    break;
                }
            }
        }
    }

    pub async fn fsm_tcp(
        neighbor: Arc<Mutex<BGPNeighbor>>,
        stream: TcpStream,
        speaker: Arc<Mutex<speaker::BGPSpeaker>>,
    ) {
        println!("starting fsm_tcp for {:?} with {:?}", neighbor, stream);

        let (tx, mut rx) = mpsc::channel::<Event>(100);

        let mut server = Framed::new(stream, bgp::BGPMessageCodec);

        let state;
        {
            let mut n = neighbor.lock().await;
            state = n.attributes.state;
            n.tx = Some(tx.clone());
            // let _ = n.ribtx.as_ref().unwrap().send(RibEvent::RibUpdated).await;
        }
        match state {
            BGPState::Active => {
                BGPNeighbor::process_event(
                    Event::TcpConnectionConfirmed,
                    speaker.clone(),
                    neighbor.clone(),
                    Some(&mut server),
                )
                .await;
            }
            BGPState::Connect => {
                // sleep(Duration::from_secs(300)).await;
                BGPNeighbor::process_event(
                    Event::TcpConnectionValid,
                    speaker.clone(),
                    neighbor.clone(),
                    Some(&mut server),
                )
                .await;
            }
            _ => {}
        };

        let na = neighbor.clone();

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let hold_task = tokio::spawn(async { BGPNeighbor::timer_hold(na, receiver).await });

        loop {
            tokio::select! {
                Some(e) = rx.recv() => {
                    BGPNeighbor::process_event(e,speaker.clone(),neighbor.clone(),Some(&mut server)).await;
                }
                Some(m) = BGPNeighbor::read_message(&mut server) => {
                    match m {
                        Ok(m) => {
                            BGPNeighbor::process_message(m,speaker.clone(),neighbor.clone()).await;
                        },
                        Err(_) => {
                            BGPNeighbor::process_event(
                                Event::TcpConnectionFails,
                                speaker.clone(),
                                neighbor.clone(),
                                Some(&mut server),
                            )
                            .await;
                            let _ = sender.send(());
                            let _ = tokio::join!(hold_task);
                            break;
                        },
                    }
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

    pub async fn connect(
        speaker: Arc<Mutex<speaker::BGPSpeaker>>,
        neighbor: Arc<Mutex<BGPNeighbor>>,
    ) {
        let socket;
        {
            let mut n = neighbor.lock().await;
            socket = TcpStream::connect(n.remote_ip.to_string() + ":" + &n.remote_port.to_string())
                .await
                .unwrap();
            n.attributes.state = BGPState::Connect;
            {
                let s = speaker.lock().await;
                n.ribtx = s.ribtx.clone();
            }
        }

        tokio::spawn(async move { BGPNeighbor::fsm_tcp(neighbor.clone(), socket, speaker).await });
    }

    async fn timer_hold(
        n: Arc<Mutex<BGPNeighbor>>,
        mut receiver: tokio::sync::oneshot::Receiver<()>,
    ) {
        loop {
            let s;
            let tx;
            {
                let n = n.lock().await;
                s = n.attributes.hold_time;
                tx = n.tx.clone();
            }
            let tx = tx.unwrap();
            sleep(Duration::from_secs(s as u64 / 3)).await;
            if receiver.try_recv().is_ok() {
                println!("Exiting hold timer");
                break;
            }
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
        s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: Option<&mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>>,
    ) {
        let state;
        {
            let nb = nb.lock().await;
            state = nb.attributes.state;
        }

        match server {
            Some(server) => match state {
                BGPState::Active => {
                    println!("FSM ACTIVE: received {:?}", e);
                    BGPNeighbor::process_event_active(e, s, nb, server).await;
                }
                BGPState::Connect => {
                    println!("FSM CONNECT: received {:?}", e);
                    BGPNeighbor::process_event_connect(e, s, nb, server).await;
                }
                BGPState::OpenConfirm => {
                    println!("FSM OPENCONFIRM: received {:?}", e);
                    BGPNeighbor::process_event_openconfirm(e, s, nb, server).await;
                }
                BGPState::OpenSent => {
                    println!("FSM OPENSENT: received {:?}", e);
                    BGPNeighbor::process_event_opensent(e, nb, server).await;
                }
                BGPState::Established => {
                    println!("FSM ESTABLISHED: received {:?}", e);
                    BGPNeighbor::process_event_established(e, nb, server).await;
                }
                _ => {}
            },
            None => match state {
                BGPState::Idle => {
                    println!("FSM IDLE: received {:?}", e);
                    BGPNeighbor::process_event_idle(e, nb).await;
                }
                _ => {}
            },
        }
    }

    async fn process_event_idle(
        e: Event,
        nb: Arc<Mutex<BGPNeighbor>>,
        // server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
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
        s: Arc<Mutex<speaker::BGPSpeaker>>,
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
            Event::TcpConnectionValid => {
                let asn;
                let rid;
                let hold;
                {
                    let s = s.lock().await;
                    asn = s.local_asn;
                    rid = s.router_id;
                    hold = s.hold_time;
                }
                let _ = BGPNeighbor::send_open(server, asn, rid, hold)
                    .await
                    .unwrap();
                {
                    let mut n = nb.lock().await;
                    n.attributes.state = BGPState::OpenSent;
                }
                println!("FSM: Connect to OpenSent");
            }
            _ => {
                println!("FSM CONNECT: {:?} looks like an error", e);
            }
        }
    }

    async fn process_event_active(
        e: Event,
        s: Arc<Mutex<speaker::BGPSpeaker>>,
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
            Event::TcpConnectionFails => {
                println!("FSM ACTIVE: {:?} to be implemented", e);
            }
            Event::TcpConnectionConfirmed => {
                let asn;
                let rid;
                let hold;
                {
                    let s = s.lock().await;
                    asn = s.local_asn;
                    rid = s.router_id;
                    hold = s.hold_time;
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
        s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    ) {
        match e {
            Event::KeepaliveTimerExpires => {
                let _ = BGPNeighbor::send_keepalive(server).await.unwrap();
            }
            Event::TcpConnectionFails => {
                println!("FSM OPENCONFIRM: {:?} to be implemented", e);
            }
            Event::NotifMsg => {
                println!("FSM OPENCONFIRM: {:?} to be implemented", e);
            }
            Event::BGPOpen => {
                let asn;
                let rid;
                let hold;
                {
                    let s = s.lock().await;
                    asn = s.local_asn;
                    rid = s.router_id;
                    hold = s.hold_time;
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
            Event::TcpConnectionFails => {
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
        s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
    ) {
        let state;
        {
            let nb = nb.lock().await;
            state = nb.attributes.state;
        }
        match state {
            BGPState::Active => {
                println!("FSM ACTIVE: received {:?}", m.body);
                BGPNeighbor::process_message_active(m, s, nb).await;
            }
            BGPState::Connect => {
                println!("FSM CONNECT: received {:?}", m.body);
                BGPNeighbor::process_message_connect(m, nb).await;
            }
            BGPState::OpenConfirm => {
                println!("FSM OPENCONFIRM: received {:?}", m.body);
                BGPNeighbor::process_message_openconfirm(m, nb).await;
            }
            BGPState::OpenSent => {
                println!("FSM OPENSENT: received {:?}", m.body);
                BGPNeighbor::process_message_opensent(m, s, nb).await;
            }
            BGPState::Established => {
                println!("FSM ESTABLISHED: received {:?}", m.body);
                BGPNeighbor::process_message_established(m, s, nb).await;
            }
            BGPState::Idle => {
                println!("FSM IDLE: received {:?}", m.body);
                BGPNeighbor::process_message_idle(m, nb).await;
            }
        }
    }

    async fn process_message_opensent(
        m: bgp::Message,
        s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(_body) => {
                BGPNeighbor::handle_keepalive(nb).await;
            }
            bgp::BGPMessageBody::Open(body) => {
                println!("FSM OPENSENT: Open {}", body);
                let tx;
                {
                    let n = nb.lock().await;
                    tx = n.tx.clone().unwrap();
                }
                match BGPNeighbor::collision_detection(body.clone(), s).await {
                    true => {
                        tx.send(Event::OpenCollisionDump).await.unwrap();
                    }
                    false => match BGPNeighbor::validate_open(body.clone(), nb.clone()).await {
                        false => {
                            tx.send(Event::BGPOpenMsgErr).await.unwrap();
                        }
                        true => {
                            BGPNeighbor::update_from_open(body.clone(), nb.clone()).await;
                            let ta;
                            {
                                let n = nb.lock().await;
                                ta = n.tx.clone().unwrap();
                            }
                            tokio::spawn(async {
                                BGPNeighbor::timer_keepalive(nb, ta).await;
                            });
                            println!("FSM OPENSENT: OpenSent to OpenConfirm");
                            // tx.send(Event::BGPOpen).await.unwrap();
                        }
                    },
                }
            }
            bgp::BGPMessageBody::Notification(_body) => {
                println!("FSM OPENSENT: Notification unimplemented");
            }
            _ => {
                println!("FSM OPENSENT: Unimplemented");
                let tx;
                {
                    let n = nb.lock().await;
                    tx = n.tx.clone().unwrap();
                }
                tx.send(Event::NotifMsg).await.unwrap();
            }
        };
    }

    async fn process_message_active(
        m: bgp::Message,
        s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Open(body) => {
                println!("FSM ACTIVE: Open {}", body);
                let tx;
                {
                    let n = nb.lock().await;
                    tx = n.tx.clone().unwrap();
                }
                match BGPNeighbor::collision_detection(body.clone(), s).await {
                    true => {
                        tx.send(Event::OpenCollisionDump).await.unwrap();
                    }
                    false => match BGPNeighbor::validate_open(body.clone(), nb.clone()).await {
                        false => {
                            tx.send(Event::BGPOpenMsgErr).await.unwrap();
                        }
                        true => {
                            BGPNeighbor::update_from_open(body.clone(), nb.clone()).await;
                            let ta;
                            {
                                let n = nb.lock().await;
                                ta = n.tx.clone().unwrap();
                            }
                            tokio::spawn(async {
                                BGPNeighbor::timer_keepalive(nb, ta).await;
                            });
                            println!("FSM ACTIVE: Active to OpenConfirm");
                            tx.send(Event::BGPOpen).await.unwrap();
                        }
                    },
                }
            }
            bgp::BGPMessageBody::Notification(_body) => {
                let tx;
                {
                    let n = nb.lock().await;
                    tx = n.tx.clone().unwrap();
                }
                tx.send(Event::NotifMsg).await.unwrap();
            }
            _ => {
                println!("Unimplemented");
            }
        };
    }

    async fn process_message_connect(m: bgp::Message, nb: Arc<Mutex<BGPNeighbor>>) {
        match m.body {
            _ => {
                println!("FSM: Shouldn't receive messages in Connect state");
            }
        };
    }

    async fn process_message_openconfirm(m: bgp::Message, nb: Arc<Mutex<BGPNeighbor>>) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(_body) => {
                BGPNeighbor::handle_keepalive(nb.clone()).await;
                {
                    let mut n = nb.lock().await;
                    n.attributes.state = BGPState::Established
                }
                println!("FSM: OpenConfirm to Established");
            }
            bgp::BGPMessageBody::Notification(_body) => {
                let tx;
                {
                    let n = nb.lock().await;
                    tx = n.tx.clone().unwrap();
                }
                tx.send(Event::NotifMsg).await.unwrap();
            }
            _ => {
                println!("Unimplemented");
            }
        };
    }

    async fn process_message_established(
        m: bgp::Message,
        s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(_body) => {
                BGPNeighbor::handle_keepalive(nb).await;
            }
            bgp::BGPMessageBody::Notification(_body) => {
                let tx;
                {
                    let n = nb.lock().await;
                    tx = n.tx.clone().unwrap();
                }
                tx.send(Event::NotifMsg).await.unwrap();
            }
            bgp::BGPMessageBody::Update(body) => {
                BGPNeighbor::handle_update(body, s, nb).await;
            }
            _ => {
                println!("Unimplemented");
            }
        };
    }

    async fn process_message_idle(m: bgp::Message, nb: Arc<Mutex<BGPNeighbor>>) {
        match m.body {
            _ => {
                println!("Unimplemented");
            }
        };
    }

    async fn handle_keepalive(nb: Arc<Mutex<BGPNeighbor>>) {
        let mut n = nb.lock().await;
        n.attributes.keepalive_timer = 0;
    }

    async fn handle_update(
        m: bgp::BGPUpdateMessage,
        s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
    ) {
        let ra = rib::RouteAttributes::new(m.path_attributes, s.clone(), nb.clone()).await;
        {
            let mut s = s.lock().await;
            for nlri in m.nlri {
                match s.rib.get_mut(&nlri) {
                    None => {
                        s.rib.insert(nlri, vec![ra.clone()]);
                    }
                    Some(attributes) => {
                        attributes.push(ra.clone());
                    }
                }
            }
            let n;
            // let ribtx;
            {
                let nb = nb.lock().await;
                n = nb.router_id;
                // ribtx = nb.ribtx.clone();
            }
            for nlri in m.withdrawn_routes {
                match s.rib.get_mut(&nlri) {
                    None => {}
                    Some(attributes) => {
                        attributes.retain(|x| !x.from_neighbor(n));
                    }
                }
            }
            println!("RIB : {:?}", s.rib);
            {
                let nb = nb.lock().await;
                let _ = nb
                    .ribtx
                    .as_ref()
                    .unwrap()
                    .send(speaker::RibEvent::RibUpdated)
                    .await;
            }
        }
    }

    async fn collision_detection(
        message: bgp::BGPOpenMessage,
        speaker: Arc<Mutex<speaker::BGPSpeaker>>,
    ) -> bool {
        let s = speaker.lock().await;
        let ns = s.neighbors.clone();
        for n in ns {
            let n = n.lock().await;
            println!("Checking collision for {:?}", n);
            let tx = n.tx.clone();
            match tx {
                None => {}
                Some(t) => match n.attributes.state {
                    BGPState::OpenConfirm => {
                        if n.router_id == message.router_id {
                            if n.router_id < s.router_id {
                                let _ = t.send(Event::OpenCollisionDump).await;
                            }
                            return true;
                        }
                    }
                    BGPState::OpenSent => {
                        if n.router_id == message.router_id {
                            if n.router_id < s.router_id {
                                let _ = t.send(Event::OpenCollisionDump).await;
                            }
                            return true;
                        }
                    }
                    _ => {}
                },
            }
        }
        false
    }

    async fn validate_open(
        message: bgp::BGPOpenMessage,
        neighbor: Arc<Mutex<BGPNeighbor>>,
    ) -> bool {
        let n = neighbor.lock().await;
        if n.remote_asn != message.asn {
            println!(
                "n.remote_asn: {} != message.asn:{}",
                n.remote_asn, message.asn
            );
            return false;
            // panic!("ASN received doesn't match config");
        }
        true
    }

    async fn update_from_open(message: bgp::BGPOpenMessage, neighbor: Arc<Mutex<BGPNeighbor>>) {
        let mut n = neighbor.lock().await;
        n.attributes.hold_time = message.hold_time;
        n.router_id = message.router_id;
        n.attributes.state = BGPState::OpenConfirm;
        // println!("Neighbor updated : {:?}", n);
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
    ) -> Option<Result<bgp::Message, std::io::Error>> {
        let message = server.next().await;
        match message {
            Some(bytes) => match bytes {
                Err(e) => Some(Err(e)),
                Ok(r) => {
                    let bytes: bgp::Message = bgp::Message::from(r);
                    Some(Ok(bytes))
                }
            },
            None => None,
        }
    }
}

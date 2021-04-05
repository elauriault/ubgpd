use async_std::sync::{Arc, Mutex};
use futures::prelude::sink::SinkExt;
use std::error::Error;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::bgp;
use crate::config;
use crate::rib;

#[derive(Builder, Debug)]
#[builder(setter(into))]
pub struct BGPSpeaker {
    local_asn: u16,
    router_id: u32,
    hold_time: u16,
    local_ip: Ipv4Addr,
    local_port: u16,

    neighbors: Vec<Arc<Mutex<BGPNeighbor>>>,
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
            .neighbors(vec![])
            .build()
            .unwrap()
    }

    pub async fn add_neighbor(&mut self, config: config::Neighbor) {
        let n = Arc::new(Mutex::new(BGPNeighbor::new(
            config.ip.parse().unwrap(),
            config.port,
            config.asn,
            self.hold_time,
            BGPState::Idle,
        )));
        self.neighbors.push(n);
    }

    async fn add_incoming(speaker: Arc<Mutex<BGPSpeaker>>, socket: TcpStream, addr: SocketAddr) {
        let n;
        {
            let mut s = speaker.lock().await;
            let mut asn = 0;
            let mut port = addr.port();
            let mut hold_time = s.hold_time;
            for neighbor in s.neighbors.clone() {
                let n = neighbor.lock().await;
                if addr.ip() == n.remote_ip {
                    asn = n.remote_asn;
                    port = n.remote_port;
                    hold_time = n.attributes.hold_time;
                }
            }
            n = Arc::new(Mutex::new(BGPNeighbor::new(
                addr.ip(),
                port,
                asn,
                hold_time,
                BGPState::Active,
            )));
            s.neighbors.push(n.clone());
        }
        tokio::spawn(async move { BGPNeighbor::fsm(n, socket, speaker.clone()).await });
    }

    pub async fn start(speaker: Arc<Mutex<BGPSpeaker>>) {
        let s = speaker.clone();
        tokio::spawn(async move { BGPSpeaker::connection_mgr(s).await });

        tokio::spawn(async move { BGPSpeaker::listen(speaker).await });
    }

    async fn connection_mgr(speaker: Arc<Mutex<BGPSpeaker>>) {
        let neighbors;
        {
            let s = speaker.lock().await;
            neighbors = s.neighbors.clone();
        }
        for neighbor in neighbors {
            let speaker = speaker.clone();
            tokio::spawn(async move { BGPSpeaker::connect(speaker, neighbor).await });
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

    async fn connect(speaker: Arc<Mutex<BGPSpeaker>>, neighbor: Arc<Mutex<BGPNeighbor>>) {
        let socket;
        {
            let mut n = neighbor.lock().await;
            socket = TcpStream::connect(n.remote_ip.to_string() + ":" + &n.remote_port.to_string())
                .await
                .unwrap();
            n.attributes.state = BGPState::Connect;
        }

        tokio::spawn(async move { BGPNeighbor::fsm(neighbor.clone(), socket, speaker).await });
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

#[derive(Debug, Clone)]
pub struct BGPNeighbor {
    remote_ip: IpAddr,
    remote_port: u16,
    remote_asn: u16,
    router_id: u32,
    tx: Option<tokio::sync::mpsc::Sender<Event>>,
    attributes: BGPSessionAttributes,
}

impl BGPNeighbor {
    pub fn new(
        remote_ip: IpAddr,
        remote_port: u16,
        remote_asn: u16,
        hold_time: u16,
        state: BGPState,
    ) -> Self {
        let tx = None;
        let attributes = BGPSessionAttributesBuilder::default()
            .hold_time(hold_time)
            .state(state)
            .build()
            .unwrap();
        BGPNeighbor {
            remote_ip,
            remote_port,
            remote_asn,
            router_id: 0,
            tx,
            attributes,
        }
    }

    async fn fsm(n: Arc<Mutex<BGPNeighbor>>, s: TcpStream, speaker: Arc<Mutex<BGPSpeaker>>) {
        println!("starting fsm for {:?} with {:?}", n, s);

        let (tx, mut rx) = mpsc::channel::<Event>(100);

        let mut server = Framed::new(s, bgp::BGPMessageCodec);

        let state;
        {
            let mut n = n.lock().await;
            state = n.attributes.state;
            n.tx = Some(tx.clone());
        }
        match state {
            BGPState::Active => {
                BGPNeighbor::process_event(
                    Event::TcpConnectionConfirmed,
                    speaker.clone(),
                    n.clone(),
                    &mut server,
                )
                .await;
            }
            BGPState::Connect => {
                // sleep(Duration::from_secs(300)).await;
                BGPNeighbor::process_event(
                    Event::TcpConnectionValid,
                    speaker.clone(),
                    n.clone(),
                    &mut server,
                )
                .await;
            }
            _ => {}
        };

        let na = n.clone();

        tokio::spawn(async {
            BGPNeighbor::timer_hold(na).await;
        });

        loop {
            tokio::select! {
                Some(e) = rx.recv() => {
                    BGPNeighbor::process_event(e,speaker.clone(),n.clone(),&mut server).await;
                }
                Some(m) = BGPNeighbor::read_message(&mut server) => {
                    BGPNeighbor::process_message(m,speaker.clone(),n.clone()).await;
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

    async fn timer_hold(n: Arc<Mutex<BGPNeighbor>>) {
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
        s: Arc<Mutex<BGPSpeaker>>,
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
        s: Arc<Mutex<BGPSpeaker>>,
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
        s: Arc<Mutex<BGPSpeaker>>,
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
        s: Arc<Mutex<BGPSpeaker>>,
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
        s: Arc<Mutex<BGPSpeaker>>,
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
                BGPNeighbor::process_message_established(m, nb).await;
            }
            BGPState::Idle => {
                println!("FSM IDLE: received {:?}", m.body);
                BGPNeighbor::process_message_idle(m, nb).await;
            }
        }
    }

    async fn process_message_opensent(
        m: bgp::Message,
        s: Arc<Mutex<BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
    ) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(_body) => {
                BGPNeighbor::process_message_keepalive(nb).await;
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
        s: Arc<Mutex<BGPSpeaker>>,
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
                BGPNeighbor::process_message_keepalive(nb.clone()).await;
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

    async fn process_message_established(m: bgp::Message, nb: Arc<Mutex<BGPNeighbor>>) {
        match m.body {
            bgp::BGPMessageBody::Keepalive(_body) => {
                BGPNeighbor::process_message_keepalive(nb).await;
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
                let tx;
                {
                    let n = nb.lock().await;
                    tx = n.tx.clone().unwrap();
                }
                tx.send(Event::UpdateMsg).await.unwrap();
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

    async fn process_message_idle(m: bgp::Message, nb: Arc<Mutex<BGPNeighbor>>) {
        match m.body {
            _ => {
                println!("Unimplemented");
            }
        };
    }

    async fn collision_detection(
        message: bgp::BGPOpenMessage,
        speaker: Arc<Mutex<BGPSpeaker>>,
    ) -> bool {
        let s = speaker.lock().await;
        let ns = s.neighbors.clone();
        for n in ns {
            let n = n.lock().await;
            println!("{:?}", n);
            let tx = n.tx.clone().unwrap();
            match n.attributes.state {
                BGPState::OpenConfirm => {
                    if n.router_id == message.router_id {
                        if n.router_id < s.router_id {
                            let _ = tx.send(Event::OpenCollisionDump).await;
                        }
                        return true;
                    }
                }
                BGPState::OpenSent => {
                    if n.router_id == message.router_id {
                        if n.router_id < s.router_id {
                            let _ = tx.send(Event::OpenCollisionDump).await;
                        }
                        return true;
                    }
                }
                _ => {}
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
        println!("Neighbor updated : {:?}", n);
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

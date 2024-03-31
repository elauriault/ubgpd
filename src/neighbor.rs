use async_std::sync::{Arc, Mutex};
use futures::prelude::sink::SinkExt;
use itertools::Itertools;
use num_traits::FromPrimitive;
use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::bgp::{self, AddressFamily, Message, PathAttributeType, PathAttributeValue, NLRI};
use crate::rib::{self, RibUpdate, RouteAttributes};
use crate::speaker::{self};

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
    connect_retry_time: u16,
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
pub enum Event {
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
    RibUpdate(Vec<(bgp::NLRI, Option<rib::RouteAttributes>)>),
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
    capabilities_advertised: Capabilities,
    capabilities_received: Capabilities,
    adjrib: HashMap<bgp::AddressFamily, rib::Rib>,
    pub tx: Option<tokio::sync::mpsc::Sender<Event>>,
    ribtx: HashMap<bgp::AddressFamily, tokio::sync::mpsc::Sender<speaker::RibEvent>>,
    attributes: BGPSessionAttributes,
}

#[derive(Debug, Clone)]
pub struct Capabilities {
    pub multiprotocol: Option<Vec<bgp::AddressFamily>>,
    pub route_refresh: bool,
    pub outbound_route_filtering: bool,
    pub extended_next_hop_encoding: bool,
    pub graceful_restart: bool,
    pub four_octect_asn: Option<u32>,
}

impl From<bgp::BGPCapabilities> for Capabilities {
    fn from(src: bgp::BGPCapabilities) -> Self {
        let mut capabilities = Capabilities::default();
        let mut afs = vec![];
        for c in src.params {
            match c.capability_code {
                bgp::BGPCapabilityCode::Multiprotocol => {
                    if c.capability_length != 4 {
                        panic!("Unexpected length of BGP capability");
                    }
                    let mut afi = [0u8; 2];
                    let mut safi = [0u8; 1];
                    afi.copy_from_slice(&c.capability_value[0..2]);
                    safi.copy_from_slice(&c.capability_value[3..4]);
                    let afi = u16::from_be_bytes(afi);
                    let safi = u8::from_be_bytes(safi);
                    let afi = FromPrimitive::from_u16(afi).unwrap();
                    let safi = FromPrimitive::from_u8(safi).unwrap();
                    let af = bgp::AddressFamily { afi, safi };
                    afs.push(af);
                }
                bgp::BGPCapabilityCode::RouteRefresh => capabilities.route_refresh = true,
                bgp::BGPCapabilityCode::ExtendedNextHopEncoding => {
                    capabilities.extended_next_hop_encoding = true
                }
                bgp::BGPCapabilityCode::OutboundRouteFiltering => {
                    capabilities.outbound_route_filtering = true
                }
                bgp::BGPCapabilityCode::GracefulRestart => capabilities.graceful_restart = true,
                bgp::BGPCapabilityCode::FourOctectASN => {
                    if c.capability_length != 4 {
                        panic!("Unexpected length of BGP capability");
                    }
                    let mut v = [0u8; 4];
                    v.copy_from_slice(&c.capability_value);
                    let asn = u32::from_be_bytes(v);
                    capabilities.four_octect_asn = Some(asn);
                } // _ => {}
            }
        }
        capabilities.multiprotocol = Some(afs.into_iter().unique().collect());

        capabilities
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            multiprotocol: None,
            route_refresh: false,
            outbound_route_filtering: false,
            extended_next_hop_encoding: false,
            graceful_restart: false,
            four_octect_asn: None,
        }
    }
}

impl BGPNeighbor {
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
        // let ribtx = None;
        let attributes = BGPSessionAttributesBuilder::default()
            .connect_retry_time(connect_retry_time)
            .hold_time(hold_time)
            .state(state)
            .allow_automatic_start(true)
            .build()
            .unwrap();
        let mut capabilities_advertised = Capabilities::default();
        capabilities_advertised.multiprotocol = families;
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
            socket = TcpStream::connect(
                n.remote_ip.unwrap().to_string() + ":" + &n.remote_port.unwrap().to_string(),
            )
            .await
            .unwrap();
            n.attributes.state = BGPState::Connect;
            let local_addr = socket.local_addr().unwrap();
            n.local_ip = Some(local_addr.ip());
            n.local_port = Some(local_addr.port());
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
                let capabilities;
                {
                    let s = s.lock().await;
                    asn = s.local_asn;
                    rid = s.router_id;
                    hold = s.hold_time;
                }
                {
                    let n = nb.lock().await;
                    capabilities = n.capabilities_advertised.clone();
                }
                let _ = BGPNeighbor::send_open(server, asn, rid, hold, capabilities)
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
                let capabilities;
                {
                    let s = s.lock().await;
                    asn = s.local_asn;
                    rid = s.router_id;
                    hold = s.hold_time;
                }
                {
                    let n = nb.lock().await;
                    capabilities = n.capabilities_advertised.clone();
                }
                let _ = BGPNeighbor::send_open(server, asn, rid, hold, capabilities)
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
        _nb: Arc<Mutex<BGPNeighbor>>,
        _server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
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
                let capabilities;
                {
                    let s = s.lock().await;
                    asn = s.local_asn;
                    rid = s.router_id;
                    hold = s.hold_time;
                }
                {
                    let n = nb.lock().await;
                    capabilities = n.capabilities_advertised.clone();
                }
                let _ = BGPNeighbor::send_open(server, asn, rid, hold, capabilities)
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
            Event::RibUpdate(nlris) => {
                let _ = BGPNeighbor::send_update(server, nb.clone(), nlris).await;
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

    async fn process_message_connect(m: bgp::Message, _nb: Arc<Mutex<BGPNeighbor>>) {
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

    async fn process_message_idle(m: bgp::Message, _nb: Arc<Mutex<BGPNeighbor>>) {
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

    async fn adjrib_add(&mut self, af: AddressFamily, routes: RibUpdate) {
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

    async fn adjrib_withdraw(&mut self, af: AddressFamily, routes: RibUpdate) {
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

    async fn handle_update(
        m: bgp::BGPUpdateMessage,
        s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<BGPNeighbor>>,
    ) {
        let mut af = AddressFamily {
            afi: bgp::AFI::Ipv4,
            safi: bgp::SAFI::NLRIUnicast,
        };
        let mut nlris = vec![];
        let mut withdrawn = vec![];
        let mut nh = None;
        match m
            .path_attributes
            .clone()
            .into_iter()
            .find(|x| {
                x.type_code == PathAttributeType::MPReachableNLRI
                    || x.type_code == PathAttributeType::MPUnreachableNLRI
            })
            .map(|x| x.value)
        {
            Some(PathAttributeValue::MPReachableNLRI(n)) => {
                nlris = n.nlris;
                nh = Some(n.nh);
                af = n.af;
            }
            Some(PathAttributeValue::MPUnreachableNLRI(n)) => {
                withdrawn = n.nlris;
                af = n.af;
            }
            _ => {
                nlris = m.nlri;
                withdrawn = m.withdrawn_routes;
                match m
                    .path_attributes
                    .clone()
                    .into_iter()
                    .find(|x| x.type_code == PathAttributeType::NextHop)
                    .map(|x| x.value)
                {
                    Some(PathAttributeValue::NextHop(n)) => {
                        nh = Some(IpAddr::V4(n));
                    }
                    _ => {}
                }
            }
        }
        let local_asn;
        {
            let s = s.lock().await;
            local_asn = s.local_asn;
        }
        let attributes =
            RouteAttributes::new(m.path_attributes.clone(), local_asn.into(), nb.clone(), nh).await;

        let mut msg = speaker::Update {
            added: None,
            withdrawn: None,
            rid: 0,
        };

        if withdrawn.len() > 0 {
            let updates = RibUpdate {
                nlris: withdrawn,
                attributes: attributes.clone(),
            };
            msg.added = Some(updates.clone());
            {
                let mut nb = nb.lock().await;
                nb.adjrib_add(af.clone(), updates.clone()).await;
            }
        }
        if nlris.len() > 0 {
            let updates = RibUpdate { nlris, attributes };
            msg.withdrawn = Some(updates.clone());
            {
                let mut nb = nb.lock().await;
                nb.adjrib_withdraw(af.clone(), updates.clone()).await;
            }
        }
        {
            let nb = nb.lock().await;
            msg.rid = nb.remote_rid.unwrap();
            let _ = nb
                .ribtx
                .get(&af)
                .unwrap()
                .send(speaker::RibEvent::UpdateRoutes(msg))
                .await;
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
                        if n.remote_rid == Some(message.router_id) {
                            if n.remote_rid < Some(s.router_id) {
                                let _ = t.send(Event::OpenCollisionDump).await;
                            }
                            return true;
                        }
                    }
                    BGPState::OpenSent => {
                        if n.remote_rid == Some(message.router_id) {
                            if n.remote_rid < Some(s.router_id) {
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
        if n.remote_asn != Some(message.asn) {
            println!(
                "n.remote_asn: {} != message.asn:{}",
                n.remote_asn.unwrap(),
                message.asn
            );
            return false;
            // panic!("ASN received doesn't match config");
        }
        true
    }

    async fn update_from_open(message: bgp::BGPOpenMessage, neighbor: Arc<Mutex<BGPNeighbor>>) {
        let mut n = neighbor.lock().await;
        n.attributes.hold_time = message.hold_time;
        n.remote_rid = Some(message.router_id);
        n.attributes.state = BGPState::OpenConfirm;
        let caps: bgp::BGPCapabilities = message.opt_params.into();
        n.capabilities_received = caps.into();
        println!("Neighbor updated from Open : {:?}", n);
    }

    async fn send_open(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
        asn: u16,
        rid: u32,
        hold: u16,
        capabilities: Capabilities,
    ) -> Result<(), Box<dyn Error>> {
        let body = bgp::BGPOpenMessage::new(asn, rid, hold, capabilities).unwrap();
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

    async fn send_update(
        server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
        neighbor: Arc<Mutex<BGPNeighbor>>,
        nlris: Vec<(NLRI, Option<RouteAttributes>)>,
    ) -> Result<(), Box<dyn Error>> {
        let mut wd: Vec<NLRI> = vec![];
        let mut updates: HashMap<RouteAttributes, Vec<NLRI>> = HashMap::new();
        for (n, a) in nlris {
            match a {
                None => wd.push(n.clone()),
                Some(route_attributes) => match updates.get_mut(&route_attributes) {
                    None => {
                        updates.insert(route_attributes.clone(), vec![n]);
                    }
                    Some(atr) => {
                        atr.push(n);
                    }
                },
            }
        }
        match updates.len() {
            0 => {
                let body = bgp::BGPUpdateMessageBuilder::default()
                    .withdrawn_routes(wd)
                    .path_attributes(vec![])
                    .nlri(vec![])
                    .build()
                    .unwrap();
                let message: Vec<u8> =
                    Message::new(bgp::MessageType::UPDATE, bgp::BGPMessageBody::Update(body))
                        .unwrap()
                        .into();
                match server.send(message).await {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        println!("{:?}", e);
                        Err(Box::new(e))
                    }
                }
            }
            _ => {
                for (mut ra, routes) in updates {
                    let local_asn;
                    let local_ip;
                    let remote_asn;
                    {
                        let neighbor = neighbor.lock().await;
                        local_asn = neighbor.local_asn;
                        local_ip = neighbor.local_ip.unwrap();
                        remote_asn = neighbor.remote_asn.unwrap();
                    }
                    if local_asn != remote_asn {
                        ra.next_hop = local_ip;
                        ra.prepend(local_asn, 1);
                    } else {
                        if ra.from_ibgp() {
                            break;
                        }
                    }
                    let pa: Vec<bgp::PathAttribute> = ra.into();
                    let body = bgp::BGPUpdateMessageBuilder::default()
                        .withdrawn_routes(wd.clone())
                        .path_attributes(pa)
                        .nlri(routes)
                        .build()
                        .unwrap();
                    let message: Vec<u8> =
                        Message::new(bgp::MessageType::UPDATE, bgp::BGPMessageBody::Update(body))
                            .unwrap()
                            .into();
                    match server.send(message).await {
                        Ok(_) => {}
                        Err(e) => {
                            println!("{:?}", e);
                            return Err(Box::new(e));
                        }
                    };
                    wd.clear();
                }
                Ok(())
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

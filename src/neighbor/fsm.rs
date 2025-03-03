// src/neighbor/fsm.rs

use super::connection;
use super::message_handler;
use super::session::BGPNeighbor;
use super::timers;
use super::types::{BGPState, Event};
use crate::bgp;
use crate::speaker;
use async_std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

pub async fn init_peer(n: Arc<Mutex<BGPNeighbor>>) {
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
            n.ribtx.clone_from(&s.ribtx);
        }
    }

    tokio::spawn(async move { fsm_tcp(neighbor.clone(), socket, speaker).await });
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
    }
    match state {
        BGPState::Active => {
            process_event(
                Event::TcpConnectionConfirmed,
                speaker.clone(),
                neighbor.clone(),
                Some(&mut server),
            )
            .await;
        }
        BGPState::Connect => {
            process_event(
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
    let hold_task = tokio::spawn(async { timers::timer_hold(na, receiver).await });

    loop {
        tokio::select! {
            Some(e) = rx.recv() => {
                process_event(e, speaker.clone(), neighbor.clone(), Some(&mut server)).await;
            }
            Some(m) = connection::read_message(&mut server) => {
                match m {
                    Ok(m) => {
                        message_handler::process_message(m, speaker.clone(), neighbor.clone()).await;
                    },
                    Err(_) => {
                        process_event(
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

pub async fn process_event(
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
                process_event_active(e, s, nb, server).await;
            }
            BGPState::Connect => {
                println!("FSM CONNECT: received {:?}", e);
                process_event_connect(e, s, nb, server).await;
            }
            BGPState::OpenConfirm => {
                println!("FSM OPENCONFIRM: received {:?}", e);
                process_event_openconfirm(e, s, nb, server).await;
            }
            BGPState::OpenSent => {
                println!("FSM OPENSENT: received {:?}", e);
                process_event_opensent(e, nb, server).await;
            }
            BGPState::Established => {
                println!("FSM ESTABLISHED: received {:?}", e);
                process_event_established(e, nb, server).await;
            }
            _ => {}
        },
        None => {
            if let BGPState::Idle = state {
                println!("FSM IDLE: received {:?}", e);
                process_event_idle(e, nb).await;
            }
        }
    }
}

pub async fn process_event_idle(
    e: Event,
    nb: Arc<Mutex<BGPNeighbor>>,
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
            init_peer(nb).await;
        }
        _ => {
            println!("{:?}", e);
        }
    }
}

pub async fn process_event_connect(
    e: Event,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) {
    match e {
        Event::KeepaliveTimerExpires => {
            connection::send_keepalive(server).await.unwrap();
        }
        Event::ManualStart => {
            connection::send_keepalive(server).await.unwrap();
        }
        Event::AutomaticStart => {
            init_peer(nb).await;
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
            connection::send_open(server, asn, rid, hold, capabilities)
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

pub async fn process_event_active(
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
            connection::send_open(server, asn, rid, hold, capabilities)
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

pub async fn process_event_opensent(
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

pub async fn process_event_openconfirm(
    e: Event,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) {
    match e {
        Event::KeepaliveTimerExpires => {
            connection::send_keepalive(server).await.unwrap();
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
            connection::send_open(server, asn, rid, hold, capabilities)
                .await
                .unwrap();
        }
        _ => {
            println!("FSM OPENCONFIRM: {:?} looks like an error", e);
        }
    }
}

pub async fn process_event_established(
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
            connection::send_keepalive(server).await.unwrap();
        }
        Event::RibUpdate(nlris) => {
            let _ = connection::send_update(server, nb.clone(), nlris).await;
        }
        _ => {
            println!("FSM ESTABLISHED: {:?} looks like an error", e);
        }
    }
}

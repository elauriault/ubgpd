// src/neighbor/fsm.rs

use super::connection;
use super::message_handler;
use super::session::BGPNeighbor;
use super::timers;
use super::types::{BGPState, Event};
use crate::bgp;
use crate::speaker;
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio_util::codec::Framed;

pub async fn init_peer(n: Arc<Mutex<BGPNeighbor>>) {
    {
        let mut n = n.lock().await;
        n.attributes.connect_retry_counter = 0;
        n.attributes.state = BGPState::Active;
    }
    log::debug!("FSM init_peer: Idle to Active");
}

pub async fn connect(
    speaker: Arc<Mutex<speaker::BGPSpeaker>>,
    neighbor: Arc<Mutex<BGPNeighbor>>,
) -> Result<()> {
    let (remote_addr, local_ip, local_asn, ribtx) = {
        let n = neighbor.lock().await;
        let remote_ip = n
            .remote_ip
            .ok_or_else(|| anyhow!("Remote IP not configured"))?;
        let remote_port = n
            .remote_port
            .ok_or_else(|| anyhow!("Remote port not configured"))?;
        let remote_addr = format!("{}:{}", remote_ip, remote_port);
        (remote_addr, n.local_ip, n.local_asn, n.ribtx.clone())
    };

    // Attempt connection with retry logic
    let socket = match TcpStream::connect(&remote_addr).await {
        Ok(socket) => socket,
        Err(e) => {
            log::error!("Failed to connect to {}: {}", remote_addr, e);
            // Update neighbor state to Idle on connection failure
            {
                let mut n = neighbor.lock().await;
                n.attributes.state = BGPState::Idle;
                n.attributes.connect_retry_counter += 1;
            }
            // Schedule retry if appropriate
            let connect_retry_time = {
                let n = neighbor.lock().await;
                n.attributes.connect_retry_time
            };
            tokio::time::sleep(Duration::from_secs(connect_retry_time as u64)).await;
            return Err(anyhow!("Connection failed: {}", e));
        }
    };

    // Update neighbor with connection details
    {
        let mut n = neighbor.lock().await;
        n.attributes.state = BGPState::Connect;
        let local_addr = socket.local_addr().context("Failed to get local address")?;
        n.local_ip = Some(local_addr.ip());
        n.local_port = Some(local_addr.port());
        n.ribtx = ribtx;
    }

    // Spawn FSM handler
    let speaker_clone = speaker.clone();
    let neighbor_clone = neighbor.clone();
    tokio::spawn(async move {
        if let Err(e) = fsm_tcp(neighbor_clone, socket, speaker_clone).await {
            log::error!("FSM error: {}", e);
        }
    });

    Ok(())
}

pub async fn fsm_tcp(
    neighbor: Arc<Mutex<BGPNeighbor>>,
    stream: TcpStream,
    speaker: Arc<Mutex<speaker::BGPSpeaker>>,
) -> Result<()> {
    log::debug!("starting fsm_tcp for neighbor");

    let (tx, mut rx) = mpsc::channel::<Event>(100);
    let mut server = Framed::new(stream, bgp::BGPMessageCodec);

    let state = {
        let mut n = neighbor.lock().await;
        n.tx = Some(tx.clone());
        n.attributes.state
    };

    // Process initial state
    match state {
        BGPState::Active => {
            process_event(
                Event::TcpConnectionConfirmed,
                speaker.clone(),
                neighbor.clone(),
                Some(&mut server),
            )
            .await?;
        }
        BGPState::Connect => {
            process_event(
                Event::TcpConnectionValid,
                speaker.clone(),
                neighbor.clone(),
                Some(&mut server),
            )
            .await?;
        }
        _ => {}
    };

    let na = neighbor.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let hold_task = tokio::spawn(async {
        if let Err(e) = timers::timer_hold(na, receiver).await {
            log::error!("Hold timer error: {}", e);
        }
    });

    let result = fsm_loop(&mut rx, &mut server, speaker.clone(), neighbor.clone()).await;

    // Cleanup
    let _ = sender.send(());
    let _ = tokio::join!(hold_task);

    // Update neighbor state
    {
        let mut n = neighbor.lock().await;
        n.attributes.state = BGPState::Idle;
        n.tx = None;
    }

    result
}

async fn fsm_loop(
    rx: &mut mpsc::Receiver<Event>,
    server: &mut Framed<TcpStream, bgp::BGPMessageCodec>,
    speaker: Arc<Mutex<speaker::BGPSpeaker>>,
    neighbor: Arc<Mutex<BGPNeighbor>>,
) -> Result<()> {
    loop {
        tokio::select! {
            Some(e) = rx.recv() => {
                if matches!(e, Event::TcpConnectionFails) {
                    log::info!("TCP connection termination requested");
                    return Ok(());
                }
                process_event(e, speaker.clone(), neighbor.clone(), Some(server)).await?;
            }
            Some(m) = connection::read_message(server) => {
                match m {
                    Ok(m) => {
                        message_handler::process_message(m, speaker.clone(), neighbor.clone()).await?;
                    },
                    Err(e) => {
                        log::error!("Failed to read message: {}", e);
                        process_event(
                            Event::TcpConnectionFails,
                            speaker.clone(),
                            neighbor.clone(),
                            Some(server),
                        )
                        .await?;
                        return Err(anyhow!("Connection read error: {}", e));
                    },
                }
            }
            else => {
                log::debug!("FSM loop ended - no more events or messages");
                break;
            }
        }
    }
    Ok(())
}

pub async fn process_event(
    e: Event,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
    server: Option<&mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>>,
) -> Result<()> {
    let state = {
        let nb = nb.lock().await;
        nb.attributes.state
    };

    match server {
        Some(server) => match state {
            BGPState::Active => {
                log::debug!("FSM ACTIVE: received {:?}", e);
                process_event_active(e, s, nb, server).await
            }
            BGPState::Connect => {
                log::debug!("FSM CONNECT: received {:?}", e);
                process_event_connect(e, s, nb, server).await
            }
            BGPState::OpenConfirm => {
                log::debug!("FSM OPENCONFIRM: received {:?}", e);
                process_event_openconfirm(e, s, nb, server).await
            }
            BGPState::OpenSent => {
                log::debug!("FSM OPENSENT: received {:?}", e);
                process_event_opensent(e, nb, server).await
            }
            BGPState::Established => {
                log::debug!("FSM ESTABLISHED: received {:?}", e);
                process_event_established(e, nb, server).await
            }
            _ => Ok(()),
        },
        None => {
            if let BGPState::Idle = state {
                log::debug!("FSM IDLE: received {:?}", e);
                process_event_idle(e, nb).await
            } else {
                Ok(())
            }
        }
    }
}

pub async fn process_event_idle(e: Event, nb: Arc<Mutex<BGPNeighbor>>) -> Result<()> {
    match e {
        Event::ManualStartWithPassiveTcpEstablishment => {
            log::debug!("FSM IDLE: {:?} to be implemented", e);
        }
        Event::AutomaticStartWithPassiveTcpEstablishment => {
            log::debug!("FSM IDLE: {:?} to be implemented", e);
        }
        Event::ManualStart => {
            log::debug!("FSM IDLE: {:?} to be implemented", e);
        }
        Event::AutomaticStart => {
            init_peer(nb).await;
        }
        _ => {
            log::debug!("{:?}", e);
        }
    }
    Ok(())
}

pub async fn process_event_connect(
    e: Event,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) -> Result<()> {
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
            log::debug!("FSM Connect to OpenSent");
        }
        _ => {
            log::debug!("FSM CONNECT: {:?} looks like an error", e);
        }
    }
    Ok(())
}

pub async fn process_event_active(
    e: Event,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) -> Result<()> {
    match e {
        Event::ManualStop => {
            log::debug!("FSM ACTIVE: {:?} to be implemented", e);
        }
        Event::ConnectRetryTimerExpires => {
            log::debug!("FSM ACTIVE: {:?} to be implemented", e);
        }
        Event::DelayOpenTimerExpires => {
            log::debug!("FSM ACTIVE: {:?} to be implemented", e);
        }
        Event::TcpConnectionFails => {
            log::debug!("FSM ACTIVE: {:?} to be implemented", e);
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
            log::debug!("FSM Active to OpenSent");
        }
        Event::NotifMsg => {
            log::debug!("FSM ACTIVE: {:?} to be implemented", e);
        }
        _ => {
            log::debug!("FSM Looks {:?} like an error", e);
        }
    }
    Ok(())
}

pub async fn process_event_opensent(
    e: Event,
    _nb: Arc<Mutex<BGPNeighbor>>,
    _server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) -> Result<()> {
    match e {
        Event::HoldTimerExpires => {
            log::debug!("FSM OPENSENT: {:?} to be implemented", e);
        }
        Event::ManualStop => {
            log::debug!("FSM OPENSENT: {:?} to be implemented", e);
        }
        Event::AutomaticStop => {
            log::debug!("FSM OPENSENT: {:?} to be implemented", e);
        }
        Event::TcpConnectionValid => {
            log::debug!("FSM OPENSENT: {:?} to be implemented", e);
        }
        Event::TcpConnectionConfirmed => {
            log::debug!("FSM OPENSENT: {:?} to be implemented", e);
        }
        Event::TcpConnectionFails => {
            log::debug!("FSM OPENSENT: {:?} to be implemented", e);
        }
        Event::NotifMsg => {
            log::debug!("FSM OPENSENT: {:?} to be implemented", e);
        }
        _ => {
            log::debug!("FSM OPENSENT: {:?} looks like an error", e);
        }
    }
    Ok(())
}

pub async fn process_event_openconfirm(
    e: Event,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) -> Result<()> {
    match e {
        Event::KeepaliveTimerExpires => {
            connection::send_keepalive(server).await.unwrap();
        }
        Event::TcpConnectionFails => {
            log::debug!("FSM OPENCONFIRM: {:?} to be implemented", e);
        }
        Event::NotifMsg => {
            log::debug!("FSM OPENCONFIRM: {:?} to be implemented", e);
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
            log::debug!("FSM OPENCONFIRM: {:?} looks like an error", e);
        }
    }
    Ok(())
}

pub async fn process_event_established(
    e: Event,
    nb: Arc<Mutex<BGPNeighbor>>,
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) -> Result<()> {
    match e {
        Event::HoldTimerExpires => {
            log::debug!("FSM ESTABLISHED: {:?} to be implemented", e);
        }
        Event::AutomaticStop => {
            log::debug!("FSM ESTABLISHED: {:?} to be implemented", e);
        }
        Event::ManualStop => {
            log::debug!("FSM ESTABLISHED: {:?} to be implemented", e);
        }
        Event::TcpConnectionFails => {
            log::debug!("FSM ESTABLISHED: {:?} to be implemented", e);
        }
        Event::TcpConnectionValid => {
            log::debug!("FSM ESTABLISHED: {:?} to be implemented", e);
        }
        Event::NotifMsg => {
            log::info!("FSM ESTABLISHED: {:?} to be implemented", e);
        }
        Event::KeepaliveTimerExpires => {
            connection::send_keepalive(server).await.unwrap();
        }
        Event::RibUpdate(nlris) => {
            let _ = connection::send_update(server, nb.clone(), nlris).await;
        }
        _ => {
            log::debug!("FSM ESTABLISHED: {:?} looks like an error", e);
        }
    }
    Ok(())
}

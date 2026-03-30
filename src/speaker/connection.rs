use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::neighbor;

use super::types::BGPSpeaker;

pub async fn add_incoming(speaker: Arc<Mutex<BGPSpeaker>>, socket: TcpStream, addr: SocketAddr) {
    log::info!("New incoming connection from {}", addr);

    let remote_ip = addr.ip();
    let remote_port = addr.port();
    let matched_neighbor = {
        let s = speaker.lock().await;
        s.neighbors
            .iter()
            .find(|n| {
                if let Ok(neighbor) = n.try_lock() {
                    neighbor.remote_ip == Some(remote_ip)
                } else {
                    false
                }
            })
            .cloned()
    };

    match matched_neighbor {
        Some(existing_neighbor) => {
            let should_accept = {
                let n = existing_neighbor.lock().await;

                log::info!(
                    "Found configured neighbor for {} (ASN: {:?}, State: {:?})",
                    remote_ip,
                    n.remote_asn,
                    n.attributes.state
                );
                match n.attributes.state {
                    neighbor::BGPState::Idle
                    | neighbor::BGPState::Active
                    | neighbor::BGPState::Connect => true,
                    neighbor::BGPState::OpenSent
                    | neighbor::BGPState::OpenConfirm
                    | neighbor::BGPState::Established => {
                        log::warn!(
                            "Rejecting connection from {} - already in state {:?}",
                            remote_ip,
                            n.attributes.state
                        );
                        false
                    }
                }
            };

            if should_accept {
                {
                    let mut n = existing_neighbor.lock().await;
                    let local_addr = socket
                        .local_addr()
                        .expect("BUG: Socket should have a local address after accept");
                    n.local_ip = Some(local_addr.ip());
                    n.local_port = Some(local_addr.port());
                    n.attributes.state = neighbor::BGPState::Active;
                    log::info!(
                        "Using existing neighbor config for passive connection from {}",
                        remote_ip
                    );
                }
                tokio::spawn(async move {
                    if let Err(e) = neighbor::fsm_tcp(existing_neighbor, socket, speaker).await {
                        log::error!("FSM error for {}: {}", remote_ip, e);
                    }
                });
            } else {
                log::info!("Closing duplicate connection from {}", remote_ip);
                drop(socket);
            }
        }
        None => {
            log::warn!(
                "Rejecting connection from unconfigured peer {}:{}",
                remote_ip,
                remote_port
            );
            drop(socket);
        }
    }
}
pub async fn listen(speaker: Arc<Mutex<BGPSpeaker>>) -> Result<()> {
    let local_ips;
    let local_port;
    {
        let s = speaker.lock().await;
        local_ips = s.local_ips.clone();
        local_port = s.local_port;
    }

    let socket_addr = format!("{}:{}", local_ips[0], local_port);
    let listener = TcpListener::bind(&socket_addr)
        .await
        .context(format!("Failed to bind BGP listener to {}", socket_addr))?;

    loop {
        let (socket, addr) = listener
            .accept()
            .await
            .context("Failed to accept BGP connection".to_string())?;
        add_incoming(speaker.clone(), socket, addr).await;
    }
}
pub async fn connect_mgr(speaker: Arc<Mutex<BGPSpeaker>>) {
    let neighbors;
    {
        let s = speaker.lock().await;
        neighbors = s.neighbors.clone();
    }
    for neighbor in neighbors {
        let speaker = speaker.clone();
        tokio::spawn(async move { neighbor::connect(speaker, neighbor).await });
    }
}

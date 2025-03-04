// File: src/speaker/connection.rs
//
// This file handles connection-related functionality.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::neighbor;

use super::types::BGPSpeaker;

/// Add an incoming BGP connection.
pub async fn add_incoming(speaker: Arc<Mutex<BGPSpeaker>>, socket: TcpStream, addr: SocketAddr) {
    use crate::neighbor;

    println!("A new connection!");
    let n;
    {
        let mut s = speaker.lock().await;
        let remote_asn = None;
        let local_addr = socket.local_addr().unwrap();
        let local_ip = local_addr.ip();
        let local_port = local_addr.port();
        let local_asn = s.local_asn;
        let local_rid = s.router_id;
        let remote_ip = addr.ip();
        let remote_port = addr.port();
        let hold_time = s.hold_time;
        let connect_retry_time = 120; // This is a default value

        {
            let speaker = speaker.lock().await;
            n = Arc::new(Mutex::new(neighbor::BGPNeighbor::new(
                Some(local_ip),
                Some(local_port),
                local_asn,
                local_rid,
                Some(remote_ip),
                Some(remote_port),
                remote_asn,
                hold_time,
                connect_retry_time,
                neighbor::BGPState::Active,
                Some(speaker.families.clone()),
                speaker.ribtx.clone(),
            )));
        }
        s.neighbors.push(n.clone());
    }
    tokio::spawn(async move { neighbor::fsm_tcp(n, socket, speaker.clone()).await });
}

/// Listen for incoming BGP connections.
pub async fn listen(speaker: Arc<Mutex<BGPSpeaker>>) {
    let local_ip;
    let local_port;
    {
        let s = speaker.lock().await;
        local_ip = s.local_ip;
        local_port = s.local_port;
    }

    let socket_addr = format!("{}:{}", local_ip, local_port);
    let listener = TcpListener::bind(&socket_addr).await.unwrap();

    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        add_incoming(speaker.clone(), socket, addr).await;
    }
}

/// Manage connections to BGP neighbors.
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

/// Connect to a BGP neighbor.
pub async fn connect(speaker: Arc<Mutex<BGPSpeaker>>, neighbor: Arc<Mutex<neighbor::BGPNeighbor>>) {
    // Delegating to the neighbor module's connect function
    neighbor::connect(speaker, neighbor).await;
}

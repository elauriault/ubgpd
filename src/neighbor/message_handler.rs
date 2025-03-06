// src/neighbor/message_handler.rs

use super::session::BGPNeighbor;
use super::timers;
use super::types::{BGPState, Event};
use crate::bgp::{self, AddressFamily, Nlri};
use crate::rib::{RibUpdate, RouteAttributes};
use crate::speaker::{self};
use log::log;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::rib;

pub async fn process_message(
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
            log::debug!("FSM ACTIVE: received {:?}", m.body);
            process_message_active(m, s, nb).await;
        }
        BGPState::Connect => {
            log::debug!("FSM CONNECT: received {:?}", m.body);
            process_message_connect(m, nb).await;
        }
        BGPState::OpenConfirm => {
            log::debug!("FSM OPENCONFIRM: received {:?}", m.body);
            process_message_openconfirm(m, s, nb).await;
        }
        BGPState::OpenSent => {
            log::debug!("FSM OPENSENT: received {:?}", m.body);
            process_message_opensent(m, s, nb).await;
        }
        BGPState::Established => {
            log::debug!("FSM ESTABLISHED: received {:?}", m.body);
            process_message_established(m, s, nb).await;
        }
        BGPState::Idle => {
            log::debug!("FSM IDLE: received {:?}", m.body);
            process_message_idle(m, nb).await;
        }
    }
}

pub async fn process_message_opensent(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) {
    match m.body {
        bgp::BGPMessageBody::Keepalive(_body) => {
            handle_keepalive(nb).await;
        }
        bgp::BGPMessageBody::Open(body) => {
            log::debug!("FSM OPENSENT: Open {}", body);
            let tx;
            {
                let n = nb.lock().await;
                tx = n.tx.clone().unwrap();
            }
            match collision_detection(body.clone(), s).await {
                true => {
                    tx.send(Event::OpenCollisionDump).await.unwrap();
                }
                false => match validate_open(body.clone(), nb.clone()).await {
                    false => {
                        tx.send(Event::BGPOpenMsgErr).await.unwrap();
                    }
                    true => {
                        update_from_open(body.clone(), nb.clone()).await;
                        let ta;
                        {
                            let n = nb.lock().await;
                            ta = n.tx.clone().unwrap();
                        }
                        tokio::spawn(async {
                            timers::timer_keepalive(nb, ta).await;
                        });
                        log::debug!("FSM OPENSENT: OpenSent to OpenConfirm");
                        // tx.send(Event::BGPOpen).await.unwrap();
                    }
                },
            }
        }
        bgp::BGPMessageBody::Notification(_body) => {
            log::debug!("FSM OPENSENT: Notification unimplemented");
        }
        _ => {
            log::debug!("FSM OPENSENT: Unimplemented");
            let tx;
            {
                let n = nb.lock().await;
                tx = n.tx.clone().unwrap();
            }
            tx.send(Event::NotifMsg).await.unwrap();
        }
    };
}

pub async fn process_message_active(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) {
    match m.body {
        bgp::BGPMessageBody::Open(body) => {
            log::debug!("FSM ACTIVE: Open {}", body);
            let tx;
            {
                let n = nb.lock().await;
                tx = n.tx.clone().unwrap();
            }
            match collision_detection(body.clone(), s).await {
                true => {
                    tx.send(Event::OpenCollisionDump).await.unwrap();
                }
                false => match validate_open(body.clone(), nb.clone()).await {
                    false => {
                        tx.send(Event::BGPOpenMsgErr).await.unwrap();
                    }
                    true => {
                        update_from_open(body.clone(), nb.clone()).await;
                        let ta;
                        {
                            let n = nb.lock().await;
                            ta = n.tx.clone().unwrap();
                        }
                        tokio::spawn(async {
                            timers::timer_keepalive(nb, ta).await;
                        });
                        log::debug!("FSM ACTIVE: Active to OpenConfirm");
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
            log::debug!("Unimplemented");
        }
    };
}

pub async fn process_message_connect(_m: bgp::Message, _nb: Arc<Mutex<BGPNeighbor>>) {
    {
        log::debug!("FSM Shouldn't receive messages in Connect state");
    };
}

pub async fn process_message_openconfirm(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) {
    match m.body {
        bgp::BGPMessageBody::Keepalive(_body) => {
            handle_keepalive(nb.clone()).await;
            {
                let mut n = nb.lock().await;
                n.attributes.state = BGPState::Established;
                log::info!("Established BGP neigborship with {}", n.remote_ip.unwrap());
                send_locrib(s.clone(), n.clone()).await;
            }
            log::debug!("FSM OpenConfirm to Established");
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
            log::debug!("Unimplemented");
        }
    };
}

pub async fn process_message_established(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) {
    match m.body {
        bgp::BGPMessageBody::Keepalive(_body) => {
            handle_keepalive(nb).await;
        }
        bgp::BGPMessageBody::Notification(body) => {
            handle_notification(body, s, nb).await;
        }
        bgp::BGPMessageBody::Update(body) => {
            handle_update(body, s, nb).await;
        }
        _ => {
            log::debug!("Unimplemented");
        }
    };
}

pub async fn process_message_idle(_m: bgp::Message, _nb: Arc<Mutex<BGPNeighbor>>) {
    {
        log::debug!("Unimplemented");
    };
}

pub async fn handle_notification(
    m: bgp::BGPNotificationMessage,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) {
    log::warn!(
        "Received NOTIFICATION message: Error Code: {:?}, Subcode: {}",
        m.error_code,
        m.error_subcode
    );

    // Get the neighbor's remote router ID and configured address families before lock contention
    let remote_rid;
    let remote_ip;
    let supported_families;
    {
        let n = nb.lock().await;
        remote_rid = n.remote_rid.unwrap_or(0);
        remote_ip = n
            .remote_ip
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)));
        supported_families = n
            .capabilities_advertised
            .multiprotocol
            .clone()
            .unwrap_or_else(|| {
                vec![bgp::AddressFamily {
                    afi: bgp::Afi::Ipv4,
                    safi: bgp::Safi::NLRIUnicast,
                }]
            });
    }

    log::warn!(
        "Closing BGP session with {} (RID: {}) due to NOTIFICATION: {:?}",
        remote_ip,
        remote_rid,
        m.error_code
    );

    // Withdraw routes learned from this neighbor from the RIB
    withdraw_neighbor_routes(s.clone(), remote_rid, remote_ip, supported_families).await;

    {
        let mut n = nb.lock().await;
        n.attributes.state = BGPState::Idle;
        n.adjrib.clear();
        log::info!("Transitioned neighbor {} to IDLE state", remote_ip);
    }

    // Signal connection termination via Event channel
    if let Some(tx) = nb.lock().await.tx.clone() {
        if let Err(e) = tx.send(Event::TcpConnectionFails).await {
            log::error!("Failed to send TcpConnectionFails event: {}", e);
        }
    }
}

// Helper function to withdraw all routes learned from this neighbor
async fn withdraw_neighbor_routes(
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    remote_rid: u32,
    remote_ip: IpAddr,
    families: Vec<bgp::AddressFamily>,
) {
    let speaker = s.lock().await;

    for af in families {
        if let Some(rib_tx) = speaker.ribtx.get(&af) {
            // Create empty withdrawals for all routes from this neighbor
            let mut attr = rib::RouteAttributes::default();
            attr.peer_rid = remote_rid;
            let mut msg = speaker::Update {
                added: None,
                withdrawn: Some(rib::RibUpdate {
                    nlris: vec![], // Will be populated by RIB manager
                    attributes: attr,
                }),
                rid: remote_rid,
            };

            log::info!(
                "Withdrawing all routes for AF {:?}/{:?} from peer {}",
                af.afi,
                af.safi,
                remote_ip
            );

            // Send withdraw message to RIB manager
            if let Err(e) = rib_tx.send(speaker::RibEvent::UpdateRoutes(msg)).await {
                log::error!("Failed to send withdrawal message: {}", e);
            }
        }
    }
}

pub async fn handle_keepalive(nb: Arc<Mutex<BGPNeighbor>>) {
    let mut n = nb.lock().await;
    n.attributes.keepalive_timer = 0;
}

pub async fn collision_detection(
    message: bgp::BGPOpenMessage,
    speaker: Arc<Mutex<speaker::BGPSpeaker>>,
) -> bool {
    let s = speaker.lock().await;
    let ns = s.neighbors.clone();
    for n in ns {
        let n = n.lock().await;
        log::debug!("Checking collision for {:?}", n);
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

pub async fn validate_open(
    message: bgp::BGPOpenMessage,
    neighbor: Arc<Mutex<BGPNeighbor>>,
) -> bool {
    let n = neighbor.lock().await;
    if n.remote_asn != Some(message.asn) {
        log::debug!(
            "n.remote_asn: {} != message.asn:{}",
            n.remote_asn.unwrap(),
            message.asn
        );
        return false;
    }
    true
}

pub async fn update_from_open(message: bgp::BGPOpenMessage, neighbor: Arc<Mutex<BGPNeighbor>>) {
    let mut n = neighbor.lock().await;
    n.attributes.hold_time = message.hold_time;
    n.remote_rid = Some(message.router_id);
    n.attributes.state = BGPState::OpenConfirm;
    let caps: bgp::BGPCapabilities = message.opt_params.into();
    n.capabilities_received = caps.into();
    log::debug!("Neighbor updated from Open : {:?}", n);
}

pub async fn send_locrib(s: Arc<Mutex<speaker::BGPSpeaker>>, nb: BGPNeighbor) {
    let adv = nb.capabilities_advertised.multiprotocol.unwrap().clone();
    let rec = nb.capabilities_received.multiprotocol.unwrap().clone();

    for af in adv {
        if rec.contains(&af) {
            let s = s.lock().await;
            let r = s.rib.get(&af).unwrap().lock().await;
            let routes: Vec<(Nlri, Option<RouteAttributes>)> = r
                .iter()
                .map(|(n, a)| (*n, Some(a.first().unwrap().clone())))
                .collect();
            let tx = nb.tx.clone();
            tx.unwrap().send(Event::RibUpdate(routes)).await.unwrap();
        }
    }
}

pub async fn handle_update(
    m: bgp::BGPUpdateMessage,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) {
    let mut af = AddressFamily {
        afi: bgp::Afi::Ipv4,
        safi: bgp::Safi::NLRIUnicast,
    };
    let mut nlris = vec![];
    let mut withdrawn = vec![];
    let mut nh = None;
    log::debug!("handle_update {:?}", m);
    match m
        .path_attributes
        .clone()
        .into_iter()
        .find(|x| {
            x.type_code == bgp::PathAttributeType::MPReachableNLRI
                || x.type_code == bgp::PathAttributeType::MPUnreachableNLRI
        })
        .map(|x| x.value)
    {
        Some(bgp::PathAttributeValue::MPReachableNLRI(n)) => {
            nlris = n.nlris;
            nh = Some(n.nh);
            af = n.af;
        }
        Some(bgp::PathAttributeValue::MPUnreachableNLRI(n)) => {
            withdrawn = n.nlris;
            af = n.af;
        }
        _ => {
            nlris = m.nlri;
            withdrawn = m.withdrawn_routes;
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

    if !withdrawn.is_empty() {
        let updates = RibUpdate {
            nlris: withdrawn,
            attributes: attributes.clone(),
        };
        msg.withdrawn = Some(updates.clone());
        {
            let mut nb = nb.lock().await;
            nb.adjrib_withdraw(af.clone(), updates.clone()).await;
        }
    }
    if !nlris.is_empty() {
        let updates = RibUpdate { nlris, attributes };
        msg.added = Some(updates.clone());
        {
            let mut nb = nb.lock().await;
            nb.adjrib_add(af.clone(), updates.clone()).await;
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

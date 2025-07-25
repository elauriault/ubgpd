// src/neighbor/message_handler.rs

use super::session::BGPNeighbor;
use super::timers;
use super::types::{BGPState, Event};
use crate::bgp::{self, AddressFamily, Nlri};
use crate::rib::{RibUpdate, RouteAttributes};
use crate::speaker::{self};
use anyhow::{anyhow, Context, Result};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::rib;

pub async fn process_message(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) -> Result<()> {
    let state = {
        let nb = nb.lock().await;
        nb.attributes.state
    };

    match state {
        BGPState::Active => {
            log::debug!("FSM ACTIVE: received {:?}", m.body);
            process_message_active(m, s, nb).await
        }
        BGPState::Connect => {
            log::debug!("FSM CONNECT: received {:?}", m.body);
            process_message_connect(m, nb).await
        }
        BGPState::OpenConfirm => {
            log::debug!("FSM OPENCONFIRM: received {:?}", m.body);
            process_message_openconfirm(m, s, nb).await
        }
        BGPState::OpenSent => {
            log::debug!("FSM OPENSENT: received {:?}", m.body);
            process_message_opensent(m, s, nb).await
        }
        BGPState::Established => {
            log::debug!("FSM ESTABLISHED: received {:?}", m.body);
            process_message_established(m, s, nb).await
        }
        BGPState::Idle => {
            log::debug!("FSM IDLE: received {:?}", m.body);
            process_message_idle(m, nb).await
        }
    }
}

pub async fn process_message_opensent(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) -> Result<()> {
    match m.body {
        bgp::BGPMessageBody::Keepalive(_body) => {
            handle_keepalive(nb).await;
            Ok(())
        }
        bgp::BGPMessageBody::Open(body) => {
            log::debug!("FSM OPENSENT: Open {}", body);
            let tx = {
                let n = nb.lock().await;
                n.tx.clone()
                    .ok_or_else(|| anyhow!("No tx channel available"))?
            };

            match collision_detection(body.clone(), s).await {
                true => {
                    tx.send(Event::OpenCollisionDump)
                        .await
                        .context("Failed to send OpenCollisionDump event")?;
                }
                false => match validate_open(body.clone(), nb.clone()).await {
                    false => {
                        tx.send(Event::BGPOpenMsgErr)
                            .await
                            .context("Failed to send BGPOpenMsgErr event")?;
                    }
                    true => {
                        update_from_open(body.clone(), nb.clone()).await;
                        let ta = {
                            let n = nb.lock().await;
                            n.tx.clone()
                                .ok_or_else(|| anyhow!("No tx channel available"))?
                        };

                        let nb_clone = nb.clone();
                        tokio::spawn(async move {
                            if let Err(e) = timers::timer_keepalive(nb_clone, ta).await {
                                log::error!("Keepalive timer error: {}", e);
                            }
                        });
                        log::debug!("FSM OPENSENT: OpenSent to OpenConfirm");
                    }
                },
            }
            Ok(())
        }
        bgp::BGPMessageBody::Notification(_body) => {
            log::debug!("FSM OPENSENT: Notification unimplemented");
            Ok(())
        }
        _ => {
            log::debug!("FSM OPENSENT: Unimplemented");
            let tx = {
                let n = nb.lock().await;
                n.tx.clone()
                    .ok_or_else(|| anyhow!("No tx channel available"))?
            };
            tx.send(Event::NotifMsg)
                .await
                .context("failed to send notifmsg event")?;
            Ok(())
        }
    }
}

pub async fn process_message_active(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) -> Result<()> {
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
                    tx.send(Event::OpenCollisionDump)
                        .await
                        .context("failed to send OpenCollisionDump event")?;
                    Ok(())
                }
                false => match validate_open(body.clone(), nb.clone()).await {
                    false => {
                        tx.send(Event::BGPOpenMsgErr)
                            .await
                            .context("failed to send BGPOpenMsgErr event")?;
                        Ok(())
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
                        tx.send(Event::BGPOpen)
                            .await
                            .context("failed to send BGPOpen event")?;
                        Ok(())
                    }
                },
            }
        }
        bgp::BGPMessageBody::Notification(_body) => {
            let tx;
            {
                let n = nb.lock().await;
                tx =
                    n.tx.clone()
                        .ok_or_else(|| anyhow!("No tx channel available"))?
            }
            tx.send(Event::NotifMsg)
                .await
                .context("failed to send notifmsg event")?;
            Ok(())
        }
        _ => {
            log::debug!("Unimplemented");
            Ok(())
        }
    }
}

pub async fn process_message_connect(_m: bgp::Message, _nb: Arc<Mutex<BGPNeighbor>>) -> Result<()> {
    log::debug!("FSM Shouldn't receive messages in Connect state");
    Ok(())
}

pub async fn process_message_openconfirm(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) -> Result<()> {
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
            Ok(())
        }
        bgp::BGPMessageBody::Notification(_body) => {
            let tx;
            {
                let n = nb.lock().await;
                tx = n.tx.clone().unwrap();
            }
            tx.send(Event::NotifMsg).await.unwrap();
            Ok(())
        }
        _ => {
            log::debug!("Unimplemented");
            Ok(())
        }
    }
}

pub async fn process_message_established(
    m: bgp::Message,
    s: Arc<Mutex<speaker::BGPSpeaker>>,
    nb: Arc<Mutex<BGPNeighbor>>,
) -> Result<()> {
    match m.body {
        bgp::BGPMessageBody::Keepalive(_body) => {
            handle_keepalive(nb).await;
            Ok(())
        }
        bgp::BGPMessageBody::Notification(body) => {
            handle_notification(body, s, nb).await;
            Ok(())
        }
        bgp::BGPMessageBody::Update(body) => {
            handle_update(body, s, nb).await;
            Ok(())
        }
        _ => {
            log::debug!("Unimplemented");
            Ok(())
        }
    }
}

pub async fn process_message_idle(_m: bgp::Message, _nb: Arc<Mutex<BGPNeighbor>>) -> Result<()> {
    log::debug!("FSM Shouldn't receive messages in Idle state");
    Ok(())
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
            let msg = speaker::Update {
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
    let ns;
    let rid;
    {
        let s = speaker.lock().await;
        ns = s.neighbors.clone();
        rid = s.router_id;
    }
    log::debug!("Checking collision for bgp::BGPOpenMessage");
    for n in ns {
        let n = n.lock().await;
        let tx = n.tx.clone();
        match tx {
            None => {}
            Some(t) => match n.attributes.state {
                BGPState::OpenConfirm => {
                    if n.remote_rid == Some(message.router_id) {
                        if n.remote_rid < Some(rid) {
                            let _ = t.send(Event::OpenCollisionDump).await;
                        }
                        log::debug!("Collision detected!");
                        return true;
                    }
                }
                BGPState::OpenSent => {
                    if n.remote_rid == Some(message.router_id) {
                        if n.remote_rid < Some(rid) {
                            let _ = t.send(Event::OpenCollisionDump).await;
                        }
                        log::debug!("Collision detected!");
                        return true;
                    }
                }
                _ => {}
            },
        }
    }
    log::debug!("No collision detected bgp::BGPOpenMessage!");
    false
}

pub async fn validate_open(
    message: bgp::BGPOpenMessage,
    neighbor: Arc<Mutex<BGPNeighbor>>,
) -> bool {
    log::debug!("bgp::BGPOpenMessage validation in progress");
    let n = neighbor.lock().await;

    // For passive connections, we might not know the remote ASN yet
    match n.remote_asn {
        Some(configured_asn) => {
            if configured_asn != message.asn {
                log::debug!(
                    "n.remote_asn: {} != message.asn:{}",
                    configured_asn,
                    message.asn
                );
                return false;
            }
        }
        None => {
            // This is likely a passive connection where we don't know the peer ASN yet
            log::debug!(
                "No remote ASN configured - accepting ASN {} from peer",
                message.asn
            );
            // You might want to check against a list of allowed ASNs here
        }
    }

    log::debug!("bgp::BGPOpenMessage has been validated");
    true
}

pub async fn update_from_open(message: bgp::BGPOpenMessage, neighbor: Arc<Mutex<BGPNeighbor>>) {
    let mut n = neighbor.lock().await;
    n.attributes.hold_time = message.hold_time;
    n.remote_rid = Some(message.router_id);
    n.remote_asn = Some(message.asn);
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
    log::info!("handle_update {:?}", m);
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
        if let Some(tx) = nb.ribtx.get(&af) {
            let _ = tx.send(speaker::RibEvent::UpdateRoutes(msg)).await;
        } else {
            log::warn!(
                "No RIB TX channel found for AFI/SAFI {:?} from peer {:?}",
                af,
                nb.remote_ip
            );
            return;
        }
    }
}

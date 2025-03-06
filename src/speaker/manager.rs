// File: src/speaker/manager.rs
//
// This file contains RIB and FIB management functionality.

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::bgp::{self};
use crate::fib::{self};
use crate::neighbor;
use crate::rib::{self};

use super::events::{FibEvent, RibEvent};

/// Find the best reachable route among a set of route attributes.
pub async fn best_reachable(
    fib: Arc<Mutex<fib::Fib>>,
    attributes: Vec<rib::RouteAttributes>,
) -> Option<rib::RouteAttributes> {
    let fib = fib.lock().await;
    attributes
        .iter()
        .find(|a| fib.has_route(a.next_hop))
        .cloned()
}

/// Handle route additions to the local RIB.
async fn loc_rib_added(
    rib: Arc<Mutex<rib::Rib>>,
    fib: Arc<Mutex<fib::Fib>>,
    asn: u16,
    routes: rib::RibUpdate,
) -> Vec<(bgp::Nlri, Option<rib::RouteAttributes>)> {
    let mut modified = vec![];
    let mut rib = rib.lock().await;

    for nlri in routes.nlris {
        match rib.get_mut(&nlri) {
            None => {
                if routes.attributes.is_valid(asn).await {
                    rib.insert(nlri, vec![routes.attributes.clone()]);
                    {
                        let fib = fib.lock().await;
                        if fib.has_route(routes.attributes.clone().next_hop) {
                            modified.push((nlri, Some(routes.attributes.clone())));
                        }
                    }
                }
            }
            Some(all_attributes) => {
                if routes.attributes.is_valid(asn).await {
                    let previous_best = best_reachable(fib.clone(), all_attributes.to_vec()).await;

                    all_attributes.push(routes.attributes.clone());
                    all_attributes.sort();
                    all_attributes.reverse();

                    {
                        let fib = fib.lock().await;
                        if fib.has_route(routes.attributes.clone().next_hop) {
                            match previous_best {
                                None => {
                                    modified.push((nlri, Some(routes.attributes.clone())));
                                }
                                Some(best) => {
                                    if routes.attributes > best {
                                        modified.push((nlri, Some(routes.attributes.clone())));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    modified
}

/// Handle route withdrawals from the local RIB.
async fn loc_rib_withdraw(
    rib: Arc<Mutex<rib::Rib>>,
    fib: Arc<Mutex<fib::Fib>>,
    routes: rib::RibUpdate,
) -> Vec<(bgp::Nlri, Option<rib::RouteAttributes>)> {
    let mut modified = vec![];
    let mut rib = rib.lock().await;

    // Special case for neighbor withdrawal by router ID
    let mut to_withdraw: Vec<bgp::Nlri> = vec![];
    if routes.nlris.is_empty() {
        // This is a request to withdraw all routes from a specific peer
        let peer_rid = routes.attributes.peer_rid;
        log::info!("Withdrawing all routes from peer RID {}", peer_rid);

        // Find all routes from this peer
        to_withdraw = rib
            .iter()
            .filter_map(|(prefix, attrs)| {
                if attrs.iter().any(|attr| attr.peer_rid == peer_rid) {
                    Some(*prefix)
                } else {
                    None
                }
            })
            .collect();
        log::debug!(
            "Found {} prefixes affected by peer {} disconnection",
            to_withdraw.len(),
            peer_rid
        );
    } else {
        to_withdraw = routes.nlris;
    }

    for nlri in to_withdraw {
        match rib.get_mut(&nlri) {
            None => {}
            Some(all_attributes) => {
                let previous_best = best_reachable(fib.clone(), all_attributes.to_vec()).await;

                all_attributes.retain(|a| !a.is_from_neighbor(routes.attributes.peer_rid));
                if all_attributes.is_empty() {
                    rib.remove(&nlri);
                }

                match previous_best {
                    None => {}
                    Some(best) => {
                        if best.peer_rid == routes.attributes.peer_rid {
                            modified.push((nlri, None));
                        }
                    }
                }
            }
        }
    }
    modified
}

/// Manage the Routing Information Base (RIB).
pub async fn rib_mgr(
    rib: Arc<Mutex<rib::Rib>>,
    fib: Arc<Mutex<fib::Fib>>,
    neighbors: Vec<Arc<Mutex<neighbor::BGPNeighbor>>>,
    asn: u16,
    mut rx: tokio::sync::mpsc::Receiver<RibEvent>,
    tx: tokio::sync::mpsc::Sender<FibEvent>,
) {
    loop {
        let e = rx.recv().await.unwrap();
        log::debug!("Rib Manager got {:?}", e);

        match e {
            RibEvent::UpdateRoutes(msg) => {
                let mut modified = vec![];

                if let Some(routes) = msg.added {
                    log::debug!("Adding routes {:?} from {:?}", routes, msg.rid);
                    let mut added =
                        loc_rib_added(rib.clone(), fib.clone(), asn, routes.clone()).await;
                    modified.append(&mut added);
                }

                if let Some(routes) = msg.withdrawn {
                    let mut withdraw =
                        loc_rib_withdraw(rib.clone(), fib.clone(), routes.clone()).await;
                    modified.append(&mut withdraw);
                }

                log::debug!(
                    "The following have modified best route and need to be propagated {:?}",
                    modified
                );

                if !modified.is_empty() {
                    let _ = tx.send(FibEvent::RibUpdated).await;
                    for n in &neighbors {
                        let n = n.lock().await;
                        if n.is_established().await {
                            let tx = n.tx.clone();
                            tx.unwrap()
                                .send(neighbor::Event::RibUpdate(modified.clone()))
                                .await
                                .unwrap();
                        }
                    }
                }
            }
        }
        sleep(Duration::from_secs(1)).await;
    }
}

/// Manage the Forwarding Information Base (FIB).
pub async fn fib_mgr(
    fib: Arc<Mutex<fib::Fib>>,
    rib: Arc<Mutex<rib::Rib>>,
    mut rx: tokio::sync::mpsc::Receiver<FibEvent>,
) {
    log::debug!("starting fib manager");

    loop {
        let e = rx.recv().await.unwrap();
        match e {
            FibEvent::RibUpdated => {
                log::debug!("Fib Manager : Got {:?}", e);
                let mut fib = fib.lock().await;
                fib.refresh().await;
                fib.sync(rib.clone()).await;
            }
        }
        sleep(Duration::from_secs(1)).await;
        let mut fib = fib.lock().await;
        fib.refresh().await;
    }
}

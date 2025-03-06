use super::capabilities::Capabilities;
use super::session::BGPNeighbor;
use crate::bgp::{self, Message, Nlri};
use crate::rib::RouteAttributes;
use anyhow::{anyhow, Context, Result};
use futures::SinkExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

pub async fn send_open(
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    asn: u16,
    rid: u32,
    hold: u16,
    capabilities: Capabilities,
) -> Result<()> {
    let body = bgp::BGPOpenMessage::new(asn, rid, hold, capabilities)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let message: Vec<u8> =
        bgp::Message::new(bgp::MessageType::Open, bgp::BGPMessageBody::Open(body))
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .into();

    server
        .send(message)
        .await
        .context("Failed to send OPEN message")?;

    Ok(())
}

pub async fn send_update(
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
    neighbor: Arc<Mutex<BGPNeighbor>>,
    nlris: Vec<(Nlri, Option<RouteAttributes>)>,
) -> Result<()> {
    let mut wd: Vec<Nlri> = vec![];
    let mut updates: HashMap<RouteAttributes, Vec<Nlri>> = HashMap::new();
    for (n, a) in nlris {
        match a {
            None => wd.push(n),
            Some(route_attributes) => match updates.get_mut(&route_attributes) {
                None => {
                    let router_id;
                    {
                        let neighbor = neighbor.lock().await;
                        router_id = neighbor.remote_rid.unwrap();
                    }
                    if !route_attributes.is_from_neighbor(router_id) {
                        updates.insert(route_attributes.clone(), vec![n]);
                    }
                }
                Some(atr) => {
                    atr.push(n);
                }
            },
        }
    }
    if !updates.is_empty() || !wd.is_empty() {
        let mut nlris = vec![];
        let mut attributes = vec![];
        for (mut ra, mut routes) in updates {
            {
                let neighbor = neighbor.lock().await;
                let local_asn = neighbor.local_asn;
                let local_ip = neighbor.local_ip.unwrap();
                let remote_asn = neighbor.remote_asn.unwrap();
                if local_asn != remote_asn {
                    ra.next_hop = local_ip;
                    ra.prepend(local_asn, 1);
                } else if ra.is_from_ibgp() {
                    break;
                }
            }
            let mut pa = Into::<Vec<bgp::PathAttribute>>::into(ra)
                .into_iter()
                .filter(|x| x.is_transitive())
                .collect::<Vec<bgp::PathAttribute>>();
            attributes.append(&mut pa);
            nlris.append(&mut routes);
        }
        let body = bgp::BGPUpdateMessageBuilder::default()
            .withdrawn_routes(wd.clone())
            .path_attributes(attributes)
            .nlri(nlris)
            .build()
            .unwrap();
        log::info!("Sending UPDATE {:?}", body);
        let message: Vec<u8> =
            Message::new(bgp::MessageType::Update, bgp::BGPMessageBody::Update(body))
                .unwrap()
                .into();
        match server.send(message).await {
            Ok(_) => {}
            Err(e) => {
                return Err(anyhow!("Failed to send UPDATE message: {}", e));
            }
        };
        wd.clear();
    }
    Ok(())
}

pub async fn send_keepalive(
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) -> Result<()> {
    let body = bgp::BGPKeepaliveMessage::new().unwrap();
    let message: Vec<u8> = bgp::Message::new(
        bgp::MessageType::Keepalive,
        bgp::BGPMessageBody::Keepalive(body.clone()),
    )
    .unwrap()
    .into();
    log::debug!("FSM KeepaliveTimerExpires: Sending {:?}", body);
    server
        .send(message)
        .await
        .context("Failed to send KEEPALIVE message")
}

pub async fn read_message(
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

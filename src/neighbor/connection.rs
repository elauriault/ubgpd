use super::capabilities::Capabilities;
use super::session::BGPNeighbor;
use crate::bgp::{self, Message, Nlri};
use crate::rib::RouteAttributes;
use anyhow::{anyhow, Context, Result};
use futures::SinkExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
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
        .map_err(|e| anyhow!("Failed to create OPEN message: {}", e))?;

    let message: Vec<u8> =
        bgp::Message::new(bgp::MessageType::Open, bgp::BGPMessageBody::Open(body))
            .context("Failed to create BGP message")?
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

    // Get router ID safely
    let router_id = {
        let n = neighbor.lock().await;
        n.remote_rid
            .ok_or_else(|| anyhow!("Remote router ID not set"))?
    };

    for (n, a) in nlris {
        match a {
            None => wd.push(n),
            Some(route_attributes) => {
                if !route_attributes.is_from_neighbor(router_id) {
                    updates
                        .entry(route_attributes.clone())
                        .or_insert_with(Vec::new)
                        .push(n);
                }
            }
        }
    }

    if updates.is_empty() && wd.is_empty() {
        return Ok(());
    }

    let mut nlris = vec![];
    let mut attributes = vec![];

    for (mut ra, mut routes) in updates {
        let should_send = {
            let neighbor = neighbor.lock().await;
            let local_asn = neighbor.local_asn;
            let local_ip = neighbor
                .local_ip
                .ok_or_else(|| anyhow!("Local IP not set"))?;
            let remote_asn = neighbor
                .remote_asn
                .ok_or_else(|| anyhow!("Remote ASN not set"))?;

            if local_asn != remote_asn {
                ra.next_hop = local_ip;
                ra.prepend(local_asn, 1);
                true
            } else if ra.is_from_ibgp() {
                false
            } else {
                true
            }
        };

        if !should_send {
            continue;
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
        .map_err(|e| anyhow!("Failed to build UPDATE message: {}", e))?;

    log::info!("Sending UPDATE {:?}", body);

    let message: Vec<u8> =
        Message::new(bgp::MessageType::Update, bgp::BGPMessageBody::Update(body))
            .context("Failed to create UPDATE message")?
            .into();

    server
        .send(message)
        .await
        .context("Failed to send UPDATE message")?;

    Ok(())
}

pub async fn send_keepalive(
    server: &mut Framed<tokio::net::TcpStream, bgp::BGPMessageCodec>,
) -> Result<()> {
    let body = bgp::BGPKeepaliveMessage::new()
        .map_err(|e| anyhow!("Failed to create KEEPALIVE message: {}", e))?;

    let message: Vec<u8> = bgp::Message::new(
        bgp::MessageType::Keepalive,
        bgp::BGPMessageBody::Keepalive(body.clone()),
    )
    .context("Failed to create KEEPALIVE message")?
    .into();

    log::debug!("Sending KEEPALIVE");

    server
        .send(message)
        .await
        .context("Failed to send KEEPALIVE message")
}

pub async fn read_message(
    server: &mut Framed<TcpStream, bgp::BGPMessageCodec>,
) -> Option<Result<bgp::Message, std::io::Error>> {
    match server.next().await {
        Some(Ok(bytes)) => match bgp::Message::try_from(bytes) {
            Ok(message) => Some(Ok(message)),
            Err(e) => {
                log::error!("Failed to parse BGP message: {}", e);
                Some(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("BGP message parse error: {}", e),
                )))
            }
        },
        Some(Err(e)) => Some(Err(e)),
        None => None,
    }
}

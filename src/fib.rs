use futures::stream::TryStreamExt;
use futures::stream::{self, StreamExt};
use ipnet::IpNet;
use netlink_packet::RouteProtocol;
use rtnetlink::packet::link::nlas::Nla as lnla;
use rtnetlink::packet::nlas::route::Nla as rnla;
use rtnetlink::packet::RouteMessage;
use rtnetlink::{new_connection, Handle, IpVersion};
use std::net::IpAddr;

use crate::rib;

#[derive(Debug, PartialEq)]
struct FibEntry {
    prefix: Option<IpNet>,
    next_hop: Option<IpAddr>,
    dev: String,
    metric: Option<u32>,
    proto: RouteProtocol,
}

impl FibEntry {
    async fn from_rtnl(msg: RouteMessage) -> FibEntry {
        let (connection, handle, _) = new_connection().unwrap();
        tokio::spawn(connection);
        let prefix = RouteMessage::destination_prefix(&msg);
        let prefix = match prefix {
            None => None,
            Some(v) => {
                let str = format!("{}/{}", v.0, v.1);
                Some(str.parse().unwrap())
            }
        };

        let next_hop = RouteMessage::gateway(&msg);
        let dev = RouteMessage::output_interface(&msg);
        let dev = get_link_name(handle, dev.unwrap()).await;
        let metric = msg.nlas.iter().find_map(|nla| {
            if let rnla::Priority(v) = nla {
                Some(*v)
            } else {
                None
            }
        });
        let proto: RouteProtocol = msg.header.protocol.into();
        FibEntry {
            prefix,
            next_hop,
            dev,
            metric,
            proto,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
struct Fib {
    routes: Vec<FibEntry>,
}

impl Fib {
    async fn new() -> Self {
        let v = get_routes().await;

        Fib { routes: v }
    }

    async fn refresh(&mut self) {
        let v = get_routes().await;

        self.routes = v;
    }

    async fn sync(&mut self, rib: rib::Rib) {
        for r in rib {
            println!("{:?}", r);
        }
    }
}

async fn get_routes() -> Vec<FibEntry> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);
    let mut routes = handle.route().get(IpVersion::V4).execute();
    let mut v = vec![];
    while let Some(route) = routes.try_next().await.unwrap_or(None) {
        v.push(route);
    }
    let z = stream::iter(v.clone())
        .then(|b| FibEntry::from_rtnl(b))
        .collect::<Vec<FibEntry>>()
        .await;
    z
}

async fn get_link_name(handle: Handle, index: u32) -> String {
    let mut links = handle.link().get().match_index(index).execute();
    let msg = links.try_next().await.unwrap().unwrap();

    msg.nlas
        .iter()
        .find_map(|nla| {
            if let lnla::IfName(v) = nla {
                Some(v.clone())
            } else {
                None
            }
        })
        .unwrap()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_refresh() {
        let f = tokio_test::block_on(Fib::new());
        let g = Fib::default();
        // tokio_test::block_on(g.refresh());
        assert_eq!(f, g);
    }
}

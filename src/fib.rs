use futures::stream::TryStreamExt;
use futures::stream::{self, StreamExt};
use ipnet::IpNet;
use netlink_packet::RouteProtocol;
use netlink_packet_route::link::nlas::Nla as lnla;
use netlink_packet_route::nlas::route::Nla as rnla;
use netlink_packet_route::RouteMessage;
use rtnetlink::{new_connection, Handle, IpVersion};
// use std::error::Error;
use std::net::IpAddr;

use crate::bgp::{AddressFamily, AFI};
use crate::rib;

#[derive(Debug, PartialEq, Clone)]
pub struct FibEntry {
    prefix: Option<IpNet>,
    next_hop: Option<IpAddr>,
    dev: String,
    metric: Option<u32>,
    proto: RouteProtocol,
    rm: RouteMessage,
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
            rm: msg,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Fib {
    af: AddressFamily,
    routes: Vec<FibEntry>,
}

impl Fib {
    pub async fn new(af: AddressFamily) -> Self {
        let mut fib = Fib { af, routes: vec![] };
        fib.refresh().await;
        fib
    }

    pub async fn refresh(&mut self) {
        let v = self.get_routes().await;

        self.routes = v;
    }

    pub async fn sync(&mut self, rib: rib::Rib) {
        // println!("sync : {:?}", self);
        let (connection, handle, _) = new_connection().unwrap();
        tokio::spawn(connection);
        for (n, mut a) in rib {
            let _ = a.sort();
            let a = a.first().unwrap();
            println!("{:?} : {:?}", n, a);
            match self
                .find_route(n.clone().into(), a.next_hop, handle.clone())
                .await
            {
                Some(_t) => {
                    println!("Route {:?} already present, skipping it", n);
                }
                None => {
                    println!("Route {:?}  is not present, adding it", n);
                    self.add_route(n.into(), a.next_hop, handle.clone()).await;
                }
            }
        }
    }
    pub async fn find_route(
        &mut self,
        subnet: IpNet,
        nexthop: IpAddr,
        _handle: Handle,
    ) -> Option<FibEntry> {
        let routes = self.routes.clone();
        let subnet = IpNet::from(subnet);
        let nexthop = IpAddr::from(nexthop);
        routes.into_iter().find_map(|fe| {
            if fe.prefix == Some(subnet) && fe.next_hop == Some(nexthop) {
                Some(fe)
            } else {
                None
            }
        })
    }

    pub async fn add_route(&mut self, subnet: IpNet, nexthop: IpAddr, handle: Handle) {
        let route = handle.route();

        println!(
            "\nAdding route {:?} via {:?} on {:?}, handle {:?}\n",
            subnet, nexthop, self.af, handle
        );

        match subnet {
            IpNet::V6(t) => match nexthop {
                IpAddr::V6(n) => {
                    let _ = route
                        .add()
                        .v6()
                        .destination_prefix(t.addr(), t.prefix_len())
                        .gateway(n)
                        // .protocol(3)
                        .execute()
                        .await;
                    // .unwrap();
                }
                IpAddr::V4(_n) => {}
            },
            IpNet::V4(t) => match nexthop {
                IpAddr::V6(_n) => {}
                IpAddr::V4(n) => {
                    let _ = route
                        .add()
                        .v4()
                        .destination_prefix(t.addr(), t.prefix_len())
                        .gateway(n)
                        // .protocol(3)
                        .execute()
                        .await;
                    // .unwrap();
                }
            },
        };
    }

    pub async fn _del_route(&mut self, entry: FibEntry, handle: Handle) {
        let route = handle.route();
        route.del(entry.rm).execute().await.unwrap();
    }

    async fn get_routes(&mut self) -> Vec<FibEntry> {
        let (connection, handle, _) = new_connection().unwrap();
        tokio::spawn(connection);
        let mut routes = match self.af.afi {
            AFI::Ipv4 => handle.route().get(IpVersion::V4).execute(),
            AFI::Ipv6 => handle.route().get(IpVersion::V6).execute(),
        };
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
        // let f = tokio_test::block_on(Fib::new());
        // let g = Fib::default();
        // tokio_test::block_on(g.refresh());
        // assert_eq!(f, g);
    }
}

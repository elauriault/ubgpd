use futures::stream::TryStreamExt;
use futures::stream::{self, StreamExt};
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
// use netlink_packet::route::RouteProtool;
// use netlink_packet_route::link::nlas::Nla as lnla;
use netlink_packet_route::link::LinkAttribute;
// use netlink_packet_route::route::nlas::Nla as rnla;
// use netlink_packet_route::route::message::RouteMessage;
use netlink_packet_route::route::{RouteAddress, RouteAttribute, RouteMessage, RouteProtocol};
use rtnetlink::{new_connection, Handle, IpVersion};
// use std::error::Error;

use crate::bgp::{AddressFamily, Afi};
use crate::rib::{self};

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
        let plen = msg.header.destination_prefix_length;
        let prefix = msg.attributes.iter().find_map(|nla| {
            if let RouteAttribute::Destination(v) = nla {
                match v {
                    RouteAddress::Inet(t) => Some(Ipv4Net::new(*t, plen).unwrap().into()),
                    RouteAddress::Inet6(t) => Some(Ipv6Net::new(*t, plen).unwrap().into()),
                    _ => None,
                }
            } else {
                None
            }
        });
        let next_hop = msg.attributes.iter().find_map(|nla| {
            if let RouteAttribute::Gateway(v) = nla {
                match v {
                    RouteAddress::Inet(t) => Some(IpAddr::V4(*t)),
                    RouteAddress::Inet6(t) => Some(IpAddr::V6(*t)),
                    _ => None,
                }
            } else {
                None
            }
        });
        let dev = msg.attributes.iter().find_map(|nla| {
            if let RouteAttribute::Oif(v) = nla {
                Some(*v)
            } else {
                None
            }
        });
        let dev = get_link_name(handle, dev.unwrap()).await;
        let metric = msg.attributes.iter().find_map(|nla| {
            if let RouteAttribute::Metrics(_v) = nla {
                // Some(*v)
                None
            } else {
                None
            }
        });
        let proto = msg.header.protocol;
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
        let v = self.get_routes(self.af.clone()).await;

        self.routes = v;
    }

    pub async fn sync(&mut self, rib: Arc<Mutex<rib::Rib>>) {
        let (connection, handle, _) = new_connection().unwrap();
        tokio::spawn(connection);
        {
            let rib = rib.lock().await;
            for (n, a) in rib.iter() {
                if let Some(a) = a.iter().find(|a| self.has_route(a.next_hop)) {
                    log::debug!("{:?} : {:?}", n, a);
                    match self
                        .find_route((*n).into(), a.next_hop, handle.clone())
                        .await
                    {
                        Some(_t) => {
                            log::debug!("Route {:?} already present, skipping it", n);
                        }
                        None => {
                            log::debug!("Route {:?}  is not present, adding it", n);
                            self.add_route(n.into(), a.next_hop, handle.clone()).await;
                        }
                    }
                }
            }
        }
    }

    pub fn has_route(&self, addr: IpAddr) -> bool {
        self.routes.iter().any(|fe| match fe.prefix {
            None => false,
            Some(prefix) => prefix.contains(&addr),
        })
    }

    async fn find_route(
        &mut self,
        subnet: IpNet,
        nexthop: IpAddr,
        _handle: Handle,
    ) -> Option<FibEntry> {
        let routes = self.routes.clone();
        // let subnet = IpNet::from(subnet);
        // let nexthop = IpAddr::from(nexthop);
        routes.into_iter().find_map(|fe| {
            if fe.prefix == Some(subnet) && fe.next_hop == Some(nexthop) {
                Some(fe)
            } else {
                None
            }
        })
    }

    async fn add_route(&mut self, subnet: IpNet, nexthop: IpAddr, handle: Handle) {
        let route = handle.route();

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

    async fn _del_route(&mut self, entry: FibEntry, handle: Handle) {
        let route = handle.route();
        // route.del(entry.rm).execute().await.unwrap();
        if let Err(e) = route.del(entry.rm).execute().await {
            log::error!("Failed to delete route: {}", e);
            // Handle error without returning - maybe set a state flag or retry
        } else {
            log::debug!("Route deleted successfully");
        }
    }

    async fn get_routes(&mut self, af: AddressFamily) -> Vec<FibEntry> {
        let (connection, handle, _) = new_connection().unwrap();
        tokio::spawn(connection);
        let mut routes = match af.afi {
            Afi::Ipv4 => handle.route().get(IpVersion::V4).execute(),
            Afi::Ipv6 => handle.route().get(IpVersion::V6).execute(),
        };
        // let mut v: Vec<RouteMessage> = vec![];
        let mut v = vec![];
        while let Some(route) = routes.try_next().await.unwrap_or(None) {
            v.push(route);
        }
        stream::iter(v.clone())
            .then(FibEntry::from_rtnl)
            .collect::<Vec<FibEntry>>()
            .await
    }
}

async fn get_link_name(handle: Handle, index: u32) -> String {
    let mut links = handle.link().get().match_index(index).execute();
    let msg = links.try_next().await.unwrap().unwrap();

    msg.attributes
        .iter()
        .find_map(|nla| {
            if let LinkAttribute::IfName(v) = nla {
                Some(v.clone())
            } else {
                None
            }
        })
        .unwrap()
}

// #[cfg(test)]
// mod tests {
//
//     use super::*;
//
//     #[test]
//     fn test_refresh() {
//         // let f = tokio_test::block_on(Fib::new());
//         // let g = Fib::default();
//         // tokio_test::block_on(g.refresh());
//         // assert_eq!(f, g);
//     }
// }

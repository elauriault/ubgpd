use anyhow::Context;
use futures::stream::TryStreamExt;
use futures::stream::{self, StreamExt};
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use netlink_packet_route::link::LinkAttribute;
use netlink_packet_route::route::{RouteAddress, RouteAttribute, RouteMessage, RouteProtocol};
use rtnetlink::{new_connection, Handle, RouteMessageBuilder};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::bgp::{AddressFamily, Afi};
use crate::rib::{self};

#[derive(Debug, PartialEq, Clone)]
pub struct FibEntry {
    prefix: Option<IpNet>,
    next_hop: Option<IpAddr>,
    dev: Option<String>,
    metric: Option<u32>,
    proto: RouteProtocol,
    rm: RouteMessage,
}

impl FibEntry {
    async fn from_rtnl(msg: RouteMessage) -> Option<FibEntry> {
        let (connection, handle, _) = new_connection()
            .map_err(|e| log::error!("Failed to create netlink connection: {}", e))
            .ok()?;
        tokio::spawn(connection);
        let plen = msg.header.destination_prefix_length;
        let prefix = msg.attributes.iter().find_map(|nla| {
            if let RouteAttribute::Destination(v) = nla {
                match v {
                    RouteAddress::Inet(t) => Ipv4Net::new(*t, plen)
                        .map_err(|e| log::warn!("Invalid IPv4 prefix: {}", e))
                        .ok()
                        .map(|net| net.into()),
                    RouteAddress::Inet6(t) => Ipv6Net::new(*t, plen)
                        .map_err(|e| log::warn!("Invalid IPv6 prefix: {}", e))
                        .ok()
                        .map(|net| net.into()),
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
        let dev = match dev {
            Some(dev_id) => match get_link_name(handle, dev_id).await {
                Ok(name) => Some(name),
                Err(e) => {
                    log::error!(
                        "Failed to get link name for device {}: {} (route will be skipped)",
                        dev_id,
                        e
                    );
                    return None;
                }
            },
            None => {
                log::debug!("Route has no output interface specified");
                None
            }
        };
        let metric = msg.attributes.iter().find_map(|nla| {
            if let RouteAttribute::Metrics(_v) = nla {
                // Some(*v)
                None
            } else {
                None
            }
        });
        let proto = msg.header.protocol;
        Some(FibEntry {
            prefix,
            next_hop,
            dev,
            metric,
            proto,
            rm: msg,
        })
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
        let (connection, handle, _) = match new_connection() {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("Failed to create netlink connection for FIB sync: {}", e);
                return;
            }
        };
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
                        .add(
                            RouteMessageBuilder::<Ipv6Addr>::new()
                                .destination_prefix(t.addr(), t.prefix_len())
                                .gateway(n)
                                .protocol(RouteProtocol::Bgp)
                                .build(),
                        )
                        .execute()
                        .await;
                }
                IpAddr::V4(_n) => {}
            },
            IpNet::V4(t) => match nexthop {
                IpAddr::V6(_n) => {}
                IpAddr::V4(n) => {
                    let _ = route
                        .add(
                            RouteMessageBuilder::<Ipv4Addr>::new()
                                .destination_prefix(t.addr(), t.prefix_len())
                                .gateway(n)
                                .protocol(RouteProtocol::Bgp)
                                .build(),
                        )
                        .execute()
                        .await;
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
            Afi::Ipv4 => handle
                .route()
                .get(RouteMessageBuilder::<Ipv4Addr>::new().build())
                .execute(),
            Afi::Ipv6 => handle
                .route()
                .get(RouteMessageBuilder::<Ipv6Addr>::new().build())
                .execute(),
        };
        // let mut v: Vec<RouteMessage> = vec![];
        let mut v = vec![];
        while let Some(route) = routes.try_next().await.unwrap_or_else(|e| {
            log::error!("Failed to read route from netlink: {}", e);
            None
        }) {
            v.push(route);
        }
        let all_results: Vec<Option<FibEntry>> = stream::iter(v.clone())
            .then(FibEntry::from_rtnl)
            .collect()
            .await;

        all_results.into_iter().flatten().collect()
    }
}

async fn get_link_name(handle: Handle, index: u32) -> Result<String, anyhow::Error> {
    let mut links = handle.link().get().match_index(index).execute();
    let msg = links
        .try_next()
        .await
        .context("Failed to get link information")?
        .context("No link found with specified index")?;

    msg.attributes
        .iter()
        .find_map(|nla| {
            if let LinkAttribute::IfName(v) = nla {
                Some(v.clone())
            } else {
                None
            }
        })
        .context("Link has no interface name")
}

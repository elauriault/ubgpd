use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::Mutex;
// use futures::stream::Next;
// use netlink_packet_route::AddressFamily;
// use ipnet::IpAdd;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::time::Instant;

use crate::bgp::Flatten;
use crate::bgp::{self, PathAttribute};
// use crate::fib;
use crate::neighbor;

#[derive(Debug, Eq, Clone)]
pub struct RouteAttributes {
    as_path: bgp::Aspath,
    origin: bgp::OriginType,
    pub next_hop: IpAddr,
    local_pref: Option<u32>,
    multi_exit_disc: Option<u32>,
    path_type: PathType,
    peer_type: PeeringType,
    recv_time: Instant,
    pub peer_rid: u32,
    peer_ip: IpAddr,
}

#[derive(Debug, Clone)]
pub struct RibUpdate {
    pub nlris: Vec<bgp::Nlri>,
    pub attributes: RouteAttributes,
}

impl Default for RouteAttributes {
    fn default() -> Self {
        RouteAttributes {
            as_path: Vec::new(),                             // Empty AS path
            origin: bgp::OriginType::Igp,                    // Default origin is IGP
            next_hop: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), // Default next-hop is 0.0.0.0
            local_pref: None,                                // No local preference by default
            multi_exit_disc: None,                           // No MED by default
            path_type: PathType::External,                   // Default path type is External
            peer_type: PeeringType::Ebgp,                    // Default peer type is EBGP
            recv_time: Instant::now(),                       // Current time as receive time
            peer_rid: 0,                                     // Default router ID is 0
            peer_ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),  // Default peer IP is 0.0.0.0
        }
    }
}

impl RouteAttributes {
    pub fn prepend(&mut self, asn: u16, times: u8) -> bgp::Aspath {
        let sequence = bgp::ASPATHSegment {
            path_type: bgp::ASPATHSegmentType::AsSequence,
            as_list: vec![asn; times.into()],
        };
        self.as_path.insert(0, sequence);
        self.as_path.clone()
    }

    pub fn is_from_ibgp(&self) -> bool {
        if self.peer_type == PeeringType::Ibgp {
            return true;
        }
        false
    }

    pub fn is_from_neighbor(&self, n: u32) -> bool {
        if self.peer_rid == n {
            return true;
        }
        false
    }

    pub async fn is_valid(&self, asn: u16) -> bool {
        if self.as_path.flatten_aspath().contains(&asn) {
            return false;
        }
        true
    }

    pub async fn new(
        src: Vec<bgp::PathAttribute>,
        local_asn: u32,
        // s: Arc<Mutex<speaker::BGPSpeaker>>,
        nb: Arc<Mutex<neighbor::BGPNeighbor>>,
        nh: Option<IpAddr>,
    ) -> RouteAttributes {
        let mut multi_exit_disc = None;
        let mut local_pref = None;
        let mut next_hop = Some(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
        let mut as_path: Vec<bgp::ASPATHSegment> = vec![];
        let mut origin = bgp::OriginType::Igp;
        for p in src {
            match p.value {
                bgp::PathAttributeValue::Origin(o) => {
                    origin = o;
                }
                bgp::PathAttributeValue::AsPath(a) => {
                    as_path = a;
                }
                bgp::PathAttributeValue::NextHop(n) => {
                    next_hop = Some(IpAddr::V4(n));
                }
                bgp::PathAttributeValue::MultiExitDisc(m) => {
                    multi_exit_disc = Some(m);
                }
                bgp::PathAttributeValue::LocalPref(l) => {
                    local_pref = Some(l);
                }
                bgp::PathAttributeValue::AtomicAggregate => {}
                bgp::PathAttributeValue::Aggregator(_) => {}
                _ => {}
            }
        }

        if let Some(n) = nh {
            next_hop = Some(n)
        };

        let next_hop = next_hop.unwrap();
        let remote_asn;
        let peer_rid;
        let peer_ip;
        {
            let nb = nb.lock().await;
            remote_asn = nb.remote_asn.unwrap();
            peer_rid = nb.remote_rid.unwrap();
            peer_ip = nb.remote_ip.unwrap();
        }

        let peer_type;
        let path_type;

        if local_asn == remote_asn as u32 {
            peer_type = PeeringType::Ibgp;
            path_type = PathType::Internal;
        } else {
            peer_type = PeeringType::Ebgp;
            path_type = PathType::External;
        }

        RouteAttributes {
            next_hop,
            multi_exit_disc,
            local_pref,
            as_path,
            origin,
            path_type,
            peer_type,
            peer_rid,
            peer_ip,
            recv_time: Instant::now(),
        }
    }
}

impl From<RouteAttributes> for Vec<PathAttribute> {
    fn from(val: RouteAttributes) -> Self {
        let mut ret = vec![];
        // let mut mpnh = None;
        match val.next_hop {
            IpAddr::V6(_ip6) => {
                // mpnh = Some(ip6);
            }
            IpAddr::V4(ip4) => {
                ret.push(PathAttribute::nexthop(ip4));
            }
        }
        ret.push(PathAttribute::origin(val.origin));
        ret.push(PathAttribute::aspath(val.as_path));
        if let Some(pref) = val.local_pref {
            ret.push(PathAttribute::local_pref(pref));
        }
        if let Some(med) = val.multi_exit_disc {
            ret.push(PathAttribute::med(med));
        }
        ret
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Ord, Hash)]
pub enum PathType {
    External,
    Internal,
    // Aggregate,
    // Redist,
    // Local,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Ord, Hash)]
pub enum PeeringType {
    Ibgp,
    Ebgp,
}

impl PartialEq for RouteAttributes {
    fn eq(&self, other: &Self) -> bool {
        let slen: usize = self.as_path.iter().map(|x| x.len()).sum();
        let olen: usize = other.as_path.iter().map(|x| x.len()).sum();
        self.local_pref == other.local_pref
            && self.multi_exit_disc == other.multi_exit_disc
            && self.origin == other.origin
            && slen == olen
    }
}

impl Hash for RouteAttributes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let len: usize = self.as_path.iter().map(|x| x.len()).sum();
        self.local_pref.hash(state);
        self.multi_exit_disc.hash(state);
        self.origin.hash(state);
        len.hash(state);
    }
}

impl PartialOrd for RouteAttributes {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RouteAttributes {
    fn cmp(&self, other: &Self) -> Ordering {
        let lp = self.local_pref.cmp(&other.local_pref);
        if lp != Ordering::Equal {
            return lp;
        }

        let pt = self.path_type.cmp(&other.path_type);
        if pt != Ordering::Equal {
            return pt;
        }

        let slen: usize = self.as_path.iter().map(|x| x.len()).sum();
        let olen: usize = other.as_path.iter().map(|x| x.len()).sum();
        let path_len = slen.cmp(&olen);
        if path_len != Ordering::Equal {
            return path_len;
        }

        let otype = self.origin.cmp(&other.origin);
        if otype != Ordering::Equal {
            return otype;
        }

        let med = self.multi_exit_disc.cmp(&other.multi_exit_disc);
        if med != Ordering::Equal {
            return med;
        }

        let peer = self.peer_type.cmp(&other.peer_type);
        if peer != Ordering::Equal {
            return peer;
        }

        if self.peer_type == PeeringType::Ibgp {
            // check igp of path and return the lowest
        }

        if self.peer_type == PeeringType::Ebgp {
            let r_time = self.recv_time.cmp(&other.recv_time);
            if r_time != Ordering::Equal {
                return r_time;
            }
        }

        let rid = self.peer_rid.cmp(&other.peer_rid);
        if rid != Ordering::Equal {
            return rid;
        }

        self.peer_ip.cmp(&other.peer_ip)
    }
}

pub type Rib = HashMap<bgp::Nlri, Vec<RouteAttributes>>;

use async_std::sync::{Arc, Mutex};
// use ipnet::IpAdd;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::net::IpAddr;
// use std::net::Ipv4Addr;
use std::time::Instant;

use crate::bgp;
use crate::neighbor;
// use crate::speaker;

#[derive(Debug, Eq, Clone)]
pub struct RouteAttributes {
    as_path: bgp::ASPATH,
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
    pub nlris: Vec<bgp::NLRI>,
    pub attributes: RouteAttributes,
}

impl RouteAttributes {
    pub fn from_neighbor(&self, n: u32) -> bool {
        if self.peer_rid == n {
            return true;
        }
        false
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
        let mut next_hop = None;
        let mut as_path: Vec<bgp::ASPATHSegment> = vec![];
        let mut origin = bgp::OriginType::IGP;
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
        match nh {
            Some(n) => next_hop = Some(n),
            None => {}
        }
        let next_hop = next_hop.unwrap();
        let remote_asn;
        let peer_rid;
        let peer_ip;
        {
            let nb = nb.lock().await;
            remote_asn = nb.remote_asn;
            peer_rid = nb.router_id;
            peer_ip = nb.remote_ip;
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Ord)]
pub enum PathType {
    External,
    Internal,
    // Aggregate,
    // Redist,
    // Local,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Ord)]
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

impl PartialOrd for RouteAttributes {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let lp = self.local_pref.partial_cmp(&other.local_pref);
        if lp != Some(Ordering::Equal) {
            return lp;
        }

        let pt = self.path_type.partial_cmp(&other.path_type);
        if pt != Some(Ordering::Equal) {
            return pt;
        }

        let slen: usize = self.as_path.iter().map(|x| x.len()).sum();
        let olen: usize = other.as_path.iter().map(|x| x.len()).sum();
        let path_len = slen.partial_cmp(&olen);
        if path_len != Some(Ordering::Equal) {
            return path_len;
        }

        let otype = self.origin.partial_cmp(&other.origin);
        if otype != Some(Ordering::Equal) {
            return otype;
        }

        let med = self.multi_exit_disc.partial_cmp(&other.multi_exit_disc);
        if med != Some(Ordering::Equal) {
            return med;
        }

        let peer = self.peer_type.partial_cmp(&other.peer_type);
        if peer != Some(Ordering::Equal) {
            return peer;
        }

        if self.peer_type == PeeringType::Ibgp {
            // check igp of path and return the lowest
        }

        if self.peer_type == PeeringType::Ebgp {
            let r_time = self.recv_time.partial_cmp(&other.recv_time);
            if r_time != Some(Ordering::Equal) {
                return r_time;
            }
        }

        let rid = self.peer_rid.partial_cmp(&other.peer_rid);
        if rid != Some(Ordering::Equal) {
            return rid;
        }

        self.peer_ip.partial_cmp(&other.peer_ip)
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

pub type Rib = HashMap<bgp::NLRI, Vec<RouteAttributes>>;

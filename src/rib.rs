use crate::bgp;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::time::Instant;

#[derive(Debug, Eq)]
pub struct RouteAttributes {
    next_hop: Ipv4Addr,
    multi_exit_disc: u16,
    local_pref: u16,
    as_path: bgp::ASPATH,
    origin: bgp::OriginType,
    path_type: PathType,
    peer_type: PeeringType,
    recv_time: Instant,
    peer_rid: u32,
    peer_ip: u32,
}

#[derive(Debug, PartialEq, Eq, PartialOrd)]
enum PathType {
    External,
    Internal,
    Aggregate,
    Redist,
    Local,
}

#[derive(Debug, PartialEq, Eq, PartialOrd)]
enum PeeringType {
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
            && self.as_path.len() == other.as_path.len()
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

pub type Rib = HashMap<bgp::NLRI, Vec<RouteAttributes>>;

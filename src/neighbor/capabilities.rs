// src/neighbor/capabilities.rs

use crate::bgp;
use itertools::Itertools;
use num_traits::FromPrimitive;

#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub multiprotocol: Option<Vec<bgp::AddressFamily>>,
    pub route_refresh: bool,
    pub outbound_route_filtering: bool,
    pub extended_next_hop_encoding: bool,
    pub graceful_restart: bool,
    pub four_octect_asn: Option<u32>,
}

impl From<bgp::BGPCapabilities> for Capabilities {
    fn from(src: bgp::BGPCapabilities) -> Self {
        let mut capabilities = Capabilities::default();
        let mut afs = vec![];
        for c in src.params {
            match c.capability_code {
                bgp::BGPCapabilityCode::Multiprotocol => {
                    if c.capability_length != 4 {
                        panic!("Unexpected length of BGP capability");
                    }
                    let mut afi = [0u8; 2];
                    let mut safi = [0u8; 1];
                    afi.copy_from_slice(&c.capability_value[0..2]);
                    safi.copy_from_slice(&c.capability_value[3..4]);
                    let afi = u16::from_be_bytes(afi);
                    let safi = u8::from_be_bytes(safi);
                    let afi = FromPrimitive::from_u16(afi).unwrap();
                    let safi = FromPrimitive::from_u8(safi).unwrap();
                    let af = bgp::AddressFamily { afi, safi };
                    afs.push(af);
                }
                bgp::BGPCapabilityCode::RouteRefresh => capabilities.route_refresh = true,
                bgp::BGPCapabilityCode::ExtendedNextHopEncoding => {
                    capabilities.extended_next_hop_encoding = true
                }
                bgp::BGPCapabilityCode::OutboundRouteFiltering => {
                    capabilities.outbound_route_filtering = true
                }
                bgp::BGPCapabilityCode::GracefulRestart => capabilities.graceful_restart = true,
                bgp::BGPCapabilityCode::FourOctectASN => {
                    if c.capability_length != 4 {
                        panic!("Unexpected length of BGP capability");
                    }
                    let mut v = [0u8; 4];
                    v.copy_from_slice(&c.capability_value);
                    let asn = u32::from_be_bytes(v);
                    capabilities.four_octect_asn = Some(asn);
                }
            }
        }
        capabilities.multiprotocol = Some(afs.into_iter().unique().collect());

        capabilities
    }
}

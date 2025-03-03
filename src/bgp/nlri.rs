use byteorder::{BigEndian, WriteBytesExt};
use derive_builder::Builder;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use num_traits::FromPrimitive;
use std::io::prelude::*;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::types::*;

#[derive(Builder, Debug, Clone, PartialEq, Eq, Hash, Copy)]
#[builder(setter(into))]
pub struct Nlri {
    pub net: IpNet,
}

pub struct Ipv4Octets {
    pub octets: Vec<u8>,
}

pub struct Ipv6Octets {
    pub octets: Vec<u8>,
}

impl From<Nlri> for Vec<u8> {
    fn from(val: Nlri) -> Self {
        let mut buf = Cursor::new(vec![]);
        buf.write_u8(val.net.prefix_len()).unwrap();
        let blen = (val.net.prefix_len() as f32 / 8.0).ceil() as usize;
        match val.net {
            IpNet::V4(v) => {
                let addrv4: u32 = v.network().into();
                let addr = addrv4.to_be_bytes();
                buf.write_all(&addr[0..blen]).unwrap();
            }
            IpNet::V6(v) => {
                let addrv6: u128 = v.network().into();
                let addr = addrv6.to_be_bytes();
                buf.write_all(&addr[0..blen]).unwrap();
            }
        }
        buf.into_inner()
    }
}

impl From<Nlri> for IpNet {
    fn from(val: Nlri) -> Self {
        val.net
    }
}

impl From<&Nlri> for IpNet {
    fn from(val: &Nlri) -> Self {
        val.net
    }
}

impl From<Ipv4Octets> for Nlri {
    fn from(src: Ipv4Octets) -> Self {
        let mut addr = src.octets;
        let plen = addr.remove(0);
        addr.resize(4, 0);
        let net = Ipv4Net::new(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]), plen).unwrap();
        NlriBuilder::default().net(net).build().unwrap()
    }
}

impl From<Ipv6Octets> for Nlri {
    fn from(src: Ipv6Octets) -> Self {
        let mut addr = src.octets;
        let plen = addr.remove(0);
        addr.resize(16, 0);
        let mut addr6: Vec<u16> = vec![];
        let mut i = 0;
        let end = addr.len();
        while i < end {
            let mut bytes = [0u8; 2];
            bytes.copy_from_slice(&addr[i..i + 2]);
            let val = u16::from_be_bytes(bytes);
            addr6.push(val);
            i += 2;
        }
        let net = Ipv6Net::new(
            Ipv6Addr::new(
                addr6[0], addr6[1], addr6[2], addr6[3], addr6[4], addr6[5], addr6[6], addr6[7],
            ),
            plen,
        )
        .unwrap();
        NlriBuilder::default().net(net).build().unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mpnlri {
    pub af: AddressFamily,
    pub nh: IpAddr,
    pub nlris: Vec<Nlri>,
}

impl Default for Mpnlri {
    fn default() -> Self {
        Mpnlri {
            af: AddressFamily {
                afi: Afi::Ipv6,
                safi: Safi::NLRIUnicast,
            },
            nh: IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)),
            nlris: vec![],
        }
    }
}

impl From<Vec<u8>> for Mpnlri {
    fn from(src: Vec<u8>) -> Self {
        let mut src = src;
        let total_len = src.remove(0) as usize;

        let mut afi = [0u8; 2];
        afi.copy_from_slice(&src[0..2]);
        let afi = u16::from_be_bytes(afi);
        let afi: Afi = FromPrimitive::from_u16(afi).unwrap();

        let mut safi = [0u8; 1];
        safi.copy_from_slice(&src[2..3]);
        let safi = u8::from_be_bytes(safi);
        let safi: Safi = FromPrimitive::from_u8(safi).unwrap();

        let mut nhl = [0u8; 1];
        nhl.copy_from_slice(&src[3..4]);
        let nhl: usize = u8::from_be_bytes(nhl).into();

        let mut addr: Vec<u8> = vec![0; nhl];
        addr.copy_from_slice(&src[4..4 + nhl]);

        let nh: IpAddr;
        let mut i = 4 + nhl + 1;

        let mut nlris: Vec<Nlri> = vec![];
        match afi {
            Afi::Ipv4 => {
                addr.resize(4, 0);
                nh = IpAddr::V4(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]));

                while i < total_len {
                    let plen = src[i];
                    let end = i + (plen as f32 / 8.0).ceil() as usize + 1;
                    let buf = Ipv4Octets {
                        octets: src[i..end].to_vec(),
                    };
                    let n: Nlri = buf.into();
                    nlris.push(n);
                    let blen = ((n.net.prefix_len() as f32 / 8.0).ceil() + 1.0) as usize;
                    i += blen;
                }
            }
            Afi::Ipv6 => {
                addr.resize(16, 0);
                let mut addr6: Vec<u16> = vec![];
                let mut j = 0;
                let end = addr.len();
                while j < end {
                    let mut bytes = [0u8; 2];
                    bytes.copy_from_slice(&addr[j..j + 2]);
                    let val = u16::from_be_bytes(bytes);
                    addr6.push(val);
                    j += 2;
                }
                nh = IpAddr::V6(Ipv6Addr::new(
                    addr6[0], addr6[1], addr6[2], addr6[3], addr6[4], addr6[5], addr6[6], addr6[7],
                ));

                while i < total_len {
                    let plen = src[i];
                    let end = i + (plen as f32 / 8.0).ceil() as usize + 1;
                    let buf = Ipv6Octets {
                        octets: src[i..end].to_vec(),
                    };
                    let n: Nlri = buf.into();
                    nlris.push(n);
                    let blen = ((n.net.prefix_len() as f32 / 8.0).ceil() + 1.0) as usize;
                    i += blen;
                }
            }
        }
        let af = AddressFamily { afi, safi };
        Mpnlri { af, nh, nlris }
    }
}

impl From<Mpnlri> for Vec<u8> {
    fn from(val: Mpnlri) -> Self {
        let mut buf = Cursor::new(vec![]);
        let mut blen = 3;
        buf.write_u8(blen as u8).unwrap();
        buf.write_u16::<BigEndian>(val.af.afi as u16).unwrap();
        buf.write_u8(val.af.safi as u8).unwrap();
        match val.nh {
            IpAddr::V4(v) => {
                buf.write_u8(4).unwrap();
                let addrv4: u32 = v.into();
                let addr = addrv4.to_be_bytes();
                buf.write_all(&addr[0..4]).unwrap();
                blen += 5
            }
            IpAddr::V6(v) => {
                buf.write_u8(16).unwrap();
                let addrv6: u128 = v.into();
                let addr = addrv6.to_be_bytes();
                buf.write_all(&addr[0..16]).unwrap();
                blen += 17;
            }
        }
        for n in val.nlris {
            let nbuf: Vec<u8> = n.into();
            blen += nbuf.len();
            buf.write_all(&nbuf).unwrap();
        }
        buf.rewind().unwrap();
        buf.write_u8(blen as u8).unwrap();
        buf.into_inner()
    }
}

#[cfg(test)]
mod tests {
    // use super::*;

    // Include the NLRI tests from the original bgp.rs
}

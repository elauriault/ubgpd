use byteorder::{BigEndian, WriteBytesExt};
use derive_builder::Builder;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use num_traits::FromPrimitive;
use std::io::prelude::*;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::types::*;
use crate::error::BgpError;

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

/// Returns the number of bytes needed to represent a prefix of length `plen`.
pub fn prefix_bytes(plen: u8) -> usize {
    (plen as usize).div_ceil(8)
}

impl From<Nlri> for Vec<u8> {
    fn from(val: Nlri) -> Self {
        let mut buf = Cursor::new(vec![]);
        buf.write_u8(val.net.prefix_len()).unwrap();
        let blen = prefix_bytes(val.net.prefix_len());
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

impl TryFrom<Ipv4Octets> for Nlri {
    type Error = BgpError;

    fn try_from(src: Ipv4Octets) -> Result<Self, Self::Error> {
        let mut addr = src.octets;
        if addr.is_empty() {
            return Err(BgpError::Message("Empty octets data".to_string()));
        }
        let plen = addr.remove(0);
        let expected_bytes = if plen == 0 {
            0
        } else {
            (plen as usize).div_ceil(8)
        };
        if addr.len() < expected_bytes {
            return Err(BgpError::Message(format!(
                "Insufficient octets for prefix length {}: need {}, got {}",
                plen,
                expected_bytes,
                addr.len()
            )));
        }
        addr.resize(4, 0);
        let net = Ipv4Net::new(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]), plen)
            .map_err(|e| BgpError::Message(format!("Invalid IPv4 network: {}", e)))?;
        Ok(NlriBuilder::default()
            .net(net)
            .build()
            .map_err(|e| BgpError::Message(format!("Failed to build NLRI: {}", e)))?)
    }
}

impl TryFrom<Ipv6Octets> for Nlri {
    type Error = BgpError;

    fn try_from(src: Ipv6Octets) -> Result<Self, Self::Error> {
        let mut addr = src.octets;
        if addr.is_empty() {
            return Err(BgpError::Message("Empty octets data".to_string()));
        }
        let plen = addr.remove(0);
        let expected_bytes = if plen == 0 {
            0
        } else {
            (plen as usize).div_ceil(8)
        };
        if addr.len() < expected_bytes {
            return Err(BgpError::Message(format!(
                "Insufficient octets for prefix length {}: need {}, got {}",
                plen,
                expected_bytes,
                addr.len()
            )));
        }
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
        .map_err(|e| BgpError::Message(format!("Invalid IPv6 network: {}", e)))?;
        Ok(NlriBuilder::default()
            .net(net)
            .build()
            .map_err(|e| BgpError::Message(format!("Failed to build NLRI: {}", e)))?)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mpnlri {
    pub af: AddressFamily,
    pub nh: IpAddr,
    pub nlris: Vec<Nlri>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mpunlri {
    pub af: AddressFamily,
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

impl Default for Mpunlri {
    fn default() -> Self {
        Mpunlri {
            af: AddressFamily {
                afi: Afi::Ipv6,
                safi: Safi::NLRIUnicast,
            },
            nlris: vec![],
        }
    }
}

impl TryFrom<Vec<u8>> for Mpnlri {
    type Error = BgpError;

    fn try_from(src: Vec<u8>) -> Result<Self, Self::Error> {
        let mut src = src;

        // Check if we have at least 1 byte for total_len
        if src.is_empty() {
            return Err(BgpError::Message("Empty MP_REACH_NLRI data".to_string()));
        }

        let total_len = src.remove(0) as usize;

        // Check if we have at least 4 bytes for AFI, SAFI, and NHL
        if src.len() < 4 {
            return Err(BgpError::Message(
                "Insufficient data for MP_REACH_NLRI header".to_string(),
            ));
        }

        let afi = u16::from_be_bytes([src[0], src[1]]);
        let afi: Afi = FromPrimitive::from_u16(afi)
            .ok_or_else(|| BgpError::Message(format!("Invalid AFI: {}", afi)))?;

        let safi = src[2];
        let safi: Safi = FromPrimitive::from_u8(safi)
            .ok_or_else(|| BgpError::Message(format!("Invalid SAFI: {}", safi)))?;

        let nhl = src[3];
        let nhl = nhl as usize;

        // Check if we have enough data for the next hop
        if src.len() < 4 + nhl {
            return Err(BgpError::Message(
                "Insufficient data for next hop address".to_string(),
            ));
        }

        let mut addr = src[4..4 + nhl].to_vec();

        let nh: IpAddr;
        let mut i = 4 + nhl;

        let mut nlris: Vec<Nlri> = vec![];
        match afi {
            Afi::Ipv4 => {
                addr.resize(4, 0);
                nh = IpAddr::V4(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]));

                while i < total_len {
                    let plen = src[i];
                    let end = i + prefix_bytes(plen) + 1;
                    let buf = Ipv4Octets {
                        octets: src[i..end].to_vec(),
                    };
                    let n: Nlri = buf.try_into()?;
                    nlris.push(n);
                    let blen = prefix_bytes(n.net.prefix_len()) + 1;
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
                    let end = i + prefix_bytes(plen) + 1;
                    let buf = Ipv6Octets {
                        octets: src[i..end].to_vec(),
                    };
                    let n: Nlri = buf.try_into()?;
                    nlris.push(n);
                    let blen = prefix_bytes(n.net.prefix_len()) + 1;
                    i += blen;
                }
            }
        }
        let af = AddressFamily { afi, safi };
        Ok(Mpnlri { af, nh, nlris })
    }
}

impl TryFrom<Vec<u8>> for Mpunlri {
    type Error = BgpError;

    fn try_from(src: Vec<u8>) -> Result<Self, Self::Error> {
        let mut src = src;

        // Check if we have at least 1 byte for total_len
        if src.is_empty() {
            return Err(BgpError::Message("Empty MP_UNREACH_NLRI data".to_string()));
        }

        let total_len = src.remove(0) as usize;

        // Check if we have at least 3 bytes for AFI and SAFI
        if src.len() < 3 {
            return Err(BgpError::Message(
                "Insufficient data for MP_UNREACH_NLRI header".to_string(),
            ));
        }

        let mut afi = [0u8; 2];
        afi.copy_from_slice(&src[0..2]);
        let afi = u16::from_be_bytes(afi);
        let afi: Afi = FromPrimitive::from_u16(afi)
            .ok_or_else(|| BgpError::Message(format!("Invalid AFI: {}", afi)))?;

        let mut safi = [0u8; 1];
        safi.copy_from_slice(&src[2..3]);
        let safi = u8::from_be_bytes(safi);
        let safi: Safi = FromPrimitive::from_u8(safi)
            .ok_or_else(|| BgpError::Message(format!("Invalid SAFI: {}", safi)))?;

        let mut nlris: Vec<Nlri> = vec![];
        let mut i = 3;
        match afi {
            Afi::Ipv4 => {
                while i < total_len {
                    if i >= src.len() {
                        return Err(BgpError::Message(
                            "Insufficient data for NLRI prefix length".to_string(),
                        ));
                    }
                    let plen = src[i];
                    let end = i + prefix_bytes(plen) + 1;
                    if end > src.len() {
                        return Err(BgpError::Message(
                            "Insufficient data for NLRI prefix".to_string(),
                        ));
                    }
                    let buf = Ipv4Octets {
                        octets: src[i..end].to_vec(),
                    };
                    let n: Nlri = buf.try_into()?;
                    nlris.push(n);
                    let blen = prefix_bytes(n.net.prefix_len()) + 1;
                    i += blen;
                }
            }
            Afi::Ipv6 => {
                while i < total_len {
                    if i >= src.len() {
                        return Err(BgpError::Message(
                            "Insufficient data for NLRI prefix length".to_string(),
                        ));
                    }
                    let plen = src[i];
                    let end = i + prefix_bytes(plen) + 1;
                    if end > src.len() {
                        return Err(BgpError::Message(
                            "Insufficient data for NLRI prefix".to_string(),
                        ));
                    }
                    let buf = Ipv6Octets {
                        octets: src[i..end].to_vec(),
                    };
                    let n: Nlri = buf.try_into()?;
                    nlris.push(n);
                    let blen = prefix_bytes(n.net.prefix_len()) + 1;
                    i += blen;
                }
            }
        }
        let af = AddressFamily { afi, safi };
        Ok(Mpunlri { af, nlris })
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

impl From<Mpunlri> for Vec<u8> {
    fn from(val: Mpunlri) -> Self {
        let mut buf = Cursor::new(vec![]);
        let mut blen = 3;
        buf.write_u8(blen as u8).unwrap();
        buf.write_u16::<BigEndian>(val.af.afi as u16).unwrap();
        buf.write_u8(val.af.safi as u8).unwrap();
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
    use super::*;
    use ipnet::{Ipv4Net, Ipv6Net};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_nlri_ipv4_vec_conversion() {
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(192, 0, 2, 0), 24).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.clone().into();
        assert_eq!(bytes, vec![24, 192, 0, 2]);
        let octets = Ipv4Octets {
            octets: bytes.clone(),
        };
        let nlri2: Nlri = octets.try_into().unwrap();
        assert_eq!(nlri, nlri2);
    }

    #[test]
    fn test_nlri_ipv6_vec_conversion() {
        let net =
            IpNet::V6(Ipv6Net::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.clone().into();
        assert_eq!(bytes[0], 32);
        assert_eq!(&bytes[1..5], &[0x20, 0x01, 0x0d, 0xb8]);
        let octets = Ipv6Octets {
            octets: bytes.clone(),
        };
        let nlri2: Nlri = octets.try_into().unwrap();
        assert_eq!(nlri, nlri2);
    }

    #[test]
    fn test_mpnlri_vec_conversion_ipv6() {
        let af = AddressFamily {
            afi: Afi::Ipv6,
            safi: Safi::NLRIUnicast,
        };
        let nh = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let net = IpNet::V6(Ipv6Net::new(Ipv6Addr::LOCALHOST, 128).unwrap());
        let nlri = Nlri { net };
        let mpnlri = Mpnlri {
            af,
            nh,
            nlris: vec![nlri.clone()],
        };
        let bytes: Vec<u8> = mpnlri.clone().into();
        let mpnlri2: Mpnlri = bytes.try_into().unwrap();
        assert_eq!(mpnlri.af, mpnlri2.af);
        assert_eq!(mpnlri.nlris, mpnlri2.nlris);
        assert_eq!(mpnlri.nh, mpnlri2.nh);
    }

    #[test]
    fn test_mpunlri_vec_conversion_ipv4() {
        let af = AddressFamily {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        };
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(10, 0, 0, 0), 8).unwrap());
        let nlri = Nlri { net };
        let mpunlri = Mpunlri {
            af,
            nlris: vec![nlri.clone()],
        };
        let bytes: Vec<u8> = mpunlri.clone().into();
        let mpunlri2: Mpunlri = bytes.try_into().unwrap();
        assert_eq!(mpunlri.af, mpunlri2.af);
        assert_eq!(mpunlri.nlris, mpunlri2.nlris);
    }

    #[test]
    fn test_prefix_bytes() {
        assert_eq!(prefix_bytes(0), 0);
        assert_eq!(prefix_bytes(1), 1);
        assert_eq!(prefix_bytes(8), 1);
        assert_eq!(prefix_bytes(9), 2);
        assert_eq!(prefix_bytes(16), 2);
        assert_eq!(prefix_bytes(17), 3);
        assert_eq!(prefix_bytes(24), 3);
        assert_eq!(prefix_bytes(25), 4);
        assert_eq!(prefix_bytes(32), 4);
    }

    #[test]
    fn test_nlri_ipv4_various_prefix_lengths() {
        // Test /8
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(10, 0, 0, 0), 8).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.into();
        assert_eq!(bytes, vec![8, 10]);

        // Test /16
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(172, 16, 0, 0), 16).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.into();
        assert_eq!(bytes, vec![16, 172, 16]);

        // Test /32
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(192, 0, 2, 1), 32).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.into();
        assert_eq!(bytes, vec![32, 192, 0, 2, 1]);

        // Test /25
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(192, 0, 2, 128), 25).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.into();
        assert_eq!(bytes, vec![25, 192, 0, 2, 128]);
    }

    #[test]
    fn test_nlri_ipv6_various_prefix_lengths() {
        // Test /64
        let net =
            IpNet::V6(Ipv6Net::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 1, 0, 0, 0, 0), 64).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.into();
        assert_eq!(bytes[0], 64);
        assert_eq!(bytes.len(), 9); // 1 byte prefix + 8 bytes address

        // Test /128
        let net =
            IpNet::V6(Ipv6Net::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 1, 0, 0, 0, 1), 128).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.into();
        assert_eq!(bytes[0], 128);
        assert_eq!(bytes.len(), 17); // 1 byte prefix + 16 bytes address
    }

    #[test]
    fn test_mpnlri_vec_conversion_ipv4() {
        let af = AddressFamily {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        };
        let nh = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(10, 0, 0, 0), 8).unwrap());
        let nlri = Nlri { net };
        let mpnlri = Mpnlri {
            af,
            nh,
            nlris: vec![nlri.clone()],
        };
        let bytes: Vec<u8> = mpnlri.clone().into();
        let mpnlri2: Mpnlri = bytes.try_into().unwrap();
        assert_eq!(mpnlri.af, mpnlri2.af);
        assert_eq!(mpnlri.nlris, mpnlri2.nlris);
        assert_eq!(mpnlri.nh, mpnlri2.nh);
    }

    #[test]
    fn test_mpnlri_multiple_nlris() {
        let af = AddressFamily {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        };
        let nh = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let nlris = vec![
            Nlri {
                net: "10.0.0.0/8".parse().unwrap(),
            },
            Nlri {
                net: "172.16.0.0/12".parse().unwrap(),
            },
            Nlri {
                net: "192.168.0.0/16".parse().unwrap(),
            },
        ];
        let mpnlri = Mpnlri {
            af,
            nh,
            nlris: nlris.clone(),
        };
        let bytes: Vec<u8> = mpnlri.clone().into();
        let mpnlri2: Mpnlri = bytes.try_into().unwrap();
        assert_eq!(mpnlri.nlris.len(), mpnlri2.nlris.len());
        for (n1, n2) in mpnlri.nlris.iter().zip(mpnlri2.nlris.iter()) {
            assert_eq!(n1, n2);
        }
    }

    #[test]
    fn test_mpunlri_vec_conversion_ipv6() {
        let af = AddressFamily {
            afi: Afi::Ipv6,
            safi: Safi::NLRIUnicast,
        };
        let net =
            IpNet::V6(Ipv6Net::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32).unwrap());
        let nlri = Nlri { net };
        let mpunlri = Mpunlri {
            af,
            nlris: vec![nlri.clone()],
        };
        let bytes: Vec<u8> = mpunlri.clone().into();
        let mpunlri2: Mpunlri = bytes.try_into().unwrap();
        assert_eq!(mpunlri.af, mpunlri2.af);
        assert_eq!(mpunlri.nlris, mpunlri2.nlris);
    }

    #[test]
    fn test_nlri_ipnet_conversions() {
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(10, 0, 0, 0), 8).unwrap());
        let nlri = Nlri { net };

        // Test From<Nlri> for IpNet
        let converted: IpNet = nlri.into();
        assert_eq!(converted, net);

        // Test From<&Nlri> for IpNet
        let nlri2 = Nlri { net };
        let converted2: IpNet = (&nlri2).into();
        assert_eq!(converted2, net);
    }

    #[test]
    fn test_default_mpnlri() {
        let mpnlri = Mpnlri::default();
        assert_eq!(mpnlri.af.afi, Afi::Ipv6);
        assert_eq!(mpnlri.af.safi, Safi::NLRIUnicast);
        assert_eq!(mpnlri.nh, IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)));
        assert!(mpnlri.nlris.is_empty());
    }

    #[test]
    fn test_default_mpunlri() {
        let mpunlri = Mpunlri::default();
        assert_eq!(mpunlri.af.afi, Afi::Ipv6);
        assert_eq!(mpunlri.af.safi, Safi::NLRIUnicast);
        assert!(mpunlri.nlris.is_empty());
    }

    #[test]
    fn test_nlri_edge_cases() {
        // Test /0 prefix
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.into();
        assert_eq!(bytes, vec![0]); // Just the prefix length

        // Test odd prefix lengths
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(192, 168, 0, 0), 23).unwrap());
        let nlri = Nlri { net };
        let bytes: Vec<u8> = nlri.into();
        assert_eq!(bytes, vec![23, 192, 168, 0]);
    }
}

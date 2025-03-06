use byteorder::{BigEndian, WriteBytesExt};
use derive_builder::Builder;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::io::prelude::*;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr};

use super::nlri::*;
use super::types::*;

#[derive(Debug, PartialEq, Eq, Clone, FromPrimitive, PartialOrd, Ord, Hash)]
pub enum OriginType {
    Igp = 0,
    Egp,
    Incomplete,
}

#[derive(Debug, Eq, Clone, FromPrimitive, PartialEq, Hash)]
pub enum ASPATHSegmentType {
    AsSet = 1,
    AsSequence,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct ASPATHSegment {
    pub path_type: ASPATHSegmentType,
    pub as_list: Vec<u16>,
}

impl ASPATHSegment {
    pub fn len(&self) -> usize {
        match &self.path_type {
            ASPATHSegmentType::AsSequence => self.as_list.len(),
            ASPATHSegmentType::AsSet => 1,
        }
    }
}

impl From<ASPATHSegment> for Vec<u8> {
    fn from(val: ASPATHSegment) -> Self {
        let mut v: Vec<u8> = vec![];
        v.push(val.path_type as u8);
        v.push(val.as_list.len() as u8);
        let mut buf = Cursor::new(vec![]);
        for asn in val.as_list {
            buf.write_u16::<BigEndian>(asn).unwrap();
        }
        let mut buf = buf.into_inner();
        v.append(&mut buf);
        v
    }
}

pub type Aspath = Vec<ASPATHSegment>;

pub trait Flatten {
    fn flatten_aspath(&self) -> Vec<u16>;
}

impl Flatten for Aspath {
    fn flatten_aspath(&self) -> Vec<u16> {
        let mut v: Vec<u16> = vec![];
        for segment in self {
            v.append(&mut segment.as_list.clone());
        }
        v
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct AggregatorValue {
    pub last_as: u16,
    pub aggregator: Ipv4Addr,
}

#[derive(Debug, PartialEq, Clone, FromPrimitive, Copy)]
pub enum PathAttributeType {
    Origin = 1,
    AsPath,
    NextHop,
    MultiExitDisc,
    LocalPref,
    AtomicAggregate,
    Aggregator,
    Community,
    OriginatorId,
    ClusterList,
    Dpa,
    Advertiser,
    RcidPathClusterId,
    MPReachableNLRI,
    MPUnreachableNLRI,
    ExtCommunities,
}

#[derive(Debug, PartialEq, Clone)]
pub enum PathAttributeValue {
    Origin(OriginType),
    AsPath(Aspath),
    NextHop(Ipv4Addr),
    MultiExitDisc(u32),
    LocalPref(u32),
    AtomicAggregate,
    Aggregator(AggregatorValue),
    Community,
    OriginatorId,
    ClusterList,
    Dpa,
    Advertiser,
    RcidPathClusterId,
    MPReachableNLRI(Mpnlri),
    MPUnreachableNLRI(Mpunlri),
    ExtCommunities,
}

#[derive(Builder, Debug, PartialEq, Clone)]
#[builder(setter(into))]
pub struct PathAttribute {
    pub optional: bool,
    pub transitive: bool,
    pub partial: bool,
    pub extended_length: bool,
    pub type_code: PathAttributeType,
    pub value: PathAttributeValue,
}

impl PathAttribute {
    pub fn origin(origin: OriginType) -> Self {
        PathAttribute {
            type_code: PathAttributeType::Origin,
            value: PathAttributeValue::Origin(origin),
            optional: false,
            transitive: true,
            partial: false,
            extended_length: false,
        }
    }

    pub fn aspath(aspath: Aspath) -> Self {
        PathAttribute {
            type_code: PathAttributeType::AsPath,
            value: PathAttributeValue::AsPath(aspath),
            optional: false,
            transitive: true,
            partial: false,
            extended_length: false,
        }
    }

    pub fn nexthop(nh: Ipv4Addr) -> Self {
        PathAttribute {
            type_code: PathAttributeType::NextHop,
            value: PathAttributeValue::NextHop(nh),
            optional: false,
            transitive: true,
            partial: false,
            extended_length: false,
        }
    }

    pub fn med(med: u32) -> Self {
        PathAttribute {
            type_code: PathAttributeType::MultiExitDisc,
            value: PathAttributeValue::MultiExitDisc(med),
            optional: true,
            transitive: false,
            partial: false,
            extended_length: false,
        }
    }

    pub fn local_pref(pref: u32) -> Self {
        PathAttribute {
            type_code: PathAttributeType::LocalPref,
            value: PathAttributeValue::LocalPref(pref),
            optional: true,
            transitive: false,
            partial: false,
            extended_length: false,
        }
    }

    pub fn aggregator(last_as: u16, aggregator: Ipv4Addr) -> Self {
        PathAttribute {
            type_code: PathAttributeType::Aggregator,
            value: PathAttributeValue::Aggregator(AggregatorValue {
                last_as,
                aggregator,
            }),
            optional: true,
            transitive: true,
            partial: false,
            extended_length: false,
        }
    }

    pub fn mp_reachable(af: AddressFamily, nh: IpAddr, nlris: Vec<Nlri>) -> Self {
        PathAttribute {
            type_code: PathAttributeType::MPReachableNLRI,
            value: PathAttributeValue::MPReachableNLRI(Mpnlri { af, nh, nlris }),
            optional: true,
            transitive: false,
            partial: false,
            extended_length: false,
        }
    }

    pub fn mp_unreachable(af: AddressFamily, nlris: Vec<Nlri>) -> Self {
        PathAttribute {
            type_code: PathAttributeType::MPUnreachableNLRI,
            value: PathAttributeValue::MPUnreachableNLRI(Mpunlri { af, nlris }),
            optional: true,
            transitive: false,
            partial: false,
            extended_length: false,
        }
    }

    pub fn is_transitive(&self) -> bool {
        self.transitive
    }
}

impl From<Vec<u8>> for PathAttribute {
    fn from(src: Vec<u8>) -> Self {
        let mask = src[0];

        let mask = mask >> 4;
        let extended_length: bool = !matches!(mask & 0b0001, 0);

        let partial: bool = !matches!(mask & 0b0010, 0);

        let transitive: bool = !matches!(mask & 0b0100, 0);

        let optional: bool = !matches!(mask & 0b1000, 0);

        let type_code: PathAttributeType = FromPrimitive::from_u8(src[1]).unwrap();

        let value = match type_code {
            PathAttributeType::Origin => {
                PathAttributeValue::Origin(FromPrimitive::from_u8(src[3]).unwrap())
            }
            PathAttributeType::AsPath => {
                let mut total_len;
                let i;
                match extended_length {
                    false => {
                        total_len = src[2] as usize;
                        i = 3
                    }
                    true => {
                        let mut l = [0u8; 2];
                        l.copy_from_slice(&src[2..4]);
                        total_len = u16::from_be_bytes(l) as usize;
                        i = 4;
                    }
                }
                let mut asp: Aspath = vec![];
                let mut offset = 0;

                while total_len > 0 {
                    let path_type: ASPATHSegmentType =
                        FromPrimitive::from_u8(src[i + offset]).unwrap();
                    let as_list_len = src[i + offset + 1] as usize;
                    // let mut as_list = Box::<Vec<u16>>::new(vec![]);
                    let mut as_list = vec![];

                    for x in 0..as_list_len {
                        let j = i + offset + 2 + x * 2;
                        let mut asn = [0u8; 2];
                        asn.copy_from_slice(&src[j..j + 2]);
                        let asn = u16::from_be_bytes(asn);
                        as_list.push(asn);
                    }

                    // let as_list = Box::leak(as_list).to_vec();
                    let asg = ASPATHSegment { path_type, as_list };

                    asp.push(asg);

                    total_len -= 2 + 2 * as_list_len;
                    offset += 2 + 2 * as_list_len;
                }

                PathAttributeValue::AsPath(asp)
            }
            PathAttributeType::NextHop => {
                PathAttributeValue::NextHop(Ipv4Addr::new(src[3], src[4], src[5], src[6]))
            }
            PathAttributeType::MultiExitDisc => {
                let mut med = [0u8; 4];
                med.copy_from_slice(&src[3..7]);
                let med = u32::from_be_bytes(med);
                PathAttributeValue::MultiExitDisc(med)
            }
            PathAttributeType::LocalPref => {
                let mut lp = [0u8; 4];
                lp.copy_from_slice(&src[3..7]);
                let lp = u32::from_be_bytes(lp);
                PathAttributeValue::LocalPref(lp)
            }
            PathAttributeType::AtomicAggregate => PathAttributeValue::AtomicAggregate,
            PathAttributeType::Aggregator => {
                let mut asn = [0u8; 2];
                asn.copy_from_slice(&src[3..5]);
                let asn = u16::from_be_bytes(asn);
                let ag = AggregatorValue {
                    last_as: asn,
                    aggregator: Ipv4Addr::new(src[5], src[6], src[7], src[8]),
                };
                PathAttributeValue::Aggregator(ag)
            }
            PathAttributeType::Community => PathAttributeValue::Community,
            PathAttributeType::OriginatorId => PathAttributeValue::OriginatorId,
            PathAttributeType::ClusterList => PathAttributeValue::ClusterList,
            PathAttributeType::Dpa => PathAttributeValue::Dpa,
            PathAttributeType::Advertiser => PathAttributeValue::Advertiser,
            PathAttributeType::RcidPathClusterId => PathAttributeValue::RcidPathClusterId,
            PathAttributeType::MPReachableNLRI => {
                PathAttributeValue::MPReachableNLRI(src[2..].to_vec().into())
            }
            PathAttributeType::MPUnreachableNLRI => {
                PathAttributeValue::MPUnreachableNLRI(src[2..].to_vec().into())
            }
            PathAttributeType::ExtCommunities => PathAttributeValue::ExtCommunities,
        };

        PathAttribute {
            optional,
            transitive,
            partial,
            extended_length,
            type_code,
            value,
        }
    }
}

impl From<PathAttribute> for Vec<u8> {
    fn from(val: PathAttribute) -> Self {
        let mut buf: Vec<u8> = vec![];

        let mut mask: u8 = 0;

        if val.extended_length {
            mask += 1;
        }
        if val.partial {
            mask += 2;
        }
        if val.transitive {
            mask += 4;
        }
        if val.optional {
            mask += 8;
        }

        buf.push(mask << 4);
        let code: u8;
        let mut bufval = Cursor::new(vec![]);

        match val.value {
            PathAttributeValue::Origin(value) => {
                code = 1;
                bufval.write_u8(value as u8).unwrap();
            }
            PathAttributeValue::AsPath(value) => {
                code = 2;
                for i in value {
                    let v: Vec<u8> = i.into();
                    bufval.write_all(&v).unwrap();
                }
            }
            PathAttributeValue::NextHop(value) => {
                code = 3;
                bufval.write_u32::<BigEndian>(value.into()).unwrap();
            }
            PathAttributeValue::MultiExitDisc(value) => {
                code = 4;
                bufval.write_u32::<BigEndian>(value).unwrap();
            }
            PathAttributeValue::LocalPref(value) => {
                code = 5;
                bufval.write_u32::<BigEndian>(value).unwrap();
            }
            PathAttributeValue::AtomicAggregate => {
                code = 6;
            }
            PathAttributeValue::Aggregator(value) => {
                code = 7;
                bufval.write_u16::<BigEndian>(value.last_as).unwrap();
                bufval
                    .write_u32::<BigEndian>(value.aggregator.into())
                    .unwrap();
            }
            PathAttributeValue::Community => {
                code = 8;
            }
            PathAttributeValue::OriginatorId => {
                code = 9;
            }
            PathAttributeValue::ClusterList => {
                code = 10;
            }
            PathAttributeValue::Dpa => {
                code = 11;
            }
            PathAttributeValue::Advertiser => {
                code = 12;
            }
            PathAttributeValue::RcidPathClusterId => {
                code = 13;
            }
            PathAttributeValue::MPReachableNLRI(value) => {
                code = 14;
                let v: Vec<u8> = value.into();
                bufval.write_all(&v).unwrap();
            }
            PathAttributeValue::MPUnreachableNLRI(value) => {
                code = 15;
                let v: Vec<u8> = value.into();
                bufval.write_all(&v).unwrap();
            }
            PathAttributeValue::ExtCommunities => {
                code = 16;
            }
        }
        buf.push(code);
        let mut val = bufval.into_inner();
        let len = val.len() as u8;
        buf.push(len);
        buf.append(&mut val);
        buf
    }
}

#![allow(dead_code)]
use byteorder::{BigEndian, WriteBytesExt};
use bytes::{Buf, BytesMut};
use ipnet::IpNet;
use ipnet::Ipv4Net;
use ipnet::Ipv6Net;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde_derive::Deserialize;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
// use std::convert::TryInto;
use std::io::prelude::*;
use std::io::Cursor;
use std::mem::size_of;
use std::net::IpAddr;
use std::result::Result;
use std::{error::Error, fmt};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::codec::{Decoder, Encoder};

use crate::neighbor;

const MARKER: [u8; 16] = [0xff; 16];
const VERSION: u8 = 4;
const MAX: usize = 4096;

#[derive(Debug)]
struct MissingMarker;

impl fmt::Display for MissingMarker {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Message should start with marker")
    }
}

impl Error for MissingMarker {}

#[derive(Error, Debug)]
pub enum BGPError {
    #[error("Builder could not complete")]
    BuilderError,

    #[error("Parameter could not be parsed from bgp message")]
    ParameterParsingError,
    /// Represents an empty source. For example, an empty text file being given
    /// as input to `count_words()`.
    #[error("Source contains no data")]
    EmptySource,

    /// Represents a failure to read from input.
    #[error("Codec error")]
    CodecError { source: std::io::Error },

    /// Represents all other cases of `std::io::Error`.
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

#[derive(Debug, Clone, FromPrimitive, PartialEq, Deserialize, Hash, Eq)]
#[repr(u16)]
pub enum AFI {
    Ipv4 = 1,
    Ipv6,
}

#[derive(Debug, Clone, FromPrimitive, PartialEq, Deserialize, Hash, Eq)]
#[repr(u8)]
pub enum SAFI {
    NLRIUnicast = 1,
    NLRIMulticast,
}

#[derive(Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct AddressFamily {
    pub afi: AFI,
    pub safi: SAFI,
}

#[derive(Debug, Clone, FromPrimitive, PartialEq)]
#[repr(u8)]
pub enum MessageType {
    OPEN = 1,
    UPDATE,
    NOTIFICATION,
    KEEPALIVE,
}

impl Default for MessageType {
    fn default() -> Self {
        MessageType::UPDATE
    }
}

#[derive(Debug, Clone, FromPrimitive)]
#[repr(u8)]
pub enum ErrorCode {
    MessageHeader,
    OpenMessage,
    UpdateMessage,
    HoldTimerExpired,
    FSMError,
    Cease,
}

#[derive(Debug, Clone)]
#[repr(u8)]
enum HeaderSubCode {
    ConnectionNotSynchronized = 1,
    BadMessageLength = 2,
    BadMessageType = 3,
}

#[derive(Debug, Clone)]
#[repr(u8)]
enum OpenSubCode {
    UnsupportedVersionNumber = 1,
    BadPeerAS = 2,
    BadBGPIdentifier = 3,
    UnsupportedOptionalParameter = 4,
    Deprecated = 5,
    UnacceptableHoldTime = 6,
}

#[derive(Debug, Clone)]
#[repr(u8)]
enum UpdateSubCode {
    MalformedAttributeList = 1,
    UnrecognizedWellKnownAttribute = 2,
    MissingWellKnownAttribute = 3,
    AttributeFlagsError = 4,
    AttributeLengthError = 5,
    InvalidORIGINAttribute = 6,
    Deprecated = 7,
    InvalidNEXTHOPAttribute = 8,
    OptionalAttributeError = 9,
    InvalidNetworkField = 10,
    MalformedASPATH = 11,
}

#[derive(Default, Builder, Debug, Clone, PartialEq)]
#[builder(setter(into))]
pub struct BGPMessageHeader {
    // message_length: u16,
    pub message_type: MessageType,
}

#[derive(Default, Builder, Debug, Clone)]
#[builder(setter(into))]
pub struct BGPOpenMessage {
    version: u8,
    pub asn: u16,
    pub hold_time: u16,
    pub router_id: u32,
    pub opt_params: BGPOptionalParameters,
}

impl fmt::Display for BGPOpenMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "version : {} local_asn : {} hold_time : {} router_id : {} opt_params : {:?}",
            self.version,
            self.asn,
            self.hold_time,
            IpAddr::from(Ipv4Addr::from(self.router_id)),
            self.opt_params
        )
    }
}

impl Into<Vec<u8>> for BGPOpenMessage {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        // let len = self.opt_params.len;
        let opt_params: Vec<u8> = self.opt_params.into();
        buf.write(&vec![self.version.clone()]).unwrap();
        buf.write_u16::<BigEndian>(self.asn).unwrap();
        buf.write_u16::<BigEndian>(self.hold_time).unwrap();
        buf.write_u32::<BigEndian>(self.router_id).unwrap();
        // buf.write(&vec![len as u8]).unwrap();
        buf.write(&opt_params).unwrap();
        buf.into_inner()
    }
}
impl From<Vec<u8>> for BGPOpenMessage {
    fn from(src: Vec<u8>) -> Self {
        let mut version = [0u8; 1];
        version.copy_from_slice(&src[0..1]);
        let version = u8::from_be_bytes(version);

        let mut asn = [0u8; 2];
        asn.copy_from_slice(&src[1..3]);
        let asn = u16::from_be_bytes(asn);

        let mut hold = [0u8; 2];
        hold.copy_from_slice(&src[3..5]);
        let hold = u16::from_be_bytes(hold);

        let mut rid = [0u8; 4];
        rid.copy_from_slice(&src[5..9]);
        let rid = u32::from_be_bytes(rid);

        let mut opt_len = [0u8; 1];
        opt_len.copy_from_slice(&src[9..10]);
        // let opt_len = u8::from_be_bytes(opt_len);

        // let tlen = src.len();
        //
        let opt: BGPOptionalParameters = src[9..].to_vec().into();

        BGPOpenMessageBuilder::default()
            .version(version)
            .asn(asn)
            .hold_time(hold)
            .router_id(rid)
            .opt_params(opt)
            .build()
            .unwrap()
    }
}

impl BGPOpenMessage {
    pub fn byte_len(&self) -> usize {
        self.opt_params.len + 10 * size_of::<u16>()
    }

    pub fn new(
        asn: u16,
        rid: u32,
        hold: u16,
        capabilities: neighbor::Capabilities,
    ) -> Result<BGPOpenMessage, String> {
        // let opt: Vec<u8> = match families {
        let families = capabilities.multiprotocol;
        let params: Vec<BGPOptionalParameter> = match families {
            // None => BGPOptionalParameter::default().into(),
            None => vec![BGPOptionalParameter::default()],
            Some(families) => {
                // let mut opt: Vec<u8> = vec![];
                let mut caps: Vec<BGPCapability> = vec![];
                for fam in families {
                    let mp: BGPCapabilityMultiprotocol = BGPCapabilityMultiprotocol {
                        afi: fam.afi,
                        safi: fam.safi,
                    };
                    let mp: Vec<u8> = mp.into();
                    let pc: BGPCapability = BGPCapability {
                        capability_code: BGPCapabilityCode::Multiprotocol,
                        capability_length: mp.len(),
                        capability_value: mp,
                    };
                    caps.push(pc);
                }
                let caps: Vec<Vec<u8>> = caps.into_iter().map(|x| x.into()).collect();
                let caps: Vec<u8> = caps.into_iter().flatten().collect();
                let o = BGPOptionalParameter {
                    param_type: BGPOptionalParameterType::Capability,
                    param_length: caps.len(),
                    param_value: caps,
                };
                vec![o]
            }
        };
        // let opt: Vec<u8> = BGPOptionalParameter::default().into();
        let opt = BGPOptionalParameters::new(params);
        BGPOpenMessageBuilder::default()
            .version(VERSION)
            .asn(asn)
            .hold_time(hold)
            .router_id(rid)
            .opt_params(opt)
            .build()
    }
}

#[derive(Debug, Clone)]
pub struct BGPOptionalParameter {
    param_type: BGPOptionalParameterType,
    param_length: usize,
    param_value: Vec<u8>,
}

impl Default for BGPOptionalParameter {
    fn default() -> Self {
        let cv: BGPCapabilityMultiprotocol = BGPCapabilityMultiprotocol {
            afi: AFI::Ipv4,
            safi: SAFI::NLRIUnicast,
        };
        let cv: Vec<u8> = cv.into();
        let pc: BGPCapability = BGPCapability {
            capability_code: BGPCapabilityCode::Multiprotocol,
            capability_length: cv.len(),
            capability_value: cv,
        };
        let pc: Vec<u8> = pc.into();
        BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: pc.len(),
            param_value: pc,
        }
    }
}

impl From<Vec<u8>> for BGPOptionalParameter {
    fn from(src: Vec<u8>) -> Self {
        let mut ptype = [0u8; 1];
        ptype.copy_from_slice(&src[0..1]);
        let ptype = u8::from_be_bytes(ptype);

        let mut plen = [0u8; 1];
        plen.copy_from_slice(&src[1..2]);
        let plen = u8::from_be_bytes(plen);

        BGPOptionalParameter {
            param_type: BGPOptionalParameterType::from_u8(ptype).unwrap(),
            param_length: plen as usize,
            param_value: src[2..].to_vec(),
        }
    }
}

impl Into<Vec<u8>> for BGPOptionalParameter {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write(&vec![self.param_type.clone() as u8]).unwrap();
        buf.write(&vec![self.param_value.len() as u8]).unwrap();
        buf.write(&self.param_value).unwrap();
        buf.into_inner()
    }
}

#[derive(Debug, Clone)]
pub struct BGPOptionalParameters {
    len: usize,
    params: Vec<BGPOptionalParameter>,
}

impl BGPOptionalParameters {
    fn new(params: Vec<BGPOptionalParameter>) -> BGPOptionalParameters {
        let mut len = 0;
        for p in params.clone() {
            len += 2;
            len += p.param_length as usize;
        }
        BGPOptionalParameters { len, params }
    }
}

impl Default for BGPOptionalParameters {
    fn default() -> Self {
        let p: BGPOptionalParameter = BGPOptionalParameter::default();
        BGPOptionalParameters {
            len: p.param_value[1] as usize + 1,
            params: vec![p],
        }
    }
}

impl Into<Vec<u8>> for BGPOptionalParameters {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write(&vec![self.len.clone() as u8]).unwrap();
        for p in self.params {
            let p: Vec<u8> = p.into();
            buf.write(&p).unwrap();
        }
        buf.into_inner()
    }
}

impl From<Vec<u8>> for BGPOptionalParameters {
    fn from(src: Vec<u8>) -> Self {
        let mut len = [0u8; 1];
        len.copy_from_slice(&src[0..1]);
        let len = u8::from_be_bytes(len);

        let mut wd: Vec<BGPOptionalParameter> = vec![];
        let mut used = 0;
        let mut i = 1;

        while len > used {
            let mut optlen = [0u8; 1];
            optlen.copy_from_slice(&src[i + 1..i + 2]);
            let optlen = u8::from_be_bytes(optlen);
            let end: usize = optlen as usize + 2;

            let n: BGPOptionalParameter = src[i..(i + end)].to_vec().into();
            wd.push(n.clone());
            used += optlen + 2;
            i += optlen as usize + 2;
        }
        BGPOptionalParameters { len: i, params: wd }
    }
}

#[derive(Debug, Clone, FromPrimitive, PartialEq)]
#[repr(u8)]
enum BGPOptionalParameterType {
    Authentication = 1, // deprecated
    Capability = 2,
}

#[derive(Debug, Clone)]
pub struct BGPCapability {
    pub capability_code: BGPCapabilityCode,
    pub capability_length: usize,
    pub capability_value: Vec<u8>,
}

impl From<Vec<u8>> for BGPCapability {
    fn from(src: Vec<u8>) -> Self {
        let mut code = [0u8; 1];
        code.copy_from_slice(&src[0..1]);
        let code = u8::from_be_bytes(code);

        BGPCapability {
            capability_code: BGPCapabilityCode::from_u8(code).unwrap(),
            capability_length: src[2..].to_vec().len(),
            capability_value: src[2..].to_vec(),
        }
    }
}

impl Into<Vec<u8>> for BGPCapability {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write(&vec![self.capability_code.clone() as u8])
            .unwrap();
        // buf.write(&vec![self.capability_value.len() as u8]).unwrap();
        buf.write(&vec![self.capability_length as u8]).unwrap();
        buf.write(&self.capability_value).unwrap();
        buf.into_inner()
    }
}

#[derive(Debug, Clone, FromPrimitive)]
#[repr(u8)]
pub enum BGPCapabilityCode {
    Multiprotocol = 1,
    RouteRefresh = 2,
    OutboundRouteFiltering = 3,
    ExtendedNextHopEncoding = 5,
    GracefulRestart = 64,
    FourOctectASN = 65,
}

#[derive(Debug)]
pub struct BGPCapabilityMultiprotocol {
    afi: AFI,
    safi: SAFI,
}

impl Into<Vec<u8>> for BGPCapabilityMultiprotocol {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write_u16::<BigEndian>(self.afi as u16).unwrap();
        buf.write_u8(0).unwrap();
        buf.write(&vec![self.safi as u8]).unwrap();
        buf.into_inner()
    }
}

#[derive(Debug, Clone, Default)]
pub struct BGPCapabilities {
    len: usize,
    pub params: Vec<BGPCapability>,
}

impl From<BGPOptionalParameters> for BGPCapabilities {
    fn from(src: BGPOptionalParameters) -> Self {
        let p = src
            .params
            .iter()
            .find(|x| x.param_type == BGPOptionalParameterType::Capability);
        match p {
            None => BGPCapabilities::default(),
            Some(v) => {
                let src = &v.param_value;
                let mut i = 0;
                let end = src.len();
                let mut caps = vec![];
                while i < end {
                    let mut t = [0u8; 1];
                    let mut l = [0u8; 1];
                    t.copy_from_slice(&src[i..i + 1]);
                    l.copy_from_slice(&src[i + 1..i + 2]);
                    let y = u8::from_be_bytes(t);
                    let t: Option<BGPCapabilityCode> = FromPrimitive::from_u8(y);
                    let l = u8::from_be_bytes(l);
                    let mut v = vec![];

                    if l > 0 {
                        v.extend_from_slice(&src[i + 2..i + 2 + l as usize]);
                    }

                    match t {
                        None => {
                            println!("\nUnknown BGPCapabilityCode : {:?}\n", y);
                        }
                        Some(t) => {
                            let c = BGPCapability {
                                capability_code: t,
                                capability_length: l as usize,
                                capability_value: v,
                            };
                            caps.push(c);
                        }
                    }

                    i += 2 + l as usize;
                }
                BGPCapabilities {
                    len: v.param_length,
                    params: caps,
                }
            }
        }
    }
}

#[derive(Default, Builder, Debug, Clone, PartialEq)]
#[builder(setter(into))]
pub struct BGPUpdateMessage {
    pub withdrawn_routes: Vec<NLRI>,
    pub path_attributes: Vec<PathAttribute>,
    pub nlri: Vec<NLRI>,
}

impl BGPUpdateMessage {
    pub fn byte_len(&self) -> usize {
        self.withdrawn_routes.len()
            + self.path_attributes.len()
            + self.nlri.len()
            + 2 * size_of::<u16>()
    }

    pub fn new() -> Result<BGPUpdateMessage, String> {
        BGPUpdateMessageBuilder::default().build()
    }
}

impl Into<Vec<u8>> for BGPUpdateMessage {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);

        let mut wd: Vec<u8> = vec![];
        for w in self.withdrawn_routes {
            let mut v: Vec<u8> = w.into();
            wd.append(&mut v);
        }
        buf.write_u16::<BigEndian>(wd.len() as u16).unwrap();
        buf.write(&wd).unwrap();

        let mut pa: Vec<u8> = vec![];
        for a in self.path_attributes {
            let mut v: Vec<u8> = a.into();
            pa.append(&mut v);
        }
        buf.write_u16::<BigEndian>(pa.len() as u16).unwrap();
        buf.write(&pa).unwrap();

        let mut nl: Vec<u8> = vec![];
        for w in self.nlri {
            let mut v: Vec<u8> = w.into();
            nl.append(&mut v);
        }
        buf.write(&nl).unwrap();
        buf.into_inner()
    }
}
impl From<Vec<u8>> for BGPUpdateMessage {
    fn from(src: Vec<u8>) -> Self {
        let mut wdl = [0u8; 2];
        wdl.copy_from_slice(&src[0..2]);
        let wdl = u16::from_be_bytes(wdl) as usize;

        let mut wd: Vec<NLRI> = vec![];
        let mut used = 0;
        let mut i = 2;

        while wdl > used {
            let plen = src[i];
            let end = i + (plen as f32 / 8.0).ceil() as usize + 1;
            let buf = Ipv4Octets {
                octets: src[i..end].to_vec(),
            };
            let n: NLRI = buf.into();
            wd.push(n.clone());
            let blen = ((n.net.prefix_len() as f32 / 8.0).ceil() + 1.0) as usize;
            used += blen;
            i += blen;
        }

        let mut atl = [0u8; 2];
        atl.copy_from_slice(&src[i..i + 2]);
        let atl = u16::from_be_bytes(atl) as usize;

        i += 2;

        let mut pa: Vec<PathAttribute> = vec![];
        let mut used = 0;
        while atl > used {
            let atn = src[i + 2] as usize;
            let n: PathAttribute = src[i..i + 3 + atn].to_vec().into();
            pa.push(n);
            used += 3 + atn;
            i += 3 + atn;
        }

        let total_len = src.len();

        let mut routes: Vec<NLRI> = vec![];
        while i < total_len {
            let plen = src[i];
            let end = i + (plen as f32 / 8.0).ceil() as usize + 1;
            let buf = Ipv4Octets {
                octets: src[i..end].to_vec(),
            };
            let n: NLRI = buf.into();
            routes.push(n.clone());
            let blen = ((n.net.prefix_len() as f32 / 8.0).ceil() + 1.0) as usize;
            i += blen;
        }

        BGPUpdateMessageBuilder::default()
            .withdrawn_routes(wd)
            .path_attributes(pa)
            .nlri(routes)
            .build()
            .unwrap()
    }
}
#[derive(Builder, Debug, PartialEq, Clone)]
#[builder(setter(into))]
pub struct PathAttribute {
    optional: bool,
    transitive: bool,
    partial: bool,
    extended_length: bool,
    pub type_code: PathAttributeType,
    pub value: PathAttributeValue,
}

impl From<Vec<u8>> for PathAttribute {
    fn from(src: Vec<u8>) -> Self {
        let mask = src[0];

        let mask = mask >> 4;
        let extended_length: bool = match mask & 0b0001 {
            0 => false,
            _ => true,
        };

        let partial: bool = match mask & 0b0010 {
            0 => false,
            _ => true,
        };

        let transitive: bool = match mask & 0b0100 {
            0 => false,
            _ => true,
        };

        let optional: bool = match mask & 0b1000 {
            0 => false,
            _ => true,
        };

        let type_code: PathAttributeType = FromPrimitive::from_u8(src[1]).unwrap();

        let value = match type_code {
            PathAttributeType::Origin => {
                PathAttributeValue::Origin(FromPrimitive::from_u8(src[3]).unwrap())
            }
            PathAttributeType::AsPath => {
                let mut total_len = src[2] as usize;
                let mut asp: ASPATH = vec![];
                let i = 3;
                let mut offset = 0;

                while total_len > 0 {
                    let path_type: ASPATHSegmentType =
                        FromPrimitive::from_u8(src[i + offset]).unwrap();
                    let as_list_len = src[i + offset + 1] as usize;
                    let mut as_list = Box::<Vec<u16>>::new(vec![]);

                    for x in 0..as_list_len {
                        let j = i + offset + 2 + x * 2;
                        let mut asn = [0u8; 2];
                        asn.copy_from_slice(&src[j..j + 2]);
                        let asn = u16::from_be_bytes(asn);
                        as_list.push(asn);
                    }

                    let as_list = Box::leak(as_list).to_vec();
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
            PathAttributeType::DPA => PathAttributeValue::DPA,
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

impl Into<Vec<u8>> for PathAttribute {
    fn into(self) -> Vec<u8> {
        let mut buf: Vec<u8> = vec![];

        let mut mask: u8 = 0;

        if self.extended_length {
            mask += 1;
        }
        if self.partial {
            mask += 2;
        }
        if self.transitive {
            mask += 4;
        }
        if self.optional {
            mask += 8;
        }

        buf.push(mask);
        buf.push(0x0);
        let code: u8;
        let mut val = Cursor::new(vec![]);

        match self.value {
            PathAttributeValue::Origin(value) => {
                code = 1;
                val.write_u8(value as u8).unwrap();
            }
            PathAttributeValue::AsPath(value) => {
                code = 2;
                for i in value {
                    let v: Vec<u8> = i.into();
                    val.write(&v).unwrap();
                }
            }
            PathAttributeValue::NextHop(value) => {
                code = 3;
                val.write_u32::<BigEndian>(value.into()).unwrap();
            }
            PathAttributeValue::MultiExitDisc(value) => {
                code = 4;
                val.write_u32::<BigEndian>(value).unwrap();
            }
            PathAttributeValue::LocalPref(value) => {
                code = 5;
                val.write_u32::<BigEndian>(value).unwrap();
            }
            PathAttributeValue::AtomicAggregate => {
                code = 6;
            }
            PathAttributeValue::Aggregator(value) => {
                code = 7;
                val.write_u16::<BigEndian>(value.last_as).unwrap();
                val.write_u32::<BigEndian>(value.aggregator.into()).unwrap();
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
            PathAttributeValue::DPA => {
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
                val.write(&v).unwrap();
            }
            PathAttributeValue::MPUnreachableNLRI(value) => {
                code = 15;
                let v: Vec<u8> = value.into();
                val.write(&v).unwrap();
            }
            PathAttributeValue::ExtCommunities => {
                code = 16;
            }
        }
        buf.push(code);
        let mut val = val.into_inner();
        let len = val.len() as u8;
        buf.push(len);
        buf.append(&mut val);
        buf
    }
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
    DPA,
    Advertiser,
    RcidPathClusterId,
    MPReachableNLRI,
    MPUnreachableNLRI,
    ExtCommunities,
}

#[derive(Debug, PartialEq, Clone)]
pub enum PathAttributeValue {
    Origin(OriginType),
    AsPath(ASPATH),
    NextHop(Ipv4Addr),
    MultiExitDisc(u32),
    LocalPref(u32),
    AtomicAggregate,
    Aggregator(AggregatorValue),
    Community,
    OriginatorId,
    ClusterList,
    DPA,
    Advertiser,
    RcidPathClusterId,
    MPReachableNLRI(MPNLRI),
    MPUnreachableNLRI(MPNLRI),
    ExtCommunities,
}

#[derive(Debug, PartialEq, Eq, Clone, FromPrimitive, PartialOrd, Ord)]
pub enum OriginType {
    IGP = 0,
    EGP,
    INCOMPLETE,
}

#[derive(Debug, Eq, Clone, FromPrimitive)]
enum ASPATHSegmentType {
    AsSet = 1,
    AsSequence,
}

impl PartialEq for ASPATHSegmentType {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ASPATHSegment {
    path_type: ASPATHSegmentType,
    as_list: Vec<u16>,
}

impl Into<Vec<u8>> for ASPATHSegment {
    fn into(self) -> Vec<u8> {
        let mut v: Vec<u8> = vec![];
        v.push(self.path_type as u8);
        v.push(self.as_list.len() as u8);
        let mut buf = Cursor::new(vec![]);
        for asn in self.as_list {
            buf.write_u16::<BigEndian>(asn).unwrap();
        }
        let mut buf = buf.into_inner();
        v.append(&mut buf);
        v
    }
}

impl ASPATHSegment {
    pub fn len(&self) -> usize {
        match &self.path_type {
            ASPATHSegmentType::AsSequence => self.as_list.len(),
            ASPATHSegmentType::AsSet => 1,
        }
    }
}

pub type ASPATH = Vec<ASPATHSegment>;

pub trait Flatten {
    fn flatten_aspath(&self) -> Vec<u16>;
}

impl Flatten for ASPATH {
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
    last_as: u16,
    aggregator: Ipv4Addr,
}

#[derive(Builder, Debug, Clone, PartialEq, Eq, Hash)]
#[builder(setter(into))]
pub struct NLRI {
    net: IpNet,
}

pub struct Ipv4Octets {
    octets: Vec<u8>,
}

pub struct Ipv6Octets {
    octets: Vec<u8>,
}

impl Into<Vec<u8>> for NLRI {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write_u8(self.net.prefix_len()).unwrap();
        let blen = (self.net.prefix_len() as f32 / 8.0).ceil() as usize;
        match self.net {
            IpNet::V4(v) => {
                let addrv4: u32 = v.network().into();
                let addr = addrv4.to_be_bytes();
                buf.write(&addr[0..blen]).unwrap();
            }
            IpNet::V6(v) => {
                let addrv6: u128 = v.network().into();
                let addr = addrv6.to_be_bytes();
                buf.write(&addr[0..blen]).unwrap();
            }
        }
        buf.into_inner()
    }
}

impl Into<IpNet> for NLRI {
    fn into(self) -> IpNet {
        self.net.into()
    }
}

impl Into<IpNet> for &NLRI {
    fn into(self) -> IpNet {
        self.net.into()
    }
}

impl From<Ipv4Octets> for NLRI {
    fn from(src: Ipv4Octets) -> Self {
        let mut addr = src.octets;
        let plen = addr.remove(0);
        addr.resize(4, 0);
        let net = Ipv4Net::new(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]), plen).unwrap();
        NLRIBuilder::default().net(net).build().unwrap()
    }
}

impl From<Ipv6Octets> for NLRI {
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
        NLRIBuilder::default().net(net).build().unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MPNLRI {
    pub af: AddressFamily,
    // pub afi: AFI,
    // pub safi: SAFI,
    pub nh: IpAddr,
    pub nlris: Vec<NLRI>,
}

impl Default for MPNLRI {
    fn default() -> Self {
        MPNLRI {
            af: AddressFamily {
                afi: AFI::Ipv6,
                safi: SAFI::NLRIUnicast,
            },
            nh: IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)),
            nlris: vec![],
        }
    }
}

impl From<Vec<u8>> for MPNLRI {
    fn from(src: Vec<u8>) -> Self {
        let mut src = src;
        let total_len = src.remove(0) as usize;

        let mut afi = [0u8; 2];
        afi.copy_from_slice(&src[0..2]);
        let afi = u16::from_be_bytes(afi);
        let afi: AFI = FromPrimitive::from_u16(afi).unwrap();

        let mut safi = [0u8; 1];
        safi.copy_from_slice(&src[2..3]);
        let safi = u8::from_be_bytes(safi);
        let safi: SAFI = FromPrimitive::from_u8(safi).unwrap();

        let mut nhl = [0u8; 1];
        nhl.copy_from_slice(&src[3..4]);
        let nhl: usize = u8::from_be_bytes(nhl).into();

        let mut addr: Vec<u8> = vec![0; nhl];
        addr.copy_from_slice(&src[4..4 + nhl]);

        let nh: IpAddr;
        let mut i = 4 + nhl + 1;

        let mut nlris: Vec<NLRI> = vec![];
        match afi {
            AFI::Ipv4 => {
                addr.resize(4, 0);
                nh = IpAddr::V4(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]));

                while i < total_len {
                    let plen = src[i];
                    let end = i + (plen as f32 / 8.0).ceil() as usize + 1;
                    let buf = Ipv4Octets {
                        octets: src[i..end].to_vec(),
                    };
                    let n: NLRI = buf.into();
                    nlris.push(n.clone());
                    let blen = ((n.net.prefix_len() as f32 / 8.0).ceil() + 1.0) as usize;
                    i += blen;
                }
            }
            AFI::Ipv6 => {
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
                    let n: NLRI = buf.into();
                    nlris.push(n.clone());
                    let blen = ((n.net.prefix_len() as f32 / 8.0).ceil() + 1.0) as usize;
                    i += blen;
                }
            }
        }
        let af = AddressFamily { afi, safi };
        MPNLRI {
            af,
            // afi,
            // safi,
            nh,
            nlris,
        }
    }
}

impl Into<Vec<u8>> for MPNLRI {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        let mut blen = 3;
        buf.write(&vec![blen as u8]).unwrap();
        buf.write_u16::<BigEndian>((self.af.afi as u16).into())
            .unwrap();
        buf.write(&vec![self.af.safi as u8]).unwrap();
        match self.nh {
            IpAddr::V4(v) => {
                buf.write(&vec![4 as u8]).unwrap();
                let addrv4: u32 = v.into();
                let addr = addrv4.to_be_bytes();
                buf.write(&addr[0..4]).unwrap();
                blen += 5;
            }
            IpAddr::V6(v) => {
                buf.write(&vec![16 as u8]).unwrap();
                let addrv6: u128 = v.into();
                let addr = addrv6.to_be_bytes();
                buf.write(&addr[0..16]).unwrap();
                blen += 17;
            }
        }
        for n in self.nlris {
            let nbuf: Vec<u8> = n.into();
            blen += nbuf.len();
            buf.write(&nbuf).unwrap();
        }
        buf.rewind().unwrap();
        buf.write(&vec![blen as u8]).unwrap();
        buf.into_inner()
    }
}

#[derive(Builder, Debug, Clone)]
#[builder(setter(into))]
pub struct BGPNotificationMessage {
    error_code: ErrorCode,
    error_subcode: u8,
    data: Vec<u8>,
}

impl BGPNotificationMessage {
    pub fn byte_len(&self) -> usize {
        2 + self.data.len()
    }

    pub fn new(code: ErrorCode, sub: usize) -> Result<BGPNotificationMessage, String> {
        BGPNotificationMessageBuilder::default()
            .error_code(code)
            .error_subcode(sub as u8)
            .build()
    }
}
impl From<Vec<u8>> for BGPNotificationMessage {
    fn from(src: Vec<u8>) -> Self {
        let e: ErrorCode = FromPrimitive::from_u8(src[0]).unwrap();
        BGPNotificationMessageBuilder::default()
            .error_code(e)
            .error_subcode(src[1])
            .data(vec![])
            .build()
            .unwrap()
    }
}

impl Into<Vec<u8>> for BGPNotificationMessage {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write(&vec![self.error_code.clone() as u8]).unwrap();
        buf.write(&vec![self.error_subcode.clone()]).unwrap();
        buf.write(&self.data).unwrap();
        buf.into_inner()
    }
}

#[derive(Default, Builder, Debug, Clone)]
#[builder(setter(into))]
pub struct BGPKeepaliveMessage {}

impl BGPKeepaliveMessage {
    pub fn byte_len(&self) -> u16 {
        0
    }

    pub fn new() -> std::result::Result<BGPKeepaliveMessage, String> {
        BGPKeepaliveMessageBuilder::default().build()
    }
}

impl Into<Vec<u8>> for BGPKeepaliveMessage {
    fn into(self) -> Vec<u8> {
        vec![]
    }
}

pub struct BGPMessageCodec;
pub type BGPConnection = Framed<TcpStream, BGPMessageCodec>;

impl BGPMessageCodec {
    pub async fn frame_it(socket: TcpStream) -> Result<BGPConnection, std::io::Error> {
        let server = Framed::new(socket, BGPMessageCodec);
        Ok(server)
    }
}

impl Decoder for BGPMessageCodec {
    type Item = Vec<u8>;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 19 {
            return Ok(None);
        }
        if !src.starts_with(&MARKER) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Message should start with marker",),
            ));
        }
        let mut length_bytes = [0u8; 2];
        length_bytes.copy_from_slice(&src[16..18]);
        let length = u16::from_be_bytes(length_bytes) as usize;
        if length > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", length),
            ));
        }

        let data = src[0..length].to_vec();
        src.advance(length);

        Ok(Some(data))
    }
}

impl Encoder<Vec<u8>> for BGPMessageCodec {
    type Error = std::io::Error;

    fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> Result<(), Self::Error> {
        if data.len() + MARKER.len() > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", data.len()),
            ));
        }
        let len_slice = u16::to_be_bytes(data.len() as u16 + MARKER.len() as u16 + 2);
        buf.reserve(MARKER.len() + 2 + data.len());
        buf.extend_from_slice(&MARKER);
        buf.extend_from_slice(&len_slice);
        buf.extend_from_slice(data.as_slice());
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum BGPMessageBody {
    Open(BGPOpenMessage),
    Update(BGPUpdateMessage),
    Notification(BGPNotificationMessage),
    Keepalive(BGPKeepaliveMessage),
}

impl Default for BGPMessageBody {
    fn default() -> Self {
        let msg = BGPKeepaliveMessage::new().unwrap();
        Self::Keepalive(msg)
    }
}
impl Into<Vec<u8>> for BGPMessageBody {
    fn into(self) -> Vec<u8> {
        match self {
            Self::Open(body) => body.into(),
            Self::Update(body) => body.into(),
            Self::Notification(body) => body.into(),
            Self::Keepalive(body) => body.into(),
        }
    }
}

#[derive(Default, Builder, Debug)]
#[builder(setter(into))]
pub struct Message {
    pub header: BGPMessageHeader,
    pub body: BGPMessageBody,
}

impl From<Vec<u8>> for Message {
    fn from(src: Vec<u8>) -> Self {
        let mut mtype = [0u8; 1];
        mtype.copy_from_slice(&src[18..19]);
        let mtype = MessageType::from_u8(mtype[0]).unwrap();
        let header = BGPMessageHeaderBuilder::default()
            .message_type(mtype.clone())
            .build()
            .unwrap();
        let mut length_bytes = [0u8; 2];
        length_bytes.copy_from_slice(&src[16..18]);
        let srclength = src.len();
        let v = src[19..srclength].to_vec();
        let body = match mtype {
            MessageType::OPEN => {
                let msg: BGPOpenMessage = v.into();
                BGPMessageBody::Open(msg)
            }
            MessageType::UPDATE => {
                let msg: BGPUpdateMessage = v.into();
                BGPMessageBody::Update(msg)
            }
            MessageType::NOTIFICATION => {
                let msg: BGPNotificationMessage = v.into();
                BGPMessageBody::Notification(msg)
            }
            MessageType::KEEPALIVE => {
                let msg = BGPKeepaliveMessage::new().unwrap();
                BGPMessageBody::Keepalive(msg)
            }
        };

        MessageBuilder::default()
            .header(header)
            .body(body)
            .build()
            .unwrap()
    }
}
impl Into<Vec<u8>> for Message {
    fn into(self) -> Vec<u8> {
        // 3 is the static number of bytes in a bgp header msg
        // let len: u16 = (MARKER.len() + 3 + self.body.len()) as u16;

        // let mut buf = Cursor::new(MARKER.to_vec());
        let mut buf = Cursor::new(vec![]);
        // let _ = buf.seek(SeekFrom::End(0));
        // buf.write_u16::<BigEndian>(len).unwrap();
        buf.write_u8(self.header.message_type.clone() as u8)
            .unwrap();

        let v: Vec<u8> = self.body.into();
        buf.write(&v[0..]).unwrap();
        buf.into_inner()
    }
}

impl Message {
    pub fn new(
        mtype: MessageType,
        body: BGPMessageBody,
    ) -> Result<Message, Box<dyn Error + Sync + Send>> {
        let header = BGPMessageHeaderBuilder::default()
            .message_type(mtype.clone())
            .build()?;
        Ok(MessageBuilder::default()
            .header(header)
            .body(body)
            .build()?)
    }

    fn add_marker(buf: &mut Vec<u8>) {
        let mut marker = MARKER.to_vec();
        buf.append(&mut marker)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_u8_to_update() {
        let v: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x14, 0x40, 0x01, 0x01, 0x00, 0x40, 0x02, 0x06, 0x02, 0x02, 0xfe,
            0xb0, 0xfe, 0x4c, 0x40, 0x03, 0x04, 0x02, 0x02, 0x02, 0x02, 0x18, 0x0a, 0x0a, 0x01,
            0x18, 0x0a, 0x0a, 0x02, 0x18, 0x0a, 0x0a, 0x03,
        ];
        let u: BGPUpdateMessage = v.into();
        let wdr = vec![];
        let pa: Vec<PathAttribute> = [
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::Origin,
                value: PathAttributeValue::Origin(OriginType::IGP),
            },
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::AsPath,
                value: PathAttributeValue::AsPath(
                    [ASPATHSegment {
                        path_type: ASPATHSegmentType::AsSequence,
                        as_list: [65200, 65100].to_vec(),
                    }]
                    .to_vec(),
                ),
            },
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::NextHop,
                value: PathAttributeValue::NextHop("2.2.2.2".parse().unwrap()),
            },
        ]
        .to_vec();
        let nlri: Vec<NLRI> = [
            NLRI {
                net: "10.10.1.0/24".parse().unwrap(),
            },
            NLRI {
                net: "10.10.2.0/24".parse().unwrap(),
            },
            NLRI {
                net: "10.10.3.0/24".parse().unwrap(),
            },
        ]
        .to_vec();
        let w = BGPUpdateMessageBuilder::default()
            .withdrawn_routes(wdr)
            .path_attributes(pa)
            .nlri(nlri)
            .build()
            .unwrap();
        assert_eq!(u, w);
    }

    #[test]
    fn test_u8_to_update_med() {
        let v: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x19, 0x40, 0x01, 0x01, 0x00, 0x40, 0x02, 0x04, 0x02, 0x01, 0x00,
            0xc8, 0x40, 0x03, 0x04, 0x0a, 0x01, 0x0c, 0x02, 0x80, 0x04, 0x04, 0x00, 0x00, 0x00,
            0xf2, 0x08, 0x02,
        ];
        let u: BGPUpdateMessage = v.into();
        let pa: Vec<PathAttribute> = [
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::Origin,
                value: PathAttributeValue::Origin(OriginType::IGP),
            },
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::AsPath,
                value: PathAttributeValue::AsPath(
                    [ASPATHSegment {
                        path_type: ASPATHSegmentType::AsSequence,
                        as_list: [200].to_vec(),
                    }]
                    .to_vec(),
                ),
            },
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::NextHop,
                value: PathAttributeValue::NextHop("10.1.12.2".parse().unwrap()),
            },
            PathAttribute {
                optional: true,
                transitive: false,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::MultiExitDisc,
                value: PathAttributeValue::MultiExitDisc(242),
            },
        ]
        .to_vec();
        let nlri: Vec<NLRI> = [NLRI {
            net: "2.0.0.0/8".parse().unwrap(),
        }]
        .to_vec();
        let w = BGPUpdateMessageBuilder::default()
            .withdrawn_routes(vec![])
            .path_attributes(pa)
            .nlri(nlri)
            .build()
            .unwrap();
        assert_eq!(u, w);
    }

    #[test]
    fn test_u8_to_update_asset() {
        let v: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x28, 0x40, 0x01, 0x01, 0x02, 0x40, 0x02, 0x0a, 0x02, 0x01, 0x00,
            0x1e, 0x01, 0x02, 0x00, 0x0a, 0x00, 0x14, 0x40, 0x03, 0x04, 0x0a, 0x00, 0x00, 0x09,
            0x80, 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x07, 0x06, 0x00, 0x1e, 0x0a, 0x00,
            0x00, 0x09, 0x15, 0xac, 0x10, 0x00,
        ];
        let u: BGPUpdateMessage = v.into();
        let pa: Vec<PathAttribute> = [
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::Origin,
                value: PathAttributeValue::Origin(OriginType::INCOMPLETE),
            },
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::AsPath,
                value: PathAttributeValue::AsPath(
                    [
                        ASPATHSegment {
                            path_type: ASPATHSegmentType::AsSequence,
                            as_list: [30].to_vec(),
                        },
                        ASPATHSegment {
                            path_type: ASPATHSegmentType::AsSet,
                            as_list: [10, 20].to_vec(),
                        },
                    ]
                    .to_vec(),
                ),
            },
            PathAttribute {
                optional: false,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::NextHop,
                value: PathAttributeValue::NextHop("10.0.0.9".parse().unwrap()),
            },
            PathAttribute {
                optional: true,
                transitive: false,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::MultiExitDisc,
                value: PathAttributeValue::MultiExitDisc(0),
            },
            PathAttribute {
                optional: true,
                transitive: true,
                partial: false,
                extended_length: false,
                type_code: PathAttributeType::Aggregator,
                value: PathAttributeValue::Aggregator(AggregatorValue {
                    last_as: 30,
                    aggregator: "10.0.0.9".parse().unwrap(),
                }),
            },
        ]
        .to_vec();
        let nlri: Vec<NLRI> = [NLRI {
            net: "172.16.0.0/21".parse().unwrap(),
        }]
        .to_vec();
        let w = BGPUpdateMessageBuilder::default()
            .withdrawn_routes(vec![])
            .path_attributes(pa)
            .nlri(nlri)
            .build()
            .unwrap();
        assert_eq!(u, w);
    }

    #[test]
    fn test_u8_to_nlri1() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 0), 24).unwrap();
        let n = NLRI {
            net: IpNet::V4(net),
        };
        let v = Ipv4Octets {
            octets: vec![24, 192, 168, 1],
        };
        let u: NLRI = v.into();

        assert_eq!(n, u);
    }

    #[test]
    fn test_u8_to_nlri2() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 1), 32).unwrap();
        let n = NLRI {
            net: IpNet::V4(net),
        };
        let v = Ipv4Octets {
            octets: vec![32, 192, 168, 1, 1],
        };
        let u: NLRI = v.into();

        assert_eq!(n, u);
    }

    #[test]
    fn test_u8_to_nlri3() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 128), 25).unwrap();
        let n = NLRI {
            net: IpNet::V4(net),
        };
        let v = Ipv4Octets {
            octets: vec![25, 192, 168, 1, 128],
        };
        let u: NLRI = v.into();

        assert_eq!(n, u);
    }

    #[test]
    fn test_into_nlri_24() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 0), 24).unwrap();
        let n = NLRI {
            net: IpNet::V4(net),
        };
        let v = Ipv4Octets {
            octets: vec![24, 192, 168, 1],
        };
        let n1: NLRI = v.into();
        assert_eq!(n.net, n1.net);
    }

    #[test]
    fn test_into_nlri_32() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 1), 32).unwrap();
        let n = NLRI {
            net: IpNet::V4(net),
        };
        let v = Ipv4Octets {
            octets: vec![32, 192, 168, 1, 1],
        };
        let n1: NLRI = v.into();
        assert_eq!(n.net, n1.net);
    }

    #[test]
    fn test_into_nlri_25() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 128), 25).unwrap();
        let n = NLRI {
            net: IpNet::V4(net),
        };
        let v = Ipv4Octets {
            octets: vec![25, 192, 168, 1, 128],
        };
        let n1: NLRI = v.into();
        assert_eq!(n.net, n1.net);
    }

    #[test]
    fn test_opt_params() {
        let mut plist: Vec<BGPOptionalParameter> = vec![];
        let cv: BGPCapabilityMultiprotocol = BGPCapabilityMultiprotocol {
            afi: AFI::Ipv4,
            safi: SAFI::NLRIUnicast,
        };
        let cv: Vec<u8> = cv.into();
        let pc: BGPCapability = BGPCapability {
            capability_code: BGPCapabilityCode::Multiprotocol,
            capability_length: cv.len(),
            capability_value: cv,
        };
        let pc: Vec<u8> = pc.into();
        let p1: BGPOptionalParameter = BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: pc.len(),
            param_value: pc,
        };

        plist.push(p1);

        let mut v: Vec<u8> = vec![];

        for param in plist {
            let mut p: Vec<u8> = param.try_into().unwrap();
            v.append(&mut p);
        }

        let u: Vec<u8> = vec![0x2, 0x6, 0x1, 0x4, 0x0, 0x1, 0x0, 0x1];
        assert_eq!(v, u)
    }

    #[test]
    fn test_from_primitives() {
        let t = MessageType::OPEN;
        let u: MessageType = FromPrimitive::from_u64(1).unwrap();
        assert_eq!(t, u)
    }

    #[test]
    fn test_keepalive_message() {
        let body = BGPKeepaliveMessage::new().unwrap();
        let test_msg: Vec<u8> =
            Message::new(MessageType::KEEPALIVE, BGPMessageBody::Keepalive(body))
                .unwrap()
                .try_into()
                .unwrap();
        let keepalive: Vec<u8> = vec![0x4];
        assert_eq!(test_msg, keepalive)
    }
    #[test]
    fn test_open_message() {
        let body = BGPOpenMessage::new(123, 345, 3, neighbor::Capabilities::default()).unwrap();
        let test_msg: Vec<u8> = Message::new(MessageType::OPEN, BGPMessageBody::Open(body))
            .unwrap()
            .try_into()
            .unwrap();
        let open: Vec<u8> = vec![
            0x1, 0x4, 0x0, 0x7b, 0x0, 0x3, 0x0, 0x0, 0x1, 0x59, 0x8, 0x2, 0x6, 0x1, 0x4, 0x0, 0x1,
            0x0, 0x1,
        ];
        assert_eq!(test_msg, open)
    }
}

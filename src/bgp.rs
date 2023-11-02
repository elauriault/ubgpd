#![allow(dead_code)]
use byteorder::{BigEndian, WriteBytesExt};
use bytes::{Buf, BytesMut};
use ipnet::IpNet;
use ipnet::Ipv4Net;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde_derive::Deserialize;
// use std::convert::TryInto;
use std::io::prelude::*;
use std::io::Cursor;
use std::mem::size_of;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::result::Result;
use std::{error::Error, fmt};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::codec::{Decoder, Encoder};

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

#[derive(Debug, Clone, FromPrimitive, PartialEq, Deserialize)]
#[repr(u8)]
pub enum AFI {
    Ipv4 = 1,
    Ipv6,
}

#[derive(Debug, Clone, FromPrimitive, PartialEq, Deserialize)]
#[repr(u8)]
pub enum SAFI {
    NLRIUnicast = 1,
    NLRIMulticast,
}

#[derive(Deserialize, Debug, Clone)]
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
    // opt_param_length: u8,
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
        let len = self.opt_params.len;
        let opt_params: Vec<u8> = self.opt_params.into();
        buf.write(&vec![self.version.clone()]).unwrap();
        buf.write_u16::<BigEndian>(self.asn).unwrap();
        buf.write_u16::<BigEndian>(self.hold_time).unwrap();
        buf.write_u32::<BigEndian>(self.router_id).unwrap();
        buf.write(&vec![len as u8]).unwrap();
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
        let opt: BGPOptionalParameters = src[10..].to_vec().into();

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
        families: Option<Vec<AddressFamily>>,
    ) -> Result<BGPOpenMessage, String> {
        // let opt: Vec<u8> = match families {
        let params: Vec<BGPOptionalParameter> = match families {
            // None => BGPOptionalParameter::default().into(),
            None => vec![BGPOptionalParameter::default()],
            Some(families) => {
                // let mut opt: Vec<u8> = vec![];
                let mut caps: Vec<BGPCapability> = vec![];
                for fam in families {
                    let cv: BGPCapabilityMultiprotocol = BGPCapabilityMultiprotocol {
                        afi: fam.afi,
                        safi: fam.safi,
                    };
                    let pc: BGPCapability = BGPCapability {
                        capability_code: BGPCapabilityCode::Multiprotocol,
                        capability_value: cv.into(),
                    };
                    caps.push(pc);
                }
                let a: Vec<Vec<u8>> = caps.into_iter().map(|x| x.into()).collect();
                let o = BGPOptionalParameter {
                    param_type: BGPOptionalParameterType::Capability,
                    param_value: a.into_iter().flatten().collect(),
                };
                vec![o]
            }
        };
        // let opt: Vec<u8> = BGPOptionalParameter::default().into();
        let mut len = 0;
        for p in params.clone() {
            len += 2;
            len += p.param_value[1] as usize;
        }
        let opt = BGPOptionalParameters { len, params };
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
    // param_length: u8,
    param_value: Vec<u8>,
}

impl Default for BGPOptionalParameter {
    fn default() -> Self {
        let cv: BGPCapabilityMultiprotocol = BGPCapabilityMultiprotocol {
            afi: AFI::Ipv4,
            safi: SAFI::NLRIUnicast,
        };
        let pc: BGPCapability = BGPCapability {
            capability_code: BGPCapabilityCode::Multiprotocol,
            capability_value: cv.into(),
        };
        BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_value: pc.into(),
        }
    }
}

impl From<Vec<u8>> for BGPOptionalParameter {
    fn from(src: Vec<u8>) -> Self {
        let mut ptype = [0u8; 1];
        ptype.copy_from_slice(&src[0..1]);
        let ptype = u8::from_be_bytes(ptype);

        BGPOptionalParameter {
            param_type: BGPOptionalParameterType::from_u8(ptype).unwrap(),
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
        // Need to add self.params[]
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
            // println!("WD : {:?}", n);
            wd.push(n.clone());
            used += optlen + 2;
            i += optlen as usize + 2;
        }
        BGPOptionalParameters { len: i, params: wd }
    }
}

#[derive(Debug, Clone, FromPrimitive)]
#[repr(u8)]
enum BGPOptionalParameterType {
    Authentication = 1, // deprecated
    Capability = 2,
}

#[derive(Debug)]
pub struct BGPCapability {
    capability_code: BGPCapabilityCode,
    // param_length: u8,
    capability_value: Vec<u8>,
}

impl From<Vec<u8>> for BGPCapability {
    fn from(src: Vec<u8>) -> Self {
        let mut code = [0u8; 1];
        code.copy_from_slice(&src[0..1]);
        let code = u8::from_be_bytes(code);

        BGPCapability {
            capability_code: BGPCapabilityCode::from_u8(code).unwrap(),
            capability_value: src[2..].to_vec(),
        }
    }
}

impl Into<Vec<u8>> for BGPCapability {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write(&vec![self.capability_code.clone() as u8])
            .unwrap();
        buf.write(&vec![self.capability_value.len() as u8]).unwrap();
        buf.write(&self.capability_value).unwrap();
        buf.into_inner()
    }
}

#[derive(Debug, Clone, FromPrimitive)]
#[repr(u8)]
enum BGPCapabilityCode {
    Multiprotocol = 1,
    RouteRefresh = 2,
    OutboundRouteFiltering = 3,
    GracefulRestart = 64,
    FourOctectASN = 65,
}

#[derive(Debug)]
pub struct BGPCapabilityMultiprotocol {
    afi: AFI,
    safi: SAFI,
}

#[derive(Debug)]
pub struct BGPCapabilityRouteRefresh {
    supported: bool,
}

#[derive(Debug)]
pub struct BGPCapabilityOutboundRouteFiltering {
    supported: bool,
}

#[derive(Debug)]
pub struct BGPCapabilityGracefulRestart {
    supported: bool,
}

#[derive(Debug)]
pub struct BGPCapabilityFourOctectASN {
    supported: bool,
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

        // println!("wdl is {}", wdl);

        let mut wd: Vec<NLRI> = vec![];
        let mut used = 0;
        let mut i = 2;

        while wdl > used {
            let n: NLRI = src[i..i + 4].to_vec().into();
            // println!("WD : {:?}", n);
            wd.push(n.clone());
            let blen = ((n.net.prefix_len() as f32 / 8.0).ceil() + 1.0) as usize;
            used += blen;
            i += blen;
        }

        let mut atl = [0u8; 2];
        atl.copy_from_slice(&src[i..i + 2]);
        let atl = u16::from_be_bytes(atl) as usize;
        // println!("atl is {}", atl);

        i += 2;

        // println!("i is {}", i);

        let mut pa: Vec<PathAttribute> = vec![];
        let mut used = 0;
        while atl > used {
            let atn = src[i + 2] as usize;
            let n: PathAttribute = src[i..i + 3 + atn].to_vec().into();
            // println!("PathAttribute : {:?}", n);
            pa.push(n);
            used += 3 + atn;
            i += 3 + atn;
        }

        let total_len = src.len();

        let mut routes: Vec<NLRI> = vec![];
        while i < total_len {
            // println!("i : {:?}", i);
            let n: NLRI = src[i..].to_vec().into();
            // println!("NLRI : {:?}", n);
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
    type_code: PathAttributeType,
    pub value: PathAttributeValue,
}

impl From<Vec<u8>> for PathAttribute {
    fn from(src: Vec<u8>) -> Self {
        let mask = src[0];

        // println!("mask is {:#x}", mask);

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

        // println!("type_code is {:#x}, {:?}", src[1], type_code);

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
            PathAttributeType::MPReachableNLRI => PathAttributeValue::MPReachableNLRI,
            PathAttributeType::MPUnreachableNLRI => PathAttributeValue::MPUnreachableNLRI,
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
            PathAttributeValue::MPReachableNLRI => {
                code = 14;
            }
            PathAttributeValue::MPUnreachableNLRI => {
                code = 15;
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
    MPReachableNLRI,
    MPUnreachableNLRI,
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

#[derive(Debug, PartialEq, Clone)]
pub struct AggregatorValue {
    last_as: u16,
    aggregator: Ipv4Addr,
}

#[derive(Builder, Debug, Clone, PartialEq, Eq, Hash)]
#[builder(setter(into))]
pub struct NLRI {
    net: Ipv4Net,
}

impl Into<Vec<u8>> for NLRI {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write_u8(self.net.prefix_len()).unwrap();
        let addr: u32 = self.net.network().into();
        let addr = addr.to_be_bytes();
        let blen = (self.net.prefix_len() as f32 / 8.0).ceil() as usize;
        buf.write(&addr[0..blen]).unwrap();
        buf.into_inner()
    }
}
impl Into<Ipv4Net> for NLRI {
    fn into(self) -> Ipv4Net {
        self.net
    }
}

impl Into<IpNet> for NLRI {
    fn into(self) -> IpNet {
        self.net.into()
    }
}

impl From<Vec<u8>> for NLRI {
    fn from(src: Vec<u8>) -> Self {
        let mut addr = src;
        let plen = addr.remove(0);
        // println!("plen {:?}", plen);
        let blen = (plen as f32 / 8.0).ceil() as usize;
        // println!("blen {:?}", blen);
        let mut t: Vec<u8> = vec![0; 4 - blen];
        addr.append(&mut t);
        let net = Ipv4Net::new(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]), plen).unwrap();
        NLRIBuilder::default().net(net).build().unwrap()
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
        // println!("{:?}", buf);
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
        // let w: BGPUpdateMessage = {
        //         withdrawn_routes: NLRi =[],
        //     path_attributes: [
        //         PathAttribute { optional: false, transitive: true, partial: false, extended_length: false, value: Origin(IGP) },
        //         PathAttribute { optional: false, transitive: true, partial: false, extended_length: false, value: AsPath([ASPATHSegment { path_type: AsSequence, as_list: [65200, 65100] }]) },
        //         PathAttribute { optional: false, transitive: true, partial: false, extended_length: false, value: NextHop(2.2.2.2) }
        //     ],
        //     nlri: [
        //         NLRI { net: 10.10.1.24/24 },
        //         NLRI { net: 10.10.2.24/24 },
        //         NLRI { net: 10.10.3.0/24 }
        // };
        let w = BGPUpdateMessageBuilder::default()
            .withdrawn_routes(vec![])
            .path_attributes(vec![])
            .nlri(vec![])
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
        let w = BGPUpdateMessageBuilder::default()
            .withdrawn_routes(vec![])
            .path_attributes(vec![])
            .nlri(vec![])
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
        let w = BGPUpdateMessageBuilder::default()
            .withdrawn_routes(vec![])
            .path_attributes(vec![])
            .nlri(vec![])
            .build()
            .unwrap();
        assert_eq!(u, w);
    }

    #[test]
    fn test_u8_to_nlri1() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 0), 24).unwrap();
        let n = NLRI { net };
        let v: Vec<u8> = vec![24, 192, 168, 1];
        let u: NLRI = v.into();

        assert_eq!(n, u);
    }

    #[test]
    fn test_u8_to_nlri2() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 1), 32).unwrap();
        let n = NLRI { net };
        let v: Vec<u8> = vec![32, 192, 168, 1, 1];
        let u: NLRI = v.into();

        assert_eq!(n, u);
    }

    #[test]
    fn test_u8_to_nlri3() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 128), 25).unwrap();
        let n = NLRI { net };
        let v: Vec<u8> = vec![25, 192, 168, 1, 128];
        let u: NLRI = v.into();

        assert_eq!(n, u);
    }

    #[test]
    fn test_into_nlri_24() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 0), 24).unwrap();
        let n = NLRI { net };
        let v: Vec<u8> = vec![24, 192, 168, 1];
        let n1: NLRI = v.into();
        assert_eq!(n.net, n1.net);
    }

    #[test]
    fn test_into_nlri_32() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 1), 32).unwrap();
        let n = NLRI { net };
        let v: Vec<u8> = vec![32, 192, 168, 1, 1];
        let n1: NLRI = v.into();
        assert_eq!(n.net, n1.net);
    }

    #[test]
    fn test_into_nlri_25() {
        let net = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 128), 25).unwrap();
        let n = NLRI { net };
        let v: Vec<u8> = vec![25, 192, 168, 1, 128];
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
        let pc: BGPCapability = BGPCapability {
            capability_code: BGPCapabilityCode::Multiprotocol,
            capability_value: cv.try_into().unwrap(),
        };
        let p1: BGPOptionalParameter = BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_value: pc.try_into().unwrap(),
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
        let body = BGPOpenMessage::new(123, 345, 3, None).unwrap();
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

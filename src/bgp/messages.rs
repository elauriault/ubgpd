use anyhow::Result;
use byteorder::{BigEndian, WriteBytesExt};
use derive_builder::Builder;
use num_traits::FromPrimitive;
use std::fmt;
use std::io::prelude::*;
use std::io::Cursor;
use std::mem::size_of;
use std::net::IpAddr;

use crate::neighbor;

use super::attributes::*;
use super::capabilities::*;
use super::nlri::*;
use super::types::*;

#[derive(Default, Builder, Debug, Clone, PartialEq)]
#[builder(setter(into))]
pub struct BGPMessageHeader {
    pub message_type: MessageType,
}

#[derive(Default, Builder, Debug, Clone)]
#[builder(setter(into))]
pub struct BGPOpenMessage {
    pub version: u8,
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
            IpAddr::from(std::net::Ipv4Addr::from(self.router_id)),
            self.opt_params
        )
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

impl From<BGPOpenMessage> for Vec<u8> {
    fn from(val: BGPOpenMessage) -> Self {
        let mut buf = Cursor::new(vec![]);
        let opt_params: Vec<u8> = val.opt_params.into();
        buf.write_u8(val.version).unwrap();
        buf.write_u16::<BigEndian>(val.asn).unwrap();
        buf.write_u16::<BigEndian>(val.hold_time).unwrap();
        buf.write_u32::<BigEndian>(val.router_id).unwrap();
        buf.write_all(&opt_params).unwrap();
        buf.into_inner()
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
        let families = capabilities.multiprotocol;
        let params: Vec<BGPOptionalParameter> = match families {
            None => vec![BGPOptionalParameter::default()],
            Some(families) => {
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

#[derive(Default, Builder, Debug, Clone, PartialEq)]
#[builder(setter(into))]
pub struct BGPUpdateMessage {
    pub withdrawn_routes: Vec<Nlri>,
    pub path_attributes: Vec<PathAttribute>,
    pub nlri: Vec<Nlri>,
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

impl From<BGPUpdateMessage> for Vec<u8> {
    fn from(val: BGPUpdateMessage) -> Self {
        let mut buf = Cursor::new(vec![]);

        let mut wd: Vec<u8> = vec![];
        for w in val.withdrawn_routes {
            let mut v: Vec<u8> = w.into();
            wd.append(&mut v);
        }
        buf.write_u16::<BigEndian>(wd.len() as u16).unwrap();
        buf.write_all(&wd).unwrap();

        let mut pa: Vec<u8> = vec![];
        for a in val.path_attributes {
            let mut v: Vec<u8> = a.into();
            pa.append(&mut v);
        }
        buf.write_u16::<BigEndian>(pa.len() as u16).unwrap();
        buf.write_all(&pa).unwrap();

        let mut nl: Vec<u8> = vec![];
        for w in val.nlri {
            let mut v: Vec<u8> = w.into();
            nl.append(&mut v);
        }
        buf.write_all(&nl).unwrap();
        buf.into_inner()
    }
}

impl From<Vec<u8>> for BGPUpdateMessage {
    fn from(src: Vec<u8>) -> Self {
        let mut wdl = [0u8; 2];
        wdl.copy_from_slice(&src[0..2]);
        let wdl = u16::from_be_bytes(wdl) as usize;

        let mut wd: Vec<Nlri> = vec![];
        let mut used = 0;
        let mut i = 2;

        while wdl > used {
            let plen = src[i];
            let end = i + (plen as f32 / 8.0).ceil() as usize + 1;
            let buf = Ipv4Octets {
                octets: src[i..end].to_vec(),
            };
            let n: Nlri = buf.into();
            wd.push(n);
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
            let atn: usize;
            let n: PathAttribute;
            match is_extended_len(src[i]) {
                false => {
                    atn = src[i + 2] as usize;
                    n = src[i..i + 3 + atn].to_vec().into();
                    used += 3 + atn;
                    i += 3 + atn;
                }
                true => {
                    let mut l = [0u8; 2];
                    l.copy_from_slice(&src[i + 2..i + 4]);
                    atn = u16::from_be_bytes(l) as usize;
                    n = src[i..i + 4 + atn].to_vec().into();
                    used += 4 + atn;
                    i += 4 + atn;
                }
            }
            pa.push(n);
        }

        let total_len = src.len();

        let mut routes: Vec<Nlri> = vec![];
        while i < total_len {
            let plen = src[i];
            let end = i + (plen as f32 / 8.0).ceil() as usize + 1;
            let buf = Ipv4Octets {
                octets: src[i..end].to_vec(),
            };
            let n: Nlri = buf.into();
            routes.push(n);
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

#[derive(Builder, Debug, Clone)]
#[builder(setter(into))]
pub struct BGPNotificationMessage {
    pub error_code: ErrorCode,
    pub error_subcode: u8,
    pub data: Vec<u8>,
}

impl BGPNotificationMessage {
    pub fn byte_len(&self) -> usize {
        2 + self.data.len()
    }

    pub fn new(code: ErrorCode, sub: usize) -> Result<BGPNotificationMessage, String> {
        BGPNotificationMessageBuilder::default()
            .error_code(code)
            .error_subcode(sub as u8)
            .data(vec![])
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

impl From<BGPNotificationMessage> for Vec<u8> {
    fn from(val: BGPNotificationMessage) -> Self {
        let mut buf = Cursor::new(vec![]);
        buf.write_u8(val.error_code as u8).unwrap();
        buf.write_u8(val.error_subcode).unwrap();
        buf.write_all(&val.data).unwrap();
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

impl From<BGPKeepaliveMessage> for Vec<u8> {
    fn from(_val: BGPKeepaliveMessage) -> Self {
        vec![]
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

impl From<BGPMessageBody> for Vec<u8> {
    fn from(val: BGPMessageBody) -> Self {
        match val {
            BGPMessageBody::Open(body) => body.into(),
            BGPMessageBody::Update(body) => body.into(),
            BGPMessageBody::Notification(body) => body.into(),
            BGPMessageBody::Keepalive(body) => body.into(),
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
            MessageType::Open => {
                let msg: BGPOpenMessage = v.into();
                BGPMessageBody::Open(msg)
            }
            MessageType::Update => {
                let msg: BGPUpdateMessage = v.into();
                BGPMessageBody::Update(msg)
            }
            MessageType::Notification => {
                let msg: BGPNotificationMessage = v.into();
                BGPMessageBody::Notification(msg)
            }
            MessageType::Keepalive => {
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

impl From<Message> for Vec<u8> {
    fn from(val: Message) -> Self {
        let mut buf = Cursor::new(vec![]);
        buf.write_u8(val.header.message_type.clone() as u8).unwrap();
        let v: Vec<u8> = val.body.into();
        buf.write_all(&v[0..]).unwrap();
        buf.into_inner()
    }
}

impl Message {
    pub fn new(mtype: MessageType, body: BGPMessageBody) -> anyhow::Result<Message> {
        let header = BGPMessageHeaderBuilder::default()
            .message_type(mtype.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(MessageBuilder::default()
            .header(header)
            .body(body)
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?)
    }
}

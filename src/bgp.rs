#![allow(dead_code)]
use byteorder::{BigEndian, WriteBytesExt};
use bytes::{Buf, BytesMut};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::io::prelude::*;
use std::io::Cursor;
use std::mem::size_of;
use std::net::Ipv4Addr;
use std::result::Result;
use std::{error::Error, fmt};
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

#[derive(Debug, Clone)]
#[repr(u8)]
enum ErrorCode {
    MessageHeader = 1,
    OpenMessage = 2,
    UpdateMessage = 3,
    HoldTimerExpired = 4,
    FSMError = 5,
    Cease = 6,
}

#[derive(Debug)]
#[repr(u8)]
enum HeaderSubCode {
    ConnectionNotSynchronized = 1,
    BadMessageLength = 2,
    BadMessageType = 3,
}

#[derive(Debug)]
#[repr(u8)]
enum OpenSubCode {
    UnsupportedVersionNumber = 1,
    BadPeerAS = 2,
    BadBGPIdentifier = 3,
    UnsupportedOptionalParameter = 4,
    Deprecated = 5,
    UnacceptableHoldTime = 6,
}

#[derive(Debug)]
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

#[derive(Default, Builder, Debug)]
#[builder(setter(into))]
pub struct BGPOpenMessage {
    version: u8,
    pub local_asn: u16,
    pub hold_time: u16,
    pub router_id: u32,
    // opt_param_length: u8,
    pub opt_params: Vec<u8>,
}

impl fmt::Display for BGPOpenMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "version : {} local_asn : {} hold_time : {} router_id : {} opt_params : {:?}",
            self.version,
            self.local_asn,
            self.hold_time,
            Ipv4Addr::from(self.router_id),
            self.opt_params
        )
    }
}

impl Into<Vec<u8>> for BGPOpenMessage {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write(&vec![self.version.clone()]).unwrap();
        buf.write_u16::<BigEndian>(self.local_asn).unwrap();
        buf.write_u16::<BigEndian>(self.hold_time).unwrap();
        buf.write_u32::<BigEndian>(self.router_id).unwrap();
        buf.write(&vec![self.opt_params.len() as u8]).unwrap();
        buf.write(&self.opt_params).unwrap();
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

        BGPOpenMessageBuilder::default()
            .version(version)
            .local_asn(asn)
            .hold_time(hold)
            .router_id(rid)
            .opt_params(src[10..].to_vec())
            .build()
            .unwrap()
    }
}

impl BGPOpenMessage {
    pub fn byte_len(&self) -> usize {
        self.opt_params.len() + 10 * size_of::<u16>()
    }

    pub fn new(
        asn: u16,
        rid: u32,
        hold: u16,
    ) -> Result<BGPOpenMessage, Box<dyn Error + Sync + Send>> {
        let opt: Vec<u8> = BGPOptionalParameter::default().into();
        let open_body = BGPOpenMessageBuilder::default()
            .version(VERSION)
            .local_asn(asn)
            .hold_time(hold)
            .router_id(rid)
            .opt_params(opt)
            .build()?;
        Ok(open_body)
    }
}

#[derive(Debug)]
pub struct BGPOptionalParameter {
    param_type: BGPOptionalParameterType,
    // param_length: u8,
    param_value: Vec<u8>,
}

impl Default for BGPOptionalParameter {
    fn default() -> Self {
        let cv: BGPMultiprotocolCapability = BGPMultiprotocolCapability { afi: 1, safi: 1 };
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

#[derive(Debug, Clone, FromPrimitive)]
#[repr(u8)]
enum BGPOptionalParameterType {
    Authentication = 1,
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
}

#[derive(Debug)]
pub struct BGPMultiprotocolCapability {
    afi: u16,
    safi: u8,
}

impl Into<Vec<u8>> for BGPMultiprotocolCapability {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write_u16::<BigEndian>(self.afi as u16).unwrap();
        buf.write_u8(0).unwrap();
        buf.write(&vec![self.safi as u8]).unwrap();
        buf.into_inner()
    }
}

#[derive(Default, Builder, Debug)]
#[builder(setter(into))]
struct BGPUpdateMessage {
    // withdrawn_route_length: u16,
    withdrawn_routes: Vec<u8>,
    // path_attribute_length: u16,
    path_attributes: Vec<u8>,
    nlri: Vec<u8>,
}

impl BGPUpdateMessage {
    pub fn byte_len(&self) -> usize {
        self.withdrawn_routes.len()
            + self.path_attributes.len()
            + self.nlri.len()
            + 2 * size_of::<u16>()
    }

    pub fn new() -> Result<BGPUpdateMessage, Box<dyn Error + Sync + Send>> {
        Ok(BGPUpdateMessageBuilder::default().build()?)
    }
}

impl Into<Vec<u8>> for BGPUpdateMessage {
    fn into(self) -> Vec<u8> {
        let mut buf = Cursor::new(vec![]);
        buf.write_u16::<BigEndian>(self.withdrawn_routes.len() as u16)
            .unwrap();
        buf.write(&self.withdrawn_routes).unwrap();
        buf.write_u16::<BigEndian>(self.path_attributes.len() as u16)
            .unwrap();
        buf.write(&self.path_attributes).unwrap();
        buf.write(&self.nlri).unwrap();
        buf.into_inner()
    }
}

#[derive(Builder, Debug)]
#[builder(setter(into))]
struct BGPNotificationMessage {
    error_code: ErrorCode,
    error_subcode: u8,
    data: Vec<u8>,
}

impl BGPNotificationMessage {
    pub fn byte_len(&self) -> usize {
        2 + self.data.len()
    }

    pub fn new(
        code: ErrorCode,
        sub: usize,
    ) -> Result<BGPNotificationMessage, Box<dyn Error + Sync + Send>> {
        Ok(BGPNotificationMessageBuilder::default()
            .error_code(code)
            .error_subcode(sub as u8)
            .build()?)
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

#[derive(Default, Builder, Debug)]
#[builder(setter(into))]
pub struct BGPKeepaliveMessage {}

impl BGPKeepaliveMessage {
    pub fn byte_len(&self) -> u16 {
        0
    }

    pub fn new() -> Result<BGPKeepaliveMessage, Box<dyn Error + Sync + Send>> {
        Ok(BGPKeepaliveMessageBuilder::default().build()?)
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

#[derive(Default, Builder, Debug, PartialEq)]
#[builder(setter(into))]
pub struct Message {
    pub header: BGPMessageHeader,
    pub body: Vec<u8>,
}

impl From<Vec<u8>> for Message {
    fn from(src: Vec<u8>) -> Self {
        let mut mtype = [0u8; 1];
        mtype.copy_from_slice(&src[18..19]);
        let header = BGPMessageHeaderBuilder::default()
            .message_type(MessageType::from_u8(mtype[0]).unwrap())
            .build()
            .unwrap();
        let mut length_bytes = [0u8; 2];
        length_bytes.copy_from_slice(&src[16..18]);
        let srclength = src.len();
        MessageBuilder::default()
            .header(header)
            .body(src[19..srclength].to_vec())
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
        buf.write(&self.body).unwrap();
        buf.into_inner()
    }
}

impl Message {
    pub fn new(mtype: MessageType, body: Vec<u8>) -> Result<Message, Box<dyn Error + Sync + Send>> {
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
    fn test_opt_params() {
        let mut plist: Vec<BGPOptionalParameter> = vec![];
        let cv: BGPMultiprotocolCapability = BGPMultiprotocolCapability { afi: 1, safi: 1 };
        let pc: BGPCapability = BGPCapability {
            capability_code: BGPCapabilityCode::Multiprotocol,
            capability_value: cv.into(),
        };
        let p1: BGPOptionalParameter = BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_value: pc.into(),
        };

        plist.push(p1);

        let mut v: Vec<u8> = vec![];

        for param in plist {
            let mut p: Vec<u8> = param.into();
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
        let body: Vec<u8> = BGPKeepaliveMessage::new().unwrap().into();
        let test_msg: Vec<u8> = Message::new(MessageType::KEEPALIVE, body).unwrap().into();
        let keepalive: Vec<u8> = vec![0x4];
        assert_eq!(test_msg, keepalive)
    }
    #[test]
    fn test_open_message() {
        let body: Vec<u8> = BGPOpenMessage::new(123, 345, 3).unwrap().into();
        let test_msg: Vec<u8> = Message::new(MessageType::OPEN, body).unwrap().into();
        let open: Vec<u8> = vec![
            0x1, 0x4, 0x0, 0x7b, 0x0, 0x3, 0x0, 0x0, 0x1, 0x59, 0x8, 0x2, 0x6, 0x1, 0x4, 0x0, 0x1,
            0x0, 0x1,
        ];
        assert_eq!(test_msg, open)
    }
}

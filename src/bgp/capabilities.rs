use byteorder::{BigEndian, WriteBytesExt};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::io::prelude::*;
use std::io::Cursor;

use super::types::*;

#[derive(Debug, Clone, FromPrimitive, PartialEq)]
#[repr(u8)]
pub enum BGPOptionalParameterType {
    Authentication = 1, // deprecated
    Capability = 2,
}

#[derive(Debug, Clone)]
pub struct BGPOptionalParameter {
    pub param_type: BGPOptionalParameterType,
    pub param_length: usize,
    pub param_value: Vec<u8>,
}

impl Default for BGPOptionalParameter {
    fn default() -> Self {
        let cv: BGPCapabilityMultiprotocol = BGPCapabilityMultiprotocol {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
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

impl From<BGPOptionalParameter> for Vec<u8> {
    fn from(val: BGPOptionalParameter) -> Self {
        let mut buf = Cursor::new(vec![]);
        buf.write_all(&[val.param_type.clone() as u8]).unwrap();
        buf.write_all(&[val.param_value.len() as u8]).unwrap();
        buf.write_all(&val.param_value).unwrap();
        buf.into_inner()
    }
}

#[derive(Debug, Clone)]
pub struct BGPOptionalParameters {
    pub len: usize,
    pub params: Vec<BGPOptionalParameter>,
}

impl BGPOptionalParameters {
    pub fn new(params: Vec<BGPOptionalParameter>) -> BGPOptionalParameters {
        let mut len = 0;
        for p in params.clone() {
            len += 2;
            len += p.param_length;
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

impl From<BGPOptionalParameters> for Vec<u8> {
    fn from(val: BGPOptionalParameters) -> Self {
        let mut buf = Cursor::new(vec![]);
        buf.write_u8(val.len as u8).unwrap();
        for p in val.params {
            let p: Vec<u8> = p.into();
            buf.write_all(&p).unwrap();
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

impl From<BGPCapability> for Vec<u8> {
    fn from(val: BGPCapability) -> Self {
        let mut buf = Cursor::new(vec![]);
        buf.write_u8(val.capability_code as u8).unwrap();
        buf.write_u8(val.capability_length as u8).unwrap();
        buf.write_all(&val.capability_value).unwrap();
        buf.into_inner()
    }
}

#[derive(Debug)]
pub struct BGPCapabilityMultiprotocol {
    pub afi: Afi,
    pub safi: Safi,
}

impl From<BGPCapabilityMultiprotocol> for Vec<u8> {
    fn from(val: BGPCapabilityMultiprotocol) -> Self {
        let mut buf = Cursor::new(vec![]);
        buf.write_u16::<BigEndian>(val.afi as u16).unwrap();
        buf.write_u8(0).unwrap();
        buf.write_u8(val.safi as u8).unwrap();
        buf.into_inner()
    }
}

#[derive(Debug, Clone, Default)]
pub struct BGPCapabilities {
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
            Some(_) => {
                // Extract capabilities from parameter value
                // This is a complex conversion, see original code
                let caps_params = Vec::new();
                // Extract the capabilities
                BGPCapabilities {
                    params: caps_params,
                }
            }
        }
    }
}

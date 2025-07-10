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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, FromPrimitive, PartialEq)]
#[repr(u8)]
pub enum BGPCapabilityCode {
    Multiprotocol = 1,
    RouteRefresh = 2,
    OutboundRouteFiltering = 3,
    ExtendedNextHopEncoding = 5,
    GracefulRestart = 64,
    FourOctectASN = 65,
    DynamicCapability = 67,
    MultisessionBGP = 68,
    AddPath = 69,
    EnhancedRouteRefresh = 70,
    LongLivedGracefulRestart = 71,
    FQDNCapability = 73,
    #[doc(hidden)]
    Unknown = 255,
}

#[derive(Debug, Clone)]
pub struct BGPCapability {
    pub capability_code: BGPCapabilityCode,
    pub capability_length: usize,
    pub capability_value: Vec<u8>,
}

impl From<Vec<u8>> for BGPCapability {
    fn from(src: Vec<u8>) -> Self {
        if src.len() < 2 {
            log::warn!("Capability buffer too short: {:?}", src);
            return BGPCapability {
                capability_code: BGPCapabilityCode::Multiprotocol, // fallback
                capability_length: 0,
                capability_value: vec![],
            };
        }

        let code = src[0];
        let length = src[1] as usize;

        let cap_code = match BGPCapabilityCode::from_u8(code) {
            Some(c) => c,
            None => {
                log::warn!("Unrecognized capability code: {} ({} bytes)", code, length);
                return BGPCapability {
                    capability_code: BGPCapabilityCode::Multiprotocol, // dummy default to parse
                    capability_length: 0,
                    capability_value: vec![],
                };
            }
        };

        let value = if src.len() >= 2 + length {
            src[2..2 + length].to_vec()
        } else {
            log::warn!(
                "Capability code {} claims length {}, but buffer is only {} bytes",
                code,
                length,
                src.len()
            );
            vec![]
        };

        BGPCapability {
            capability_code: cap_code,
            capability_length: length,
            capability_value: value,
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
        let mut all_caps = Vec::new();

        for param in src.params {
            if param.param_type == BGPOptionalParameterType::Capability {
                // Parse all capabilities from this parameter
                let mut offset = 0;
                let data = &param.param_value;

                while offset < data.len() {
                    // Need at least 2 bytes for code and length
                    if offset + 2 > data.len() {
                        log::warn!("Incomplete capability at offset {}", offset);
                        break;
                    }

                    let cap_code = data[offset];
                    let cap_len = data[offset + 1] as usize;

                    // Check if we have enough data for this capability
                    if offset + 2 + cap_len > data.len() {
                        log::warn!("Capability length {} exceeds available data", cap_len);
                        // For capabilities with excessive length claims, try to parse with available data
                        // We handle malformed data gracefully if the claimed length is much larger than available
                        // but skip capabilities that are just slightly incomplete
                        let available_data = data.len() - offset - 2;
                        let is_excessive_claim = cap_len > available_data * 2; // Claim is more than double available

                        if is_excessive_claim && available_data > 0 {
                            // Malformed length field - parse with available data
                            let available_len = data.len() - offset;
                            let cap_data = data[offset..offset + available_len].to_vec();
                            let cap: BGPCapability = cap_data.into();
                            all_caps.push(cap);
                        }
                        // Skip capabilities that are just incomplete (not malformed)
                        break;
                    }

                    // Extract this capability
                    let cap_data = data[offset..offset + 2 + cap_len].to_vec();
                    let cap: BGPCapability = cap_data.into();
                    all_caps.push(cap);

                    // Move to next capability
                    offset += 2 + cap_len;
                }
            }
        }

        BGPCapabilities { params: all_caps }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bgp_optional_parameter_default() {
        let param = BGPOptionalParameter::default();
        assert_eq!(param.param_type, BGPOptionalParameterType::Capability);
        assert!(param.param_length > 0);
    }

    #[test]
    fn test_bgp_optional_parameter_serialization() {
        let param = BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: 4,
            param_value: vec![1, 2, 3, 4],
        };
        let bytes: Vec<u8> = param.clone().into();
        assert_eq!(bytes[0], BGPOptionalParameterType::Capability as u8);
        assert_eq!(bytes[1], 4);
        assert_eq!(&bytes[2..], &[1, 2, 3, 4]);

        let parsed: BGPOptionalParameter = bytes.into();
        assert_eq!(parsed.param_type, param.param_type);
        assert_eq!(parsed.param_value, param.param_value);
    }

    #[test]
    fn test_bgp_optional_parameters() {
        let param1 = BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: 2,
            param_value: vec![1, 2],
        };
        let param2 = BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: 3,
            param_value: vec![3, 4, 5],
        };
        let params = BGPOptionalParameters::new(vec![param1, param2]);
        assert_eq!(params.len, 9); // 2+2 + 2+3

        let bytes: Vec<u8> = params.clone().into();
        let parsed: BGPOptionalParameters = bytes.into();
        assert_eq!(parsed.params.len(), 2);
    }

    #[test]
    fn test_bgp_capability_multiprotocol() {
        let mp = BGPCapabilityMultiprotocol {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        };
        let bytes: Vec<u8> = mp.into();
        assert_eq!(bytes.len(), 4);
        assert_eq!(&bytes[0..2], &[0, 1]); // IPv4
        assert_eq!(bytes[2], 0); // Reserved
        assert_eq!(bytes[3], 1); // Unicast
    }

    #[test]
    fn test_bgp_capability_serialization() {
        let cap = BGPCapability {
            capability_code: BGPCapabilityCode::RouteRefresh,
            capability_length: 0,
            capability_value: vec![],
        };
        let bytes: Vec<u8> = cap.clone().into();
        assert_eq!(bytes[0], BGPCapabilityCode::RouteRefresh as u8);
        assert_eq!(bytes[1], 0);

        let parsed: BGPCapability = bytes.into();
        assert_eq!(parsed.capability_code, cap.capability_code);
    }

    #[test]
    fn test_bgp_capabilities_from_optional_parameters() {
        // Test parsing multiple capabilities from a single parameter
        let mut cap_data = vec![];

        // Add Route Refresh capability (code 2, length 0)
        cap_data.extend_from_slice(&[2, 0]);

        // Add Multiprotocol capability (code 1, length 4)
        cap_data.extend_from_slice(&[1, 4, 0, 1, 0, 1]); // IPv4 Unicast

        // Add Four-octet ASN capability (code 65, length 4)
        cap_data.extend_from_slice(&[65, 4, 0, 0, 0, 123]);

        let param = BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: cap_data.len(),
            param_value: cap_data,
        };

        let opt_params = BGPOptionalParameters {
            len: param.param_length + 2,
            params: vec![param],
        };

        let caps: BGPCapabilities = opt_params.into();
        assert_eq!(caps.params.len(), 3);

        // Verify each capability
        assert_eq!(
            caps.params[0].capability_code,
            BGPCapabilityCode::RouteRefresh
        );
        assert_eq!(
            caps.params[1].capability_code,
            BGPCapabilityCode::Multiprotocol
        );
        assert_eq!(
            caps.params[2].capability_code,
            BGPCapabilityCode::FourOctectASN
        );
    }

    #[test]
    fn test_bgp_capabilities_partial_data() {
        // Test handling of incomplete capability data
        let cap_data = vec![1, 4, 0, 1]; // Incomplete multiprotocol capability

        let param = BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: cap_data.len(),
            param_value: cap_data,
        };

        let opt_params = BGPOptionalParameters {
            len: param.param_length + 2,
            params: vec![param],
        };

        let caps: BGPCapabilities = opt_params.into();
        assert_eq!(caps.params.len(), 0); // Should skip the incomplete capability
    }
}

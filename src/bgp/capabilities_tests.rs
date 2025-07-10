// Valid input tests
#[test]
fn test_bgp_optional_parameter_type_valid() {
    let auth_type = BGPOptionalParameterType::Authentication;
    assert_eq!(auth_type as u8, 1);
    
    let cap_type = BGPOptionalParameterType::Capability;
    assert_eq!(cap_type as u8, 2);
}

#[test]
fn test_bgp_optional_parameter_valid() {
    let param = BGPOptionalParameter {
        param_type: BGPOptionalParameterType::Capability,
        param_length: 6,
        param_value: vec![1, 2, 4, 0, 1, 1], // Multiprotocol capability
    };
    
    assert_eq!(param.param_type, BGPOptionalParameterType::Capability);
    assert_eq!(param.param_length, 6);
    assert_eq!(param.param_value.len(), 6);
}

#[test]
fn test_bgp_optional_parameter_default_valid() {
    let param = BGPOptionalParameter::default();
    
    assert_eq!(param.param_type, BGPOptionalParameterType::Capability);
    assert!(param.param_length > 0);
    assert!(!param.param_value.is_empty());
}

#[test]
fn test_bgp_optional_parameter_serialization_valid() {
    let param = BGPOptionalParameter {
        param_type: BGPOptionalParameterType::Capability,
        param_length: 4,
        param_value: vec![1, 2, 3, 4],
    };
    
    let bytes: Vec<u8> = param.clone().into();
    
    assert_eq!(bytes[0], BGPOptionalParameterType::Capability as u8);
    assert_eq!(bytes[1], 4); // Length
    assert_eq!(bytes[2..6], [1, 2, 3, 4]); // Value
}

#[test]
fn test_bgp_optional_parameter_deserialization_valid() {
    let bytes = vec![2, 4, 1, 2, 3, 4]; // Capability type, length 4, value [1,2,3,4]
    let param: BGPOptionalParameter = bytes.into();
    
    assert_eq!(param.param_type, BGPOptionalParameterType::Capability);
    assert_eq!(param.param_length, 4);
    assert_eq!(param.param_value, vec![1, 2, 3, 4]);
}

#[test]
fn test_bgp_optional_parameters_valid() {
    let params = vec![
        BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: 4,
            param_value: vec![1, 2, 3, 4],
        },
        BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: 2,
            param_value: vec![5, 6],
        },
    ];
    
    let opt_params = BGPOptionalParameters::new(params.clone());
    
    assert_eq!(opt_params.len, 10); // 2 + 4 + 2 + 2 = 10 (2 bytes header per param)
    assert_eq!(opt_params.params.len(), 2);
    assert_eq!(opt_params.params, params);
}

#[test]
fn test_bgp_optional_parameters_default_valid() {
    let opt_params = BGPOptionalParameters::default();
    
    assert!(opt_params.len > 0);
    assert_eq!(opt_params.params.len(), 1);
    assert_eq!(opt_params.params[0].param_type, BGPOptionalParameterType::Capability);
}

#[test]
fn test_bgp_optional_parameters_serialization_valid() {
    let params = vec![
        BGPOptionalParameter {
            param_type: BGPOptionalParameterType::Capability,
            param_length: 2,
            param_value: vec![1, 2],
        },
    ];
    
    let opt_params = BGPOptionalParameters::new(params);
    let bytes: Vec<u8> = opt_params.into();
    
    assert_eq!(bytes[0], 4); // Total length (2 + 2)
    assert_eq!(bytes[1], 2); // Capability type
    assert_eq!(bytes[2], 2); // Length
    assert_eq!(bytes[3..5], [1, 2]); // Value
}

#[test]
fn test_bgp_capability_code_valid() {
    assert_eq!(BGPCapabilityCode::Multiprotocol as u8, 1);
    assert_eq!(BGPCapabilityCode::RouteRefresh as u8, 2);
    assert_eq!(BGPCapabilityCode::OutboundRouteFiltering as u8, 3);
    assert_eq!(BGPCapabilityCode::ExtendedNextHopEncoding as u8, 5);
    assert_eq!(BGPCapabilityCode::GracefulRestart as u8, 64);
    assert_eq!(BGPCapabilityCode::FourOctectASN as u8, 65);
    assert_eq!(BGPCapabilityCode::DynamicCapability as u8, 67);
    assert_eq!(BGPCapabilityCode::Unknown as u8, 255);
}

#[test]
fn test_bgp_capability_valid() {
    let cap = BGPCapability {
        capability_code: BGPCapabilityCode::Multiprotocol,
        capability_length: 4,
        capability_value: vec![0, 1, 0, 1], // AFI=1, SAFI=1
    };
    
    assert_eq!(cap.capability_code, BGPCapabilityCode::Multiprotocol);
    assert_eq!(cap.capability_length, 4);
    assert_eq!(cap.capability_value, vec![0, 1, 0, 1]);
}

#[test]
fn test_bgp_capability_serialization_valid() {
    let cap = BGPCapability {
        capability_code: BGPCapabilityCode::Multiprotocol,
        capability_length: 4,
        capability_value: vec![0, 1, 0, 1],
    };
    
    let bytes: Vec<u8> = cap.into();
    
    assert_eq!(bytes[0], 1); // Multiprotocol code
    assert_eq!(bytes[1], 4); // Length
    assert_eq!(bytes[2..6], [0, 1, 0, 1]); // Value
}

#[test]
fn test_bgp_capability_deserialization_valid() {
    let bytes = vec![1, 4, 0, 1, 0, 1]; // Multiprotocol, length 4, AFI=1, SAFI=1
    let cap: BGPCapability = bytes.into();
    
    assert_eq!(cap.capability_code, BGPCapabilityCode::Multiprotocol);
    assert_eq!(cap.capability_length, 4);
    assert_eq!(cap.capability_value, vec![0, 1, 0, 1]);
}

#[test]
fn test_bgp_capability_multiprotocol_valid() {
    let mp_cap = BGPCapabilityMultiprotocol {
        afi: Afi::Ipv4,
        safi: Safi::NLRIUnicast,
    };
    
    let bytes: Vec<u8> = mp_cap.into();
    
    assert_eq!(bytes.len(), 4);
    assert_eq!(bytes[0..2], [0, 1]); // AFI = 1 (IPv4)
    assert_eq!(bytes[2], 0); // Reserved
    assert_eq!(bytes[3], 1); // SAFI = 1 (Unicast)
}

#[test]
fn test_bgp_capability_multiprotocol_ipv6_valid() {
    let mp_cap = BGPCapabilityMultiprotocol {
        afi: Afi::Ipv6,
        safi: Safi::NLRIMulticast,
    };
    
    let bytes: Vec<u8> = mp_cap.into();
    
    assert_eq!(bytes.len(), 4);
    assert_eq!(bytes[0..2], [0, 2]); // AFI = 2 (IPv6)
    assert_eq!(bytes[2], 0); // Reserved
    assert_eq!(bytes[3], 2); // SAFI = 2 (Multicast)
}

#[test]
fn test_bgp_capabilities_default_valid() {
    let caps = BGPCapabilities::default();
    
    assert!(caps.params.is_empty());
}

#[test]
fn test_bgp_capabilities_from_optional_parameters_valid() {
    let cap_bytes = vec![1, 4, 0, 1, 0, 1]; // Multiprotocol capability
    let param = BGPOptionalParameter {
        param_type: BGPOptionalParameterType::Capability,
        param_length: 6,
        param_value: cap_bytes,
    };
    
    let opt_params = BGPOptionalParameters {
        len: 8,
        params: vec![param],
    };
    
    let caps: BGPCapabilities = opt_params.into();
    
    assert_eq!(caps.params.len(), 1);
    assert_eq!(caps.params[0].capability_code, BGPCapabilityCode::Multiprotocol);
    assert_eq!(caps.params[0].capability_length, 4);
}

// Invalid input tests
#[test]
fn test_bgp_optional_parameter_from_empty_bytes_invalid() {
    let empty_bytes: Vec<u8> = vec![];
    
    // This should panic or handle gracefully
    std::panic::catch_unwind(|| {
        let _param: BGPOptionalParameter = empty_bytes.into();
    }).expect_err("Should panic on empty input");
}

#[test]
fn test_bgp_optional_parameter_from_insufficient_bytes_invalid() {
    let insufficient_bytes: Vec<u8> = vec![2]; // Only type, missing length
    
    // This should panic or handle gracefully
    std::panic::catch_unwind(|| {
        let _param: BGPOptionalParameter = insufficient_bytes.into();
    }).expect_err("Should panic on insufficient input");
}

#[test]
fn test_bgp_optional_parameter_invalid_type_invalid() {
    let invalid_bytes: Vec<u8> = vec![99, 2, 1, 2]; // Invalid type 99
    
    // This should panic or handle gracefully
    std::panic::catch_unwind(|| {
        let _param: BGPOptionalParameter = invalid_bytes.into();
    }).expect_err("Should panic on invalid type");
}

#[test]
fn test_bgp_optional_parameter_length_mismatch_invalid() {
    let invalid_bytes: Vec<u8> = vec![2, 5, 1, 2]; // Says length 5 but only 2 bytes provided
    
    // This should handle gracefully or panic
    let param: BGPOptionalParameter = invalid_bytes.into();
    
    // The implementation should handle this gracefully
    assert_eq!(param.param_type, BGPOptionalParameterType::Capability);
    assert_eq!(param.param_length, 5);
    assert_eq!(param.param_value, vec![1, 2]); // Takes what's available
}

#[test]
fn test_bgp_optional_parameters_from_empty_bytes_invalid() {
    let empty_bytes: Vec<u8> = vec![];
    
    // This should panic or handle gracefully
    std::panic::catch_unwind(|| {
        let _params: BGPOptionalParameters = empty_bytes.into();
    }).expect_err("Should panic on empty input");
}

#[test]
fn test_bgp_optional_parameters_invalid_length_invalid() {
    let invalid_bytes: Vec<u8> = vec![10, 2, 1]; // Says length 10 but only 2 bytes of data
    
    // This should handle gracefully or panic
    std::panic::catch_unwind(|| {
        let _params: BGPOptionalParameters = invalid_bytes.into();
    }).expect_err("Should panic on invalid length");
}

#[test]
fn test_bgp_capability_from_empty_bytes_invalid() {
    let empty_bytes: Vec<u8> = vec![];
    
    // The implementation handles this gracefully by returning a default
    let cap: BGPCapability = empty_bytes.into();
    
    assert_eq!(cap.capability_code, BGPCapabilityCode::Multiprotocol);
    assert_eq!(cap.capability_length, 0);
    assert!(cap.capability_value.is_empty());
}

#[test]
fn test_bgp_capability_from_insufficient_bytes_invalid() {
    let insufficient_bytes: Vec<u8> = vec![1]; // Only code, missing length
    
    // The implementation handles this gracefully by returning a default
    let cap: BGPCapability = insufficient_bytes.into();
    
    assert_eq!(cap.capability_code, BGPCapabilityCode::Multiprotocol);
    assert_eq!(cap.capability_length, 0);
    assert!(cap.capability_value.is_empty());
}

#[test]
fn test_bgp_capability_invalid_code_invalid() {
    let invalid_bytes: Vec<u8> = vec![99, 4, 1, 2, 3, 4]; // Invalid code 99
    
    // The implementation handles this gracefully by returning a default
    let cap: BGPCapability = invalid_bytes.into();
    
    assert_eq!(cap.capability_code, BGPCapabilityCode::Multiprotocol);
    assert_eq!(cap.capability_length, 0);
    assert!(cap.capability_value.is_empty());
}

#[test]
fn test_bgp_capability_length_exceeds_buffer_invalid() {
    let invalid_bytes: Vec<u8> = vec![1, 10, 1, 2]; // Says length 10 but only 2 bytes available
    
    // The implementation handles this gracefully
    let cap: BGPCapability = invalid_bytes.into();
    
    assert_eq!(cap.capability_code, BGPCapabilityCode::Multiprotocol);
    assert_eq!(cap.capability_length, 10);
    assert!(cap.capability_value.is_empty()); // Should be empty due to insufficient data
}

#[test]
fn test_bgp_capability_zero_length_valid() {
    let zero_length_bytes: Vec<u8> = vec![2, 0]; // Route refresh with 0 length
    
    let cap: BGPCapability = zero_length_bytes.into();
    
    assert_eq!(cap.capability_code, BGPCapabilityCode::RouteRefresh);
    assert_eq!(cap.capability_length, 0);
    assert!(cap.capability_value.is_empty());
}

#[test]
fn test_bgp_capabilities_from_non_capability_parameters_edge_case() {
    let auth_param = BGPOptionalParameter {
        param_type: BGPOptionalParameterType::Authentication,
        param_length: 4,
        param_value: vec![1, 2, 3, 4],
    };
    
    let opt_params = BGPOptionalParameters {
        len: 6,
        params: vec![auth_param],
    };
    
    let caps: BGPCapabilities = opt_params.into();
    
    // Should result in empty capabilities since the parameter is not a capability
    assert!(caps.params.is_empty());
}

#[test]
fn test_bgp_capabilities_from_malformed_capability_data_invalid() {
    let malformed_cap_bytes = vec![1, 10, 1, 2]; // Says length 10 but only 2 bytes
    let param = BGPOptionalParameter {
        param_type: BGPOptionalParameterType::Capability,
        param_length: 4,
        param_value: malformed_cap_bytes,
    };
    
    let opt_params = BGPOptionalParameters {
        len: 6,
        params: vec![param],
    };
    
    let caps: BGPCapabilities = opt_params.into();
    
    // Should handle malformed data gracefully
    assert_eq!(caps.params.len(), 1);
    assert_eq!(caps.params[0].capability_code, BGPCapabilityCode::Multiprotocol);
}

#[test]
fn test_bgp_capabilities_from_truncated_capability_data_invalid() {
    let truncated_cap_bytes = vec![1]; // Only code, missing length
    let param = BGPOptionalParameter {
        param_type: BGPOptionalParameterType::Capability,
        param_length: 1,
        param_value: truncated_cap_bytes,
    };
    
    let opt_params = BGPOptionalParameters {
        len: 3,
        params: vec![param],
    };
    
    let caps: BGPCapabilities = opt_params.into();
    
    // Should handle truncated data gracefully
    assert!(caps.params.is_empty()); // Should break out of loop due to insufficient data
}

#[test]
fn test_bgp_capability_multiprotocol_edge_cases() {
    // Test with maximum AFI/SAFI values
    let mp_cap = BGPCapabilityMultiprotocol {
        afi: Afi::Ipv6, // AFI 2
        safi: Safi::NLRIMulticast, // SAFI 2
    };
    
    let bytes: Vec<u8> = mp_cap.into();
    
    assert_eq!(bytes.len(), 4);
    assert_eq!(bytes[0..2], [0, 2]); // AFI = 2
    assert_eq!(bytes[2], 0); // Reserved
    assert_eq!(bytes[3], 2); // SAFI = 2
}

#[test]
fn test_bgp_optional_parameters_multiple_capabilities_valid() {
    // Test with multiple capabilities in one parameter
    let multiple_caps = vec![
        1, 4, 0, 1, 0, 1, // Multiprotocol capability
        2, 0,             // Route refresh capability
        65, 4, 0, 0, 0, 1, // 4-octet ASN capability
    ];
    
    let param = BGPOptionalParameter {
        param_type: BGPOptionalParameterType::Capability,
        param_length: 12,
        param_value: multiple_caps,
    };
    
    let opt_params = BGPOptionalParameters {
        len: 14,
        params: vec![param],
    };
    
    let caps: BGPCapabilities = opt_params.into();
    
    assert_eq!(caps.params.len(), 3);
    assert_eq!(caps.params[0].capability_code, BGPCapabilityCode::Multiprotocol);
    assert_eq!(caps.params[1].capability_code, BGPCapabilityCode::RouteRefresh);
    assert_eq!(caps.params[2].capability_code, BGPCapabilityCode::FourOctectASN);
}
// Valid input tests
#[test]
fn test_afi_values_valid() {
    assert_eq!(Afi::Ipv4 as u16, 1);
    assert_eq!(Afi::Ipv6 as u16, 2);
}

#[test]
fn test_safi_values_valid() {
    assert_eq!(Safi::NLRIUnicast as u8, 1);
    assert_eq!(Safi::NLRIMulticast as u8, 2);
}

#[test]
fn test_address_family_valid() {
    let af = AddressFamily {
        afi: Afi::Ipv4,
        safi: Safi::NLRIUnicast,
    };
    
    assert_eq!(af.afi, Afi::Ipv4);
    assert_eq!(af.safi, Safi::NLRIUnicast);
}

#[test]
fn test_address_family_equality_valid() {
    let af1 = AddressFamily {
        afi: Afi::Ipv4,
        safi: Safi::NLRIUnicast,
    };
    
    let af2 = AddressFamily {
        afi: Afi::Ipv4,
        safi: Safi::NLRIUnicast,
    };
    
    let af3 = AddressFamily {
        afi: Afi::Ipv6,
        safi: Safi::NLRIUnicast,
    };
    
    assert_eq!(af1, af2);
    assert_ne!(af1, af3);
}

#[test]
fn test_address_family_hash_valid() {
    let mut set = HashSet::new();
    
    let af1 = AddressFamily {
        afi: Afi::Ipv4,
        safi: Safi::NLRIUnicast,
    };
    
    let af2 = AddressFamily {
        afi: Afi::Ipv4,
        safi: Safi::NLRIUnicast,
    };
    
    let af3 = AddressFamily {
        afi: Afi::Ipv6,
        safi: Safi::NLRIUnicast,
    };
    
    set.insert(af1);
    assert!(!set.insert(af2)); // Should return false (duplicate)
    assert!(set.insert(af3)); // Should return true (new)
    
    assert_eq!(set.len(), 2);
}

#[test]
fn test_message_type_values_valid() {
    assert_eq!(MessageType::Open as u8, 1);
    assert_eq!(MessageType::Update as u8, 2);
    assert_eq!(MessageType::Notification as u8, 3);
    assert_eq!(MessageType::Keepalive as u8, 4);
}

#[test]
fn test_message_type_default_valid() {
    let msg_type = MessageType::default();
    assert_eq!(msg_type, MessageType::Update);
}

#[test]
fn test_error_code_values_valid() {
    assert_eq!(ErrorCode::MessageHeader as u8, 1);
    assert_eq!(ErrorCode::OpenMessage as u8, 2);
    assert_eq!(ErrorCode::UpdateMessage as u8, 3);
    assert_eq!(ErrorCode::HoldTimerExpired as u8, 4);
    assert_eq!(ErrorCode::FSMError as u8, 5);
    assert_eq!(ErrorCode::Cease as u8, 6);
}

#[test]
fn test_header_subcode_values_valid() {
    assert_eq!(HeaderSubCode::ConnectionNotSynchronized as u8, 1);
    assert_eq!(HeaderSubCode::BadMessageLength as u8, 2);
    assert_eq!(HeaderSubCode::BadMessageType as u8, 3);
}

#[test]
fn test_open_subcode_values_valid() {
    assert_eq!(OpenSubCode::UnsupportedVersionNumber as u8, 1);
    assert_eq!(OpenSubCode::BadPeerAS as u8, 2);
    assert_eq!(OpenSubCode::BadBGPIdentifier as u8, 3);
    assert_eq!(OpenSubCode::UnsupportedOptionalParameter as u8, 4);
    assert_eq!(OpenSubCode::Deprecated as u8, 5);
    assert_eq!(OpenSubCode::UnacceptableHoldTime as u8, 6);
}

#[test]
fn test_update_subcode_values_valid() {
    assert_eq!(UpdateSubCode::MalformedAttributeList as u8, 1);
    assert_eq!(UpdateSubCode::UnrecognizedWellKnownAttribute as u8, 2);
    assert_eq!(UpdateSubCode::MissingWellKnownAttribute as u8, 3);
    assert_eq!(UpdateSubCode::AttributeFlagsError as u8, 4);
    assert_eq!(UpdateSubCode::AttributeLengthError as u8, 5);
    assert_eq!(UpdateSubCode::InvalidORIGINAttribute as u8, 6);
    assert_eq!(UpdateSubCode::Deprecated as u8, 7);
    assert_eq!(UpdateSubCode::InvalidNEXTHOPAttribute as u8, 8);
    assert_eq!(UpdateSubCode::OptionalAttributeError as u8, 9);
    assert_eq!(UpdateSubCode::InvalidNetworkField as u8, 10);
    assert_eq!(UpdateSubCode::MalformedASPATH as u8, 11);
}

#[test]
fn test_constants_valid() {
    assert_eq!(MARKER.len(), 16);
    assert!(MARKER.iter().all(|&b| b == 0xff));
    assert_eq!(VERSION, 4);
    assert_eq!(MIN_MESSAGE_LENGTH, 19);
    assert_eq!(MAX_MESSAGE_LENGTH, 4096);
    assert_eq!(MAX, 4096);
}

#[test]
fn test_validate_message_length_valid() {
    assert!(validate_message_length(MIN_MESSAGE_LENGTH).is_ok());
    assert!(validate_message_length(1000).is_ok());
    assert!(validate_message_length(MAX_MESSAGE_LENGTH).is_ok());
}

#[test]
fn test_validate_marker_valid() {
    let good_marker = MARKER;
    assert!(validate_marker(&good_marker).is_ok());
}

#[test]
fn test_validate_bgp_version_valid() {
    assert!(validate_bgp_version(VERSION).is_ok());
}

#[test]
fn test_validate_asn_valid() {
    assert!(validate_asn(1).is_ok());
    assert!(validate_asn(65000).is_ok());
    assert!(validate_asn(65535).is_ok());
}

#[test]
fn test_validate_hold_time_valid() {
    assert!(validate_hold_time(0).is_ok()); // 0 is valid (no keepalives)
    assert!(validate_hold_time(3).is_ok());
    assert!(validate_hold_time(180).is_ok());
    assert!(validate_hold_time(65535).is_ok());
}

#[test]
fn test_validate_router_id_valid() {
    assert!(validate_router_id(1).is_ok());
    assert!(validate_router_id(0x01020304).is_ok());
    assert!(validate_router_id(0xFFFFFFFF).is_ok());
}

#[test]
fn test_validate_nlri_prefix_length_valid() {
    assert!(validate_nlri_prefix_length(0, &Afi::Ipv4).is_ok());
    assert!(validate_nlri_prefix_length(24, &Afi::Ipv4).is_ok());
    assert!(validate_nlri_prefix_length(32, &Afi::Ipv4).is_ok());
    
    assert!(validate_nlri_prefix_length(0, &Afi::Ipv6).is_ok());
    assert!(validate_nlri_prefix_length(64, &Afi::Ipv6).is_ok());
    assert!(validate_nlri_prefix_length(128, &Afi::Ipv6).is_ok());
}

#[test]
fn test_is_extended_len_valid() {
    assert!(!is_extended_len(0b00000000)); // Extended bit not set
    assert!(is_extended_len(0b00010000)); // Extended bit set
    assert!(!is_extended_len(0b11100000)); // Extended bit not set
    assert!(is_extended_len(0b11110000)); // Extended bit set
}

#[test]
fn test_safe_slice_valid() {
    let buffer = vec![1, 2, 3, 4, 5];
    
    assert!(safe_slice(&buffer, 0, 3).is_ok());
    assert_eq!(safe_slice(&buffer, 0, 3).unwrap(), &[1, 2, 3]);
    
    assert!(safe_slice(&buffer, 2, 5).is_ok());
    assert_eq!(safe_slice(&buffer, 2, 5).unwrap(), &[3, 4, 5]);
    
    assert!(safe_slice(&buffer, 0, 0).is_ok());
    assert_eq!(safe_slice(&buffer, 0, 0).unwrap(), &[]);
}

#[test]
fn test_safe_array_valid() {
    let buffer = vec![1, 2, 3, 4, 5];
    
    let arr: Result<[u8; 2], _> = safe_array(&buffer, 0);
    assert!(arr.is_ok());
    assert_eq!(arr.unwrap(), [1, 2]);
    
    let arr: Result<[u8; 3], _> = safe_array(&buffer, 2);
    assert!(arr.is_ok());
    assert_eq!(arr.unwrap(), [3, 4, 5]);
    
    let arr: Result<[u8; 0], _> = safe_array(&buffer, 0);
    assert!(arr.is_ok());
    assert_eq!(arr.unwrap(), []);
}

#[test]
fn test_validate_buffer_bounds_valid() {
    let buffer = vec![1, 2, 3, 4, 5];
    
    assert!(validate_buffer_bounds(&buffer, 0, 5).is_ok());
    assert!(validate_buffer_bounds(&buffer, 2, 3).is_ok());
    assert!(validate_buffer_bounds(&buffer, 0, 0).is_ok());
    assert!(validate_buffer_bounds(&buffer, 5, 0).is_ok());
}

#[test]
fn test_enum_from_primitive_valid() {
    assert_eq!(Afi::from_u16(1), Some(Afi::Ipv4));
    assert_eq!(Afi::from_u16(2), Some(Afi::Ipv6));
    
    assert_eq!(Safi::from_u8(1), Some(Safi::NLRIUnicast));
    assert_eq!(Safi::from_u8(2), Some(Safi::NLRIMulticast));
    
    assert_eq!(MessageType::from_u8(1), Some(MessageType::Open));
    assert_eq!(MessageType::from_u8(2), Some(MessageType::Update));
    assert_eq!(MessageType::from_u8(3), Some(MessageType::Notification));
    assert_eq!(MessageType::from_u8(4), Some(MessageType::Keepalive));
}

#[test]
fn test_bgp_validation_error_display_valid() {
    let err = BgpValidationError::MessageTooShort { actual: 10, minimum: 19 };
    let display = format!("{}", err);
    assert!(display.contains("Message too short"));
    assert!(display.contains("10"));
    assert!(display.contains("19"));
    
    let err = BgpValidationError::InvalidMarker;
    let display = format!("{}", err);
    assert!(display.contains("Invalid marker"));
}

#[test]
fn test_bgp_validation_error_to_notification_codes_valid() {
    let err = BgpValidationError::InvalidMarker;
    let (code, subcode) = err.to_notification_codes();
    assert_eq!(code, ErrorCode::MessageHeader);
    assert_eq!(subcode, HeaderSubCode::ConnectionNotSynchronized as u8);
    
    let err = BgpValidationError::InvalidAsn(0);
    let (code, subcode) = err.to_notification_codes();
    assert_eq!(code, ErrorCode::OpenMessage);
    assert_eq!(subcode, OpenSubCode::BadPeerAS as u8);
    
    let err = BgpValidationError::MalformedAsPath("test".to_string());
    let (code, subcode) = err.to_notification_codes();
    assert_eq!(code, ErrorCode::UpdateMessage);
    assert_eq!(subcode, UpdateSubCode::MalformedASPATH as u8);
}

// Invalid input tests
#[test]
fn test_validate_message_length_invalid() {
    assert!(validate_message_length(18).is_err()); // Too short
    assert!(validate_message_length(4097).is_err()); // Too long
    
    let err = validate_message_length(10).unwrap_err();
    match err {
        BgpValidationError::MessageTooShort { actual, minimum } => {
            assert_eq!(actual, 10);
            assert_eq!(minimum, MIN_MESSAGE_LENGTH);
        }
        _ => panic!("Expected MessageTooShort error"),
    }
}

#[test]
fn test_validate_marker_invalid() {
    let mut bad_marker = MARKER;
    bad_marker[0] = 0xfe;
    
    assert!(validate_marker(&bad_marker).is_err());
    
    let err = validate_marker(&bad_marker).unwrap_err();
    match err {
        BgpValidationError::InvalidMarker => {} // Expected
        _ => panic!("Expected InvalidMarker error"),
    }
}

#[test]
fn test_validate_bgp_version_invalid() {
    assert!(validate_bgp_version(3).is_err());
    assert!(validate_bgp_version(5).is_err());
    
    let err = validate_bgp_version(3).unwrap_err();
    match err {
        BgpValidationError::InvalidVersion { actual, expected } => {
            assert_eq!(actual, 3);
            assert_eq!(expected, VERSION);
        }
        _ => panic!("Expected InvalidVersion error"),
    }
}

#[test]
fn test_validate_asn_invalid() {
    assert!(validate_asn(0).is_err());
    
    let err = validate_asn(0).unwrap_err();
    match err {
        BgpValidationError::InvalidAsn(asn) => {
            assert_eq!(asn, 0);
        }
        _ => panic!("Expected InvalidAsn error"),
    }
}

#[test]
fn test_validate_hold_time_invalid() {
    assert!(validate_hold_time(1).is_err());
    assert!(validate_hold_time(2).is_err());
    
    let err = validate_hold_time(1).unwrap_err();
    match err {
        BgpValidationError::InvalidHoldTime(hold_time) => {
            assert_eq!(hold_time, 1);
        }
        _ => panic!("Expected InvalidHoldTime error"),
    }
}

#[test]
fn test_validate_router_id_invalid() {
    assert!(validate_router_id(0).is_err());
    
    let err = validate_router_id(0).unwrap_err();
    match err {
        BgpValidationError::InvalidRouterId(router_id) => {
            assert_eq!(router_id, 0);
        }
        _ => panic!("Expected InvalidRouterId error"),
    }
}

#[test]
fn test_validate_nlri_prefix_length_invalid() {
    assert!(validate_nlri_prefix_length(33, &Afi::Ipv4).is_err());
    assert!(validate_nlri_prefix_length(129, &Afi::Ipv6).is_err());
    
    let err = validate_nlri_prefix_length(33, &Afi::Ipv4).unwrap_err();
    match err {
        BgpValidationError::InvalidNlriPrefixLength(prefix_len) => {
            assert_eq!(prefix_len, 33);
        }
        _ => panic!("Expected InvalidNlriPrefixLength error"),
    }
}

#[test]
fn test_safe_slice_invalid() {
    let buffer = vec![1, 2, 3, 4, 5];
    
    assert!(safe_slice(&buffer, 0, 6).is_err()); // End beyond buffer
    assert!(safe_slice(&buffer, 3, 2).is_err()); // Start > end
    assert!(safe_slice(&buffer, 6, 7).is_err()); // Start beyond buffer
    
    let err = safe_slice(&buffer, 0, 6).unwrap_err();
    match err {
        BgpValidationError::InvalidBufferBounds { offset, length: _, buffer_size } => {
            assert_eq!(offset, 0);
            assert_eq!(buffer_size, 5);
        }
        _ => panic!("Expected InvalidBufferBounds error"),
    }
}

#[test]
fn test_safe_array_invalid() {
    let buffer = vec![1, 2, 3, 4, 5];
    
    let arr: Result<[u8; 6], _> = safe_array(&buffer, 0);
    assert!(arr.is_err()); // Not enough bytes
    
    let arr: Result<[u8; 3], _> = safe_array(&buffer, 3);
    assert!(arr.is_err()); // Not enough bytes from offset 3
    
    let err: BgpValidationError = safe_array::<3>(&buffer, 3).unwrap_err();
    match err {
        BgpValidationError::InvalidBufferBounds { offset, length, buffer_size } => {
            assert_eq!(offset, 3);
            assert_eq!(length, 3);
            assert_eq!(buffer_size, 5);
        }
        _ => panic!("Expected InvalidBufferBounds error"),
    }
}

#[test]
fn test_validate_buffer_bounds_invalid() {
    let buffer = vec![1, 2, 3, 4, 5];
    
    assert!(validate_buffer_bounds(&buffer, 0, 6).is_err());
    assert!(validate_buffer_bounds(&buffer, 5, 1).is_err());
    assert!(validate_buffer_bounds(&buffer, 6, 0).is_err());
    
    let err = validate_buffer_bounds(&buffer, 0, 6).unwrap_err();
    match err {
        BgpValidationError::InvalidBufferBounds { offset, length, buffer_size } => {
            assert_eq!(offset, 0);
            assert_eq!(length, 6);
            assert_eq!(buffer_size, 5);
        }
        _ => panic!("Expected InvalidBufferBounds error"),
    }
}

#[test]
fn test_enum_from_primitive_invalid() {
    assert_eq!(Afi::from_u16(0), None);
    assert_eq!(Afi::from_u16(3), None);
    assert_eq!(Afi::from_u16(65535), None);
    
    assert_eq!(Safi::from_u8(0), None);
    assert_eq!(Safi::from_u8(3), None);
    assert_eq!(Safi::from_u8(255), None);
    
    assert_eq!(MessageType::from_u8(0), None);
    assert_eq!(MessageType::from_u8(5), None);
    assert_eq!(MessageType::from_u8(255), None);
}

// Edge case tests
#[test]
fn test_validate_message_length_edge_cases() {
    // Test exact boundaries
    assert!(validate_message_length(MIN_MESSAGE_LENGTH).is_ok());
    assert!(validate_message_length(MAX_MESSAGE_LENGTH).is_ok());
    assert!(validate_message_length(MIN_MESSAGE_LENGTH - 1).is_err());
    assert!(validate_message_length(MAX_MESSAGE_LENGTH + 1).is_err());
}

#[test]
fn test_validate_hold_time_edge_cases() {
    // Test exact boundaries
    assert!(validate_hold_time(0).is_ok()); // Special case: 0 is valid
    assert!(validate_hold_time(1).is_err());
    assert!(validate_hold_time(2).is_err());
    assert!(validate_hold_time(3).is_ok()); // Minimum valid non-zero value
}

#[test]
fn test_address_family_all_combinations() {
    let combinations = vec![
        (Afi::Ipv4, Safi::NLRIUnicast),
        (Afi::Ipv4, Safi::NLRIMulticast),
        (Afi::Ipv6, Safi::NLRIUnicast),
        (Afi::Ipv6, Safi::NLRIMulticast),
    ];
    
    for (afi, safi) in combinations {
        let af = AddressFamily { afi, safi };
        assert_eq!(af.afi, afi);
        assert_eq!(af.safi, safi);
    }
}

#[test]
fn test_is_extended_len_edge_cases() {
    // Test all possible bit patterns for the extended length bit
    assert!(!is_extended_len(0b00000000)); // 0x00
    assert!(is_extended_len(0b00010000)); // 0x10
    assert!(!is_extended_len(0b10000000)); // 0x80
    assert!(is_extended_len(0b10010000)); // 0x90
    assert!(!is_extended_len(0b11100000)); // 0xE0
    assert!(is_extended_len(0b11110000)); // 0xF0
}

#[test]
fn test_safe_slice_edge_cases() {
    let buffer = vec![1, 2, 3, 4, 5];
    
    // Test empty slice
    assert!(safe_slice(&buffer, 0, 0).is_ok());
    assert_eq!(safe_slice(&buffer, 0, 0).unwrap(), &[]);
    
    // Test slice at end
    assert!(safe_slice(&buffer, 5, 5).is_ok());
    assert_eq!(safe_slice(&buffer, 5, 5).unwrap(), &[]);
    
    // Test full buffer
    assert!(safe_slice(&buffer, 0, 5).is_ok());
    assert_eq!(safe_slice(&buffer, 0, 5).unwrap(), &[1, 2, 3, 4, 5]);
}

#[test]
fn test_safe_array_edge_cases() {
    let buffer = vec![1, 2, 3, 4, 5];
    
    // Test zero-length array
    let arr: Result<[u8; 0], _> = safe_array(&buffer, 0);
    assert!(arr.is_ok());
    assert_eq!(arr.unwrap(), []);
    
    // Test array at end of buffer
    let arr: Result<[u8; 0], _> = safe_array(&buffer, 5);
    assert!(arr.is_ok());
    assert_eq!(arr.unwrap(), []);
    
    // Test maximum length array
    let arr: Result<[u8; 5], _> = safe_array(&buffer, 0);
    assert!(arr.is_ok());
    assert_eq!(arr.unwrap(), [1, 2, 3, 4, 5]);
}

#[test]
fn test_bgp_validation_error_comprehensive_coverage() {
    let errors = vec![
        BgpValidationError::MessageTooShort { actual: 10, minimum: 19 },
        BgpValidationError::MessageTooLong { actual: 5000, maximum: 4096 },
        BgpValidationError::InvalidMarker,
        BgpValidationError::InvalidMessageType(99),
        BgpValidationError::InvalidVersion { actual: 3, expected: 4 },
        BgpValidationError::InvalidAsn(0),
        BgpValidationError::InvalidHoldTime(1),
        BgpValidationError::InvalidRouterId(0),
        BgpValidationError::InvalidOptionalParameterLength(255),
        BgpValidationError::InvalidPathAttributeLength(65536),
        BgpValidationError::InvalidNlriPrefixLength(33),
        BgpValidationError::InvalidBufferBounds { offset: 0, length: 10, buffer_size: 5 },
        BgpValidationError::MissingRequiredAttribute("ORIGIN".to_string()),
        BgpValidationError::MalformedAsPath("test".to_string()),
        BgpValidationError::InvalidNextHop("0.0.0.0".to_string()),
        BgpValidationError::InvalidCapability("unknown".to_string()),
    ];
    
    for err in errors {
        let (code, subcode) = err.to_notification_codes();
        
        // Verify that all errors map to valid error codes
        assert!(matches!(code, ErrorCode::MessageHeader | ErrorCode::OpenMessage | ErrorCode::UpdateMessage));
        assert!(subcode > 0 && subcode <= 11); // Valid subcodes range from 1-11
        
        // Verify that error displays work
        let display = format!("{}", err);
        assert!(!display.is_empty());
    }
}
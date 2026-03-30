
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
    assert!(!set.insert(af2));
    assert!(set.insert(af3));

    assert_eq!(set.len(), 2);
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
fn test_validate_nlri_prefix_length_valid() {
    assert!(prefix_bytes(0, &Afi::Ipv4).is_ok());
    assert!(prefix_bytes(24, &Afi::Ipv4).is_ok());
    assert!(prefix_bytes(32, &Afi::Ipv4).is_ok());

    assert!(prefix_bytes(0, &Afi::Ipv6).is_ok());
    assert!(prefix_bytes(64, &Afi::Ipv6).is_ok());
    assert!(prefix_bytes(128, &Afi::Ipv6).is_ok());
}

#[test]
fn test_is_extended_len_valid() {
    assert!(!is_extended_len(0b00000000));
    assert!(is_extended_len(0b00010000));
    assert!(!is_extended_len(0b11100000));
    assert!(is_extended_len(0b11110000));
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
    let err = BgpValidationError::MessageTooShort {
        actual: 10,
        minimum: 19,
    };
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
#[test]
fn test_validate_message_length_invalid() {
    assert!(validate_message_length(18).is_err());
    assert!(validate_message_length(4097).is_err());

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
        BgpValidationError::InvalidMarker => {}
        _ => panic!("Expected InvalidMarker error"),
    }
}

#[test]
fn test_validate_nlri_prefix_length_invalid() {
    assert!(prefix_bytes(33, &Afi::Ipv4).is_err());
    assert!(prefix_bytes(129, &Afi::Ipv6).is_err());

    let err = prefix_bytes(33, &Afi::Ipv4).unwrap_err();
    match err {
        BgpValidationError::InvalidNlriPrefixLength(prefix_len) => {
            assert_eq!(prefix_len, 33);
        }
        _ => panic!("Expected InvalidNlriPrefixLength error"),
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

#[test]
fn test_bgp_validation_error_comprehensive_coverage() {
    let errors = vec![
        BgpValidationError::MessageTooShort {
            actual: 10,
            minimum: 19,
        },
        BgpValidationError::MessageTooLong {
            actual: 5000,
            maximum: 4096,
        },
        BgpValidationError::InvalidMarker,
        BgpValidationError::InvalidMessageType(99),
        BgpValidationError::InvalidVersion {
            actual: 3,
            expected: 4,
        },
        BgpValidationError::InvalidAsn(0),
        BgpValidationError::InvalidHoldTime(1),
        BgpValidationError::InvalidRouterId(0),
        BgpValidationError::InvalidOptionalParameterLength(255),
        BgpValidationError::InvalidPathAttributeLength(65536),
        BgpValidationError::InvalidNlriPrefixLength(33),
        BgpValidationError::InvalidBufferBounds {
            offset: 0,
            length: 10,
            buffer_size: 5,
        },
        BgpValidationError::MissingRequiredAttribute("ORIGIN".to_string()),
        BgpValidationError::MalformedAsPath("test".to_string()),
        BgpValidationError::InvalidNextHop("0.0.0.0".to_string()),
        BgpValidationError::InvalidCapability("unknown".to_string()),
    ];

    for err in errors {
        let (code, subcode) = err.to_notification_codes();
        assert!(matches!(
            code,
            ErrorCode::MessageHeader | ErrorCode::OpenMessage | ErrorCode::UpdateMessage
        ));
        assert!(subcode > 0 && subcode <= 11);
        let display = format!("{}", err);
        assert!(!display.is_empty());
    }
}

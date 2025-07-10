// Valid input tests
#[test]
fn test_origin_type_valid() {
    let origin = OriginType::Igp;
    assert_eq!(origin as u8, 0);
    
    let origin = OriginType::Egp;
    assert_eq!(origin as u8, 1);
    
    let origin = OriginType::Incomplete;
    assert_eq!(origin as u8, 2);
}

#[test]
fn test_aspath_segment_valid() {
    let segment = ASPATHSegment {
        segment_type: ASPATHSegmentType::AsSequence,
        as_list: vec![65000, 65001, 65002],
    };
    
    assert_eq!(segment.len(), 3);
    assert_eq!(segment.segment_type, ASPATHSegmentType::AsSequence);
    assert_eq!(segment.as_list, vec![65000, 65001, 65002]);
}

#[test]
fn test_aspath_segment_as_set_valid() {
    let segment = ASPATHSegment {
        segment_type: ASPATHSegmentType::AsSet,
        as_list: vec![65000, 65001],
    };
    
    // AS_SET length is always 1 regardless of actual list size
    assert_eq!(segment.len(), 1);
    assert_eq!(segment.segment_type, ASPATHSegmentType::AsSet);
}

#[test]
fn test_aspath_segment_serialization_valid() {
    let segment = ASPATHSegment {
        segment_type: ASPATHSegmentType::AsSequence,
        as_list: vec![65000, 65001],
    };
    
    let bytes: Vec<u8> = segment.into();
    
    // Format: [segment_type, as_count, as1_high, as1_low, as2_high, as2_low]
    assert_eq!(bytes[0], ASPATHSegmentType::AsSequence as u8);
    assert_eq!(bytes[1], 2); // AS count
    assert_eq!(bytes[2..4], [0xFD, 0xE8]); // 65000 in big endian
    assert_eq!(bytes[4..6], [0xFD, 0xE9]); // 65001 in big endian
}

#[test]
fn test_aspath_flatten_valid() {
    let segments = vec![
        ASPATHSegment {
            segment_type: ASPATHSegmentType::AsSequence,
            as_list: vec![65000, 65001],
        },
        ASPATHSegment {
            segment_type: ASPATHSegmentType::AsSet,
            as_list: vec![65002, 65003],
        },
    ];
    
    let flattened = segments.flatten_aspath();
    assert_eq!(flattened, vec![65000, 65001, 65002, 65003]);
}

#[test]
fn test_aggregator_value_valid() {
    let aggregator = AggregatorValue {
        last_as: 65000,
        aggregator: Ipv4Addr::new(192, 0, 2, 1),
    };
    
    assert_eq!(aggregator.last_as, 65000);
    assert_eq!(aggregator.aggregator, Ipv4Addr::new(192, 0, 2, 1));
}

#[test]
fn test_path_attribute_origin_valid() {
    let attr = PathAttribute::origin(OriginType::Igp);
    
    assert_eq!(attr.type_code, PathAttributeType::Origin);
    assert_eq!(attr.value, PathAttributeValue::Origin(OriginType::Igp));
    assert!(!attr.optional);
    assert!(attr.transitive);
    assert!(!attr.partial);
    assert!(!attr.extended_length);
}

#[test]
fn test_path_attribute_aspath_valid() {
    let aspath = vec![ASPATHSegment {
        segment_type: ASPATHSegmentType::AsSequence,
        as_list: vec![65000],
    }];
    
    let attr = PathAttribute::aspath(aspath.clone());
    
    assert_eq!(attr.type_code, PathAttributeType::AsPath);
    assert_eq!(attr.value, PathAttributeValue::AsPath(aspath));
    assert!(!attr.optional);
    assert!(attr.transitive);
    assert!(!attr.partial);
    assert!(!attr.extended_length);
}

#[test]
fn test_path_attribute_nexthop_valid() {
    let nexthop = Ipv4Addr::new(192, 0, 2, 1);
    let attr = PathAttribute::nexthop(nexthop);
    
    assert_eq!(attr.type_code, PathAttributeType::NextHop);
    assert_eq!(attr.value, PathAttributeValue::NextHop(nexthop));
    assert!(!attr.optional);
    assert!(attr.transitive);
    assert!(!attr.partial);
    assert!(!attr.extended_length);
}

#[test]
fn test_path_attribute_med_valid() {
    let med = 100;
    let attr = PathAttribute::med(med);
    
    assert_eq!(attr.type_code, PathAttributeType::MultiExitDisc);
    assert_eq!(attr.value, PathAttributeValue::MultiExitDisc(med));
    assert!(attr.optional);
    assert!(!attr.transitive);
    assert!(!attr.partial);
    assert!(!attr.extended_length);
}

#[test]
fn test_path_attribute_local_pref_valid() {
    let pref = 100;
    let attr = PathAttribute::local_pref(pref);
    
    assert_eq!(attr.type_code, PathAttributeType::LocalPref);
    assert_eq!(attr.value, PathAttributeValue::LocalPref(pref));
    assert!(attr.optional);
    assert!(!attr.transitive);
    assert!(!attr.partial);
    assert!(!attr.extended_length);
}

#[test]
fn test_path_attribute_aggregator_valid() {
    let last_as = 65000;
    let aggregator_ip = Ipv4Addr::new(192, 0, 2, 1);
    let attr = PathAttribute::aggregator(last_as, aggregator_ip);
    
    assert_eq!(attr.type_code, PathAttributeType::Aggregator);
    if let PathAttributeValue::Aggregator(agg) = attr.value {
        assert_eq!(agg.last_as, last_as);
        assert_eq!(agg.aggregator, aggregator_ip);
    } else {
        panic!("Expected Aggregator value");
    }
    assert!(attr.optional);
    assert!(attr.transitive);
    assert!(!attr.partial);
    assert!(!attr.extended_length);
}

#[test]
fn test_path_attribute_transitive_check_valid() {
    let attr = PathAttribute::origin(OriginType::Igp);
    assert!(attr.is_transitive());
    
    let attr = PathAttribute::med(100);
    assert!(!attr.is_transitive());
}

// Invalid input tests
#[test]
fn test_path_attribute_from_empty_bytes_invalid() {
    let empty_bytes: Vec<u8> = vec![];
    
    // This should panic or return error for empty input
    std::panic::catch_unwind(|| {
        let _attr: PathAttribute = empty_bytes.into();
    }).expect_err("Should panic on empty input");
}

#[test]
fn test_path_attribute_from_insufficient_bytes_invalid() {
    let insufficient_bytes: Vec<u8> = vec![0x40]; // Only flags byte
    
    // This should panic or return error for insufficient input
    std::panic::catch_unwind(|| {
        let _attr: PathAttribute = insufficient_bytes.into();
    }).expect_err("Should panic on insufficient input");
}

#[test]
fn test_path_attribute_from_invalid_type_code_invalid() {
    // Use invalid type code (255)
    let invalid_bytes: Vec<u8> = vec![0x40, 255, 1, 0]; // flags, invalid_type, length, data
    
    // This should panic or return error for invalid type code
    std::panic::catch_unwind(|| {
        let _attr: PathAttribute = invalid_bytes.into();
    }).expect_err("Should panic on invalid type code");
}

#[test]
fn test_path_attribute_origin_invalid_value() {
    // Origin attribute with invalid origin value (3 is not defined)
    let invalid_bytes: Vec<u8> = vec![0x40, 1, 1, 3]; // flags, origin_type, length, invalid_value
    
    // This should panic or return error for invalid origin value
    std::panic::catch_unwind(|| {
        let _attr: PathAttribute = invalid_bytes.into();
    }).expect_err("Should panic on invalid origin value");
}

#[test]
fn test_path_attribute_nexthop_insufficient_bytes_invalid() {
    // NextHop attribute with insufficient bytes (needs 4 bytes for IPv4)
    let invalid_bytes: Vec<u8> = vec![0x40, 3, 3, 192, 0, 2]; // flags, nexthop_type, length, partial_ip
    
    // This should panic or return error for insufficient bytes
    std::panic::catch_unwind(|| {
        let _attr: PathAttribute = invalid_bytes.into();
    }).expect_err("Should panic on insufficient bytes for nexthop");
}

#[test]
fn test_path_attribute_aspath_invalid_segment_type() {
    // AS_PATH with invalid segment type (3 is not defined)
    let invalid_bytes: Vec<u8> = vec![
        0x40, 2, 4, // flags, as_path_type, length
        3, 1, 0xFD, 0xE8 // invalid_segment_type, as_count=1, as=65000
    ];
    
    // This should panic or return error for invalid segment type
    std::panic::catch_unwind(|| {
        let _attr: PathAttribute = invalid_bytes.into();
    }).expect_err("Should panic on invalid AS_PATH segment type");
}

#[test]
fn test_path_attribute_aspath_inconsistent_length_invalid() {
    // AS_PATH with inconsistent length (says 2 ASes but only provides 1)
    let invalid_bytes: Vec<u8> = vec![
        0x40, 2, 4, // flags, as_path_type, length
        2, 2, 0xFD, 0xE8 // as_sequence, as_count=2, but only one AS (65000)
    ];
    
    // This should panic or return error for inconsistent length
    std::panic::catch_unwind(|| {
        let _attr: PathAttribute = invalid_bytes.into();
    }).expect_err("Should panic on inconsistent AS_PATH length");
}

#[test]
fn test_path_attribute_med_insufficient_bytes_invalid() {
    // MED attribute with insufficient bytes (needs 4 bytes for u32)
    let invalid_bytes: Vec<u8> = vec![0x80, 4, 2, 0x00, 0x64]; // flags, med_type, length, partial_value
    
    // This should panic or return error for insufficient bytes
    std::panic::catch_unwind(|| {
        let _attr: PathAttribute = invalid_bytes.into();
    }).expect_err("Should panic on insufficient bytes for MED");
}

#[test]
fn test_aspath_segment_empty_as_list_edge_case() {
    let segment = ASPATHSegment {
        segment_type: ASPATHSegmentType::AsSequence,
        as_list: vec![], // Empty AS list
    };
    
    assert_eq!(segment.len(), 0);
    
    let bytes: Vec<u8> = segment.into();
    assert_eq!(bytes[0], ASPATHSegmentType::AsSequence as u8);
    assert_eq!(bytes[1], 0); // AS count should be 0
    assert_eq!(bytes.len(), 2); // Only type and count bytes
}

#[test]
fn test_aspath_segment_large_as_list_edge_case() {
    // Test with maximum AS count (255)
    let large_as_list: Vec<u16> = (1..=255).collect();
    let segment = ASPATHSegment {
        segment_type: ASPATHSegmentType::AsSequence,
        as_list: large_as_list.clone(),
    };
    
    assert_eq!(segment.len(), 255);
    
    let bytes: Vec<u8> = segment.into();
    assert_eq!(bytes[0], ASPATHSegmentType::AsSequence as u8);
    assert_eq!(bytes[1], 255); // AS count
    assert_eq!(bytes.len(), 2 + 255 * 2); // type + count + 255 * 2 bytes for ASes
}

#[test]
fn test_aggregator_value_edge_cases() {
    // Test with AS 0 (should be invalid in practice but struct allows it)
    let aggregator = AggregatorValue {
        last_as: 0,
        aggregator: Ipv4Addr::new(0, 0, 0, 0),
    };
    
    assert_eq!(aggregator.last_as, 0);
    assert_eq!(aggregator.aggregator, Ipv4Addr::new(0, 0, 0, 0));
    
    // Test with maximum AS number
    let aggregator = AggregatorValue {
        last_as: 65535,
        aggregator: Ipv4Addr::new(255, 255, 255, 255),
    };
    
    assert_eq!(aggregator.last_as, 65535);
    assert_eq!(aggregator.aggregator, Ipv4Addr::new(255, 255, 255, 255));
}
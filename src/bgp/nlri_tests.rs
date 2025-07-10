#[test]
fn test_nlri_ipv4_valid() {
    let net: Ipv4Net = "192.0.2.0/24".parse().unwrap();
    let nlri = NlriBuilder::default()
        .net(IpNet::V4(net))
        .build()
        .unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 24);
    if let IpNet::V4(v4_net) = nlri.net {
        assert_eq!(v4_net.network(), Ipv4Addr::new(192, 0, 2, 0));
        assert_eq!(v4_net.prefix_len(), 24);
    } else {
        panic!("Expected IPv4 network");
    }
}

#[test]
fn test_nlri_ipv6_valid() {
    let net: Ipv6Net = "2001:db8::/32".parse().unwrap();
    let nlri = NlriBuilder::default()
        .net(IpNet::V6(net))
        .build()
        .unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 32);
    if let IpNet::V6(v6_net) = nlri.net {
        assert_eq!(v6_net.network(), "2001:db8::".parse::<Ipv6Addr>().unwrap());
        assert_eq!(v6_net.prefix_len(), 32);
    } else {
        panic!("Expected IPv6 network");
    }
}

#[test]
fn test_nlri_serialization_ipv4_valid() {
    let net: Ipv4Net = "192.0.2.0/24".parse().unwrap();
    let nlri = NlriBuilder::default()
        .net(IpNet::V4(net))
        .build()
        .unwrap();
    
    let bytes: Vec<u8> = nlri.into();
    assert_eq!(bytes[0], 24);
    assert_eq!(bytes[1], 192);
    assert_eq!(bytes[2], 0);  
    assert_eq!(bytes[3], 2);  
    assert_eq!(bytes.len(), 4);
}

#[test]
fn test_nlri_serialization_ipv6_valid() {
    let net: Ipv6Net = "2001:db8::/32".parse().unwrap();
    let nlri = NlriBuilder::default()
        .net(IpNet::V6(net))
        .build()
        .unwrap();
    
    let bytes: Vec<u8> = nlri.into();
    assert_eq!(bytes[0], 32);
    assert_eq!(bytes[1], 0x20);
    assert_eq!(bytes[2], 0x01);
    assert_eq!(bytes[3], 0x0d);
    assert_eq!(bytes[4], 0xb8);
    assert_eq!(bytes.len(), 5);
}

#[test]
fn test_nlri_from_ipv4_octets_valid() {
    let octets = Ipv4Octets {
        octets: vec![24, 192, 0, 2],
    };
    
    let nlri: Nlri = octets.try_into().unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 24);
    if let IpNet::V4(v4_net) = nlri.net {
        assert_eq!(v4_net.network(), Ipv4Addr::new(192, 0, 2, 0));
    } else {
        panic!("Expected IPv4 network");
    }
}

#[test]
fn test_nlri_from_ipv6_octets_valid() {
    let octets = Ipv6Octets {
        octets: vec![32, 0x20, 0x01, 0x0d, 0xb8],
    };
    
    let nlri: Nlri = octets.try_into().unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 32);
    if let IpNet::V6(v6_net) = nlri.net {
        assert_eq!(v6_net.network(), "2001:db8::".parse::<Ipv6Addr>().unwrap());
    } else {
        panic!("Expected IPv6 network");
    }
}

#[test]
fn test_nlri_ipv4_host_route_valid() {
    let octets = Ipv4Octets {
        octets: vec![32, 192, 0, 2, 1],
    };
    
    let nlri: Nlri = octets.try_into().unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 32);
    if let IpNet::V4(v4_net) = nlri.net {
        assert_eq!(v4_net.network(), Ipv4Addr::new(192, 0, 2, 1));
    } else {
        panic!("Expected IPv4 network");
    }
}

#[test]
fn test_nlri_ipv4_default_route_valid() {
    let octets = Ipv4Octets {
        octets: vec![0],
    };
    
    let nlri: Nlri = octets.try_into().unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 0);
    if let IpNet::V4(v4_net) = nlri.net {
        assert_eq!(v4_net.network(), Ipv4Addr::new(0, 0, 0, 0));
    } else {
        panic!("Expected IPv4 network");
    }
}

#[test]
fn test_nlri_ipv6_host_route_valid() {
    let mut octets = vec![128];
    octets.extend_from_slice(&[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    
    let ipv6_octets = Ipv6Octets { octets };
    
    let nlri: Nlri = ipv6_octets.try_into().unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 128);
    if let IpNet::V6(v6_net) = nlri.net {
        assert_eq!(v6_net.network(), "2001:db8::1".parse::<Ipv6Addr>().unwrap());
    } else {
        panic!("Expected IPv6 network");
    }
}

#[test]
fn test_nlri_conversion_to_ipnet_valid() {
    let net: Ipv4Net = "10.0.0.0/8".parse().unwrap();
    let nlri = NlriBuilder::default()
        .net(IpNet::V4(net))
        .build()
        .unwrap();
    
    let ipnet: IpNet = nlri.into();
    let ipnet_ref: IpNet = (&nlri).into();
    
    assert_eq!(ipnet, IpNet::V4(net));
    assert_eq!(ipnet_ref, IpNet::V4(net));
}

#[test]
fn test_mpnlri_ipv4_valid() {
    let af = AddressFamily {
        afi: Afi::Ipv4,
        safi: Safi::NLRIUnicast,
    };
    let nh = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
    let nlri = NlriBuilder::default()
        .net("10.0.0.0/8".parse::<IpNet>().unwrap())
        .build()
        .unwrap();
    
    let mp_nlri = Mpnlri {
        af,
        nh,
        nlris: vec![nlri],
    };
    
    assert_eq!(mp_nlri.af.afi, Afi::Ipv4);
    assert_eq!(mp_nlri.af.safi, Safi::NLRIUnicast);
    assert_eq!(mp_nlri.nh, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)));
    assert_eq!(mp_nlri.nlris.len(), 1);
}

#[test]
fn test_mpnlri_ipv6_valid() {
    let af = AddressFamily {
        afi: Afi::Ipv6,
        safi: Safi::NLRIUnicast,
    };
    let nh = IpAddr::V6("2001:db8::1".parse().unwrap());
    let nlri = NlriBuilder::default()
        .net("2001:db8::/32".parse::<IpNet>().unwrap())
        .build()
        .unwrap();
    
    let mp_nlri = Mpnlri {
        af,
        nh,
        nlris: vec![nlri],
    };
    
    assert_eq!(mp_nlri.af.afi, Afi::Ipv6);
    assert_eq!(mp_nlri.af.safi, Safi::NLRIUnicast);
    assert_eq!(mp_nlri.nh, IpAddr::V6("2001:db8::1".parse().unwrap()));
    assert_eq!(mp_nlri.nlris.len(), 1);
}

#[test]
fn test_prefix_bytes_calculation_valid() {
    assert_eq!(prefix_bytes(0), 0);
    assert_eq!(prefix_bytes(1), 1);
    assert_eq!(prefix_bytes(8), 1);
    assert_eq!(prefix_bytes(9), 2);
    assert_eq!(prefix_bytes(16), 2);
    assert_eq!(prefix_bytes(17), 3);
    assert_eq!(prefix_bytes(24), 3);
    assert_eq!(prefix_bytes(25), 4);
    assert_eq!(prefix_bytes(32), 4);
    assert_eq!(prefix_bytes(33), 5);
    assert_eq!(prefix_bytes(64), 8);
    assert_eq!(prefix_bytes(128), 16);
}
#[test]
fn test_nlri_from_empty_ipv4_octets_invalid() {
    let octets = Ipv4Octets {
        octets: vec![],
    };
    let result: Result<Nlri, _> = octets.try_into();
    assert!(result.is_err(), "Should return error on empty octets");
}

#[test]
fn test_nlri_from_empty_ipv6_octets_invalid() {
    let octets = Ipv6Octets {
        octets: vec![],
    };
    let result: Result<Nlri, _> = octets.try_into();
    assert!(result.is_err(), "Should return error on empty octets");
}

#[test]
fn test_nlri_from_invalid_ipv4_prefix_length_invalid() {
    let octets = Ipv4Octets {
        octets: vec![33, 192, 0, 2, 1],
    };
    let result: Result<Nlri, _> = octets.try_into();
    assert!(result.is_err(), "Should return error on invalid IPv4 prefix length");
}

#[test]
fn test_nlri_from_invalid_ipv6_prefix_length_invalid() {
    let mut octets = vec![129];
    octets.extend_from_slice(&[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    
    let ipv6_octets = Ipv6Octets { octets };
    let result: Result<Nlri, _> = ipv6_octets.try_into();
    assert!(result.is_err(), "Should return error on invalid IPv6 prefix length");
}

#[test]
fn test_nlri_from_insufficient_ipv4_octets_invalid() {
    let octets = Ipv4Octets {
        octets: vec![24, 192],
    };
    let result: Result<Nlri, _> = octets.try_into();
    assert!(result.is_err(), "Should return error on insufficient octets");
}

#[test]
fn test_nlri_from_insufficient_ipv6_octets_invalid() {
    let octets = Ipv6Octets {
        octets: vec![64, 0x20, 0x01],
    };
    let result: Result<Nlri, _> = octets.try_into();
    assert!(result.is_err(), "Should return error on insufficient octets");
}

#[test]
fn test_mpnlri_from_empty_bytes_invalid() {
    let empty_bytes: Vec<u8> = vec![];
    let result: Result<Mpnlri, _> = empty_bytes.try_into();
    assert!(result.is_err(), "Should return error on empty bytes");
}

#[test]
fn test_mpnlri_from_insufficient_bytes_invalid() {
    let insufficient_bytes: Vec<u8> = vec![10, 0, 1];
    let result: Result<Mpnlri, _> = insufficient_bytes.try_into();
    assert!(result.is_err(), "Should return error on insufficient bytes");
}

#[test]
fn test_mpnlri_from_invalid_afi_invalid() {
    let invalid_bytes: Vec<u8> = vec![8, 0, 99, 1, 4, 192, 0, 2, 1];
    let result: Result<Mpnlri, _> = invalid_bytes.try_into();
    assert!(result.is_err(), "Should return error on invalid AFI");
}

#[test]
fn test_mpnlri_from_invalid_safi_invalid() {
    let invalid_bytes: Vec<u8> = vec![8, 0, 1, 99, 4, 192, 0, 2, 1];
    let result: Result<Mpnlri, _> = invalid_bytes.try_into();
    assert!(result.is_err(), "Should return error on invalid SAFI");
}

#[test]
fn test_mpnlri_inconsistent_nexthop_length_invalid() {
    let invalid_bytes: Vec<u8> = vec![8, 0, 1, 1, 10, 192, 0, 2, 1];
    let result: Result<Mpnlri, _> = invalid_bytes.try_into();
    assert!(result.is_err(), "Should return error on inconsistent nexthop length");
}

#[test]
fn test_mpunlri_from_empty_bytes_invalid() {
    let empty_bytes: Vec<u8> = vec![];
    let result: Result<Mpunlri, _> = empty_bytes.try_into();
    assert!(result.is_err(), "Should return error on empty bytes");
}

#[test]
fn test_mpunlri_from_insufficient_bytes_invalid() {
    let insufficient_bytes: Vec<u8> = vec![10, 0, 1];
    let result: Result<Mpunlri, _> = insufficient_bytes.try_into();
    assert!(result.is_err(), "Should return error on insufficient bytes");
}

#[test]
fn test_mpunlri_from_invalid_afi_invalid() {
    let invalid_bytes: Vec<u8> = vec![5, 0, 99, 1, 8, 10];
    let result: Result<Mpunlri, _> = invalid_bytes.try_into();
    assert!(result.is_err(), "Should return error on invalid AFI");
}

#[test]
fn test_mpunlri_from_invalid_safi_invalid() {
    let invalid_bytes: Vec<u8> = vec![5, 0, 1, 99, 8, 10];
    let result: Result<Mpunlri, _> = invalid_bytes.try_into();
    assert!(result.is_err(), "Should return error on invalid SAFI");
}

#[test]
fn test_nlri_ipv4_partial_octet_valid() {
    let octets = Ipv4Octets {
        octets: vec![12, 172, 16],
    };
    
    let nlri: Nlri = octets.try_into().unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 12);
    if let IpNet::V4(v4_net) = nlri.net {
        assert_eq!(v4_net.network(), Ipv4Addr::new(172, 16, 0, 0));
    } else {
        panic!("Expected IPv4 network");
    }
}

#[test]
fn test_nlri_ipv6_partial_octet_valid() {
    let octets = Ipv6Octets {
        octets: vec![48, 0x20, 0x01, 0x0d, 0xb8, 0x12, 0x34],
    };
    
    let nlri: Nlri = octets.try_into().unwrap();
    
    assert_eq!(nlri.net.prefix_len(), 48);
    if let IpNet::V6(v6_net) = nlri.net {
        let expected = "2001:db8:1234::".parse::<Ipv6Addr>().unwrap();
        assert_eq!(v6_net.network(), expected);
    } else {
        panic!("Expected IPv6 network");
    }
}

#[test]
fn test_mpnlri_with_multiple_nlris_valid() {
    let nlri1 = NlriBuilder::default()
        .net("10.0.0.0/8".parse::<IpNet>().unwrap())
        .build()
        .unwrap();
    let nlri2 = NlriBuilder::default()
        .net("192.168.0.0/16".parse::<IpNet>().unwrap())
        .build()
        .unwrap();
    
    let mp_nlri = Mpnlri {
        af: AddressFamily {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        },
        nh: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        nlris: vec![nlri1, nlri2],
    };
    
    assert_eq!(mp_nlri.nlris.len(), 2);
    assert_eq!(mp_nlri.nlris[0].net.prefix_len(), 8);
    assert_eq!(mp_nlri.nlris[1].net.prefix_len(), 16);
}

#[test]
fn test_mpnlri_serialization_ipv4_valid() {
    let nlri = NlriBuilder::default()
        .net("10.0.0.0/8".parse::<IpNet>().unwrap())
        .build()
        .unwrap();
    
    let mp_nlri = Mpnlri {
        af: AddressFamily {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        },
        nh: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        nlris: vec![nlri],
    };
    
    let bytes: Vec<u8> = mp_nlri.into();
    assert!(bytes.len() > 8);
    assert_eq!(bytes[1], 0);
    assert_eq!(bytes[2], 1);
    assert_eq!(bytes[3], 1);
    assert_eq!(bytes[4], 4);
    assert_eq!(bytes[5], 192);
    assert_eq!(bytes[6], 0);  
    assert_eq!(bytes[7], 2);  
    assert_eq!(bytes[8], 1);  
}

#[test]
fn test_mpunlri_serialization_ipv4_valid() {
    let nlri = NlriBuilder::default()
        .net("10.0.0.0/8".parse::<IpNet>().unwrap())
        .build()
        .unwrap();
    
    let mp_unlri = Mpunlri {
        af: AddressFamily {
            afi: Afi::Ipv4,
            safi: Safi::NLRIUnicast,
        },
        nlris: vec![nlri],
    };
    
    let bytes: Vec<u8> = mp_unlri.into();
    assert!(bytes.len() > 5);
    assert_eq!(bytes[1], 0);
    assert_eq!(bytes[2], 1);
    assert_eq!(bytes[3], 1);
}

#[test]
fn test_decoder_minimum_length() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[0xff; 10]);
    assert!(codec.decode(&mut buf).unwrap().is_none());
}

#[test]
fn test_decoder_invalid_marker() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    let mut invalid_msg = vec![0xfe; 16];
    invalid_msg.extend_from_slice(&[0, 19]);
    invalid_msg.push(4);
    buf.extend_from_slice(&invalid_msg);
    assert!(codec.decode(&mut buf).is_err());
}

#[test]
fn test_decoder_invalid_message_type() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    let mut invalid_msg = MARKER.to_vec();
    invalid_msg.extend_from_slice(&[0, 19]);
    invalid_msg.push(99);
    buf.extend_from_slice(&invalid_msg);
    assert!(codec.decode(&mut buf).is_err());
}

#[test]
fn test_decoder_valid_keepalive() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    let mut valid_msg = MARKER.to_vec();
    valid_msg.extend_from_slice(&[0, 19]);
    valid_msg.push(4);
    buf.extend_from_slice(&valid_msg);
    let result = codec.decode(&mut buf).unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 19);
}

#[test]
fn test_encoder_message_too_large() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    let large_data = vec![0; MAX_MESSAGE_LENGTH];
    assert!(codec.encode(large_data, &mut buf).is_err());
}

#[test]
fn test_encoder_valid_message() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    let keepalive_body = vec![4];
    assert!(codec.encode(keepalive_body, &mut buf).is_ok());
    assert_eq!(buf.len(), 19);
    assert_eq!(&buf[0..16], &MARKER);
    assert_eq!(&buf[16..18], &[0, 19]);
    assert_eq!(buf[18], 4);
}

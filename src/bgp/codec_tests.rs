#[test]
fn test_decoder_minimum_length() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    // Test with insufficient data
    buf.extend_from_slice(&[0xff; 10]);
    assert!(codec.decode(&mut buf).unwrap().is_none());
}

#[test]
fn test_decoder_invalid_marker() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    // Create message with invalid marker
    let mut invalid_msg = vec![0xfe; 16]; // Wrong marker
    invalid_msg.extend_from_slice(&[0, 19]); // Length
    invalid_msg.push(4); // Keepalive type
    buf.extend_from_slice(&invalid_msg);
    assert!(codec.decode(&mut buf).is_err());
}

#[test]
fn test_decoder_invalid_message_type() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    // Create message with invalid type
    let mut invalid_msg = MARKER.to_vec();
    invalid_msg.extend_from_slice(&[0, 19]); // Length
    invalid_msg.push(99); // Invalid message type
    buf.extend_from_slice(&invalid_msg);
    assert!(codec.decode(&mut buf).is_err());
}

#[test]
fn test_decoder_valid_keepalive() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    // Create valid keepalive message
    let mut valid_msg = MARKER.to_vec();
    valid_msg.extend_from_slice(&[0, 19]); // Length
    valid_msg.push(4); // Keepalive type
    buf.extend_from_slice(&valid_msg);
    let result = codec.decode(&mut buf).unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 19);
}

#[test]
fn test_encoder_message_too_large() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    // Try to encode message that's too large
    let large_data = vec![0; MAX_MESSAGE_LENGTH];
    assert!(codec.encode(large_data, &mut buf).is_err());
}

#[test]
fn test_encoder_valid_message() {
    let mut codec = BGPMessageCodec;
    let mut buf = BytesMut::new();
    // Encode valid keepalive body (empty)
    let keepalive_body = vec![4]; // Just the message type
    assert!(codec.encode(keepalive_body, &mut buf).is_ok());
    // Verify the encoded message
    assert_eq!(buf.len(), 19); // 16 + 2 + 1
    assert_eq!(&buf[0..16], &MARKER);
    assert_eq!(&buf[16..18], &[0, 19]); // Length
    assert_eq!(buf[18], 4); // Message type
}
use bytes::{Buf, BytesMut};
use num_traits::FromPrimitive;
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Encoder, Framed};

use super::types::*;

pub struct BGPMessageCodec;
type BGPConnection = Framed<TcpStream, BGPMessageCodec>;

impl BGPMessageCodec {
    pub async fn _frame_it(socket: TcpStream) -> Result<BGPConnection, std::io::Error> {
        let server = Framed::new(socket, BGPMessageCodec);
        Ok(server)
    }
}

impl Decoder for BGPMessageCodec {
    type Item = Vec<u8>;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Check if we have enough data for the minimum BGP message
        if src.len() < MIN_MESSAGE_LENGTH {
            return Ok(None);
        }

        // Validate the marker
        if !src.starts_with(&MARKER) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid BGP marker - message should start with 16 bytes of 0xFF",
            ));
        }

        // Validate we have enough bytes for the length field
        if src.len() < 18 {
            return Ok(None);
        }

        // Extract and validate length
        let mut length_bytes = [0u8; 2];
        length_bytes.copy_from_slice(&src[16..18]);
        let length = u16::from_be_bytes(length_bytes) as usize;

        // Validate message length bounds
        if let Err(e) = validate_message_length(length) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid BGP message length: {}", e),
            ));
        }

        // Check if we have the complete message
        if src.len() < length {
            return Ok(None);
        }

        // Validate message type if we have enough data
        if length >= MIN_MESSAGE_LENGTH {
            let message_type = src[18];
            if MessageType::from_u8(message_type).is_none() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid BGP message type: {}", message_type),
                ));
            }
        }

        // Extract the complete message
        let data = src[0..length].to_vec();
        src.advance(length);

        Ok(Some(data))
    }
}

impl Encoder<Vec<u8>> for BGPMessageCodec {
    type Error = std::io::Error;

    fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> Result<(), Self::Error> {
        // Validate message length
        let total_length = data.len() + MARKER.len() + 2; // +2 for length field

        if let Err(e) = validate_message_length(total_length) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("BGP message too large for encoding: {}", e),
            ));
        }

        // Reserve space for the complete message
        buf.reserve(total_length);

        // Write marker
        buf.extend_from_slice(&MARKER);

        // Write length
        let len_slice = u16::to_be_bytes(total_length as u16);
        buf.extend_from_slice(&len_slice);

        // Write message data
        buf.extend_from_slice(data.as_slice());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

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
}

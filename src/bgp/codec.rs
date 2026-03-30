use bytes::{Buf, BytesMut};
use num_traits::FromPrimitive;
use tokio_util::codec::{Decoder, Encoder};

use super::types::*;

pub struct BGPMessageCodec;

impl Decoder for BGPMessageCodec {
    type Item = Vec<u8>;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < MIN_MESSAGE_LENGTH {
            return Ok(None);
        }
        if !src.starts_with(&MARKER) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid BGP marker - message should start with 16 bytes of 0xFF",
            ));
        }
        if src.len() < 18 {
            return Ok(None);
        }
        let mut length_bytes = [0u8; 2];
        length_bytes.copy_from_slice(&src[16..18]);
        let length = u16::from_be_bytes(length_bytes) as usize;
        if let Err(e) = validate_message_length(length) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid BGP message length: {}", e),
            ));
        }
        if src.len() < length {
            return Ok(None);
        }
        if length >= MIN_MESSAGE_LENGTH {
            let message_type = src[18];
            if MessageType::from_u8(message_type).is_none() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid BGP message type: {}", message_type),
                ));
            }
        }
        let data = src[0..length].to_vec();
        src.advance(length);

        Ok(Some(data))
    }
}

impl Encoder<Vec<u8>> for BGPMessageCodec {
    type Error = std::io::Error;

    fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> Result<(), Self::Error> {
        let total_length = data.len() + MARKER.len() + 2;

        if let Err(e) = validate_message_length(total_length) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("BGP message too large for encoding: {}", e),
            ));
        }
        buf.reserve(total_length);
        buf.extend_from_slice(&MARKER);
        let len_slice = u16::to_be_bytes(total_length as u16);
        buf.extend_from_slice(&len_slice);
        buf.extend_from_slice(data.as_slice());

        Ok(())
    }
}

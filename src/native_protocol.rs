//! Pure framing helpers shared by the Firefox native host and fuzz targets.

use serde_json::Value;

/// Decode one complete little-endian Native Messaging frame.
///
/// The caller must provide exactly one frame. Truncated frames, trailing bytes,
/// empty payloads, oversized payloads, and invalid JSON are rejected.
pub fn decode_framed_json(input: &[u8], maximum_bytes: usize) -> Result<Value, String> {
    if input.len() < 4 {
        return Err("native frame is missing its four-byte length prefix".into());
    }
    let declared = u32::from_le_bytes(input[..4].try_into().expect("prefix length checked")) as usize;
    if declared == 0 || declared > maximum_bytes {
        return Err(format!(
            "native message length must be between 1 and {maximum_bytes} bytes"
        ));
    }
    if input.len() != declared.saturating_add(4) {
        return Err("native frame length does not match its payload".into());
    }
    decode_json_body(&input[4..], maximum_bytes)
}

/// Decode a Native Messaging JSON body after its stream prefix was consumed.
pub fn decode_json_body(body: &[u8], maximum_bytes: usize) -> Result<Value, String> {
    if body.is_empty() || body.len() > maximum_bytes {
        return Err(format!(
            "native message length must be between 1 and {maximum_bytes} bytes"
        ));
    }
    serde_json::from_slice(body).map_err(|error| format!("failed to decode the native message: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_one_complete_frame() {
        let body = br#"{"id":"one"}"#;
        let mut frame = Vec::new();
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(body);
        assert_eq!(decode_framed_json(&frame, 1024).unwrap()["id"], "one");
    }

    #[test]
    fn rejects_truncated_and_trailing_frames() {
        assert!(decode_framed_json(&[1, 0, 0], 1024).is_err());
        assert!(decode_framed_json(&[2, 0, 0, 0, b'{'], 1024).is_err());
        assert!(decode_framed_json(&[2, 0, 0, 0, b'{', b'}', b'x'], 1024).is_err());
    }

    #[test]
    fn rejects_zero_and_oversized_lengths() {
        assert!(decode_framed_json(&[0, 0, 0, 0], 1024).is_err());
        assert!(decode_framed_json(&[1, 4, 0, 0], 1024).is_err());
    }
}

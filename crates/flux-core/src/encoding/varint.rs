//! Variable-length integer encoding

use crate::{Error, Result};

/// Encode a u64 as varint
pub fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value >= 0x80 {
        buf.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// Decode a varint from bytes
/// Returns (value, bytes_consumed)
pub fn decode_varint(buf: &[u8]) -> Result<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut pos = 0;

    loop {
        if pos >= buf.len() {
            return Err(Error::DecodeError("Varint truncated".into()));
        }

        let byte = buf[pos];
        result |= ((byte & 0x7F) as u64) << shift;
        pos += 1;

        if byte & 0x80 == 0 {
            break;
        }

        shift += 7;
        if shift > 63 {
            return Err(Error::DecodeError("Varint too long".into()));
        }
    }

    Ok((result, pos))
}

/// ZigZag encode a signed integer
pub fn zigzag_encode(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

/// ZigZag decode to signed integer
pub fn zigzag_decode(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

/// Encode a signed integer as zigzag varint
pub fn encode_signed_varint(value: i64, buf: &mut Vec<u8>) {
    encode_varint(zigzag_encode(value), buf);
}

/// Decode a signed varint
pub fn decode_signed_varint(buf: &[u8]) -> Result<(i64, usize)> {
    let (unsigned, len) = decode_varint(buf)?;
    Ok((zigzag_decode(unsigned), len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_encode_decode() {
        let test_values = [
            0u64,
            1,
            127,
            128,
            255,
            256,
            16383,
            16384,
            2097151,
            2097152,
            u64::MAX,
        ];

        for &value in &test_values {
            let mut buf = Vec::new();
            encode_varint(value, &mut buf);

            let (decoded, len) = decode_varint(&buf).unwrap();
            assert_eq!(decoded, value, "Failed for value {}", value);
            assert_eq!(len, buf.len());
        }
    }

    #[test]
    fn test_varint_sizes() {
        // 0-127 should be 1 byte
        let mut buf = Vec::new();
        encode_varint(127, &mut buf);
        assert_eq!(buf.len(), 1);

        // 128-16383 should be 2 bytes
        buf.clear();
        encode_varint(128, &mut buf);
        assert_eq!(buf.len(), 2);

        buf.clear();
        encode_varint(16383, &mut buf);
        assert_eq!(buf.len(), 2);

        // 16384+ should be 3+ bytes
        buf.clear();
        encode_varint(16384, &mut buf);
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn test_zigzag() {
        assert_eq!(zigzag_encode(0), 0);
        assert_eq!(zigzag_encode(-1), 1);
        assert_eq!(zigzag_encode(1), 2);
        assert_eq!(zigzag_encode(-2), 3);
        assert_eq!(zigzag_encode(2), 4);

        // Roundtrip
        for i in -1000..1000 {
            assert_eq!(zigzag_decode(zigzag_encode(i)), i);
        }

        // Edge cases
        assert_eq!(zigzag_decode(zigzag_encode(i64::MIN)), i64::MIN);
        assert_eq!(zigzag_decode(zigzag_encode(i64::MAX)), i64::MAX);
    }

    #[test]
    fn test_signed_varint() {
        let test_values = [0i64, 1, -1, 127, -128, 1000, -1000, i64::MIN, i64::MAX];

        for &value in &test_values {
            let mut buf = Vec::new();
            encode_signed_varint(value, &mut buf);

            let (decoded, _) = decode_signed_varint(&buf).unwrap();
            assert_eq!(decoded, value);
        }
    }
}

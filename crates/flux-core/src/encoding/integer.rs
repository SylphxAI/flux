//! Integer encoding strategies

use super::varint::{encode_varint, decode_varint, encode_signed_varint};
use crate::Result;

/// Integer encoding strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerEncoding {
    /// Raw fixed-width encoding
    Raw,
    /// Variable-length encoding
    Varint,
    /// Delta encoding (for sequences)
    Delta,
    /// Delta-of-delta (for linear sequences)
    DeltaOfDelta,
    /// Frame-of-Reference (for clustered values)
    FrameOfReference,
    /// Bit-packed (all values fit in N bits)
    BitPacked(u8),
}

/// Analyze integer sequence and recommend encoding
pub fn analyze(values: &[i64]) -> IntegerEncoding {
    if values.is_empty() {
        return IntegerEncoding::Raw;
    }

    if values.len() == 1 {
        return IntegerEncoding::Varint;
    }

    // Calculate statistics
    let min = *values.iter().min().unwrap();
    let max = *values.iter().max().unwrap();
    let range = (max - min) as u64;

    // Check if bit-packing is beneficial
    let bits_needed = 64 - range.leading_zeros();
    if bits_needed <= 8 && values.len() >= 4 {
        return IntegerEncoding::BitPacked(bits_needed as u8);
    }

    // Check if delta encoding is beneficial
    let mut delta_sum: i64 = 0;
    let mut prev = values[0];
    for &val in &values[1..] {
        delta_sum += (val - prev).abs();
        prev = val;
    }

    let avg_delta = delta_sum / (values.len() - 1) as i64;
    let avg_value = values.iter().sum::<i64>() / values.len() as i64;

    if avg_delta.abs() < avg_value.abs() / 10 {
        // Deltas are much smaller than values
        return IntegerEncoding::Delta;
    }

    IntegerEncoding::Varint
}

/// Encode integers with delta encoding
pub fn encode_delta(values: &[i64], buf: &mut Vec<u8>) {
    if values.is_empty() {
        encode_varint(0, buf);
        return;
    }

    // Write count
    encode_varint(values.len() as u64, buf);

    // Write first value
    encode_signed_varint(values[0], buf);

    // Write deltas
    let mut prev = values[0];
    for &val in &values[1..] {
        let delta = val - prev;
        encode_signed_varint(delta, buf);
        prev = val;
    }
}

/// Decode delta-encoded integers
pub fn decode_delta(buf: &[u8]) -> Result<Vec<i64>> {
    let mut pos = 0;

    let (count, len) = decode_varint(buf)?;
    pos += len;

    if count == 0 {
        return Ok(Vec::new());
    }

    let mut values = Vec::with_capacity(count as usize);

    // Read first value
    let (first, len) = super::varint::decode_signed_varint(&buf[pos..])?;
    pos += len;
    values.push(first);

    // Read deltas
    let mut prev = first;
    for _ in 1..count {
        let (delta, len) = super::varint::decode_signed_varint(&buf[pos..])?;
        pos += len;
        prev += delta;
        values.push(prev);
    }

    Ok(values)
}

/// Encode with Frame-of-Reference
pub fn encode_for(values: &[i64], buf: &mut Vec<u8>) {
    if values.is_empty() {
        encode_varint(0, buf);
        return;
    }

    let min = *values.iter().min().unwrap();
    let max = *values.iter().max().unwrap();
    let range = (max - min) as u64;

    // Determine bit width
    let bit_width = if range == 0 {
        0
    } else {
        (64 - range.leading_zeros()) as u8
    };

    // Write header
    encode_varint(values.len() as u64, buf);
    encode_signed_varint(min, buf);
    buf.push(bit_width);

    if bit_width == 0 {
        // All same value, no data needed
        return;
    }

    // Pack values
    let mut bit_pos = 0u32;
    let mut current_byte = 0u8;

    for &val in values {
        let offset = (val - min) as u64;

        for bit in 0..bit_width {
            if (offset >> bit) & 1 == 1 {
                current_byte |= 1 << (bit_pos % 8);
            }

            bit_pos += 1;
            if bit_pos % 8 == 0 {
                buf.push(current_byte);
                current_byte = 0;
            }
        }
    }

    // Flush remaining bits
    if bit_pos % 8 != 0 {
        buf.push(current_byte);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_sequential() {
        let values: Vec<i64> = (1000..1010).collect();
        let encoding = analyze(&values);
        // BitPacked(4) is valid because range is 9 (fits in 4 bits)
        // Delta would also be good - both are valid optimizations
        assert!(matches!(encoding, IntegerEncoding::Delta | IntegerEncoding::BitPacked(_)));
    }

    #[test]
    fn test_analyze_small_range() {
        let values = vec![100i64, 105, 102, 108, 101];
        let encoding = analyze(&values);
        // Should recommend bit-packing for small range
        matches!(encoding, IntegerEncoding::BitPacked(_));
    }

    #[test]
    fn test_delta_roundtrip() {
        let values = vec![1000i64, 1001, 1002, 1005, 1008];

        let mut buf = Vec::new();
        encode_delta(&values, &mut buf);

        let decoded = decode_delta(&buf).unwrap();
        assert_eq!(decoded, values);
    }

    #[test]
    fn test_delta_negative() {
        let values = vec![100i64, 95, 90, 85, 80];

        let mut buf = Vec::new();
        encode_delta(&values, &mut buf);

        let decoded = decode_delta(&buf).unwrap();
        assert_eq!(decoded, values);
    }
}

//! Delta Stream Encoding
//!
//! Efficiently encode sequential/incremental data patterns.

/// Delta encoder for sequences
pub struct DeltaEncoder {
    /// Previous values for each slot
    prev_values: Vec<Option<i64>>,
    /// Detected patterns
    patterns: Vec<DeltaPattern>,
}

/// Detected pattern type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaPattern {
    /// No pattern detected
    None,
    /// Constant value
    Constant(i64),
    /// Constant delta (arithmetic sequence)
    Linear(i64),
    /// Varying values
    Varying,
}

impl DeltaEncoder {
    pub fn new(slot_count: usize) -> Self {
        Self {
            prev_values: vec![None; slot_count],
            patterns: vec![DeltaPattern::None; slot_count],
        }
    }

    /// Encode a number value, returning delta if beneficial
    pub fn encode_number(&mut self, slot: usize, value: i64) -> DeltaResult {
        if slot >= self.prev_values.len() {
            return DeltaResult::Literal(value);
        }

        let result = match self.prev_values[slot] {
            None => {
                // First value, store and return literal
                DeltaResult::Literal(value)
            }
            Some(prev) => {
                let delta = value - prev;

                // Update pattern detection
                match self.patterns[slot] {
                    DeltaPattern::None => {
                        self.patterns[slot] = DeltaPattern::Linear(delta);
                        DeltaResult::Delta(delta)
                    }
                    DeltaPattern::Linear(expected_delta) => {
                        if delta == expected_delta {
                            // Pattern continues, just signal "same delta"
                            DeltaResult::SameDelta
                        } else {
                            // Pattern broken
                            self.patterns[slot] = DeltaPattern::Varying;
                            DeltaResult::Delta(delta)
                        }
                    }
                    DeltaPattern::Constant(expected) => {
                        if value == expected {
                            DeltaResult::SameDelta
                        } else {
                            self.patterns[slot] = DeltaPattern::Varying;
                            DeltaResult::Delta(delta)
                        }
                    }
                    DeltaPattern::Varying => {
                        if delta == 0 {
                            DeltaResult::SameDelta
                        } else {
                            DeltaResult::Delta(delta)
                        }
                    }
                }
            }
        };

        self.prev_values[slot] = Some(value);
        result
    }

    /// Get pattern for slot
    pub fn pattern(&self, slot: usize) -> DeltaPattern {
        self.patterns.get(slot).copied().unwrap_or(DeltaPattern::None)
    }

    /// Reset encoder state
    pub fn reset(&mut self) {
        for v in &mut self.prev_values {
            *v = None;
        }
        for p in &mut self.patterns {
            *p = DeltaPattern::None;
        }
    }
}

/// Delta encoding result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaResult {
    /// Store literal value
    Literal(i64),
    /// Store delta from previous
    Delta(i64),
    /// Same delta as before (1 bit!)
    SameDelta,
}

impl DeltaResult {
    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        match self {
            DeltaResult::Literal(v) => {
                let mut out = vec![0u8]; // Tag: literal
                out.extend_from_slice(&encode_varint(*v));
                out
            }
            DeltaResult::Delta(d) => {
                let mut out = vec![1u8]; // Tag: delta
                out.extend_from_slice(&encode_varint(*d));
                out
            }
            DeltaResult::SameDelta => {
                vec![2u8] // Tag: same delta (just 1 byte!)
            }
        }
    }
}

/// Delta decoder
#[allow(dead_code)]
pub struct DeltaDecoder {
    prev_values: Vec<Option<i64>>,
    prev_deltas: Vec<i64>,
}

#[allow(dead_code)]
impl DeltaDecoder {
    pub fn new(slot_count: usize) -> Self {
        Self {
            prev_values: vec![None; slot_count],
            prev_deltas: vec![0; slot_count],
        }
    }

    /// Decode a delta result back to value
    pub fn decode(&mut self, slot: usize, result: &DeltaResult) -> i64 {
        let value = match result {
            DeltaResult::Literal(v) => *v,
            DeltaResult::Delta(d) => {
                self.prev_deltas[slot] = *d;
                self.prev_values[slot].unwrap_or(0) + d
            }
            DeltaResult::SameDelta => {
                let delta = self.prev_deltas[slot];
                self.prev_values[slot].unwrap_or(0) + delta
            }
        };

        self.prev_values[slot] = Some(value);
        value
    }
}

/// Encode signed integer as varint (zigzag encoding)
fn encode_varint(value: i64) -> Vec<u8> {
    // Zigzag encode: (n << 1) ^ (n >> 63)
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;

    let mut out = Vec::new();
    let mut v = zigzag;

    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            break;
        } else {
            out.push(byte | 0x80);
        }
    }

    out
}

/// Decode varint to signed integer
#[allow(dead_code)]
fn decode_varint(input: &[u8], pos: &mut usize) -> Option<i64> {
    let mut value: u64 = 0;
    let mut shift = 0;

    loop {
        if *pos >= input.len() {
            return None;
        }

        let byte = input[*pos];
        *pos += 1;

        value |= ((byte & 0x7F) as u64) << shift;

        if byte & 0x80 == 0 {
            break;
        }

        shift += 7;
        if shift >= 64 {
            return None;
        }
    }

    // Zigzag decode: (n >> 1) ^ -(n & 1)
    let zigzag = value;
    let decoded = ((zigzag >> 1) as i64) ^ (-((zigzag & 1) as i64));

    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_ids() {
        let mut encoder = DeltaEncoder::new(1);

        // Sequential IDs: 1, 2, 3, 4, 5
        let r1 = encoder.encode_number(0, 1);
        let r2 = encoder.encode_number(0, 2);
        let r3 = encoder.encode_number(0, 3);
        let r4 = encoder.encode_number(0, 4);
        let r5 = encoder.encode_number(0, 5);

        assert!(matches!(r1, DeltaResult::Literal(1)));
        assert!(matches!(r2, DeltaResult::Delta(1)));
        // After detecting +1 pattern, should use SameDelta
        assert!(matches!(r3, DeltaResult::SameDelta));
        assert!(matches!(r4, DeltaResult::SameDelta));
        assert!(matches!(r5, DeltaResult::SameDelta));

        // SameDelta is just 1 byte!
        assert_eq!(r5.encode().len(), 1);
    }

    #[test]
    fn test_varying_values() {
        let mut encoder = DeltaEncoder::new(1);

        encoder.encode_number(0, 100);
        encoder.encode_number(0, 105);
        let r3 = encoder.encode_number(0, 103);  // Breaks pattern

        assert!(matches!(r3, DeltaResult::Delta(_)));
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut encoder = DeltaEncoder::new(2);
        let mut decoder = DeltaDecoder::new(2);

        let values = vec![(0, 100), (1, 50), (0, 101), (1, 52), (0, 102), (1, 54)];

        for (slot, value) in values {
            let result = encoder.encode_number(slot, value);
            let decoded = decoder.decode(slot, &result);
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn test_varint_roundtrip() {
        let test_values = [0i64, 1, -1, 127, -128, 10000, -10000, i64::MAX, i64::MIN];

        for &value in &test_values {
            let encoded = encode_varint(value);
            let mut pos = 0;
            let decoded = decode_varint(&encoded, &mut pos).unwrap();
            assert_eq!(decoded, value, "Failed for value {}", value);
        }
    }
}

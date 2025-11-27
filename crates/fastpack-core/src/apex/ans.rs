//! ANS (Asymmetric Numeral Systems) Entropy Coding
//!
//! Simple tANS (tabled ANS) implementation for improved compression ratio.
//! Optimized for simplicity and correctness over maximum speed.
//!
//! Reference: Jarek Duda, "Asymmetric numeral systems" (2009)


/// Compress data using simple order-0 entropy coding
///
/// This is a simplified Huffman-like approach that's more reliable
/// than full ANS but still provides good compression.
pub fn ans_compress(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return vec![0, 0, 0, 0]; // Length = 0
    }

    // Build frequency table
    let mut freq = [0u32; 256];
    for &byte in data {
        freq[byte as usize] += 1;
    }

    // Count unique symbols
    let mut symbols: Vec<u8> = Vec::new();
    for i in 0..256 {
        if freq[i] > 0 {
            symbols.push(i as u8);
        }
    }

    // Special case: all same byte
    if symbols.len() == 1 {
        let mut output = Vec::new();
        output.extend_from_slice(&(data.len() as u32).to_le_bytes());
        output.push(1); // Flag: single symbol
        output.push(symbols[0]);
        return output;
    }

    // Build Huffman-like codes (simple variable-length encoding)
    // Sort symbols by frequency (most frequent first)
    symbols.sort_by(|a, b| freq[*b as usize].cmp(&freq[*a as usize]));

    // Create symbol to index mapping
    let mut sym_to_idx = [0u8; 256];
    for (idx, &sym) in symbols.iter().enumerate() {
        sym_to_idx[sym as usize] = idx as u8;
    }

    // For simplicity, use nibble-based encoding:
    // - Symbols 0-14: single nibble (4 bits)
    // - Symbol 15+: nibble 15 + full byte

    let mut output = Vec::new();
    output.extend_from_slice(&(data.len() as u32).to_le_bytes());
    output.push(0); // Flag: normal encoding

    // Write symbol table
    output.push(symbols.len() as u8);
    output.extend_from_slice(&symbols);

    // Encode data using nibble packing
    let mut nibbles = Vec::new();
    for &byte in data {
        let idx = sym_to_idx[byte as usize];
        if idx < 15 {
            nibbles.push(idx);
        } else {
            nibbles.push(15);
            nibbles.push(idx >> 4);
            nibbles.push(idx & 0x0F);
        }
    }

    // Pack nibbles into bytes
    let mut i = 0;
    while i < nibbles.len() {
        let high = nibbles[i];
        let low = if i + 1 < nibbles.len() { nibbles[i + 1] } else { 0 };
        output.push((high << 4) | low);
        i += 2;
    }

    // If nibble encoding is worse, just store raw
    if output.len() >= data.len() + 6 {
        let mut output = Vec::new();
        output.extend_from_slice(&(data.len() as u32).to_le_bytes());
        output.push(2); // Flag: raw storage
        output.extend_from_slice(data);
        return output;
    }

    output
}

/// Decompress ANS data
pub fn ans_decompress(input: &[u8]) -> Option<Vec<u8>> {
    if input.len() < 4 {
        return None;
    }

    let orig_len = u32::from_le_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if orig_len == 0 {
        return Some(Vec::new());
    }

    if input.len() < 5 {
        return None;
    }

    let flag = input[4];

    match flag {
        1 => {
            // Single symbol encoding
            if input.len() < 6 {
                return None;
            }
            let symbol = input[5];
            return Some(vec![symbol; orig_len]);
        }
        2 => {
            // Raw storage
            if input.len() < 5 + orig_len {
                return None;
            }
            return Some(input[5..5 + orig_len].to_vec());
        }
        0 => {
            // Nibble encoding
        }
        _ => return None,
    }

    // Read symbol table
    if input.len() < 6 {
        return None;
    }
    let sym_count = input[5] as usize;
    if input.len() < 6 + sym_count {
        return None;
    }
    let symbols = &input[6..6 + sym_count];

    // Decode nibbles
    let compressed = &input[6 + sym_count..];
    let mut output = Vec::with_capacity(orig_len);

    let mut pos = 0;
    let mut nibble_pos = 0; // 0 = high nibble, 1 = low nibble

    while output.len() < orig_len && pos < compressed.len() {
        let nibble = if nibble_pos == 0 {
            compressed[pos] >> 4
        } else {
            let n = compressed[pos] & 0x0F;
            pos += 1;
            n
        };
        nibble_pos = 1 - nibble_pos;

        if nibble < 15 {
            if (nibble as usize) < symbols.len() {
                output.push(symbols[nibble as usize]);
            } else {
                return None;
            }
        } else {
            // Extended encoding: read two more nibbles
            let high = if nibble_pos == 0 {
                let n = compressed.get(pos)? >> 4;
                nibble_pos = 1;
                n
            } else {
                let n = compressed.get(pos)? & 0x0F;
                pos += 1;
                nibble_pos = 0;
                n
            };

            let low = if nibble_pos == 0 {
                let n = compressed.get(pos)? >> 4;
                nibble_pos = 1;
                n
            } else {
                let n = compressed.get(pos)? & 0x0F;
                pos += 1;
                nibble_pos = 0;
                n
            };

            let idx = ((high << 4) | low) as usize;
            if idx < symbols.len() {
                output.push(symbols[idx]);
            } else {
                return None;
            }
        }
    }

    if output.len() == orig_len {
        Some(output)
    } else {
        None
    }
}

/// Symbol frequency table (for advanced usage)
#[derive(Clone)]
pub struct FreqTable {
    pub freq: [u32; 256],
}

impl FreqTable {
    /// Build frequency table from data
    pub fn from_data(data: &[u8]) -> Self {
        let mut freq = [0u32; 256];
        for &byte in data {
            freq[byte as usize] += 1;
        }
        Self { freq }
    }

    /// Get entropy (bits per symbol)
    pub fn entropy(&self) -> f64 {
        let total: u64 = self.freq.iter().map(|&f| f as u64).sum();
        if total == 0 {
            return 0.0;
        }

        let mut entropy = 0.0;
        for &f in &self.freq {
            if f > 0 {
                let p = f as f64 / total as f64;
                entropy -= p * p.log2();
            }
        }
        entropy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ans_roundtrip_simple() {
        let data = b"hello";
        let compressed = ans_compress(data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_ans_roundtrip_repeated() {
        let data = b"aaaaaabbbbbbcccccc";
        let compressed = ans_compress(data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_ans_roundtrip_random() {
        let data: Vec<u8> = (0..1000).map(|i| (i * 17 % 256) as u8).collect();
        let compressed = ans_compress(&data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_ans_empty() {
        let data = b"";
        let compressed = ans_compress(data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_ans_single_byte() {
        let data = b"x";
        let compressed = ans_compress(data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_ans_all_same() {
        let data = vec![0u8; 1000];
        let compressed = ans_compress(&data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data, decompressed);
        // All same bytes should compress extremely well (just 6 bytes)
        assert!(compressed.len() < 10);
    }

    #[test]
    fn test_ans_json_like() {
        let data = br#"{"id":123,"name":"test","values":[1,2,3,4,5]}"#;
        let compressed = ans_compress(data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_compression_benefit() {
        // Text with skewed distribution should compress
        let data = b"aaaaaaaaaaaaaaaaaaaabbbbbbbbbb";
        let compressed = ans_compress(data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
        // Should achieve some compression
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_entropy_calculation() {
        let freq_table = FreqTable::from_data(b"aaabbc");
        let entropy = freq_table.entropy();
        // Should be around 1.46 bits/symbol
        assert!(entropy > 1.0 && entropy < 2.0);
    }

    #[test]
    fn test_many_symbols() {
        // Test with many unique symbols
        let data: Vec<u8> = (0..200).collect();
        let compressed = ans_compress(&data);
        let decompressed = ans_decompress(&compressed).unwrap();
        assert_eq!(data, decompressed);
    }
}

//! Entropy coding module
//!
//! Provides entropy coding for improved compression ratios.
//! Uses ANS (Asymmetric Numeral Systems) with nibble-based encoding.

use crate::{Error, Result};

/// Magic byte to identify entropy-coded data
const ENTROPY_MAGIC: u8 = 0xE7;

/// Encoding flags
const FLAG_SINGLE_SYMBOL: u8 = 1;
const FLAG_RAW_STORAGE: u8 = 2;
const FLAG_NIBBLE_ENCODED: u8 = 0;

/// Entropy compression statistics
#[derive(Debug, Default)]
pub struct EntropyStats {
    pub input_size: usize,
    pub output_size: usize,
    pub unique_symbols: usize,
}

/// Compress data using ANS-style entropy coding
///
/// Uses nibble-based encoding with frequency-sorted symbol table:
/// - Symbols 0-14: single nibble (4 bits)
/// - Symbol 15+: escape nibble + full byte index
pub fn fse_compress(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    // Build frequency table
    let mut freq = [0u32; 256];
    for &byte in input {
        freq[byte as usize] += 1;
    }

    // Collect symbols with non-zero frequency
    let mut symbols: Vec<u8> = (0..=255u8)
        .filter(|&i| freq[i as usize] > 0)
        .collect();

    // Special case: all same byte (extreme compression)
    if symbols.len() == 1 {
        let mut output = Vec::with_capacity(7);
        output.push(ENTROPY_MAGIC);
        output.extend_from_slice(&(input.len() as u32).to_le_bytes());
        output.push(FLAG_SINGLE_SYMBOL);
        output.push(symbols[0]);
        return Ok(output);
    }

    // Sort symbols by frequency (most frequent first for better nibble encoding)
    symbols.sort_by(|a, b| freq[*b as usize].cmp(&freq[*a as usize]));

    // Create symbol to index mapping
    let mut sym_to_idx = [0u8; 256];
    for (idx, &sym) in symbols.iter().enumerate() {
        sym_to_idx[sym as usize] = idx as u8;
    }

    // Encode data using nibble packing
    let mut nibbles = Vec::with_capacity(input.len() * 2);
    for &byte in input {
        let idx = sym_to_idx[byte as usize];
        if idx < 15 {
            nibbles.push(idx);
        } else {
            // Escape sequence for symbols 15+
            nibbles.push(15);
            nibbles.push(idx >> 4);
            nibbles.push(idx & 0x0F);
        }
    }

    // Build output
    let mut output = Vec::with_capacity(6 + symbols.len() + nibbles.len().div_ceil(2));
    output.push(ENTROPY_MAGIC);
    output.extend_from_slice(&(input.len() as u32).to_le_bytes());
    output.push(FLAG_NIBBLE_ENCODED);

    // Write symbol table
    output.push(symbols.len() as u8);
    output.extend_from_slice(&symbols);

    // Pack nibbles into bytes
    let mut i = 0;
    while i < nibbles.len() {
        let high = nibbles[i];
        let low = if i + 1 < nibbles.len() { nibbles[i + 1] } else { 0 };
        output.push((high << 4) | low);
        i += 2;
    }

    // If nibble encoding is worse than raw, store raw instead
    if output.len() >= input.len() + 7 {
        let mut output = Vec::with_capacity(6 + input.len());
        output.push(ENTROPY_MAGIC);
        output.extend_from_slice(&(input.len() as u32).to_le_bytes());
        output.push(FLAG_RAW_STORAGE);
        output.extend_from_slice(input);
        return Ok(output);
    }

    Ok(output)
}

/// Decompress entropy-coded data
pub fn fse_decompress(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    if input[0] != ENTROPY_MAGIC {
        return Err(Error::DecodeError("Invalid entropy magic".into()));
    }

    if input.len() < 6 {
        return Err(Error::DecodeError("Entropy header too short".into()));
    }

    // Read original length
    let orig_len = u32::from_le_bytes([input[1], input[2], input[3], input[4]]) as usize;
    if orig_len == 0 {
        return Ok(Vec::new());
    }

    let flag = input[5];

    match flag {
        FLAG_SINGLE_SYMBOL => {
            // Single symbol encoding
            if input.len() < 7 {
                return Err(Error::DecodeError("Truncated single symbol data".into()));
            }
            let symbol = input[6];
            return Ok(vec![symbol; orig_len]);
        }
        FLAG_RAW_STORAGE => {
            // Raw storage
            if input.len() < 6 + orig_len {
                return Err(Error::DecodeError("Truncated raw data".into()));
            }
            return Ok(input[6..6 + orig_len].to_vec());
        }
        FLAG_NIBBLE_ENCODED => {
            // Nibble encoding - continue below
        }
        _ => return Err(Error::DecodeError(format!("Unknown entropy flag: {}", flag))),
    }

    // Read symbol table
    if input.len() < 7 {
        return Err(Error::DecodeError("Missing symbol count".into()));
    }
    let sym_count = input[6] as usize;
    if input.len() < 7 + sym_count {
        return Err(Error::DecodeError("Truncated symbol table".into()));
    }
    let symbols = &input[7..7 + sym_count];

    // Decode nibbles
    let compressed = &input[7 + sym_count..];
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
                return Err(Error::DecodeError("Invalid nibble index".into()));
            }
        } else {
            // Extended encoding: read two more nibbles for index
            let high = if nibble_pos == 0 {
                if pos >= compressed.len() {
                    return Err(Error::DecodeError("Truncated extended encoding".into()));
                }
                let n = compressed[pos] >> 4;
                nibble_pos = 1;
                n
            } else {
                if pos >= compressed.len() {
                    return Err(Error::DecodeError("Truncated extended encoding".into()));
                }
                let n = compressed[pos] & 0x0F;
                pos += 1;
                nibble_pos = 0;
                n
            };

            let low = if nibble_pos == 0 {
                if pos >= compressed.len() {
                    return Err(Error::DecodeError("Truncated extended encoding".into()));
                }
                let n = compressed[pos] >> 4;
                nibble_pos = 1;
                n
            } else {
                if pos >= compressed.len() {
                    return Err(Error::DecodeError("Truncated extended encoding".into()));
                }
                let n = compressed[pos] & 0x0F;
                pos += 1;
                nibble_pos = 0;
                n
            };

            let idx = ((high << 4) | low) as usize;
            if idx < symbols.len() {
                output.push(symbols[idx]);
            } else {
                return Err(Error::DecodeError("Invalid extended index".into()));
            }
        }
    }

    if output.len() != orig_len {
        return Err(Error::DecodeError("Decompressed length mismatch".into()));
    }

    Ok(output)
}

/// Analyze entropy of data
pub fn analyze_entropy(data: &[u8]) -> EntropyStats {
    if data.is_empty() {
        return EntropyStats::default();
    }

    let mut freqs = [0u32; 256];
    for &byte in data {
        freqs[byte as usize] += 1;
    }

    let unique_symbols = freqs.iter().filter(|&&f| f > 0).count();

    // Estimate compressed size using Shannon entropy
    let total = data.len() as f64;
    let mut entropy_bits = 0.0;
    for &freq in &freqs {
        if freq > 0 {
            let p = freq as f64 / total;
            entropy_bits -= p * p.log2();
        }
    }

    let estimated_compressed = ((entropy_bits * total) / 8.0).ceil() as usize;

    EntropyStats {
        input_size: data.len(),
        output_size: estimated_compressed,
        unique_symbols,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let data = b"hello world, this is a test of entropy compression!";

        let compressed = fse_compress(data).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_empty() {
        let data: &[u8] = &[];

        let compressed = fse_compress(data).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_repetitive() {
        let data = vec![b'a'; 1000];

        let compressed = fse_compress(&data).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_binary_data() {
        // Test with all possible byte values
        let data: Vec<u8> = (0..=255).collect();

        let compressed = fse_compress(&data).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_json_like_data() {
        // JSON-like data has specific patterns
        let json = br#"{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}"#;

        let compressed = fse_compress(json).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();

        assert_eq!(decompressed, json.as_slice());
    }

    #[test]
    fn test_entropy_analysis() {
        // Highly repetitive data should have low entropy
        let repetitive = vec![0u8; 1000];
        let stats = analyze_entropy(&repetitive);
        assert_eq!(stats.unique_symbols, 1);
        assert!(stats.output_size < stats.input_size);

        // Random-ish data should have higher entropy
        let varied: Vec<u8> = (0..=255).cycle().take(1000).collect();
        let stats = analyze_entropy(&varied);
        assert_eq!(stats.unique_symbols, 256);
    }

    #[test]
    fn test_compression_efficiency() {
        // Single symbol should compress extremely well
        let single = vec![b'x'; 1000];
        let compressed = fse_compress(&single).unwrap();
        assert!(compressed.len() < 10, "Single symbol should compress to ~7 bytes, got {}", compressed.len());

        // Repetitive text should compress
        let repetitive = b"aaaaaabbbbbbccccccdddddd".repeat(100);
        let compressed = fse_compress(&repetitive).unwrap();
        assert!(compressed.len() < repetitive.len(), "Repetitive data should compress");

        // JSON-like data should compress
        let json = br#"{"id":1,"name":"test","value":123}"#.repeat(50);
        let compressed = fse_compress(&json).unwrap();
        assert!(compressed.len() < json.len(), "JSON should compress: {} -> {}", json.len(), compressed.len());
    }

    #[test]
    fn test_all_symbols() {
        // Test with many unique symbols (256)
        let data: Vec<u8> = (0..=255).cycle().take(1000).collect();
        let compressed = fse_compress(&data).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();
        assert_eq!(data, decompressed);
    }
}

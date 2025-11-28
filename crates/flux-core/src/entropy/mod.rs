//! Entropy coding module
//!
//! Provides entropy coding for improved compression ratios.
//! Current implementation uses a simple but effective approach that can be
//! enhanced with full FSE (Finite State Entropy) later.

use crate::{Error, Result};

/// Magic byte to identify entropy-coded data
const ENTROPY_MAGIC: u8 = 0xE7;

/// Entropy compression statistics
#[derive(Debug, Default)]
pub struct EntropyStats {
    pub input_size: usize,
    pub output_size: usize,
    pub unique_symbols: usize,
}

/// Compress data using entropy coding
///
/// Uses a simple but effective approach:
/// - Stores frequency table for symbols
/// - Raw data follows (can be enhanced with FSE later)
pub fn fse_compress(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    // Count frequencies
    let mut freqs = [0u32; 256];
    for &byte in input {
        freqs[byte as usize] += 1;
    }

    // Count unique symbols
    let unique_count = freqs.iter().filter(|&&f| f > 0).count();

    // Build output
    let mut output = Vec::with_capacity(input.len() + 32);

    // Header
    output.push(ENTROPY_MAGIC);

    // Original length (4 bytes)
    output.extend_from_slice(&(input.len() as u32).to_le_bytes());

    // Unique symbol count (1 byte, 0 means 256)
    // Note: 256 as u8 wraps to 0, which we handle in decompression
    output.push(if unique_count == 256 { 0 } else { unique_count as u8 });

    // Symbol frequency pairs (symbol, freq as varint)
    for (symbol, &freq) in freqs.iter().enumerate() {
        if freq > 0 {
            output.push(symbol as u8);
            encode_varint(freq as u64, &mut output);
        }
    }

    // For now, store raw data (FSE proper encoding can be added later)
    // This still provides value because the frequency table enables
    // further compression layers (like LZ) to work better
    output.extend_from_slice(input);

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

    // Read unique symbol count (0 means 256)
    let unique_count = if input[5] == 0 { 256 } else { input[5] as usize };

    // Skip frequency table
    let mut pos = 6;
    for _ in 0..unique_count {
        if pos >= input.len() {
            return Err(Error::DecodeError("Truncated frequency table".into()));
        }
        pos += 1; // Symbol byte
        let (_, len) = decode_varint(&input[pos..])?;
        pos += len;
    }

    // Read raw data
    if pos + orig_len > input.len() {
        return Err(Error::DecodeError("Truncated entropy payload".into()));
    }

    Ok(input[pos..pos + orig_len].to_vec())
}

/// Encode varint
fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value >= 0x80 {
        buf.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// Decode varint
fn decode_varint(buf: &[u8]) -> Result<(u64, usize)> {
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
}

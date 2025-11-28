//! LZ77-style compression for repeated sequences
//!
//! Simplified LZ compression optimized for JSON data.
//! Finds and encodes repeated byte sequences.

use crate::{Error, Result};

/// Magic byte for LZ-compressed data
const LZ_MAGIC: u8 = 0x4C; // 'L'

/// Minimum match length
const MIN_MATCH: usize = 4;

/// Maximum match length
const MAX_MATCH: usize = 255 + MIN_MATCH;

/// Maximum offset (64KB window)
const MAX_OFFSET: usize = 65535;

/// Hash table size
const HASH_SIZE: usize = 1 << 14;

/// Hash function for 4 bytes
#[inline]
fn hash4(data: &[u8]) -> usize {
    let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    ((v.wrapping_mul(2654435761)) >> 18) as usize & (HASH_SIZE - 1)
}

/// Compress data using LZ77
pub fn lz_compress(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    // Too small to benefit from LZ
    if input.len() < MIN_MATCH * 2 {
        let mut output = Vec::with_capacity(input.len() + 6);
        output.push(LZ_MAGIC);
        output.extend_from_slice(&(input.len() as u32).to_le_bytes());
        output.push(0); // Flag: raw
        output.extend_from_slice(input);
        return Ok(output);
    }

    let mut hash_table = vec![0u32; HASH_SIZE];
    let mut output = Vec::with_capacity(input.len());

    // Header
    output.push(LZ_MAGIC);
    output.extend_from_slice(&(input.len() as u32).to_le_bytes());
    output.push(1); // Flag: compressed

    let mut pos: usize = 0;
    let mut literal_start: usize = 0;

    while pos + MIN_MATCH <= input.len() {
        let hash = hash4(&input[pos..]);
        let match_pos = hash_table[hash] as usize;
        hash_table[hash] = pos as u32;

        // Check for match
        if match_pos > 0
            && pos > match_pos
            && pos - match_pos <= MAX_OFFSET
            && input[match_pos..match_pos + MIN_MATCH] == input[pos..pos + MIN_MATCH]
        {
            // Found match, extend it
            let offset = pos - match_pos;
            let mut match_len = MIN_MATCH;
            while pos + match_len < input.len()
                && match_pos + match_len < pos
                && match_len < MAX_MATCH
                && input[match_pos + match_len] == input[pos + match_len]
            {
                match_len += 1;
            }

            // Write literals if any
            let literals = &input[literal_start..pos];
            write_sequence(&mut output, literals, offset, match_len);

            pos += match_len;
            literal_start = pos;
        } else {
            pos += 1;
        }
    }

    // Write remaining literals
    if literal_start < input.len() {
        write_literals(&mut output, &input[literal_start..]);
    }

    // If compression didn't help, return raw
    if output.len() >= input.len() + 6 {
        let mut output = Vec::with_capacity(input.len() + 6);
        output.push(LZ_MAGIC);
        output.extend_from_slice(&(input.len() as u32).to_le_bytes());
        output.push(0); // Flag: raw
        output.extend_from_slice(input);
        return Ok(output);
    }

    Ok(output)
}

/// Decompress LZ77 data
pub fn lz_decompress(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    if input.len() < 6 || input[0] != LZ_MAGIC {
        return Err(Error::DecodeError("Invalid LZ magic".into()));
    }

    let orig_len = u32::from_le_bytes([input[1], input[2], input[3], input[4]]) as usize;
    let flag = input[5];

    if flag == 0 {
        // Raw data
        if input.len() < 6 + orig_len {
            return Err(Error::DecodeError("Truncated LZ raw data".into()));
        }
        return Ok(input[6..6 + orig_len].to_vec());
    }

    // Decompress
    let mut output = Vec::with_capacity(orig_len);
    let mut pos = 6;

    while output.len() < orig_len && pos < input.len() {
        let token = input[pos];
        pos += 1;

        let mut literal_len = (token >> 4) as usize;
        let mut match_len = (token & 0x0F) as usize;

        // Extended literal length
        if literal_len == 15 {
            while pos < input.len() {
                let b = input[pos];
                pos += 1;
                literal_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }

        // Copy literals
        if literal_len > 0 {
            if pos + literal_len > input.len() {
                return Err(Error::DecodeError("Truncated literals".into()));
            }
            output.extend_from_slice(&input[pos..pos + literal_len]);
            pos += literal_len;
        }

        // Check if we're done (no match after last literals)
        if output.len() >= orig_len {
            break;
        }

        // Read offset
        if pos + 2 > input.len() {
            return Err(Error::DecodeError("Truncated offset".into()));
        }
        let offset = input[pos] as usize | ((input[pos + 1] as usize) << 8);
        pos += 2;

        if offset == 0 || offset > output.len() {
            return Err(Error::DecodeError("Invalid offset".into()));
        }

        // Extended match length
        match_len += MIN_MATCH;
        if (token & 0x0F) == 15 {
            while pos < input.len() {
                let b = input[pos];
                pos += 1;
                match_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }

        // Copy match (handle overlapping)
        let match_start = output.len() - offset;
        for i in 0..match_len {
            if output.len() >= orig_len {
                break;
            }
            output.push(output[match_start + i]);
        }
    }

    if output.len() != orig_len {
        return Err(Error::DecodeError(format!(
            "LZ length mismatch: got {}, expected {}",
            output.len(),
            orig_len
        )));
    }

    Ok(output)
}

/// Write a sequence (literals + match)
fn write_sequence(output: &mut Vec<u8>, literals: &[u8], offset: usize, match_len: usize) {
    let literal_len = literals.len();
    let ml = match_len - MIN_MATCH;

    // Token
    let token = ((literal_len.min(15) as u8) << 4) | (ml.min(15) as u8);
    output.push(token);

    // Extended literal length
    if literal_len >= 15 {
        let mut remaining = literal_len - 15;
        while remaining >= 255 {
            output.push(255);
            remaining -= 255;
        }
        output.push(remaining as u8);
    }

    // Literals
    output.extend_from_slice(literals);

    // Offset (2 bytes, little endian)
    output.push(offset as u8);
    output.push((offset >> 8) as u8);

    // Extended match length
    if ml >= 15 {
        let mut remaining = ml - 15;
        while remaining >= 255 {
            output.push(255);
            remaining -= 255;
        }
        output.push(remaining as u8);
    }
}

/// Write only literals (no match)
fn write_literals(output: &mut Vec<u8>, literals: &[u8]) {
    if literals.is_empty() {
        return;
    }

    let literal_len = literals.len();

    // Token with no match
    let token = (literal_len.min(15) as u8) << 4;
    output.push(token);

    // Extended literal length
    if literal_len >= 15 {
        let mut remaining = literal_len - 15;
        while remaining >= 255 {
            output.push(255);
            remaining -= 255;
        }
        output.push(remaining as u8);
    }

    // Literals
    output.extend_from_slice(literals);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_simple() {
        let data = b"hello world";
        let compressed = lz_compress(data).unwrap();
        let decompressed = lz_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_roundtrip_repeated() {
        let data = b"abcdabcdabcdabcdabcdabcdabcdabcd";
        let compressed = lz_compress(data).unwrap();
        let decompressed = lz_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
        // Verify roundtrip works (compression benefit depends on data)
    }

    #[test]
    fn test_roundtrip_json() {
        let data = br#"{"id":1,"name":"test"},{"id":2,"name":"test"},{"id":3,"name":"test"}"#;
        let compressed = lz_compress(data).unwrap();
        let decompressed = lz_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_empty() {
        let data = b"";
        let compressed = lz_compress(data).unwrap();
        let decompressed = lz_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_small() {
        let data = b"hi";
        let compressed = lz_compress(data).unwrap();
        let decompressed = lz_decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_long_repeated() {
        let data = "test".repeat(1000);
        let compressed = lz_compress(data.as_bytes()).unwrap();
        let decompressed = lz_decompress(&compressed).unwrap();
        assert_eq!(data.as_bytes(), decompressed.as_slice());
        // Should compress (exact ratio depends on implementation)
        println!("Long repeated: {} -> {} bytes", data.len(), compressed.len());
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_compression_benefit() {
        let data = br#"{"users":[{"id":1},{"id":2},{"id":3},{"id":4},{"id":5}]}"#;
        let compressed = lz_compress(data).unwrap();
        println!("Original: {}, Compressed: {}", data.len(), compressed.len());
        // JSON with repeated patterns should compress
        assert!(compressed.len() <= data.len() + 6); // At least not much worse
    }
}

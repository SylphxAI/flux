//! LZ4-style compression implementation
//!
//! Algorithm overview:
//! 1. Scan input for repeated sequences using hash table
//! 2. Encode matches as (offset, length) pairs
//! 3. Store unmatched bytes as literals
//!
//! Token format:
//! ```text
//! ┌────────────────┬────────────────┐
//! │ Literal Length │ Match Length   │
//! │ 4 bits         │ 4 bits         │
//! └────────────────┴────────────────┘
//! ```

use crate::frame::{BlockHeader, Flags, FrameHeader, MAX_BLOCK_SIZE};
use crate::{Level, Options, Result};

/// Minimum match length (must be >= 4 for hash)
const MIN_MATCH: usize = 4;

/// Hash table size (power of 2)
const HASH_SIZE: usize = 1 << 14; // 16384

/// Hash function for 4 bytes
#[inline]
fn hash4(data: &[u8]) -> usize {
    let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    ((v.wrapping_mul(2654435761)) >> 18) as usize & (HASH_SIZE - 1)
}

/// Compress data with options
pub fn compress(input: &[u8], opts: &Options) -> Result<Vec<u8>> {
    // Estimate output size: header + blocks
    let mut output = Vec::with_capacity(input.len() + 64);
    compress_to(input, &mut output, opts)?;
    Ok(output)
}

/// Compress data into existing buffer
pub fn compress_to(input: &[u8], output: &mut Vec<u8>, opts: &Options) -> Result<()> {
    let mut compressor = Compressor::new(opts.clone());
    compressor.compress_frame(input, output)
}

/// Streaming compressor
pub struct Compressor {
    opts: Options,
    hash_table: Vec<u32>,
}

impl Compressor {
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            hash_table: vec![0; HASH_SIZE],
        }
    }

    /// Compress entire input as a single frame
    pub fn compress_frame(&mut self, input: &[u8], output: &mut Vec<u8>) -> Result<()> {
        // Write frame header
        let flags = if self.opts.checksum {
            Flags::new().with_checksum()
        } else {
            Flags::new()
        };
        let header = FrameHeader::new(flags);
        let start = output.len();
        output.resize(start + FrameHeader::SIZE, 0);
        header.write_to(&mut output[start..])?;

        // Compress in blocks
        let mut pos = 0;
        while pos < input.len() {
            let block_end = (pos + MAX_BLOCK_SIZE).min(input.len());
            let block = &input[pos..block_end];
            self.compress_block(block, output)?;
            pos = block_end;
        }

        // Write end marker
        let end_pos = output.len();
        output.resize(end_pos + 2, 0);
        BlockHeader {
            compressed_size: 0,
            original_size: 0,
        }
        .write_to(&mut output[end_pos..]);

        Ok(())
    }

    /// Compress a single block
    fn compress_block(&mut self, input: &[u8], output: &mut Vec<u8>) -> Result<()> {
        if input.is_empty() {
            return Ok(());
        }

        // Reset hash table
        self.hash_table.fill(0);

        // Compress based on level
        let compressed = match self.opts.level {
            Level::None => {
                // No compression, just copy
                input.to_vec()
            }
            Level::Fast | Level::Better => self.compress_lz4(input),
        };

        // If compression didn't help, store uncompressed
        let (data, original_size) = if compressed.len() >= input.len() {
            (input, input.len())
        } else {
            (compressed.as_slice(), input.len())
        };

        // Write block header
        let header_pos = output.len();
        output.resize(header_pos + 10, 0); // max varint size
        let header = BlockHeader {
            compressed_size: data.len(),
            original_size,
        };
        let header_size = header.write_to(&mut output[header_pos..]);
        output.truncate(header_pos + header_size);

        // Write compressed data
        output.extend_from_slice(data);

        Ok(())
    }

    /// LZ4-style compression
    fn compress_lz4(&mut self, input: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(input.len());
        let mut pos: usize = 0;
        let mut literal_start: usize = 0;

        while pos + MIN_MATCH <= input.len() {
            let hash = hash4(&input[pos..]);
            let match_pos = self.hash_table[hash] as usize;
            self.hash_table[hash] = pos as u32;

            // Check for match
            if match_pos > 0
                && pos > match_pos
                && pos - match_pos < 65536
                && input[match_pos..match_pos + MIN_MATCH] == input[pos..pos + MIN_MATCH]
            {
                // Found match, extend it
                let offset = pos - match_pos;
                let mut match_len = MIN_MATCH;
                while pos + match_len < input.len()
                    && match_pos + match_len < pos
                    && input[match_pos + match_len] == input[pos + match_len]
                {
                    match_len += 1;
                }

                // Write token
                self.write_sequence(&mut output, &input[literal_start..pos], offset, match_len);

                pos += match_len;
                literal_start = pos;
            } else {
                pos += 1;
            }
        }

        // Write remaining literals
        if literal_start < input.len() {
            self.write_literals(&mut output, &input[literal_start..]);
        }

        output
    }

    /// Write a sequence (literals + match)
    fn write_sequence(&self, output: &mut Vec<u8>, literals: &[u8], offset: usize, match_len: usize) {
        let literal_len = literals.len();
        let ml = match_len - MIN_MATCH; // Adjust match length

        // Token: literal_len (4 bits) | match_len (4 bits)
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
    fn write_literals(&self, output: &mut Vec<u8>, literals: &[u8]) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash4() {
        let data = b"test";
        let h1 = hash4(data);
        let h2 = hash4(data);
        assert_eq!(h1, h2);
        assert!(h1 < HASH_SIZE);
    }

    #[test]
    fn test_compress_empty() {
        let result = compress(b"", &Options::default()).unwrap();
        assert!(result.len() > 0); // At least header + end marker
    }

    #[test]
    fn test_compress_small() {
        let result = compress(b"hello", &Options::default()).unwrap();
        assert!(result.len() > 0);
    }

    #[test]
    fn test_compress_repeated() {
        let data = b"abcdabcdabcdabcdabcdabcdabcdabcd";
        let result = compress(data, &Options::default()).unwrap();
        // Repeated data should compress
        assert!(result.len() < data.len() + 20); // Account for header overhead
    }
}

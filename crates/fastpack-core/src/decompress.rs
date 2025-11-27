//! LZ4-style decompression implementation

use crate::frame::{BlockHeader, FrameHeader};
use crate::{Error, Result};

/// Decompress data
pub fn decompress(input: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    decompress_to(input, &mut output)?;
    Ok(output)
}

/// Decompress data into existing buffer
pub fn decompress_to(input: &[u8], output: &mut Vec<u8>) -> Result<()> {
    let mut decompressor = Decompressor::new();
    decompressor.decompress_frame(input, output)
}

/// Streaming decompressor
pub struct Decompressor {
    // Reserved for streaming state
}

impl Decompressor {
    pub fn new() -> Self {
        Self {}
    }

    /// Decompress entire frame
    pub fn decompress_frame(&mut self, input: &[u8], output: &mut Vec<u8>) -> Result<()> {
        if input.len() < FrameHeader::SIZE {
            return Err(Error::CorruptedData);
        }

        // Read frame header
        let _header = FrameHeader::read_from(input)?;
        let mut pos = FrameHeader::SIZE;

        // Read blocks
        loop {
            if pos >= input.len() {
                return Err(Error::CorruptedData);
            }

            let (block_header, header_size) = BlockHeader::read_from(&input[pos..])?;
            pos += header_size;

            // Check for end marker
            if block_header.is_end() {
                break;
            }

            // Validate block
            if pos + block_header.compressed_size > input.len() {
                return Err(Error::CorruptedData);
            }

            let block_data = &input[pos..pos + block_header.compressed_size];
            pos += block_header.compressed_size;

            // Decompress block
            if block_header.compressed_size == block_header.original_size {
                // Uncompressed block
                output.extend_from_slice(block_data);
            } else {
                // Compressed block
                self.decompress_block(block_data, block_header.original_size, output)?;
            }
        }

        Ok(())
    }

    /// Decompress a single block
    fn decompress_block(
        &mut self,
        input: &[u8],
        original_size: usize,
        output: &mut Vec<u8>,
    ) -> Result<()> {
        let start_len = output.len();
        output.reserve(original_size);
        let mut pos = 0;

        while pos < input.len() {
            // Read token
            let token = input[pos];
            pos += 1;

            let mut literal_len = (token >> 4) as usize;
            let mut match_len = (token & 0x0F) as usize;

            // Extended literal length
            if literal_len == 15 {
                loop {
                    if pos >= input.len() {
                        return Err(Error::CorruptedData);
                    }
                    let byte = input[pos];
                    pos += 1;
                    literal_len += byte as usize;
                    if byte != 255 {
                        break;
                    }
                }
            }

            // Copy literals
            if literal_len > 0 {
                if pos + literal_len > input.len() {
                    return Err(Error::CorruptedData);
                }
                output.extend_from_slice(&input[pos..pos + literal_len]);
                pos += literal_len;
            }

            // Check if we have a match (not end of block)
            if pos >= input.len() {
                break;
            }

            // Read offset
            if pos + 2 > input.len() {
                return Err(Error::CorruptedData);
            }
            let offset = (input[pos] as usize) | ((input[pos + 1] as usize) << 8);
            pos += 2;

            if offset == 0 {
                return Err(Error::CorruptedData);
            }

            // Extended match length
            if match_len == 15 {
                loop {
                    if pos >= input.len() {
                        return Err(Error::CorruptedData);
                    }
                    let byte = input[pos];
                    pos += 1;
                    match_len += byte as usize;
                    if byte != 255 {
                        break;
                    }
                }
            }

            // Adjust match length
            match_len += 4; // MIN_MATCH

            // Copy match
            let match_start = output.len() - offset;
            if match_start > output.len() {
                return Err(Error::CorruptedData);
            }

            // Handle overlapping copy
            for i in 0..match_len {
                let byte = output[match_start + i];
                output.push(byte);
            }
        }

        // Verify output size
        if output.len() - start_len != original_size {
            return Err(Error::CorruptedData);
        }

        Ok(())
    }
}

impl Default for Decompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compress, Options};

    #[test]
    fn test_decompress_roundtrip() {
        let data = b"Hello, World! Hello, World! Hello, World!";
        let compressed = compress(data, &Options::default()).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_decompress_empty() {
        let data = b"";
        let compressed = compress(data, &Options::default()).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_decompress_invalid_magic() {
        let result = decompress(b"XXXX\x01\x00");
        assert!(matches!(result, Err(Error::InvalidMagic)));
    }

    #[test]
    fn test_decompress_truncated() {
        let result = decompress(b"FPC");
        assert!(matches!(result, Err(Error::CorruptedData)));
    }
}

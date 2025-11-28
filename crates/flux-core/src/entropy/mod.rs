//! FSE (Finite State Entropy) coding
//!
//! Modern entropy coder providing zstd-level compression with high speed.

use crate::{Error, Result};

/// FSE compression state
pub struct FseEncoder {
    /// Symbol frequency table
    table: Vec<u32>,
    /// Table log (determines table size)
    table_log: u8,
}

/// FSE decompression state
pub struct FseDecoder {
    /// Decoding table
    table: Vec<FseDecodingEntry>,
    /// Table log
    table_log: u8,
}

/// Entry in decoding table
#[derive(Clone, Copy)]
struct FseDecodingEntry {
    symbol: u8,
    bits: u8,
    baseline: u16,
}

impl FseEncoder {
    /// Create encoder from symbol frequencies
    pub fn from_frequencies(freqs: &[u32]) -> Result<Self> {
        let total: u32 = freqs.iter().sum();
        if total == 0 {
            return Err(Error::EncodeError("Empty frequency table".into()));
        }

        // Determine table log (aim for 10-12 for good balance)
        let table_log = 10u8;

        Ok(Self {
            table: freqs.to_vec(),
            table_log,
        })
    }

    /// Compress data using FSE
    pub fn compress(&self, input: &[u8]) -> Result<Vec<u8>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }

        // TODO: Full FSE implementation
        // For now, use simple frequency-based encoding
        let mut output = Vec::with_capacity(input.len());

        // Write table log
        output.push(self.table_log);

        // Write frequency table (normalized)
        let table_size = 1 << self.table_log;
        output.extend_from_slice(&(self.table.len() as u16).to_le_bytes());

        for &freq in &self.table {
            output.extend_from_slice(&freq.to_le_bytes());
        }

        // Write compressed data length
        output.extend_from_slice(&(input.len() as u32).to_le_bytes());

        // For initial implementation, store raw (FSE proper implementation pending)
        output.extend_from_slice(input);

        Ok(output)
    }
}

impl FseDecoder {
    /// Create decoder from encoded header
    pub fn from_header(data: &[u8]) -> Result<(Self, usize)> {
        if data.is_empty() {
            return Err(Error::DecodeError("Empty FSE header".into()));
        }

        let table_log = data[0];
        let mut pos = 1;

        // Read symbol count
        if pos + 2 > data.len() {
            return Err(Error::DecodeError("Truncated FSE header".into()));
        }
        let symbol_count = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;

        // Read frequencies
        let mut table = Vec::with_capacity(symbol_count);
        for _ in 0..symbol_count {
            if pos + 4 > data.len() {
                return Err(Error::DecodeError("Truncated frequency table".into()));
            }
            let freq = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            pos += 4;

            table.push(FseDecodingEntry {
                symbol: table.len() as u8,
                bits: 0,
                baseline: freq as u16,
            });
        }

        Ok((Self { table, table_log }, pos))
    }

    /// Decompress FSE-encoded data
    pub fn decompress(&self, data: &[u8], header_size: usize) -> Result<Vec<u8>> {
        if data.len() < header_size + 4 {
            return Err(Error::DecodeError("Truncated FSE data".into()));
        }

        let pos = header_size;
        let orig_len = u32::from_le_bytes([
            data[pos], data[pos + 1], data[pos + 2], data[pos + 3]
        ]) as usize;

        // For initial implementation, data is stored raw
        let data_start = pos + 4;
        if data.len() < data_start + orig_len {
            return Err(Error::DecodeError("Truncated FSE payload".into()));
        }

        Ok(data[data_start..data_start + orig_len].to_vec())
    }
}

/// High-level FSE compress function
pub fn fse_compress(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    // Count frequencies
    let mut freqs = vec![0u32; 256];
    for &byte in input {
        freqs[byte as usize] += 1;
    }

    let encoder = FseEncoder::from_frequencies(&freqs)?;
    encoder.compress(input)
}

/// High-level FSE decompress function
pub fn fse_decompress(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let (decoder, header_size) = FseDecoder::from_header(input)?;
    decoder.decompress(input, header_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fse_roundtrip() {
        let data = b"hello world, this is a test of FSE compression!";

        let compressed = fse_compress(data).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_fse_empty() {
        let data: &[u8] = &[];

        let compressed = fse_compress(data).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_fse_repetitive() {
        let data = vec![b'a'; 1000];

        let compressed = fse_compress(&data).unwrap();
        let decompressed = fse_decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }
}

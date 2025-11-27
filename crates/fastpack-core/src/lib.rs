//! FastPack - High-performance compression library
//!
//! A fast, streaming-capable compression format optimized for client-server communication.
//!
//! ## Algorithms
//!
//! - **LZ4-style**: Fast, general-purpose compression (default)
//! - **APEX**: Advanced JSON-aware compression with learning capabilities

mod compress;
mod decompress;
mod frame;
pub mod apex;

pub use compress::{compress, compress_to, Compressor};
pub use decompress::{decompress, decompress_to, Decompressor};
pub use frame::{FrameHeader, Flags, MAGIC, VERSION};
pub use apex::{apex_compress, apex_decompress, ApexSession, ApexOptions};

/// Compression level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// No compression, just framing
    None = 0,
    /// Fast compression (default)
    #[default]
    Fast = 1,
    /// Better compression ratio, slower
    Better = 2,
}

/// Compression options
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Compression level
    pub level: Level,
    /// Enable checksum
    pub checksum: bool,
}

/// Error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Invalid magic bytes
    InvalidMagic,
    /// Unsupported version
    UnsupportedVersion,
    /// Corrupted data
    CorruptedData,
    /// Buffer too small
    BufferTooSmall,
    /// Invalid block
    InvalidBlock,
    /// Checksum mismatch
    ChecksumMismatch,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidMagic => write!(f, "invalid magic bytes"),
            Error::UnsupportedVersion => write!(f, "unsupported version"),
            Error::CorruptedData => write!(f, "corrupted data"),
            Error::BufferTooSmall => write!(f, "buffer too small"),
            Error::InvalidBlock => write!(f, "invalid block"),
            Error::ChecksumMismatch => write!(f, "checksum mismatch"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_empty() {
        let data = b"";
        let compressed = compress(data, &Options::default()).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_roundtrip_small() {
        let data = b"Hello, FastPack!";
        let compressed = compress(data, &Options::default()).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_roundtrip_repeated() {
        let data = b"abcabcabcabcabcabcabcabcabcabc";
        let compressed = compress(data, &Options::default()).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
        // Repeated data should compress well
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_roundtrip_large() {
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let compressed = compress(&data, &Options::default()).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_roundtrip_json() {
        let data = br#"{"id":123,"name":"test","data":[1,2,3],"nested":{"key":"value"}}"#;
        let compressed = compress(data, &Options::default()).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_level_none() {
        let data = b"test data";
        let opts = Options { level: Level::None, checksum: false };
        let compressed = compress(data, &opts).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }
}

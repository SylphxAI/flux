//! FLUX error types

use thiserror::Error;

/// FLUX error type
#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid magic number")]
    InvalidMagic,

    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u8),

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Schema not found: {0}")]
    SchemaNotFound(u32),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Encode error: {0}")]
    EncodeError(String),

    #[error("Decode error: {0}")]
    DecodeError(String),

    #[error("Serialize error: {0}")]
    SerializeError(String),

    #[error("Checksum mismatch")]
    ChecksumMismatch,

    #[error("Buffer overflow")]
    BufferOverflow,

    #[error("Invalid encoding: {0}")]
    InvalidEncoding(String),

    #[error("State desync: expected hash {expected:016x}, got {actual:016x}")]
    StateDesync { expected: u64, actual: u64 },

    #[error("Unsupported type: {0}")]
    UnsupportedType(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// FLUX result type
pub type Result<T> = std::result::Result<T, Error>;

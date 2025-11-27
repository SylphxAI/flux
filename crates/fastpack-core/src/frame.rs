//! Frame format for FastPack
//!
//! ```text
//! ┌──────────┬─────────┬───────┬────────────┐
//! │ Magic    │ Version │ Flags │ Blocks...  │
//! │ 4 bytes  │ 1 byte  │ 1 byte│            │
//! └──────────┴─────────┴───────┴────────────┘
//!
//! Block format:
//! ┌─────────────────┬─────────────────┬──────────────┐
//! │ Compressed Size │ Original Size   │ Data         │
//! │ varint          │ varint          │ N bytes      │
//! └─────────────────┴─────────────────┴──────────────┘
//!
//! End marker: Compressed Size = 0
//! ```

use crate::{Error, Result};

/// Magic bytes: "FPCK"
pub const MAGIC: [u8; 4] = *b"FPCK";

/// Current format version
pub const VERSION: u8 = 1;

/// Maximum block size (64KB)
pub const MAX_BLOCK_SIZE: usize = 64 * 1024;

/// Flags for frame header
#[derive(Debug, Clone, Copy, Default)]
pub struct Flags(u8);

impl Flags {
    pub const CHECKSUM: u8 = 0b0000_0001;
    pub const DICTIONARY: u8 = 0b0000_0010;
    pub const STREAMING: u8 = 0b0000_0100;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn with_checksum(mut self) -> Self {
        self.0 |= Self::CHECKSUM;
        self
    }

    pub fn has_checksum(&self) -> bool {
        self.0 & Self::CHECKSUM != 0
    }

    pub fn as_byte(&self) -> u8 {
        self.0
    }

    pub fn from_byte(byte: u8) -> Self {
        Self(byte)
    }
}

/// Frame header
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub version: u8,
    pub flags: Flags,
}

impl FrameHeader {
    pub const SIZE: usize = 6; // magic(4) + version(1) + flags(1)

    pub fn new(flags: Flags) -> Self {
        Self {
            version: VERSION,
            flags,
        }
    }

    pub fn write_to(&self, buf: &mut [u8]) -> Result<usize> {
        if buf.len() < Self::SIZE {
            return Err(Error::BufferTooSmall);
        }
        buf[0..4].copy_from_slice(&MAGIC);
        buf[4] = self.version;
        buf[5] = self.flags.as_byte();
        Ok(Self::SIZE)
    }

    pub fn read_from(buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            return Err(Error::BufferTooSmall);
        }
        if buf[0..4] != MAGIC {
            return Err(Error::InvalidMagic);
        }
        let version = buf[4];
        if version > VERSION {
            return Err(Error::UnsupportedVersion);
        }
        let flags = Flags::from_byte(buf[5]);
        Ok(Self { version, flags })
    }
}

/// Write a varint to buffer, return bytes written
#[inline]
pub fn write_varint(mut value: usize, buf: &mut [u8]) -> usize {
    let mut i = 0;
    while value >= 0x80 {
        buf[i] = (value as u8) | 0x80;
        value >>= 7;
        i += 1;
    }
    buf[i] = value as u8;
    i + 1
}

/// Read a varint from buffer, return (value, bytes_read)
#[inline]
pub fn read_varint(buf: &[u8]) -> Result<(usize, usize)> {
    let mut value: usize = 0;
    let mut shift = 0;
    let mut i = 0;

    loop {
        if i >= buf.len() {
            return Err(Error::CorruptedData);
        }
        let byte = buf[i];
        value |= ((byte & 0x7F) as usize) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 64 {
            return Err(Error::CorruptedData);
        }
    }

    Ok((value, i))
}

/// Block header: compressed_size, original_size
pub struct BlockHeader {
    pub compressed_size: usize,
    pub original_size: usize,
}

impl BlockHeader {
    /// Write block header, return bytes written
    pub fn write_to(&self, buf: &mut [u8]) -> usize {
        let n1 = write_varint(self.compressed_size, buf);
        let n2 = write_varint(self.original_size, &mut buf[n1..]);
        n1 + n2
    }

    /// Read block header, return (header, bytes_read)
    pub fn read_from(buf: &[u8]) -> Result<(Self, usize)> {
        let (compressed_size, n1) = read_varint(buf)?;
        let (original_size, n2) = read_varint(&buf[n1..])?;
        Ok((
            Self {
                compressed_size,
                original_size,
            },
            n1 + n2,
        ))
    }

    /// Check if this is end marker
    pub fn is_end(&self) -> bool {
        self.compressed_size == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_small() {
        let mut buf = [0u8; 10];
        let n = write_varint(127, &mut buf);
        assert_eq!(n, 1);
        let (v, n2) = read_varint(&buf).unwrap();
        assert_eq!(v, 127);
        assert_eq!(n2, 1);
    }

    #[test]
    fn test_varint_medium() {
        let mut buf = [0u8; 10];
        let n = write_varint(300, &mut buf);
        assert_eq!(n, 2);
        let (v, _) = read_varint(&buf).unwrap();
        assert_eq!(v, 300);
    }

    #[test]
    fn test_varint_large() {
        let mut buf = [0u8; 10];
        let value = 1_000_000;
        write_varint(value, &mut buf);
        let (v, _) = read_varint(&buf).unwrap();
        assert_eq!(v, value);
    }

    #[test]
    fn test_frame_header() {
        let header = FrameHeader::new(Flags::new().with_checksum());
        let mut buf = [0u8; 10];
        header.write_to(&mut buf).unwrap();

        let parsed = FrameHeader::read_from(&buf).unwrap();
        assert_eq!(parsed.version, VERSION);
        assert!(parsed.flags.has_checksum());
    }
}

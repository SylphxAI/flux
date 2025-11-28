//! FLUX frame format

use crate::{Error, Result, FLUX_MAGIC, FLUX_VERSION};
use bitflags::bitflags;

bitflags! {
    /// Frame flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FrameFlags: u8 {
        /// Schema definition included in payload
        const SCHEMA_INCLUDED = 0b0000_0001;
        /// Data in columnar format
        const COLUMNAR = 0b0000_0010;
        /// FSE entropy coding applied
        const FSE_COMPRESSED = 0b0000_0100;
        /// Payload is delta update
        const DELTA_MESSAGE = 0b0000_1000;
        /// CRC32 checksum included
        const CHECKSUM_PRESENT = 0b0001_0000;
        /// Contains dictionary entries
        const DICTIONARY_UPDATE = 0b0010_0000;
        /// Part of streaming session
        const STREAMING = 0b0100_0000;
    }
}

/// FLUX frame header
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub version: u8,
    pub flags: FrameFlags,
    pub schema_id: u32,
    pub payload_len: u32,
    pub checksum: Option<u32>,
}

impl FrameHeader {
    /// Parse header from bytes (after magic)
    pub fn parse(buf: &[u8]) -> Result<Self> {
        if buf.len() < 14 {
            return Err(Error::InvalidFrame("Header too short".into()));
        }

        let version = buf[0];
        if version != FLUX_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }

        let flags = FrameFlags::from_bits_truncate(buf[1]);

        let schema_id = u32::from_le_bytes([buf[2], buf[3], buf[4], buf[5]]);
        let payload_len = u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);

        let checksum = if flags.contains(FrameFlags::CHECKSUM_PRESENT) {
            Some(u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]))
        } else {
            None
        };

        Ok(Self {
            version,
            flags,
            schema_id,
            payload_len,
            checksum,
        })
    }

    /// Serialize header to bytes
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push(self.version);
        buf.push(self.flags.bits());
        buf.extend_from_slice(&self.schema_id.to_le_bytes());
        buf.extend_from_slice(&self.payload_len.to_le_bytes());

        if let Some(checksum) = self.checksum {
            buf.extend_from_slice(&checksum.to_le_bytes());
        }
    }
}

/// Frame writer
#[allow(dead_code)]
pub struct FrameWriter {
    checksum_enabled: bool,
}

impl FrameWriter {
    pub fn new() -> Self {
        Self {
            checksum_enabled: true,
        }
    }

    /// Write frame header
    pub fn write_header(&mut self, header: &FrameHeader, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&FLUX_MAGIC);
        header.serialize(buf);
    }

    /// Write varint
    pub fn write_varint(&self, mut value: u64, buf: &mut Vec<u8>) {
        while value >= 0x80 {
            buf.push((value as u8 & 0x7F) | 0x80);
            value >>= 7;
        }
        buf.push(value as u8);
    }

    /// Write checksum
    pub fn write_checksum(&self, data: &[u8], buf: &mut Vec<u8>) {
        let checksum = crc32c::crc32c(data);
        buf.extend_from_slice(&checksum.to_le_bytes());
    }
}

impl Default for FrameWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame reader
pub struct FrameReader {
    pos: usize,
}

impl FrameReader {
    pub fn new() -> Self {
        Self { pos: 0 }
    }

    /// Read and validate magic
    pub fn read_magic(&mut self, buf: &[u8]) -> Result<()> {
        if buf.len() < 4 {
            return Err(Error::InvalidFrame("Too short for magic".into()));
        }

        if buf[0..4] != FLUX_MAGIC {
            return Err(Error::InvalidMagic);
        }

        self.pos = 4;
        Ok(())
    }

    /// Read header
    pub fn read_header(&mut self, buf: &[u8]) -> Result<FrameHeader> {
        let header = FrameHeader::parse(&buf[self.pos..])?;
        self.pos += 14;
        Ok(header)
    }

    /// Read varint
    pub fn read_varint(&mut self, buf: &[u8]) -> Result<u64> {
        let mut result: u64 = 0;
        let mut shift = 0;

        loop {
            if self.pos >= buf.len() {
                return Err(Error::InvalidFrame("Varint truncated".into()));
            }

            let byte = buf[self.pos];
            self.pos += 1;

            result |= ((byte & 0x7F) as u64) << shift;

            if byte & 0x80 == 0 {
                break;
            }

            shift += 7;
            if shift > 63 {
                return Err(Error::InvalidFrame("Varint too long".into()));
            }
        }

        Ok(result)
    }

    /// Current position
    pub fn position(&self) -> usize {
        self.pos
    }
}

impl Default for FrameReader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_roundtrip() {
        let header = FrameHeader {
            version: FLUX_VERSION,
            flags: FrameFlags::SCHEMA_INCLUDED | FrameFlags::COLUMNAR,
            schema_id: 42,
            payload_len: 1024,
            checksum: Some(0x12345678),
        };

        let mut buf = Vec::new();
        header.serialize(&mut buf);

        let parsed = FrameHeader::parse(&buf).unwrap();

        assert_eq!(parsed.version, header.version);
        assert_eq!(parsed.flags, header.flags);
        assert_eq!(parsed.schema_id, header.schema_id);
        assert_eq!(parsed.payload_len, header.payload_len);
    }

    #[test]
    fn test_varint_roundtrip() {
        let writer = FrameWriter::new();
        let mut reader = FrameReader::new();

        let values = [0u64, 1, 127, 128, 255, 256, 16383, 16384, u64::MAX];

        for &value in &values {
            let mut buf = Vec::new();
            writer.write_varint(value, &mut buf);

            reader.pos = 0;
            let decoded = reader.read_varint(&buf).unwrap();

            assert_eq!(decoded, value, "Failed for value {}", value);
        }
    }
}

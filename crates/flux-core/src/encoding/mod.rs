//! Type-specific encoding implementations

mod varint;
mod integer;
mod string;

pub use varint::{encode_varint, decode_varint};

use crate::{Error, Result};
use crate::types::Value;
use crate::schema::Schema;

/// Main encoder that orchestrates type-specific encoders
pub struct Encoder {
    string_dict: StringDictionary,
}

/// String dictionary for compression
pub struct StringDictionary {
    entries: Vec<String>,
    index: std::collections::HashMap<String, u32>,
}

impl StringDictionary {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: std::collections::HashMap::new(),
        }
    }

    pub fn get_or_add(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.index.get(s) {
            return id;
        }

        let id = self.entries.len() as u32;
        self.entries.push(s.to_string());
        self.index.insert(s.to_string(), id);
        id
    }

    pub fn get(&self, id: u32) -> Option<&str> {
        self.entries.get(id as usize).map(|s| s.as_str())
    }
}

impl Default for StringDictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            string_dict: StringDictionary::new(),
        }
    }

    /// Encode a JSON value according to schema
    pub fn encode(&mut self, value: &serde_json::Value, schema: &Schema) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.encode_value(value, &mut buf)?;
        Ok(buf)
    }

    /// Decode data according to schema
    pub fn decode(&self, data: &[u8], schema: &Schema) -> Result<serde_json::Value> {
        let mut pos = 0;
        self.decode_value(data, &mut pos)
    }

    fn encode_value(&mut self, value: &serde_json::Value, buf: &mut Vec<u8>) -> Result<()> {
        match value {
            serde_json::Value::Null => {
                buf.push(0x00);
            }
            serde_json::Value::Bool(b) => {
                buf.push(if *b { 0x01 } else { 0x00 });
            }
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    let encoded = varint::zigzag_encode(i);
                    encode_varint(encoded, buf);
                } else if let Some(f) = n.as_f64() {
                    buf.extend_from_slice(&f.to_le_bytes());
                }
            }
            serde_json::Value::String(s) => {
                encode_varint(s.len() as u64, buf);
                buf.extend_from_slice(s.as_bytes());
            }
            serde_json::Value::Array(arr) => {
                encode_varint(arr.len() as u64, buf);
                for item in arr {
                    self.encode_value(item, buf)?;
                }
            }
            serde_json::Value::Object(obj) => {
                encode_varint(obj.len() as u64, buf);
                for (key, val) in obj {
                    encode_varint(key.len() as u64, buf);
                    buf.extend_from_slice(key.as_bytes());
                    self.encode_value(val, buf)?;
                }
            }
        }
        Ok(())
    }

    fn decode_value(&self, data: &[u8], pos: &mut usize) -> Result<serde_json::Value> {
        // Simplified decoder - in production would use schema for type info
        if *pos >= data.len() {
            return Ok(serde_json::Value::Null);
        }

        // For now, just return the raw data as we need more context
        // This is a placeholder - full implementation would use schema
        Err(Error::DecodeError("Full decoder not yet implemented".into()))
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        let values = [0u64, 1, 127, 128, 255, 16383, 16384, 2097151];

        for &value in &values {
            let mut buf = Vec::new();
            encode_varint(value, &mut buf);

            let (decoded, _) = decode_varint(&buf).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn test_string_dictionary() {
        let mut dict = StringDictionary::new();

        let id1 = dict.get_or_add("hello");
        let id2 = dict.get_or_add("world");
        let id3 = dict.get_or_add("hello"); // Duplicate

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 0); // Same as id1

        assert_eq!(dict.get(0), Some("hello"));
        assert_eq!(dict.get(1), Some("world"));
    }
}

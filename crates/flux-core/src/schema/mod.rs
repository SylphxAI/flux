//! Schema inference and management

mod inference;
mod cache;

pub use inference::SchemaInferrer;
pub use cache::SchemaCache;

use crate::{Error, Result};
use crate::types::FieldType;

/// Schema definition
#[derive(Debug, Clone)]
pub struct Schema {
    pub id: u32,
    pub version: u16,
    pub hash: u64,
    pub fields: Vec<FieldDef>,
}

/// Field definition
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub field_type: FieldType,
    pub nullable: bool,
}

impl Schema {
    /// Create a new schema with auto-generated ID
    pub fn new(fields: Vec<FieldDef>) -> Self {
        let hash = Self::compute_hash(&fields);
        Self {
            id: 0,
            version: 1,
            hash,
            fields,
        }
    }

    /// Compute schema hash
    pub(crate) fn compute_hash(fields: &[FieldDef]) -> u64 {
        // FNV-1a hash
        let mut hash: u64 = 0xcbf29ce484222325;

        for field in fields {
            for byte in field.name.bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }

            hash ^= field.field_type.type_id() as u64;
            hash = hash.wrapping_mul(0x100000001b3);

            hash ^= field.nullable as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }

        hash
    }

    /// Serialize schema to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // ID and version
        buf.extend_from_slice(&self.id.to_le_bytes());
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.hash.to_le_bytes());

        // Field count
        buf.push(self.fields.len() as u8);

        // Fields
        for field in &self.fields {
            // Name length + name
            buf.push(field.name.len() as u8);
            buf.extend_from_slice(field.name.as_bytes());

            // Type ID
            buf.push(field.field_type.type_id());

            // Flags
            let flags = if field.nullable { 0x01 } else { 0x00 };
            buf.push(flags);

            // TODO: Serialize nested types
        }

        buf
    }

    /// Deserialize schema from bytes
    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        if buf.len() < 15 {
            return Err(Error::InvalidFrame("Schema too short".into()));
        }

        let id = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let version = u16::from_le_bytes([buf[4], buf[5]]);
        let hash = u64::from_le_bytes([
            buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12], buf[13],
        ]);

        let field_count = buf[14] as usize;
        let mut pos = 15;
        let mut fields = Vec::with_capacity(field_count);

        for _ in 0..field_count {
            if pos >= buf.len() {
                return Err(Error::InvalidFrame("Schema truncated".into()));
            }

            let name_len = buf[pos] as usize;
            pos += 1;

            if pos + name_len > buf.len() {
                return Err(Error::InvalidFrame("Field name truncated".into()));
            }

            let name = String::from_utf8_lossy(&buf[pos..pos + name_len]).into_owned();
            pos += name_len;

            let type_id = buf[pos];
            pos += 1;

            let flags = buf[pos];
            pos += 1;

            let field_type = match type_id {
                0x00 => FieldType::Null,
                0x01 => FieldType::Boolean,
                0x02..=0x06 => FieldType::Integer(crate::types::IntegerType::Varint),
                0x07 | 0x08 => FieldType::Float(crate::types::FloatType::Float64),
                0x09 => FieldType::String,
                _ => FieldType::String, // Fallback
            };

            fields.push(FieldDef {
                name,
                field_type,
                nullable: flags & 0x01 != 0,
            });
        }

        Ok(Self {
            id,
            version,
            hash,
            fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::IntegerType;

    #[test]
    fn test_schema_serialize_deserialize() {
        let schema = Schema::new(vec![
            FieldDef {
                name: "id".into(),
                field_type: FieldType::Integer(IntegerType::Int32),
                nullable: false,
            },
            FieldDef {
                name: "name".into(),
                field_type: FieldType::String,
                nullable: true,
            },
        ]);

        let bytes = schema.serialize();
        let parsed = Schema::deserialize(&bytes).unwrap();

        assert_eq!(parsed.fields.len(), 2);
        assert_eq!(parsed.fields[0].name, "id");
        assert_eq!(parsed.fields[1].name, "name");
        assert!(!parsed.fields[0].nullable);
        assert!(parsed.fields[1].nullable);
    }
}

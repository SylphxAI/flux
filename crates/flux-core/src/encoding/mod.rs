//! Type-specific encoding implementations

pub mod varint;
pub mod integer;
pub mod string;

pub use varint::{encode_varint, decode_varint, zigzag_encode, zigzag_decode};

use crate::{Error, Result};
use crate::types::{FieldType, IntegerType, FloatType};
use crate::schema::Schema;

/// Main encoder that orchestrates type-specific encoders
#[allow(dead_code)]
pub struct Encoder {
    /// String dictionary for key compression
    key_dict: StringDictionary,
    /// String dictionary for value compression
    value_dict: StringDictionary,
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

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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
            key_dict: StringDictionary::new(),
            value_dict: StringDictionary::new(),
        }
    }

    /// Encode a JSON value according to schema
    pub fn encode(&mut self, value: &serde_json::Value, schema: &Schema) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.encode_with_schema(value, schema, &mut buf)?;
        Ok(buf)
    }

    /// Decode data according to schema
    pub fn decode(&self, data: &[u8], schema: &Schema) -> Result<serde_json::Value> {
        let mut pos = 0;
        self.decode_with_schema(data, &mut pos, schema)
    }

    /// Encode value using schema for type information
    fn encode_with_schema(
        &mut self,
        value: &serde_json::Value,
        schema: &Schema,
        buf: &mut Vec<u8>,
    ) -> Result<()> {
        match value {
            serde_json::Value::Object(obj) => {
                // Encode fields in schema order (eliminates key storage!)
                for field in &schema.fields {
                    if let Some(field_value) = obj.get(&field.name) {
                        // Field present
                        if field.nullable {
                            buf.push(0x01); // Present flag
                        }
                        self.encode_typed_value(field_value, &field.field_type, buf)?;
                    } else {
                        // Field absent (must be nullable)
                        if field.nullable {
                            buf.push(0x00); // Absent flag
                        } else {
                            return Err(Error::EncodeError(format!(
                                "Required field '{}' missing", field.name
                            )));
                        }
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                // For array at root level
                encode_varint(arr.len() as u64, buf);
                for item in arr {
                    self.encode_with_schema(item, schema, buf)?;
                }
            }
            _ => {
                // Single value at root (unusual for JSON APIs)
                self.encode_typed_value(value, &FieldType::infer(value), buf)?;
            }
        }
        Ok(())
    }

    /// Encode a value using its type information
    fn encode_typed_value(
        &mut self,
        value: &serde_json::Value,
        field_type: &FieldType,
        buf: &mut Vec<u8>,
    ) -> Result<()> {
        match (value, field_type) {
            (serde_json::Value::Null, _) => {
                // Null is encoded as absence for nullable fields
                // If we get here, encode as 0
                buf.push(0x00);
            }

            (serde_json::Value::Bool(b), FieldType::Boolean) => {
                buf.push(if *b { 0x01 } else { 0x00 });
            }

            (serde_json::Value::Number(n), FieldType::Integer(int_type)) => {
                let i = n.as_i64().unwrap_or(0);
                match int_type {
                    IntegerType::Int8 => buf.push(i as u8),
                    IntegerType::Int16 => buf.extend_from_slice(&(i as i16).to_le_bytes()),
                    IntegerType::Int32 => buf.extend_from_slice(&(i as i32).to_le_bytes()),
                    IntegerType::Int64 => buf.extend_from_slice(&i.to_le_bytes()),
                    IntegerType::Varint => {
                        let encoded = zigzag_encode(i);
                        encode_varint(encoded, buf);
                    }
                }
            }

            (serde_json::Value::Number(n), FieldType::Float(float_type)) => {
                let f = n.as_f64().unwrap_or(0.0);
                match float_type {
                    FloatType::Float32 => buf.extend_from_slice(&(f as f32).to_le_bytes()),
                    FloatType::Float64 => buf.extend_from_slice(&f.to_le_bytes()),
                }
            }

            (serde_json::Value::String(s), FieldType::String) => {
                encode_varint(s.len() as u64, buf);
                buf.extend_from_slice(s.as_bytes());
            }

            (serde_json::Value::String(s), FieldType::Timestamp) => {
                // Parse ISO 8601 timestamp to epoch milliseconds (8 bytes)
                if let Some(millis) = parse_iso8601_to_millis(s) {
                    buf.push(0x01); // Binary timestamp flag
                    buf.extend_from_slice(&millis.to_le_bytes());
                } else {
                    // Fallback to string storage
                    buf.push(0x00); // String flag
                    encode_varint(s.len() as u64, buf);
                    buf.extend_from_slice(s.as_bytes());
                }
            }

            (serde_json::Value::String(s), FieldType::Uuid) => {
                // Store as 16 bytes if valid UUID, otherwise as string
                if s.len() == 36 {
                    // Parse UUID to bytes
                    let hex: String = s.chars().filter(|c| *c != '-').collect();
                    if hex.len() == 32 {
                        if let Ok(bytes) = hex::decode(&hex) {
                            buf.extend_from_slice(&bytes);
                            return Ok(());
                        }
                    }
                }
                // Fallback to string encoding
                encode_varint(s.len() as u64, buf);
                buf.extend_from_slice(s.as_bytes());
            }

            (serde_json::Value::Array(arr), FieldType::Array(elem_type)) => {
                encode_varint(arr.len() as u64, buf);
                for item in arr {
                    self.encode_typed_value(item, elem_type, buf)?;
                }
            }

            (serde_json::Value::Object(obj), FieldType::Object(fields)) => {
                // Encode in field order
                for (name, ftype) in fields {
                    if let Some(v) = obj.get(name) {
                        self.encode_typed_value(v, ftype, buf)?;
                    } else {
                        // Missing field - encode null
                        buf.push(0x00);
                    }
                }
            }

            // Fallback: use generic encoding
            _ => {
                self.encode_generic(value, buf)?;
            }
        }
        Ok(())
    }

    /// Generic encoding when type doesn't match schema
    fn encode_generic(&mut self, value: &serde_json::Value, buf: &mut Vec<u8>) -> Result<()> {
        match value {
            serde_json::Value::Null => buf.push(0x00),
            serde_json::Value::Bool(b) => buf.push(if *b { 0x01 } else { 0x00 }),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    encode_varint(zigzag_encode(i), buf);
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
                    self.encode_generic(item, buf)?;
                }
            }
            serde_json::Value::Object(obj) => {
                encode_varint(obj.len() as u64, buf);
                for (key, val) in obj {
                    encode_varint(key.len() as u64, buf);
                    buf.extend_from_slice(key.as_bytes());
                    self.encode_generic(val, buf)?;
                }
            }
        }
        Ok(())
    }

    /// Decode value using schema
    fn decode_with_schema(
        &self,
        data: &[u8],
        pos: &mut usize,
        schema: &Schema,
    ) -> Result<serde_json::Value> {
        let mut obj = serde_json::Map::new();

        for field in &schema.fields {
            if field.nullable {
                if *pos >= data.len() {
                    return Err(Error::DecodeError("Unexpected end of data".into()));
                }
                let present = data[*pos];
                *pos += 1;
                if present == 0x00 {
                    continue; // Field absent
                }
            }

            let value = self.decode_typed_value(data, pos, &field.field_type)?;
            obj.insert(field.name.clone(), value);
        }

        Ok(serde_json::Value::Object(obj))
    }

    /// Decode a typed value
    fn decode_typed_value(
        &self,
        data: &[u8],
        pos: &mut usize,
        field_type: &FieldType,
    ) -> Result<serde_json::Value> {
        match field_type {
            FieldType::Null => Ok(serde_json::Value::Null),

            FieldType::Boolean => {
                if *pos >= data.len() {
                    return Err(Error::DecodeError("Unexpected end of data".into()));
                }
                let v = data[*pos] != 0;
                *pos += 1;
                Ok(serde_json::Value::Bool(v))
            }

            FieldType::Integer(int_type) => {
                let i = match int_type {
                    IntegerType::Int8 => {
                        if *pos >= data.len() {
                            return Err(Error::DecodeError("Unexpected end of data".into()));
                        }
                        let v = data[*pos] as i8 as i64;
                        *pos += 1;
                        v
                    }
                    IntegerType::Int16 => {
                        if *pos + 2 > data.len() {
                            return Err(Error::DecodeError("Unexpected end of data".into()));
                        }
                        let v = i16::from_le_bytes([data[*pos], data[*pos + 1]]) as i64;
                        *pos += 2;
                        v
                    }
                    IntegerType::Int32 => {
                        if *pos + 4 > data.len() {
                            return Err(Error::DecodeError("Unexpected end of data".into()));
                        }
                        let v = i32::from_le_bytes([
                            data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]
                        ]) as i64;
                        *pos += 4;
                        v
                    }
                    IntegerType::Int64 => {
                        if *pos + 8 > data.len() {
                            return Err(Error::DecodeError("Unexpected end of data".into()));
                        }
                        let v = i64::from_le_bytes([
                            data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3],
                            data[*pos + 4], data[*pos + 5], data[*pos + 6], data[*pos + 7]
                        ]);
                        *pos += 8;
                        v
                    }
                    IntegerType::Varint => {
                        let (encoded, len) = decode_varint(&data[*pos..])?;
                        *pos += len;
                        zigzag_decode(encoded)
                    }
                };
                Ok(serde_json::Value::Number(i.into()))
            }

            FieldType::Float(float_type) => {
                let f = match float_type {
                    FloatType::Float32 => {
                        if *pos + 4 > data.len() {
                            return Err(Error::DecodeError("Unexpected end of data".into()));
                        }
                        let v = f32::from_le_bytes([
                            data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]
                        ]) as f64;
                        *pos += 4;
                        v
                    }
                    FloatType::Float64 => {
                        if *pos + 8 > data.len() {
                            return Err(Error::DecodeError("Unexpected end of data".into()));
                        }
                        let v = f64::from_le_bytes([
                            data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3],
                            data[*pos + 4], data[*pos + 5], data[*pos + 6], data[*pos + 7]
                        ]);
                        *pos += 8;
                        v
                    }
                };
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| Error::DecodeError("Invalid float".into()))
            }

            FieldType::String => {
                let (len, bytes_read) = decode_varint(&data[*pos..])?;
                *pos += bytes_read;

                if *pos + len as usize > data.len() {
                    return Err(Error::DecodeError("String length exceeds data".into()));
                }

                let s = std::str::from_utf8(&data[*pos..*pos + len as usize])
                    .map_err(|e| Error::DecodeError(e.to_string()))?;
                *pos += len as usize;
                Ok(serde_json::Value::String(s.to_string()))
            }

            FieldType::Timestamp => {
                if *pos >= data.len() {
                    return Err(Error::DecodeError("Timestamp truncated".into()));
                }
                let flag = data[*pos];
                *pos += 1;

                if flag == 0x01 {
                    // Binary timestamp (epoch millis)
                    if *pos + 8 > data.len() {
                        return Err(Error::DecodeError("Timestamp truncated".into()));
                    }
                    let millis = i64::from_le_bytes([
                        data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3],
                        data[*pos + 4], data[*pos + 5], data[*pos + 6], data[*pos + 7]
                    ]);
                    *pos += 8;
                    Ok(serde_json::Value::String(millis_to_iso8601(millis)))
                } else {
                    // String fallback
                    let (len, bytes_read) = decode_varint(&data[*pos..])?;
                    *pos += bytes_read;

                    if *pos + len as usize > data.len() {
                        return Err(Error::DecodeError("Timestamp string truncated".into()));
                    }

                    let s = std::str::from_utf8(&data[*pos..*pos + len as usize])
                        .map_err(|e| Error::DecodeError(e.to_string()))?;
                    *pos += len as usize;
                    Ok(serde_json::Value::String(s.to_string()))
                }
            }

            FieldType::Uuid => {
                // UUID stored as 16 bytes
                if *pos + 16 > data.len() {
                    return Err(Error::DecodeError("UUID truncated".into()));
                }
                let bytes = &data[*pos..*pos + 16];
                *pos += 16;

                // Convert to UUID string
                let uuid = format!(
                    "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    bytes[0], bytes[1], bytes[2], bytes[3],
                    bytes[4], bytes[5],
                    bytes[6], bytes[7],
                    bytes[8], bytes[9],
                    bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
                );
                Ok(serde_json::Value::String(uuid))
            }

            FieldType::Array(elem_type) => {
                let (len, bytes_read) = decode_varint(&data[*pos..])?;
                *pos += bytes_read;

                let mut arr = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    arr.push(self.decode_typed_value(data, pos, elem_type)?);
                }
                Ok(serde_json::Value::Array(arr))
            }

            FieldType::Object(fields) => {
                let mut obj = serde_json::Map::new();
                for (name, ftype) in fields {
                    let v = self.decode_typed_value(data, pos, ftype)?;
                    obj.insert(name.clone(), v);
                }
                Ok(serde_json::Value::Object(obj))
            }

            FieldType::Binary => {
                let (len, bytes_read) = decode_varint(&data[*pos..])?;
                *pos += bytes_read;

                if *pos + len as usize > data.len() {
                    return Err(Error::DecodeError("Binary length exceeds data".into()));
                }

                let bytes = &data[*pos..*pos + len as usize];
                *pos += len as usize;

                // Return as hex string
                let hex = hex::encode(bytes);
                Ok(serde_json::Value::String(hex))
            }

            FieldType::Union(types) => {
                // For union types, we need a type tag
                if *pos >= data.len() {
                    return Err(Error::DecodeError("Unexpected end of data".into()));
                }
                let type_idx = data[*pos] as usize;
                *pos += 1;

                if type_idx >= types.len() {
                    return Err(Error::DecodeError("Invalid union type index".into()));
                }

                self.decode_typed_value(data, pos, &types[type_idx])
            }

            FieldType::Decimal { .. } => {
                // Decimal stored as string for now
                let (len, bytes_read) = decode_varint(&data[*pos..])?;
                *pos += bytes_read;

                if *pos + len as usize > data.len() {
                    return Err(Error::DecodeError("Decimal length exceeds data".into()));
                }

                let s = std::str::from_utf8(&data[*pos..*pos + len as usize])
                    .map_err(|e| Error::DecodeError(e.to_string()))?;
                *pos += len as usize;
                Ok(serde_json::Value::String(s.to_string()))
            }
        }
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse ISO 8601 timestamp to epoch milliseconds
/// Supports: 2024-01-15T10:30:00Z, 2024-01-15T10:30:00.123Z, 2024-01-15
fn parse_iso8601_to_millis(s: &str) -> Option<i64> {
    // Full datetime with optional milliseconds: 2024-01-15T10:30:00Z or 2024-01-15T10:30:00.123Z
    if s.len() >= 20 && s.contains('T') && s.ends_with('Z') {
        let parts: Vec<&str> = s.trim_end_matches('Z').split('T').collect();
        if parts.len() == 2 {
            let date_parts: Vec<i32> = parts[0]
                .split('-')
                .filter_map(|p| p.parse().ok())
                .collect();

            // Handle time with optional milliseconds
            let time_str = parts[1];
            let (time_parts, millis) = if time_str.contains('.') {
                let tp: Vec<&str> = time_str.split('.').collect();
                let ms: i64 = tp.get(1).and_then(|m| m.parse().ok()).unwrap_or(0);
                (tp[0], ms)
            } else {
                (time_str, 0i64)
            };

            let time_nums: Vec<i32> = time_parts
                .split(':')
                .filter_map(|p| p.parse().ok())
                .collect();

            if date_parts.len() == 3 && time_nums.len() == 3 {
                let year = date_parts[0];
                let month = date_parts[1];
                let day = date_parts[2];
                let hour = time_nums[0];
                let minute = time_nums[1];
                let second = time_nums[2];

                // Calculate days since epoch (1970-01-01)
                let days = days_since_epoch(year, month, day);
                let seconds = days as i64 * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;
                return Some(seconds * 1000 + millis);
            }
        }
    }

    // Date only: 2024-01-15
    if s.len() == 10 && s.chars().filter(|c| *c == '-').count() == 2 {
        let parts: Vec<i32> = s.split('-').filter_map(|p| p.parse().ok()).collect();
        if parts.len() == 3 {
            let days = days_since_epoch(parts[0], parts[1], parts[2]);
            return Some(days as i64 * 86400 * 1000);
        }
    }

    None
}

/// Convert epoch milliseconds to ISO 8601 string
fn millis_to_iso8601(millis: i64) -> String {
    let total_seconds = millis / 1000;
    let ms = (millis % 1000) as u32;

    let days = (total_seconds / 86400) as i32;
    let remaining = (total_seconds % 86400) as i32;

    let hour = remaining / 3600;
    let minute = (remaining % 3600) / 60;
    let second = remaining % 60;

    let (year, month, day) = days_to_ymd(days);

    if ms > 0 {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            year, month, day, hour, minute, second, ms)
    } else {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year, month, day, hour, minute, second)
    }
}

/// Calculate days since Unix epoch (1970-01-01)
/// Uses Howard Hinnant's algorithm from chrono
fn days_since_epoch(year: i32, month: i32, day: i32) -> i32 {
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 12 } else { month };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Convert days since epoch to year, month, day
/// Uses Howard Hinnant's algorithm from chrono
fn days_to_ymd(days: i32) -> (i32, i32, i32) {
    let z = days + 719468;
    let era = if z >= 0 { z / 146097 } else { (z - 146096) / 146097 };
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SchemaInferrer;

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

    #[test]
    fn test_encoder_roundtrip_simple() {
        let json = serde_json::json!({
            "id": 123,
            "name": "test",
            "active": true
        });

        // Infer schema
        let mut inferrer = SchemaInferrer::new();
        inferrer.add_value(&json).unwrap();
        let schema = inferrer.infer().unwrap();

        // Encode
        let mut encoder = Encoder::new();
        let encoded = encoder.encode(&json, &schema).unwrap();

        // Decode
        let decoded = encoder.decode(&encoded, &schema).unwrap();

        // Compare
        assert_eq!(json, decoded);
    }

    #[test]
    fn test_encoder_roundtrip_nested() {
        let json = serde_json::json!({
            "user": {
                "id": 1,
                "name": "alice"
            },
            "score": 95.5
        });

        let mut inferrer = SchemaInferrer::new();
        inferrer.add_value(&json).unwrap();
        let schema = inferrer.infer().unwrap();

        let mut encoder = Encoder::new();
        let encoded = encoder.encode(&json, &schema).unwrap();
        let decoded = encoder.decode(&encoded, &schema).unwrap();

        assert_eq!(json, decoded);
    }

    #[test]
    fn test_encoder_roundtrip_array() {
        let json = serde_json::json!({
            "tags": ["a", "b", "c"],
            "count": 3
        });

        let mut inferrer = SchemaInferrer::new();
        inferrer.add_value(&json).unwrap();
        let schema = inferrer.infer().unwrap();

        let mut encoder = Encoder::new();
        let encoded = encoder.encode(&json, &schema).unwrap();
        let decoded = encoder.decode(&encoded, &schema).unwrap();

        assert_eq!(json, decoded);
    }

    #[test]
    fn test_encoder_size_savings() {
        // Create JSON with repeated keys
        let json = serde_json::json!({
            "user_id": 12345,
            "user_name": "alice",
            "user_email": "alice@example.com",
            "user_age": 30
        });

        let json_bytes = serde_json::to_vec(&json).unwrap();

        let mut inferrer = SchemaInferrer::new();
        inferrer.add_value(&json).unwrap();
        let schema = inferrer.infer().unwrap();

        let mut encoder = Encoder::new();
        let encoded = encoder.encode(&json, &schema).unwrap();

        // Schema-aware encoding should be smaller (no keys stored!)
        println!("JSON size: {}, Encoded size: {}", json_bytes.len(), encoded.len());
        assert!(encoded.len() < json_bytes.len(),
            "Encoded ({}) should be smaller than JSON ({})",
            encoded.len(), json_bytes.len());
    }

    #[test]
    fn test_timestamp_parsing() {
        // Full datetime
        let millis = parse_iso8601_to_millis("2024-01-15T10:30:00Z").unwrap();
        assert!(millis > 0);
        let back = millis_to_iso8601(millis);
        assert_eq!(back, "2024-01-15T10:30:00Z");

        // With milliseconds
        let millis = parse_iso8601_to_millis("2024-01-15T10:30:00.123Z").unwrap();
        let back = millis_to_iso8601(millis);
        assert_eq!(back, "2024-01-15T10:30:00.123Z");

        // Date only
        let millis = parse_iso8601_to_millis("2024-01-15").unwrap();
        assert!(millis > 0);
    }

    #[test]
    fn test_timestamp_size_savings() {
        // Timestamp string: "2024-01-15T10:30:00Z" = 20 bytes
        // Binary encoding: 1 flag + 8 bytes = 9 bytes
        // Savings: 11 bytes per timestamp
        let timestamp = "2024-01-15T10:30:00Z";
        assert!(parse_iso8601_to_millis(timestamp).is_some());

        // With 10 timestamps, save 110 bytes
    }
}

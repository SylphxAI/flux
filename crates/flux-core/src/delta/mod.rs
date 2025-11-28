//! Streaming delta compression
//!
//! Efficiently transmit only changes between similar JSON states.

use crate::{Error, Result};
use serde::{Serialize, Deserialize};

/// Delta operation types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeltaOp {
    /// No change - reference previous value
    Unchanged,
    /// Value was added
    Add(serde_json::Value),
    /// Value was removed
    Remove,
    /// Value was modified
    Modify(serde_json::Value),
    /// Array element operations
    ArrayOps(Vec<ArrayOp>),
    /// Object field operations
    ObjectOps(Vec<ObjectOp>),
}

/// Array-specific delta operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ArrayOp {
    /// Keep N elements unchanged
    Keep(usize),
    /// Insert elements at current position
    Insert(Vec<serde_json::Value>),
    /// Remove N elements
    Delete(usize),
    /// Replace element
    Replace(serde_json::Value),
}

/// Object-specific delta operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObjectOp {
    /// Field unchanged
    Keep(String),
    /// Field added
    Add(String, serde_json::Value),
    /// Field removed
    Remove(String),
    /// Field modified
    Modify(String, Box<DeltaOp>),
}

/// Delta encoder for streaming state changes
pub struct DeltaEncoder {
    /// Previous state for comparison
    prev_state: Option<serde_json::Value>,
    /// Schema hash for validation
    schema_hash: u64,
}

impl DeltaEncoder {
    /// Create new delta encoder
    pub fn new() -> Self {
        Self {
            prev_state: None,
            schema_hash: 0,
        }
    }

    /// Set schema hash for validation
    pub fn with_schema(mut self, hash: u64) -> Self {
        self.schema_hash = hash;
        self
    }

    /// Compute delta between previous and current state
    pub fn encode(&mut self, current: &serde_json::Value) -> Result<DeltaOp> {
        let delta = match &self.prev_state {
            None => DeltaOp::Add(current.clone()),
            Some(prev) => compute_delta(prev, current),
        };

        self.prev_state = Some(current.clone());
        Ok(delta)
    }

    /// Reset encoder state
    pub fn reset(&mut self) {
        self.prev_state = None;
    }
}

impl Default for DeltaEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Delta decoder for reconstructing state
pub struct DeltaDecoder {
    /// Current state
    current_state: Option<serde_json::Value>,
}

impl DeltaDecoder {
    /// Create new delta decoder
    pub fn new() -> Self {
        Self {
            current_state: None,
        }
    }

    /// Apply delta to reconstruct current state
    pub fn decode(&mut self, delta: &DeltaOp) -> Result<serde_json::Value> {
        let new_state = match (&self.current_state, delta) {
            (_, DeltaOp::Add(v)) => v.clone(),
            (None, _) => return Err(Error::DecodeError("No base state for delta".into())),
            (Some(prev), DeltaOp::Unchanged) => prev.clone(),
            (Some(_), DeltaOp::Remove) => serde_json::Value::Null,
            (Some(_), DeltaOp::Modify(v)) => v.clone(),
            (Some(prev), DeltaOp::ArrayOps(ops)) => apply_array_ops(prev, ops)?,
            (Some(prev), DeltaOp::ObjectOps(ops)) => apply_object_ops(prev, ops)?,
        };

        self.current_state = Some(new_state.clone());
        Ok(new_state)
    }

    /// Reset decoder state
    pub fn reset(&mut self) {
        self.current_state = None;
    }
}

impl Default for DeltaDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute delta between two JSON values
fn compute_delta(prev: &serde_json::Value, current: &serde_json::Value) -> DeltaOp {
    use serde_json::Value;

    if prev == current {
        return DeltaOp::Unchanged;
    }

    match (prev, current) {
        (Value::Object(prev_obj), Value::Object(curr_obj)) => {
            let mut ops = Vec::new();
            let mut prev_keys: std::collections::HashSet<_> = prev_obj.keys().collect();

            // Check current fields
            for (key, curr_val) in curr_obj {
                match prev_obj.get(key) {
                    None => {
                        ops.push(ObjectOp::Add(key.clone(), curr_val.clone()));
                    }
                    Some(prev_val) => {
                        prev_keys.remove(key);
                        let field_delta = compute_delta(prev_val, curr_val);
                        match field_delta {
                            DeltaOp::Unchanged => ops.push(ObjectOp::Keep(key.clone())),
                            _ => ops.push(ObjectOp::Modify(key.clone(), Box::new(field_delta))),
                        }
                    }
                }
            }

            // Check removed fields
            for key in prev_keys {
                ops.push(ObjectOp::Remove(key.clone()));
            }

            DeltaOp::ObjectOps(ops)
        }

        (Value::Array(prev_arr), Value::Array(curr_arr)) => {
            // Simple array delta - could use LCS for better compression
            let mut ops = Vec::new();
            let mut i = 0;
            let mut j = 0;

            while i < prev_arr.len() && j < curr_arr.len() {
                if prev_arr[i] == curr_arr[j] {
                    // Count consecutive keeps
                    let mut keep_count = 1;
                    i += 1;
                    j += 1;
                    while i < prev_arr.len() && j < curr_arr.len() && prev_arr[i] == curr_arr[j] {
                        keep_count += 1;
                        i += 1;
                        j += 1;
                    }
                    ops.push(ArrayOp::Keep(keep_count));
                } else {
                    // Replace element
                    ops.push(ArrayOp::Replace(curr_arr[j].clone()));
                    i += 1;
                    j += 1;
                }
            }

            // Handle remaining elements
            if i < prev_arr.len() {
                ops.push(ArrayOp::Delete(prev_arr.len() - i));
            }
            if j < curr_arr.len() {
                ops.push(ArrayOp::Insert(curr_arr[j..].to_vec()));
            }

            DeltaOp::ArrayOps(ops)
        }

        _ => DeltaOp::Modify(current.clone()),
    }
}

/// Apply array operations to reconstruct value
fn apply_array_ops(prev: &serde_json::Value, ops: &[ArrayOp]) -> Result<serde_json::Value> {
    let prev_arr = prev.as_array().ok_or_else(|| {
        Error::DecodeError("Expected array for ArrayOps".into())
    })?;

    let mut result = Vec::new();
    let mut i = 0;

    for op in ops {
        match op {
            ArrayOp::Keep(n) => {
                for _ in 0..*n {
                    if i < prev_arr.len() {
                        result.push(prev_arr[i].clone());
                        i += 1;
                    }
                }
            }
            ArrayOp::Insert(values) => {
                result.extend(values.iter().cloned());
            }
            ArrayOp::Delete(n) => {
                i += n;
            }
            ArrayOp::Replace(v) => {
                result.push(v.clone());
                i += 1;
            }
        }
    }

    Ok(serde_json::Value::Array(result))
}

/// Apply object operations to reconstruct value
fn apply_object_ops(prev: &serde_json::Value, ops: &[ObjectOp]) -> Result<serde_json::Value> {
    let prev_obj = prev.as_object().ok_or_else(|| {
        Error::DecodeError("Expected object for ObjectOps".into())
    })?;

    let mut result = serde_json::Map::new();

    for op in ops {
        match op {
            ObjectOp::Keep(key) => {
                if let Some(v) = prev_obj.get(key) {
                    result.insert(key.clone(), v.clone());
                }
            }
            ObjectOp::Add(key, value) => {
                result.insert(key.clone(), value.clone());
            }
            ObjectOp::Remove(_) => {
                // Don't include in result
            }
            ObjectOp::Modify(key, delta) => {
                if let Some(prev_val) = prev_obj.get(key) {
                    let new_val = apply_delta(prev_val, delta)?;
                    result.insert(key.clone(), new_val);
                }
            }
        }
    }

    Ok(serde_json::Value::Object(result))
}

/// Apply a delta to reconstruct a value
fn apply_delta(prev: &serde_json::Value, delta: &DeltaOp) -> Result<serde_json::Value> {
    match delta {
        DeltaOp::Unchanged => Ok(prev.clone()),
        DeltaOp::Add(v) => Ok(v.clone()),
        DeltaOp::Remove => Ok(serde_json::Value::Null),
        DeltaOp::Modify(v) => Ok(v.clone()),
        DeltaOp::ArrayOps(ops) => apply_array_ops(prev, ops),
        DeltaOp::ObjectOps(ops) => apply_object_ops(prev, ops),
    }
}

// Binary delta format tags
const TAG_UNCHANGED: u8 = 0;
const TAG_ADD: u8 = 1;
const TAG_REMOVE: u8 = 2;
const TAG_MODIFY: u8 = 3;
const TAG_ARRAY_OPS: u8 = 4;
const TAG_OBJECT_OPS: u8 = 5;

// Array op tags
const ARRAY_KEEP: u8 = 0;
const ARRAY_INSERT: u8 = 1;
const ARRAY_DELETE: u8 = 2;
const ARRAY_REPLACE: u8 = 3;

// Object op tags
const OBJ_KEEP: u8 = 0;
const OBJ_ADD: u8 = 1;
const OBJ_REMOVE: u8 = 2;
const OBJ_MODIFY: u8 = 3;

/// Serialize delta to compact binary format
pub fn serialize_delta(delta: &DeltaOp) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    encode_delta(delta, &mut buf)?;
    Ok(buf)
}

/// Deserialize delta from binary format
pub fn deserialize_delta(data: &[u8]) -> Result<DeltaOp> {
    let mut pos = 0;
    decode_delta(data, &mut pos)
}

fn encode_delta(delta: &DeltaOp, buf: &mut Vec<u8>) -> Result<()> {
    match delta {
        DeltaOp::Unchanged => {
            buf.push(TAG_UNCHANGED);
        }
        DeltaOp::Add(value) => {
            buf.push(TAG_ADD);
            encode_json_value(value, buf)?;
        }
        DeltaOp::Remove => {
            buf.push(TAG_REMOVE);
        }
        DeltaOp::Modify(value) => {
            buf.push(TAG_MODIFY);
            encode_json_value(value, buf)?;
        }
        DeltaOp::ArrayOps(ops) => {
            buf.push(TAG_ARRAY_OPS);
            encode_varint(ops.len() as u64, buf);
            for op in ops {
                encode_array_op(op, buf)?;
            }
        }
        DeltaOp::ObjectOps(ops) => {
            buf.push(TAG_OBJECT_OPS);
            encode_varint(ops.len() as u64, buf);
            for op in ops {
                encode_object_op(op, buf)?;
            }
        }
    }
    Ok(())
}

fn decode_delta(data: &[u8], pos: &mut usize) -> Result<DeltaOp> {
    if *pos >= data.len() {
        return Err(Error::DecodeError("Unexpected end of delta data".into()));
    }

    let tag = data[*pos];
    *pos += 1;

    match tag {
        TAG_UNCHANGED => Ok(DeltaOp::Unchanged),
        TAG_ADD => {
            let value = decode_json_value(data, pos)?;
            Ok(DeltaOp::Add(value))
        }
        TAG_REMOVE => Ok(DeltaOp::Remove),
        TAG_MODIFY => {
            let value = decode_json_value(data, pos)?;
            Ok(DeltaOp::Modify(value))
        }
        TAG_ARRAY_OPS => {
            let count = decode_varint(data, pos)? as usize;
            let mut ops = Vec::with_capacity(count);
            for _ in 0..count {
                ops.push(decode_array_op(data, pos)?);
            }
            Ok(DeltaOp::ArrayOps(ops))
        }
        TAG_OBJECT_OPS => {
            let count = decode_varint(data, pos)? as usize;
            let mut ops = Vec::with_capacity(count);
            for _ in 0..count {
                ops.push(decode_object_op(data, pos)?);
            }
            Ok(DeltaOp::ObjectOps(ops))
        }
        _ => Err(Error::DecodeError(format!("Unknown delta tag: {}", tag))),
    }
}

fn encode_array_op(op: &ArrayOp, buf: &mut Vec<u8>) -> Result<()> {
    match op {
        ArrayOp::Keep(n) => {
            buf.push(ARRAY_KEEP);
            encode_varint(*n as u64, buf);
        }
        ArrayOp::Insert(values) => {
            buf.push(ARRAY_INSERT);
            encode_varint(values.len() as u64, buf);
            for v in values {
                encode_json_value(v, buf)?;
            }
        }
        ArrayOp::Delete(n) => {
            buf.push(ARRAY_DELETE);
            encode_varint(*n as u64, buf);
        }
        ArrayOp::Replace(value) => {
            buf.push(ARRAY_REPLACE);
            encode_json_value(value, buf)?;
        }
    }
    Ok(())
}

fn decode_array_op(data: &[u8], pos: &mut usize) -> Result<ArrayOp> {
    if *pos >= data.len() {
        return Err(Error::DecodeError("Unexpected end of array op".into()));
    }

    let tag = data[*pos];
    *pos += 1;

    match tag {
        ARRAY_KEEP => {
            let n = decode_varint(data, pos)? as usize;
            Ok(ArrayOp::Keep(n))
        }
        ARRAY_INSERT => {
            let count = decode_varint(data, pos)? as usize;
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(decode_json_value(data, pos)?);
            }
            Ok(ArrayOp::Insert(values))
        }
        ARRAY_DELETE => {
            let n = decode_varint(data, pos)? as usize;
            Ok(ArrayOp::Delete(n))
        }
        ARRAY_REPLACE => {
            let value = decode_json_value(data, pos)?;
            Ok(ArrayOp::Replace(value))
        }
        _ => Err(Error::DecodeError(format!("Unknown array op tag: {}", tag))),
    }
}

fn encode_object_op(op: &ObjectOp, buf: &mut Vec<u8>) -> Result<()> {
    match op {
        ObjectOp::Keep(key) => {
            buf.push(OBJ_KEEP);
            encode_string(key, buf);
        }
        ObjectOp::Add(key, value) => {
            buf.push(OBJ_ADD);
            encode_string(key, buf);
            encode_json_value(value, buf)?;
        }
        ObjectOp::Remove(key) => {
            buf.push(OBJ_REMOVE);
            encode_string(key, buf);
        }
        ObjectOp::Modify(key, delta) => {
            buf.push(OBJ_MODIFY);
            encode_string(key, buf);
            encode_delta(delta, buf)?;
        }
    }
    Ok(())
}

fn decode_object_op(data: &[u8], pos: &mut usize) -> Result<ObjectOp> {
    if *pos >= data.len() {
        return Err(Error::DecodeError("Unexpected end of object op".into()));
    }

    let tag = data[*pos];
    *pos += 1;

    match tag {
        OBJ_KEEP => {
            let key = decode_string(data, pos)?;
            Ok(ObjectOp::Keep(key))
        }
        OBJ_ADD => {
            let key = decode_string(data, pos)?;
            let value = decode_json_value(data, pos)?;
            Ok(ObjectOp::Add(key, value))
        }
        OBJ_REMOVE => {
            let key = decode_string(data, pos)?;
            Ok(ObjectOp::Remove(key))
        }
        OBJ_MODIFY => {
            let key = decode_string(data, pos)?;
            let delta = decode_delta(data, pos)?;
            Ok(ObjectOp::Modify(key, Box::new(delta)))
        }
        _ => Err(Error::DecodeError(format!("Unknown object op tag: {}", tag))),
    }
}

// JSON value encoding (compact binary)
const JSON_NULL: u8 = 0;
const JSON_BOOL_FALSE: u8 = 1;
const JSON_BOOL_TRUE: u8 = 2;
const JSON_NUMBER_INT: u8 = 3;
const JSON_NUMBER_FLOAT: u8 = 4;
const JSON_STRING: u8 = 5;
const JSON_ARRAY: u8 = 6;
const JSON_OBJECT: u8 = 7;

fn encode_json_value(value: &serde_json::Value, buf: &mut Vec<u8>) -> Result<()> {
    use serde_json::Value;
    match value {
        Value::Null => buf.push(JSON_NULL),
        Value::Bool(false) => buf.push(JSON_BOOL_FALSE),
        Value::Bool(true) => buf.push(JSON_BOOL_TRUE),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                buf.push(JSON_NUMBER_INT);
                encode_signed_varint(i, buf);
            } else if let Some(f) = n.as_f64() {
                buf.push(JSON_NUMBER_FLOAT);
                buf.extend_from_slice(&f.to_le_bytes());
            } else {
                return Err(Error::EncodeError("Unsupported number type".into()));
            }
        }
        Value::String(s) => {
            buf.push(JSON_STRING);
            encode_string(s, buf);
        }
        Value::Array(arr) => {
            buf.push(JSON_ARRAY);
            encode_varint(arr.len() as u64, buf);
            for item in arr {
                encode_json_value(item, buf)?;
            }
        }
        Value::Object(obj) => {
            buf.push(JSON_OBJECT);
            encode_varint(obj.len() as u64, buf);
            for (k, v) in obj {
                encode_string(k, buf);
                encode_json_value(v, buf)?;
            }
        }
    }
    Ok(())
}

fn decode_json_value(data: &[u8], pos: &mut usize) -> Result<serde_json::Value> {
    use serde_json::Value;

    if *pos >= data.len() {
        return Err(Error::DecodeError("Unexpected end of JSON value".into()));
    }

    let tag = data[*pos];
    *pos += 1;

    match tag {
        JSON_NULL => Ok(Value::Null),
        JSON_BOOL_FALSE => Ok(Value::Bool(false)),
        JSON_BOOL_TRUE => Ok(Value::Bool(true)),
        JSON_NUMBER_INT => {
            let i = decode_signed_varint(data, pos)?;
            Ok(Value::Number(i.into()))
        }
        JSON_NUMBER_FLOAT => {
            if *pos + 8 > data.len() {
                return Err(Error::DecodeError("Truncated float".into()));
            }
            let f = f64::from_le_bytes([
                data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3],
                data[*pos + 4], data[*pos + 5], data[*pos + 6], data[*pos + 7],
            ]);
            *pos += 8;
            Ok(serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null))
        }
        JSON_STRING => {
            let s = decode_string(data, pos)?;
            Ok(Value::String(s))
        }
        JSON_ARRAY => {
            let count = decode_varint(data, pos)? as usize;
            let mut arr = Vec::with_capacity(count);
            for _ in 0..count {
                arr.push(decode_json_value(data, pos)?);
            }
            Ok(Value::Array(arr))
        }
        JSON_OBJECT => {
            let count = decode_varint(data, pos)? as usize;
            let mut obj = serde_json::Map::with_capacity(count);
            for _ in 0..count {
                let k = decode_string(data, pos)?;
                let v = decode_json_value(data, pos)?;
                obj.insert(k, v);
            }
            Ok(Value::Object(obj))
        }
        _ => Err(Error::DecodeError(format!("Unknown JSON tag: {}", tag))),
    }
}

fn encode_string(s: &str, buf: &mut Vec<u8>) {
    encode_varint(s.len() as u64, buf);
    buf.extend_from_slice(s.as_bytes());
}

fn decode_string(data: &[u8], pos: &mut usize) -> Result<String> {
    let len = decode_varint(data, pos)? as usize;
    if *pos + len > data.len() {
        return Err(Error::DecodeError("Truncated string".into()));
    }
    let s = String::from_utf8(data[*pos..*pos + len].to_vec())
        .map_err(|_| Error::DecodeError("Invalid UTF-8".into()))?;
    *pos += len;
    Ok(s)
}

fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value >= 0x80 {
        buf.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

fn decode_varint(data: &[u8], pos: &mut usize) -> Result<u64> {
    let mut result: u64 = 0;
    let mut shift = 0;

    loop {
        if *pos >= data.len() {
            return Err(Error::DecodeError("Varint truncated".into()));
        }
        let byte = data[*pos];
        *pos += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 63 {
            return Err(Error::DecodeError("Varint too long".into()));
        }
    }
    Ok(result)
}

fn encode_signed_varint(value: i64, buf: &mut Vec<u8>) {
    // Zigzag encoding
    let encoded = ((value << 1) ^ (value >> 63)) as u64;
    encode_varint(encoded, buf);
}

fn decode_signed_varint(data: &[u8], pos: &mut usize) -> Result<i64> {
    let encoded = decode_varint(data, pos)?;
    // Zigzag decoding
    Ok(((encoded >> 1) as i64) ^ (-((encoded & 1) as i64)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_unchanged() {
        let v1 = json!({"a": 1, "b": 2});
        let v2 = json!({"a": 1, "b": 2});

        let delta = compute_delta(&v1, &v2);
        assert_eq!(delta, DeltaOp::Unchanged);
    }

    #[test]
    fn test_object_modify() {
        let v1 = json!({"a": 1, "b": 2});
        let v2 = json!({"a": 1, "b": 3});

        let delta = compute_delta(&v1, &v2);

        match delta {
            DeltaOp::ObjectOps(ops) => {
                assert!(ops.iter().any(|op| matches!(op, ObjectOp::Keep(k) if k == "a")));
                assert!(ops.iter().any(|op| matches!(op, ObjectOp::Modify(k, _) if k == "b")));
            }
            _ => panic!("Expected ObjectOps"),
        }
    }

    #[test]
    fn test_object_add_remove() {
        let v1 = json!({"a": 1, "b": 2});
        let v2 = json!({"a": 1, "c": 3});

        let delta = compute_delta(&v1, &v2);

        match delta {
            DeltaOp::ObjectOps(ops) => {
                assert!(ops.iter().any(|op| matches!(op, ObjectOp::Remove(k) if k == "b")));
                assert!(ops.iter().any(|op| matches!(op, ObjectOp::Add(k, _) if k == "c")));
            }
            _ => panic!("Expected ObjectOps"),
        }
    }

    #[test]
    fn test_encoder_decoder_roundtrip() {
        let mut encoder = DeltaEncoder::new();
        let mut decoder = DeltaDecoder::new();

        let states = vec![
            json!({"count": 0, "name": "test"}),
            json!({"count": 1, "name": "test"}),
            json!({"count": 2, "name": "test", "new_field": true}),
            json!({"count": 3, "name": "updated"}),
        ];

        for state in &states {
            let delta = encoder.encode(state).unwrap();
            let decoded = decoder.decode(&delta).unwrap();
            assert_eq!(&decoded, state);
        }
    }

    #[test]
    fn test_array_delta() {
        let v1 = json!([1, 2, 3, 4, 5]);
        let v2 = json!([1, 2, 99, 4, 5, 6]);

        let delta = compute_delta(&v1, &v2);

        match delta {
            DeltaOp::ArrayOps(_) => {}
            _ => panic!("Expected ArrayOps"),
        }
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let v1 = json!({"count": 0, "items": [1, 2, 3]});
        let v2 = json!({"count": 5, "items": [1, 2, 3, 4], "new": true});

        let delta = compute_delta(&v1, &v2);

        let serialized = serialize_delta(&delta).unwrap();
        let deserialized = deserialize_delta(&serialized).unwrap();

        assert_eq!(delta, deserialized);

        // Verify applying the delta produces correct result
        let reconstructed = apply_delta(&v1, &deserialized).unwrap();
        assert_eq!(reconstructed, v2);
    }

    #[test]
    fn test_delta_size_savings() {
        // Large object with small change
        let v1 = json!({
            "users": [
                {"id": 1, "name": "Alice", "email": "alice@example.com"},
                {"id": 2, "name": "Bob", "email": "bob@example.com"},
                {"id": 3, "name": "Charlie", "email": "charlie@example.com"}
            ],
            "total": 3,
            "page": 1
        });

        let v2 = json!({
            "users": [
                {"id": 1, "name": "Alice", "email": "alice@example.com"},
                {"id": 2, "name": "Bob", "email": "bob@example.com"},
                {"id": 3, "name": "Charlie", "email": "charlie@example.com"}
            ],
            "total": 3,
            "page": 2  // Only this changed
        });

        let full_json = serde_json::to_vec(&v2).unwrap();
        let delta = compute_delta(&v1, &v2);
        let delta_bytes = serialize_delta(&delta).unwrap();

        // Delta should be much smaller than full JSON
        assert!(delta_bytes.len() < full_json.len());
    }
}

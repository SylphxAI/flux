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

/// Serialize delta to bytes
pub fn serialize_delta(delta: &DeltaOp) -> Result<Vec<u8>> {
    serde_json::to_vec(delta).map_err(|e| Error::EncodeError(e.to_string()))
}

/// Deserialize delta from bytes
pub fn deserialize_delta(data: &[u8]) -> Result<DeltaOp> {
    serde_json::from_slice(data).map_err(|e| Error::DecodeError(e.to_string()))
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

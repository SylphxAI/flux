//! FLUX core types

use std::collections::HashMap;

/// Type ID constants
pub mod type_id {
    pub const NULL: u8 = 0x00;
    pub const BOOLEAN: u8 = 0x01;
    pub const INT8: u8 = 0x02;
    pub const INT16: u8 = 0x03;
    pub const INT32: u8 = 0x04;
    pub const INT64: u8 = 0x05;
    pub const VARINT: u8 = 0x06;
    pub const FLOAT32: u8 = 0x07;
    pub const FLOAT64: u8 = 0x08;
    pub const STRING: u8 = 0x09;
    pub const BINARY: u8 = 0x0A;
    pub const ARRAY: u8 = 0x0B;
    pub const OBJECT: u8 = 0x0C;
    pub const UNION: u8 = 0x0D;
    pub const TIMESTAMP: u8 = 0x10;
    pub const UUID: u8 = 0x11;
    pub const DECIMAL: u8 = 0x12;
}

/// Field type enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Null,
    Boolean,
    Integer(IntegerType),
    Float(FloatType),
    String,
    Binary,
    Array(Box<FieldType>),
    Object(Vec<(String, FieldType)>),
    Union(Vec<FieldType>),
    Timestamp,
    Uuid,
    Decimal { precision: u8, scale: u8 },
}

/// Integer type variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerType {
    Int8,
    Int16,
    Int32,
    Int64,
    Varint,
}

/// Float type variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatType {
    Float32,
    Float64,
}

impl FieldType {
    /// Get type ID for serialization
    pub fn type_id(&self) -> u8 {
        match self {
            FieldType::Null => type_id::NULL,
            FieldType::Boolean => type_id::BOOLEAN,
            FieldType::Integer(IntegerType::Int8) => type_id::INT8,
            FieldType::Integer(IntegerType::Int16) => type_id::INT16,
            FieldType::Integer(IntegerType::Int32) => type_id::INT32,
            FieldType::Integer(IntegerType::Int64) => type_id::INT64,
            FieldType::Integer(IntegerType::Varint) => type_id::VARINT,
            FieldType::Float(FloatType::Float32) => type_id::FLOAT32,
            FieldType::Float(FloatType::Float64) => type_id::FLOAT64,
            FieldType::String => type_id::STRING,
            FieldType::Binary => type_id::BINARY,
            FieldType::Array(_) => type_id::ARRAY,
            FieldType::Object(_) => type_id::OBJECT,
            FieldType::Union(_) => type_id::UNION,
            FieldType::Timestamp => type_id::TIMESTAMP,
            FieldType::Uuid => type_id::UUID,
            FieldType::Decimal { .. } => type_id::DECIMAL,
        }
    }

    /// Check if this type can be null
    pub fn is_nullable(&self) -> bool {
        matches!(self, FieldType::Union(types) if types.contains(&FieldType::Null))
    }

    /// Infer type from JSON value
    pub fn infer(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => FieldType::Null,
            serde_json::Value::Bool(_) => FieldType::Boolean,
            serde_json::Value::Number(n) => {
                if n.is_i64() {
                    let v = n.as_i64().unwrap();
                    if v >= i8::MIN as i64 && v <= i8::MAX as i64 {
                        FieldType::Integer(IntegerType::Int8)
                    } else if v >= i16::MIN as i64 && v <= i16::MAX as i64 {
                        FieldType::Integer(IntegerType::Int16)
                    } else if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
                        FieldType::Integer(IntegerType::Int32)
                    } else {
                        FieldType::Integer(IntegerType::Int64)
                    }
                } else {
                    FieldType::Float(FloatType::Float64)
                }
            }
            serde_json::Value::String(_) => FieldType::String,
            serde_json::Value::Array(arr) => {
                if arr.is_empty() {
                    FieldType::Array(Box::new(FieldType::Null))
                } else {
                    // Use first element's type
                    let elem_type = FieldType::infer(&arr[0]);
                    FieldType::Array(Box::new(elem_type))
                }
            }
            serde_json::Value::Object(obj) => {
                let fields: Vec<(String, FieldType)> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), FieldType::infer(v)))
                    .collect();
                FieldType::Object(fields)
            }
        }
    }

    /// Merge two types (for schema inference across samples)
    pub fn merge(&self, other: &FieldType) -> FieldType {
        if self == other {
            return self.clone();
        }

        // If one is null, make the other nullable
        match (self, other) {
            (FieldType::Null, t) | (t, FieldType::Null) => {
                if let FieldType::Union(types) = t {
                    let mut new_types = types.clone();
                    if !new_types.contains(&FieldType::Null) {
                        new_types.push(FieldType::Null);
                    }
                    FieldType::Union(new_types)
                } else {
                    FieldType::Union(vec![t.clone(), FieldType::Null])
                }
            }

            // Widen integers
            (FieldType::Integer(a), FieldType::Integer(b)) => {
                use IntegerType::*;
                let wider = match (a, b) {
                    (Int64, _) | (_, Int64) => Int64,
                    (Int32, _) | (_, Int32) => Int32,
                    (Int16, _) | (_, Int16) => Int16,
                    _ => Int8,
                };
                FieldType::Integer(wider)
            }

            // Integer + Float = Float
            (FieldType::Integer(_), FieldType::Float(f))
            | (FieldType::Float(f), FieldType::Integer(_)) => FieldType::Float(*f),

            // Arrays: merge element types
            (FieldType::Array(a), FieldType::Array(b)) => {
                FieldType::Array(Box::new(a.merge(b)))
            }

            // Objects: merge fields
            (FieldType::Object(a), FieldType::Object(b)) => {
                let mut merged: HashMap<String, FieldType> = HashMap::new();

                for (name, typ) in a {
                    merged.insert(name.clone(), typ.clone());
                }

                for (name, typ) in b {
                    merged
                        .entry(name.clone())
                        .and_modify(|existing| *existing = existing.merge(typ))
                        .or_insert_with(|| {
                            // New field, might be nullable
                            FieldType::Union(vec![typ.clone(), FieldType::Null])
                        });
                }

                // Check if any field from 'a' is missing in 'b'
                for (name, _) in a {
                    if !b.iter().any(|(n, _)| n == name) {
                        merged.entry(name.clone()).and_modify(|t| {
                            if !t.is_nullable() {
                                *t = FieldType::Union(vec![t.clone(), FieldType::Null]);
                            }
                        });
                    }
                }

                let fields: Vec<_> = merged.into_iter().collect();
                FieldType::Object(fields)
            }

            // Different types: create union
            _ => FieldType::Union(vec![self.clone(), other.clone()]),
        }
    }
}

/// Runtime value representation
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Binary(Vec<u8>),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// Convert from serde_json::Value
    pub fn from_json(json: &serde_json::Value) -> Self {
        match json {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Boolean(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Null
                }
            }
            serde_json::Value::String(s) => Value::String(s.clone()),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.iter().map(Value::from_json).collect())
            }
            serde_json::Value::Object(obj) => {
                Value::Object(
                    obj.iter()
                        .map(|(k, v)| (k.clone(), Value::from_json(v)))
                        .collect(),
                )
            }
        }
    }

    /// Convert to serde_json::Value
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Boolean(b) => serde_json::Value::Bool(*b),
            Value::Integer(i) => serde_json::Value::Number((*i).into()),
            Value::Float(f) => {
                serde_json::Number::from_f64(*f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Binary(b) => {
                // Encode as base64 string
                use std::io::Write;
                let mut buf = Vec::new();
                write!(&mut buf, "{:?}", b).ok();
                serde_json::Value::String(String::from_utf8_lossy(&buf).into_owned())
            }
            Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(Value::to_json).collect())
            }
            Value::Object(obj) => {
                let map: serde_json::Map<String, serde_json::Value> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect();
                serde_json::Value::Object(map)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_type_infer() {
        let json: serde_json::Value = serde_json::json!({
            "id": 123,
            "name": "test",
            "active": true,
            "score": 95.5,
            "tags": ["a", "b"]
        });

        let ft = FieldType::infer(&json);

        match ft {
            FieldType::Object(fields) => {
                assert_eq!(fields.len(), 5);
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_field_type_merge() {
        let t1 = FieldType::Integer(IntegerType::Int8);
        let t2 = FieldType::Integer(IntegerType::Int32);
        let merged = t1.merge(&t2);
        assert_eq!(merged, FieldType::Integer(IntegerType::Int32));

        let t1 = FieldType::String;
        let t2 = FieldType::Null;
        let merged = t1.merge(&t2);
        assert!(merged.is_nullable());
    }

    #[test]
    fn test_value_roundtrip() {
        let json: serde_json::Value = serde_json::json!({
            "id": 123,
            "name": "test",
            "nested": {"a": 1}
        });

        let value = Value::from_json(&json);
        let back = value.to_json();

        assert_eq!(json, back);
    }
}

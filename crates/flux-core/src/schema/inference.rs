//! Schema inference from JSON values

use crate::{Error, Result};
use crate::types::FieldType;
use super::{Schema, FieldDef};

/// Schema inference engine
pub struct SchemaInferrer {
    current_schema: Option<Schema>,
    sample_count: usize,
    config: InferenceConfig,
}

/// Inference configuration
#[derive(Debug, Clone)]
pub struct InferenceConfig {
    pub max_samples: usize,
    pub detect_timestamps: bool,
    pub detect_uuids: bool,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            max_samples: 100,
            detect_timestamps: true,
            detect_uuids: true,
        }
    }
}

impl SchemaInferrer {
    /// Create a new schema inferrer
    pub fn new() -> Self {
        Self::with_config(InferenceConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: InferenceConfig) -> Self {
        Self {
            current_schema: None,
            sample_count: 0,
            config,
        }
    }

    /// Add a JSON value sample
    pub fn add_value(&mut self, value: &serde_json::Value) -> Result<()> {
        if self.sample_count >= self.config.max_samples {
            return Ok(()); // Enough samples
        }

        let inferred = self.infer_from_value(value)?;

        match &mut self.current_schema {
            None => {
                self.current_schema = Some(inferred);
            }
            Some(existing) => {
                // Merge with existing schema
                Self::merge_schemas(existing, &inferred);
            }
        }

        self.sample_count += 1;
        Ok(())
    }

    /// Get the inferred schema
    pub fn infer(&self) -> Result<Schema> {
        self.current_schema
            .clone()
            .ok_or_else(|| Error::ParseError("No samples provided".into()))
    }

    /// Infer schema from a single value
    fn infer_from_value(&self, value: &serde_json::Value) -> Result<Schema> {
        match value {
            serde_json::Value::Object(obj) => {
                let fields: Vec<FieldDef> = obj
                    .iter()
                    .map(|(key, val)| {
                        let field_type = self.infer_type(val);
                        FieldDef {
                            name: key.clone(),
                            field_type,
                            nullable: false, // Will be updated during merging
                        }
                    })
                    .collect();

                Ok(Schema::new(fields))
            }
            serde_json::Value::Array(arr) if !arr.is_empty() => {
                // For array of objects, use first element
                if let Some(serde_json::Value::Object(_)) = arr.first() {
                    self.infer_from_value(&arr[0])
                } else {
                    Err(Error::ParseError("Array of primitives not supported as root".into()))
                }
            }
            _ => Err(Error::ParseError("Root must be object or array of objects".into())),
        }
    }

    /// Infer type from a value
    fn infer_type(&self, value: &serde_json::Value) -> FieldType {
        let base_type = FieldType::infer(value);

        // Enhanced detection
        if self.config.detect_timestamps {
            if let serde_json::Value::String(s) = value {
                if Self::looks_like_timestamp(s) {
                    return FieldType::Timestamp;
                }
            }
        }

        if self.config.detect_uuids {
            if let serde_json::Value::String(s) = value {
                if Self::looks_like_uuid(s) {
                    return FieldType::Uuid;
                }
            }
        }

        base_type
    }

    /// Check if string looks like a timestamp
    fn looks_like_timestamp(s: &str) -> bool {
        // ISO 8601 format
        if s.len() >= 10 && s.len() <= 30 {
            let chars: Vec<char> = s.chars().collect();
            if chars.len() >= 10
                && chars[4] == '-'
                && chars[7] == '-'
                && chars[0].is_ascii_digit()
            {
                return true;
            }
        }
        false
    }

    /// Check if string looks like a UUID
    fn looks_like_uuid(s: &str) -> bool {
        if s.len() == 36 {
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() == 5
                && parts[0].len() == 8
                && parts[1].len() == 4
                && parts[2].len() == 4
                && parts[3].len() == 4
                && parts[4].len() == 12
            {
                return parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit()));
            }
        }
        false
    }

    /// Merge two schemas
    fn merge_schemas(existing: &mut Schema, new: &Schema) {
        // Track which fields exist in new schema
        let _new_fields: std::collections::HashSet<&str> =
            new.fields.iter().map(|f| f.name.as_str()).collect();

        // Update existing fields
        for field in &mut existing.fields {
            if let Some(new_field) = new.fields.iter().find(|f| f.name == field.name) {
                // Merge types
                field.field_type = field.field_type.merge(&new_field.field_type);
            } else {
                // Field missing in new schema - make nullable
                field.nullable = true;
            }
        }

        // Add new fields
        for new_field in &new.fields {
            if !existing.fields.iter().any(|f| f.name == new_field.name) {
                let mut field = new_field.clone();
                field.nullable = true; // New field might not exist in all records
                existing.fields.push(field);
            }
        }

        // Recompute hash
        existing.hash = Schema::compute_hash(&existing.fields);
    }
}

impl Default for SchemaInferrer {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_simple() {
        let mut inferrer = SchemaInferrer::new();

        inferrer
            .add_value(&serde_json::json!({
                "id": 123,
                "name": "test"
            }))
            .unwrap();

        let schema = inferrer.infer().unwrap();
        assert_eq!(schema.fields.len(), 2);
    }

    #[test]
    fn test_infer_with_merge() {
        let mut inferrer = SchemaInferrer::new();

        inferrer
            .add_value(&serde_json::json!({"id": 1, "name": "alice"}))
            .unwrap();

        inferrer
            .add_value(&serde_json::json!({"id": 2, "name": "bob", "email": "bob@test.com"}))
            .unwrap();

        let schema = inferrer.infer().unwrap();

        // Should have 3 fields, email should be nullable
        assert_eq!(schema.fields.len(), 3);

        let email_field = schema.fields.iter().find(|f| f.name == "email").unwrap();
        assert!(email_field.nullable);
    }

    #[test]
    fn test_detect_timestamp() {
        assert!(SchemaInferrer::looks_like_timestamp("2024-01-15T10:30:00Z"));
        assert!(SchemaInferrer::looks_like_timestamp("2024-01-15"));
        assert!(!SchemaInferrer::looks_like_timestamp("hello world"));
    }

    #[test]
    fn test_detect_uuid() {
        assert!(SchemaInferrer::looks_like_uuid(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
        assert!(!SchemaInferrer::looks_like_uuid("not-a-uuid"));
    }
}

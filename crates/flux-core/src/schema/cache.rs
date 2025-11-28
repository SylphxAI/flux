//! Schema cache for efficient schema lookup

use std::collections::HashMap;
use super::Schema;

/// Schema cache with ID and hash-based lookup
pub struct SchemaCache {
    schemas: HashMap<u32, Schema>,
    hash_index: HashMap<u64, u32>,
    next_id: u32,
}

impl SchemaCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
            hash_index: HashMap::new(),
            next_id: 1,
        }
    }

    /// Get schema by ID
    pub fn get(&self, id: u32) -> Option<&Schema> {
        self.schemas.get(&id)
    }

    /// Get schema by hash
    pub fn get_by_hash(&self, hash: u64) -> Option<&Schema> {
        self.hash_index
            .get(&hash)
            .and_then(|id| self.schemas.get(id))
    }

    /// Register a new schema, returns assigned ID
    pub fn register(&mut self, mut schema: Schema) -> u32 {
        // Check if already exists
        if let Some(&existing_id) = self.hash_index.get(&schema.hash) {
            return existing_id;
        }

        // Assign new ID
        let id = self.next_id;
        self.next_id += 1;

        schema.id = id;
        self.hash_index.insert(schema.hash, id);
        self.schemas.insert(id, schema);

        id
    }

    /// Number of cached schemas
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    /// Clear all cached schemas
    pub fn clear(&mut self) {
        self.schemas.clear();
        self.hash_index.clear();
        self.next_id = 1;
    }

    /// Serialize entire cache
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Schema count
        buf.extend_from_slice(&(self.schemas.len() as u32).to_le_bytes());

        // Each schema
        for schema in self.schemas.values() {
            let schema_bytes = schema.serialize();
            buf.extend_from_slice(&(schema_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(&schema_bytes);
        }

        buf
    }

    /// Deserialize cache
    pub fn deserialize(buf: &[u8]) -> crate::Result<Self> {
        let mut cache = Self::new();

        if buf.len() < 4 {
            return Ok(cache);
        }

        let count = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let mut pos = 4;

        for _ in 0..count {
            if pos + 4 > buf.len() {
                break;
            }

            let schema_len =
                u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]) as usize;
            pos += 4;

            if pos + schema_len > buf.len() {
                break;
            }

            if let Ok(schema) = Schema::deserialize(&buf[pos..pos + schema_len]) {
                cache.register(schema);
            }
            pos += schema_len;
        }

        Ok(cache)
    }
}

impl Default for SchemaCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::FieldDef;
    use crate::types::FieldType;

    #[test]
    fn test_cache_register_and_lookup() {
        let mut cache = SchemaCache::new();

        let schema = Schema::new(vec![FieldDef {
            name: "id".into(),
            field_type: FieldType::Integer(crate::types::IntegerType::Int32),
            nullable: false,
        }]);

        let hash = schema.hash;
        let id = cache.register(schema);

        assert_eq!(id, 1);
        assert!(cache.get(id).is_some());
        assert!(cache.get_by_hash(hash).is_some());
    }

    #[test]
    fn test_cache_dedup() {
        let mut cache = SchemaCache::new();

        let schema1 = Schema::new(vec![FieldDef {
            name: "id".into(),
            field_type: FieldType::Integer(crate::types::IntegerType::Int32),
            nullable: false,
        }]);

        let schema2 = Schema::new(vec![FieldDef {
            name: "id".into(),
            field_type: FieldType::Integer(crate::types::IntegerType::Int32),
            nullable: false,
        }]);

        let id1 = cache.register(schema1);
        let id2 = cache.register(schema2);

        // Same schema should get same ID
        assert_eq!(id1, id2);
        assert_eq!(cache.len(), 1);
    }
}

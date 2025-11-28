//! Columnar transformation for arrays of objects
//!
//! Transforms row-oriented data to column-oriented for better compression.

use crate::{Error, Result};
use crate::schema::Schema;

/// Columnar block representation
pub struct ColumnarBlock {
    pub row_count: usize,
    pub columns: Vec<Column>,
}

/// Single column of data
pub struct Column {
    pub name: String,
    pub encoding: ColumnEncoding,
    pub null_bitmap: Option<bitvec::vec::BitVec>,
    pub data: Vec<u8>,
}

/// Column encoding type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnEncoding {
    Raw,
    Varint,
    Delta,
    Dictionary,
    RunLength,
    BitPacked(u8),
}

impl ColumnarBlock {
    /// Create empty block
    pub fn new() -> Self {
        Self {
            row_count: 0,
            columns: Vec::new(),
        }
    }

    /// Convert array of objects to columnar format
    pub fn from_array(values: &[serde_json::Value], schema: &Schema) -> Result<Self> {
        if values.is_empty() {
            return Ok(Self::new());
        }

        let row_count = values.len();
        let mut columns = Vec::with_capacity(schema.fields.len());

        for field in &schema.fields {
            let mut column_values = Vec::with_capacity(row_count);
            let mut null_bits = bitvec::vec::BitVec::with_capacity(row_count);

            for value in values {
                if let serde_json::Value::Object(obj) = value {
                    match obj.get(&field.name) {
                        Some(v) if !v.is_null() => {
                            column_values.push(v.clone());
                            null_bits.push(true);
                        }
                        _ => {
                            column_values.push(serde_json::Value::Null);
                            null_bits.push(false);
                        }
                    }
                }
            }

            // Encode column
            let (data, encoding) = encode_column(&column_values, &field.field_type)?;

            let null_bitmap = if null_bits.iter().any(|b| !*b) {
                Some(null_bits)
            } else {
                None
            };

            columns.push(Column {
                name: field.name.clone(),
                encoding,
                null_bitmap,
                data,
            });
        }

        Ok(Self { row_count, columns })
    }

    /// Convert back to array of objects
    pub fn to_array(&self, schema: &Schema) -> Result<Vec<serde_json::Value>> {
        let mut rows = Vec::with_capacity(self.row_count);

        for i in 0..self.row_count {
            let mut obj = serde_json::Map::new();

            for (col_idx, column) in self.columns.iter().enumerate() {
                // Check null bitmap
                if let Some(ref bitmap) = column.null_bitmap {
                    if !bitmap[i] {
                        continue; // Skip null values
                    }
                }

                // Decode value (simplified)
                let field = &schema.fields[col_idx];
                let value = decode_column_value(&column.data, i, column.encoding)?;
                obj.insert(field.name.clone(), value);
            }

            rows.push(serde_json::Value::Object(obj));
        }

        Ok(rows)
    }
}

impl Default for ColumnarBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode a column of values
fn encode_column(
    values: &[serde_json::Value],
    _field_type: &crate::types::FieldType,
) -> Result<(Vec<u8>, ColumnEncoding)> {
    let mut buf = Vec::new();

    // Simple encoding for now - just serialize values
    for value in values {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| Error::EncodeError(e.to_string()))?;
        crate::encoding::encode_varint(bytes.len() as u64, &mut buf);
        buf.extend_from_slice(&bytes);
    }

    Ok((buf, ColumnEncoding::Raw))
}

/// Decode a single value from column
fn decode_column_value(
    _data: &[u8],
    _index: usize,
    _encoding: ColumnEncoding,
) -> Result<serde_json::Value> {
    // Placeholder - full implementation needed
    Ok(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{FieldDef, SchemaInferrer};

    #[test]
    fn test_columnar_conversion() {
        let values: Vec<serde_json::Value> = vec![
            serde_json::json!({"id": 1, "name": "alice"}),
            serde_json::json!({"id": 2, "name": "bob"}),
            serde_json::json!({"id": 3, "name": "charlie"}),
        ];

        let mut inferrer = SchemaInferrer::new();
        for v in &values {
            inferrer.add_value(v).unwrap();
        }
        let schema = inferrer.infer().unwrap();

        let block = ColumnarBlock::from_array(&values, &schema).unwrap();

        assert_eq!(block.row_count, 3);
        assert_eq!(block.columns.len(), 2);
    }
}

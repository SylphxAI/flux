//! Columnar transformation for arrays of objects
//!
//! Transforms row-oriented data to column-oriented for better compression.
//! Key benefits:
//! - Similar values grouped together (better for compression)
//! - Type-specific encodings applied per column
//! - Null bitmaps for sparse data
//! - Run-length encoding for repeated values

use crate::{Error, Result};
use crate::schema::Schema;
use crate::types::FieldType;
use crate::encoding::{encode_varint, decode_varint, zigzag_encode, zigzag_decode};

/// Columnar block representation
pub struct ColumnarBlock {
    pub row_count: usize,
    pub columns: Vec<Column>,
}

/// Single column of data
pub struct Column {
    pub name: String,
    pub field_type: FieldType,
    pub encoding: ColumnEncoding,
    pub null_bitmap: Option<bitvec::vec::BitVec>,
    pub data: Vec<u8>,
}

/// Column encoding type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnEncoding {
    /// Raw type-specific encoding
    Raw,
    /// Variable-length integers
    Varint,
    /// Delta encoding for sequences
    Delta,
    /// Dictionary encoding for repeated strings
    Dictionary,
    /// Run-length encoding for repeated values
    RunLength,
    /// Bit-packed integers (N bits per value)
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

            // Select optimal encoding and encode column
            let (data, encoding) = encode_column_optimized(&column_values, &field.field_type)?;

            let null_bitmap = if null_bits.iter().any(|b| !*b) {
                Some(null_bits)
            } else {
                None
            };

            columns.push(Column {
                name: field.name.clone(),
                field_type: field.field_type.clone(),
                encoding,
                null_bitmap,
                data,
            });
        }

        Ok(Self { row_count, columns })
    }

    /// Convert back to array of objects
    pub fn to_array(&self, schema: &Schema) -> Result<Vec<serde_json::Value>> {
        // First decode all columns
        let decoded_columns: Vec<Vec<serde_json::Value>> = self.columns
            .iter()
            .map(|col| decode_column(&col.data, col.encoding, &col.field_type, self.row_count))
            .collect::<Result<Vec<_>>>()?;

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

                let field = &schema.fields[col_idx];
                let value = decoded_columns[col_idx][i].clone();
                obj.insert(field.name.clone(), value);
            }

            rows.push(serde_json::Value::Object(obj));
        }

        Ok(rows)
    }

    /// Serialize columnar block to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Row count
        encode_varint(self.row_count as u64, &mut buf);

        // Column count
        encode_varint(self.columns.len() as u64, &mut buf);

        // Each column
        for col in &self.columns {
            // Name length + name
            encode_varint(col.name.len() as u64, &mut buf);
            buf.extend_from_slice(col.name.as_bytes());

            // Encoding type
            buf.push(match col.encoding {
                ColumnEncoding::Raw => 0x00,
                ColumnEncoding::Varint => 0x01,
                ColumnEncoding::Delta => 0x02,
                ColumnEncoding::Dictionary => 0x03,
                ColumnEncoding::RunLength => 0x04,
                ColumnEncoding::BitPacked(bits) => 0x10 | (bits & 0x0F),
            });

            // Null bitmap presence
            if let Some(ref bitmap) = col.null_bitmap {
                buf.push(0x01);
                // Encode bitmap as bytes
                let bitmap_bytes: Vec<u8> = bitmap.chunks(8)
                    .map(|chunk| {
                        let mut byte = 0u8;
                        for (i, bit) in chunk.iter().enumerate() {
                            if *bit {
                                byte |= 1 << i;
                            }
                        }
                        byte
                    })
                    .collect();
                encode_varint(bitmap_bytes.len() as u64, &mut buf);
                buf.extend_from_slice(&bitmap_bytes);
            } else {
                buf.push(0x00);
            }

            // Data length + data
            encode_varint(col.data.len() as u64, &mut buf);
            buf.extend_from_slice(&col.data);
        }

        buf
    }

    /// Get total encoded size
    pub fn encoded_size(&self) -> usize {
        self.columns.iter().map(|c| c.data.len()).sum()
    }
}

impl Default for ColumnarBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// Select optimal encoding and encode column
fn encode_column_optimized(
    values: &[serde_json::Value],
    field_type: &FieldType,
) -> Result<(Vec<u8>, ColumnEncoding)> {
    // For integer columns, analyze and pick best encoding
    if let FieldType::Integer(_) = field_type {
        let integers: Vec<i64> = values
            .iter()
            .filter_map(|v| v.as_i64())
            .collect();

        if !integers.is_empty() {
            return encode_integers_optimal(&integers);
        }
    }

    // For strings, check if dictionary encoding helps
    if matches!(field_type, FieldType::String) {
        let strings: Vec<&str> = values
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        if !strings.is_empty() {
            // Check cardinality for dictionary encoding
            let unique: std::collections::HashSet<&str> = strings.iter().copied().collect();
            if unique.len() < strings.len() / 2 {
                return encode_strings_dictionary(&strings);
            }
        }
    }

    // Default: raw type-specific encoding
    encode_column_raw(values, field_type)
}

/// Encode integers with optimal strategy
fn encode_integers_optimal(values: &[i64]) -> Result<(Vec<u8>, ColumnEncoding)> {
    if values.is_empty() {
        return Ok((Vec::new(), ColumnEncoding::Raw));
    }

    // Try delta encoding
    let deltas: Vec<i64> = std::iter::once(values[0])
        .chain(values.windows(2).map(|w| w[1] - w[0]))
        .collect();

    // Calculate costs
    let raw_cost = values.iter().map(|&v| varint_size(zigzag_encode(v))).sum::<usize>();
    let delta_cost = deltas.iter().map(|&d| varint_size(zigzag_encode(d))).sum::<usize>();

    // Check if bit-packing is beneficial
    let min = *values.iter().min().unwrap();
    let max = *values.iter().max().unwrap();
    let range = (max - min) as u64;
    let bits_needed = if range == 0 { 1 } else { 64 - range.leading_zeros() };

    // Choose best encoding
    if delta_cost < raw_cost && delta_cost < (bits_needed as usize * values.len() / 8 + 10) {
        // Delta encoding wins
        let mut buf = Vec::with_capacity(delta_cost + 4);
        encode_varint(values.len() as u64, &mut buf);
        for &d in &deltas {
            encode_varint(zigzag_encode(d), &mut buf);
        }
        Ok((buf, ColumnEncoding::Delta))
    } else if bits_needed <= 8 && values.len() >= 4 {
        // Bit-packing wins
        let mut buf = Vec::new();
        encode_varint(values.len() as u64, &mut buf);
        encode_varint(zigzag_encode(min), &mut buf);
        buf.push(bits_needed as u8);

        // Pack values
        let mut bit_pos = 0u32;
        let mut current_byte = 0u8;

        for &val in values {
            let offset = (val - min) as u64;

            for bit in 0..bits_needed {
                if (offset >> bit) & 1 == 1 {
                    current_byte |= 1 << (bit_pos % 8);
                }
                bit_pos += 1;
                if bit_pos % 8 == 0 {
                    buf.push(current_byte);
                    current_byte = 0;
                }
            }
        }

        if bit_pos % 8 != 0 {
            buf.push(current_byte);
        }

        Ok((buf, ColumnEncoding::BitPacked(bits_needed as u8)))
    } else {
        // Raw varint encoding
        let mut buf = Vec::with_capacity(raw_cost + 4);
        encode_varint(values.len() as u64, &mut buf);
        for &v in values {
            encode_varint(zigzag_encode(v), &mut buf);
        }
        Ok((buf, ColumnEncoding::Varint))
    }
}

/// Encode strings with dictionary
fn encode_strings_dictionary(strings: &[&str]) -> Result<(Vec<u8>, ColumnEncoding)> {
    let mut buf = Vec::new();

    // Build dictionary
    let mut dict: Vec<&str> = Vec::new();
    let mut dict_index: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();

    for &s in strings {
        if !dict_index.contains_key(s) {
            dict_index.insert(s, dict.len() as u32);
            dict.push(s);
        }
    }

    // Write dictionary
    encode_varint(dict.len() as u64, &mut buf);
    for entry in &dict {
        encode_varint(entry.len() as u64, &mut buf);
        buf.extend_from_slice(entry.as_bytes());
    }

    // Write indices
    encode_varint(strings.len() as u64, &mut buf);
    for &s in strings {
        let idx = dict_index[s];
        encode_varint(idx as u64, &mut buf);
    }

    Ok((buf, ColumnEncoding::Dictionary))
}

/// Raw type-specific encoding
fn encode_column_raw(
    values: &[serde_json::Value],
    field_type: &FieldType,
) -> Result<(Vec<u8>, ColumnEncoding)> {
    let mut buf = Vec::new();

    encode_varint(values.len() as u64, &mut buf);

    for value in values {
        match (value, field_type) {
            (serde_json::Value::Null, _) => {
                // Already handled by null bitmap
            }
            (serde_json::Value::Bool(b), FieldType::Boolean) => {
                buf.push(if *b { 1 } else { 0 });
            }
            (serde_json::Value::Number(n), FieldType::Integer(_)) => {
                let i = n.as_i64().unwrap_or(0);
                encode_varint(zigzag_encode(i), &mut buf);
            }
            (serde_json::Value::Number(n), FieldType::Float(_)) => {
                let f = n.as_f64().unwrap_or(0.0);
                buf.extend_from_slice(&f.to_le_bytes());
            }
            (serde_json::Value::String(s), _) => {
                encode_varint(s.len() as u64, &mut buf);
                buf.extend_from_slice(s.as_bytes());
            }
            _ => {
                // Fallback: JSON serialize
                let bytes = serde_json::to_vec(value)
                    .map_err(|e| Error::EncodeError(e.to_string()))?;
                encode_varint(bytes.len() as u64, &mut buf);
                buf.extend_from_slice(&bytes);
            }
        }
    }

    Ok((buf, ColumnEncoding::Raw))
}

/// Calculate varint size
fn varint_size(mut value: u64) -> usize {
    let mut size = 1;
    while value >= 0x80 {
        value >>= 7;
        size += 1;
    }
    size
}

/// Decode a full column
fn decode_column(
    data: &[u8],
    encoding: ColumnEncoding,
    field_type: &FieldType,
    expected_count: usize,
) -> Result<Vec<serde_json::Value>> {
    if data.is_empty() {
        return Ok(vec![serde_json::Value::Null; expected_count]);
    }

    let mut pos = 0;

    match encoding {
        ColumnEncoding::Varint => {
            let (count, len) = decode_varint(data)?;
            pos += len;

            let mut values = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let (encoded, len) = decode_varint(&data[pos..])?;
                pos += len;
                let i = zigzag_decode(encoded);
                values.push(serde_json::Value::Number(i.into()));
            }
            Ok(values)
        }

        ColumnEncoding::Delta => {
            let (count, len) = decode_varint(data)?;
            pos += len;

            if count == 0 {
                return Ok(Vec::new());
            }

            let mut values = Vec::with_capacity(count as usize);

            // First value
            let (encoded, len) = decode_varint(&data[pos..])?;
            pos += len;
            let mut prev = zigzag_decode(encoded);
            values.push(serde_json::Value::Number(prev.into()));

            // Deltas
            for _ in 1..count {
                let (encoded, len) = decode_varint(&data[pos..])?;
                pos += len;
                let delta = zigzag_decode(encoded);
                prev += delta;
                values.push(serde_json::Value::Number(prev.into()));
            }
            Ok(values)
        }

        ColumnEncoding::BitPacked(bits) => {
            let (count, len) = decode_varint(data)?;
            pos += len;

            let (min_encoded, len) = decode_varint(&data[pos..])?;
            pos += len;
            let min = zigzag_decode(min_encoded);

            let _bits_stored = data[pos];
            pos += 1;

            let mut values = Vec::with_capacity(count as usize);
            let mut bit_pos = 0u32;

            for _ in 0..count {
                let mut offset = 0u64;
                for bit in 0..bits {
                    let byte_idx = (bit_pos / 8) as usize;
                    let bit_idx = bit_pos % 8;
                    if byte_idx < data.len() - pos {
                        if (data[pos + byte_idx] >> bit_idx) & 1 == 1 {
                            offset |= 1 << bit;
                        }
                    }
                    bit_pos += 1;
                }
                values.push(serde_json::Value::Number((min + offset as i64).into()));
            }
            Ok(values)
        }

        ColumnEncoding::Dictionary => {
            // Read dictionary
            let (dict_len, len) = decode_varint(data)?;
            pos += len;

            let mut dict = Vec::with_capacity(dict_len as usize);
            for _ in 0..dict_len {
                let (str_len, len) = decode_varint(&data[pos..])?;
                pos += len;

                let s = std::str::from_utf8(&data[pos..pos + str_len as usize])
                    .map_err(|e| Error::DecodeError(e.to_string()))?;
                dict.push(s.to_string());
                pos += str_len as usize;
            }

            // Read indices
            let (count, len) = decode_varint(&data[pos..])?;
            pos += len;

            let mut values = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let (idx, len) = decode_varint(&data[pos..])?;
                pos += len;
                values.push(serde_json::Value::String(dict[idx as usize].clone()));
            }
            Ok(values)
        }

        ColumnEncoding::Raw => {
            let (count, len) = decode_varint(data)?;
            pos += len;

            let mut values = Vec::with_capacity(count as usize);

            for _ in 0..count {
                let value = match field_type {
                    FieldType::Boolean => {
                        let b = data[pos] != 0;
                        pos += 1;
                        serde_json::Value::Bool(b)
                    }
                    FieldType::Integer(_) => {
                        let (encoded, len) = decode_varint(&data[pos..])?;
                        pos += len;
                        serde_json::Value::Number(zigzag_decode(encoded).into())
                    }
                    FieldType::Float(_) => {
                        let f = f64::from_le_bytes([
                            data[pos], data[pos+1], data[pos+2], data[pos+3],
                            data[pos+4], data[pos+5], data[pos+6], data[pos+7],
                        ]);
                        pos += 8;
                        serde_json::Number::from_f64(f)
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null)
                    }
                    FieldType::String | FieldType::Timestamp | FieldType::Uuid => {
                        let (str_len, len) = decode_varint(&data[pos..])?;
                        pos += len;

                        let s = std::str::from_utf8(&data[pos..pos + str_len as usize])
                            .map_err(|e| Error::DecodeError(e.to_string()))?;
                        pos += str_len as usize;
                        serde_json::Value::String(s.to_string())
                    }
                    _ => {
                        // Fallback: JSON deserialize
                        let (json_len, len) = decode_varint(&data[pos..])?;
                        pos += len;

                        let v: serde_json::Value = serde_json::from_slice(&data[pos..pos + json_len as usize])
                            .map_err(|e| Error::DecodeError(e.to_string()))?;
                        pos += json_len as usize;
                        v
                    }
                };
                values.push(value);
            }
            Ok(values)
        }

        ColumnEncoding::RunLength => {
            // Not implemented yet
            Ok(vec![serde_json::Value::Null; expected_count])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SchemaInferrer;

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

    #[test]
    fn test_columnar_roundtrip() {
        let values: Vec<serde_json::Value> = vec![
            serde_json::json!({"id": 1, "name": "alice", "score": 95.5}),
            serde_json::json!({"id": 2, "name": "bob", "score": 87.0}),
            serde_json::json!({"id": 3, "name": "charlie", "score": 92.5}),
        ];

        let mut inferrer = SchemaInferrer::new();
        for v in &values {
            inferrer.add_value(v).unwrap();
        }
        let schema = inferrer.infer().unwrap();

        let block = ColumnarBlock::from_array(&values, &schema).unwrap();
        let decoded = block.to_array(&schema).unwrap();

        assert_eq!(values.len(), decoded.len());
        for (orig, dec) in values.iter().zip(decoded.iter()) {
            assert_eq!(orig, dec);
        }
    }

    #[test]
    fn test_columnar_delta_encoding() {
        // Sequential IDs should use delta encoding
        let values: Vec<serde_json::Value> = (0..100)
            .map(|i| serde_json::json!({"id": 1000 + i, "val": i * 10}))
            .collect();

        let mut inferrer = SchemaInferrer::new();
        for v in &values {
            inferrer.add_value(v).unwrap();
        }
        let schema = inferrer.infer().unwrap();

        let block = ColumnarBlock::from_array(&values, &schema).unwrap();

        // Find the 'id' column
        let id_col = block.columns.iter().find(|c| c.name == "id").unwrap();

        // Should use delta or bit-packed encoding for sequential IDs
        assert!(
            matches!(id_col.encoding, ColumnEncoding::Delta | ColumnEncoding::BitPacked(_)),
            "Expected Delta or BitPacked for sequential IDs, got {:?}",
            id_col.encoding
        );

        // Verify roundtrip
        let decoded = block.to_array(&schema).unwrap();
        for (i, dec) in decoded.iter().enumerate() {
            let id = dec.get("id").unwrap().as_i64().unwrap();
            assert_eq!(id, 1000 + i as i64);
        }
    }

    #[test]
    fn test_columnar_dictionary_encoding() {
        // Repeated strings should use dictionary encoding
        let values: Vec<serde_json::Value> = (0..100)
            .map(|i| serde_json::json!({
                "id": i,
                "status": if i % 3 == 0 { "active" } else if i % 3 == 1 { "pending" } else { "inactive" }
            }))
            .collect();

        let mut inferrer = SchemaInferrer::new();
        for v in &values {
            inferrer.add_value(v).unwrap();
        }
        let schema = inferrer.infer().unwrap();

        let block = ColumnarBlock::from_array(&values, &schema).unwrap();

        // Find the 'status' column
        let status_col = block.columns.iter().find(|c| c.name == "status").unwrap();

        // Should use dictionary encoding for low-cardinality strings
        assert_eq!(status_col.encoding, ColumnEncoding::Dictionary,
            "Expected Dictionary encoding for repeated strings");

        // Verify roundtrip
        let decoded = block.to_array(&schema).unwrap();
        for (i, dec) in decoded.iter().enumerate() {
            let expected = if i % 3 == 0 { "active" } else if i % 3 == 1 { "pending" } else { "inactive" };
            let status = dec.get("status").unwrap().as_str().unwrap();
            assert_eq!(status, expected);
        }
    }

    #[test]
    fn test_columnar_size_savings() {
        // Create data with patterns that benefit from columnar encoding
        let values: Vec<serde_json::Value> = (0..100)
            .map(|i| serde_json::json!({
                "user_id": 10000 + i,
                "username": format!("user{}", i),
                "role": if i % 2 == 0 { "admin" } else { "user" },
                "active": true
            }))
            .collect();

        let json_size: usize = values.iter()
            .map(|v| serde_json::to_vec(v).unwrap().len())
            .sum();

        let mut inferrer = SchemaInferrer::new();
        for v in &values {
            inferrer.add_value(v).unwrap();
        }
        let schema = inferrer.infer().unwrap();

        let block = ColumnarBlock::from_array(&values, &schema).unwrap();
        let columnar_size = block.encoded_size();

        println!("JSON size: {}, Columnar size: {}", json_size, columnar_size);

        // Columnar should be more compact (at least for the encoded data)
        // Note: Total serialized size includes metadata, but encoded data should be smaller
        assert!(columnar_size < json_size,
            "Columnar ({}) should be smaller than JSON ({})",
            columnar_size, json_size);
    }
}

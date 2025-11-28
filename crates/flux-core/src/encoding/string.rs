//! String encoding strategies

use super::varint::{encode_varint, decode_varint};
use crate::{Error, Result};
use std::collections::HashMap;

/// String encoding strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringEncoding {
    /// Raw length-prefixed
    Raw,
    /// Dictionary encoding
    Dictionary,
    /// Run-length encoding (for repeated strings)
    RunLength,
    /// Front compression (prefix sharing)
    FrontCompression,
}

/// Build a dictionary from string samples
pub fn build_dictionary(strings: &[&str], max_entries: usize) -> Vec<String> {
    // Count frequencies
    let mut freq: HashMap<&str, usize> = HashMap::new();
    for s in strings {
        *freq.entry(*s).or_insert(0) += 1;
    }

    // Sort by frequency
    let mut entries: Vec<_> = freq.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    // Take top entries that appear more than once
    entries
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .take(max_entries)
        .map(|(s, _)| s.to_string())
        .collect()
}

/// Encode strings with dictionary
pub fn encode_dictionary(
    strings: &[&str],
    dict: &[String],
    buf: &mut Vec<u8>,
) {
    // Build lookup
    let lookup: HashMap<&str, u32> = dict
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i as u32))
        .collect();

    // Write dictionary
    encode_varint(dict.len() as u64, buf);
    for entry in dict {
        encode_varint(entry.len() as u64, buf);
        buf.extend_from_slice(entry.as_bytes());
    }

    // Write string count
    encode_varint(strings.len() as u64, buf);

    // Write strings
    for s in strings {
        if let Some(&id) = lookup.get(s) {
            // Dictionary reference
            buf.push(0x01);
            encode_varint(id as u64, buf);
        } else {
            // Literal string
            buf.push(0x00);
            encode_varint(s.len() as u64, buf);
            buf.extend_from_slice(s.as_bytes());
        }
    }
}

/// Decode dictionary-encoded strings
pub fn decode_dictionary(buf: &[u8]) -> Result<Vec<String>> {
    let mut pos = 0;

    // Read dictionary
    let (dict_len, len) = decode_varint(buf)?;
    pos += len;

    let mut dict = Vec::with_capacity(dict_len as usize);
    for _ in 0..dict_len {
        let (str_len, len) = decode_varint(&buf[pos..])?;
        pos += len;

        let s = std::str::from_utf8(&buf[pos..pos + str_len as usize])
            .map_err(|e| Error::DecodeError(e.to_string()))?;
        dict.push(s.to_string());
        pos += str_len as usize;
    }

    // Read strings
    let (count, len) = decode_varint(&buf[pos..])?;
    pos += len;

    let mut strings = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let marker = buf[pos];
        pos += 1;

        if marker == 0x01 {
            // Dictionary reference
            let (id, len) = decode_varint(&buf[pos..])?;
            pos += len;
            strings.push(dict[id as usize].clone());
        } else {
            // Literal
            let (str_len, len) = decode_varint(&buf[pos..])?;
            pos += len;

            let s = std::str::from_utf8(&buf[pos..pos + str_len as usize])
                .map_err(|e| Error::DecodeError(e.to_string()))?;
            strings.push(s.to_string());
            pos += str_len as usize;
        }
    }

    Ok(strings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_dictionary() {
        let strings = vec!["apple", "banana", "apple", "cherry", "banana", "banana"];
        let dict = build_dictionary(&strings, 10);

        // "banana" appears most (3), then "apple" (2)
        assert!(dict.contains(&"banana".to_string()));
        assert!(dict.contains(&"apple".to_string()));
        // "cherry" appears only once, shouldn't be in dict
        assert!(!dict.contains(&"cherry".to_string()));
    }

    #[test]
    fn test_dictionary_roundtrip() {
        let strings = vec!["hello", "world", "hello", "foo", "world"];
        let dict = build_dictionary(&strings, 10);

        let mut buf = Vec::new();
        encode_dictionary(&strings, &dict, &mut buf);

        let decoded = decode_dictionary(&buf).unwrap();
        assert_eq!(decoded, strings);
    }
}

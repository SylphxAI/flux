//! Structure Template Extraction
//!
//! Extracts the structural skeleton from JSON, separating keys from values.

use super::tokenizer::{Token, Tokenizer};
use std::collections::HashMap;

/// A template represents the structure of a JSON document
#[derive(Debug, Clone)]
pub struct Template {
    /// Structural pattern (keys and structure tokens)
    pub pattern: Vec<TemplateToken>,
    /// Hash for quick comparison
    pub hash: u64,
    /// Number of value slots
    pub slot_count: usize,
}

/// Token in a template
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateToken {
    ObjectStart,
    ObjectEnd,
    ArrayStart,
    ArrayEnd,
    Key(Vec<u8>),      // Actual key bytes
    ValueSlot(u8),     // Placeholder for value (type hint)
    Colon,
    Comma,
}

/// Value types for slots
#[allow(dead_code)]
pub mod value_type {
    pub const STRING: u8 = 0;
    pub const NUMBER: u8 = 1;
    pub const BOOL: u8 = 2;
    pub const NULL: u8 = 3;
    pub const OBJECT: u8 = 4;
    pub const ARRAY: u8 = 5;
}

/// Extracts templates from JSON
pub struct TemplateExtractor {
    /// Known templates
    templates: HashMap<u64, Template>,
}

impl TemplateExtractor {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Extract template from JSON
    pub fn extract(&mut self, input: &[u8]) -> (Template, Vec<Value>) {
        let mut tokenizer = Tokenizer::new(input);
        let tokens = tokenizer.tokenize_all();

        let mut pattern = Vec::new();
        let mut values = Vec::new();
        let mut slot_count = 0;
        let mut expect_key = false;
        let mut depth_stack: Vec<bool> = Vec::new(); // true = object, false = array

        for token in &tokens {
            match token {
                Token::ObjectStart => {
                    pattern.push(TemplateToken::ObjectStart);
                    depth_stack.push(true);
                    expect_key = true;
                }
                Token::ObjectEnd => {
                    pattern.push(TemplateToken::ObjectEnd);
                    depth_stack.pop();
                    expect_key = depth_stack.last().copied().unwrap_or(false);
                }
                Token::ArrayStart => {
                    pattern.push(TemplateToken::ArrayStart);
                    depth_stack.push(false);
                    expect_key = false;
                }
                Token::ArrayEnd => {
                    pattern.push(TemplateToken::ArrayEnd);
                    depth_stack.pop();
                    expect_key = depth_stack.last().copied().unwrap_or(false);
                }
                Token::Colon => {
                    pattern.push(TemplateToken::Colon);
                    expect_key = false;
                }
                Token::Comma => {
                    pattern.push(TemplateToken::Comma);
                    // After comma in object, expect key; in array, expect value
                    expect_key = depth_stack.last().copied().unwrap_or(false);
                }
                Token::String(start, len) => {
                    let bytes = tokenizer.slice(*start, *len).to_vec();
                    if expect_key {
                        // This is a key
                        pattern.push(TemplateToken::Key(bytes));
                    } else {
                        // This is a value
                        pattern.push(TemplateToken::ValueSlot(value_type::STRING));
                        values.push(Value::String(bytes));
                        slot_count += 1;
                    }
                }
                Token::Number(start, len) => {
                    let bytes = tokenizer.slice(*start, *len).to_vec();
                    pattern.push(TemplateToken::ValueSlot(value_type::NUMBER));
                    values.push(Value::Number(bytes));
                    slot_count += 1;
                }
                Token::True => {
                    pattern.push(TemplateToken::ValueSlot(value_type::BOOL));
                    values.push(Value::Bool(true));
                    slot_count += 1;
                }
                Token::False => {
                    pattern.push(TemplateToken::ValueSlot(value_type::BOOL));
                    values.push(Value::Bool(false));
                    slot_count += 1;
                }
                Token::Null => {
                    pattern.push(TemplateToken::ValueSlot(value_type::NULL));
                    values.push(Value::Null);
                    slot_count += 1;
                }
            }
        }

        let hash = self.hash_pattern(&pattern);

        let template = Template {
            pattern,
            hash,
            slot_count,
        };

        // Cache template
        self.templates.entry(hash).or_insert_with(|| template.clone());

        (template, values)
    }

    /// Check if we have a matching template
    pub fn find_template(&self, hash: u64) -> Option<&Template> {
        self.templates.get(&hash)
    }

    /// Get all cached templates
    pub fn templates(&self) -> &HashMap<u64, Template> {
        &self.templates
    }

    fn hash_pattern(&self, pattern: &[TemplateToken]) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        for token in pattern {
            match token {
                TemplateToken::ObjectStart => 1u8.hash(&mut hasher),
                TemplateToken::ObjectEnd => 2u8.hash(&mut hasher),
                TemplateToken::ArrayStart => 3u8.hash(&mut hasher),
                TemplateToken::ArrayEnd => 4u8.hash(&mut hasher),
                TemplateToken::Key(k) => {
                    5u8.hash(&mut hasher);
                    k.hash(&mut hasher);
                }
                TemplateToken::ValueSlot(t) => {
                    6u8.hash(&mut hasher);
                    t.hash(&mut hasher);
                }
                TemplateToken::Colon => 7u8.hash(&mut hasher),
                TemplateToken::Comma => 8u8.hash(&mut hasher),
            }
        }
        hasher.finish()
    }
}

impl Default for TemplateExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Extracted value
#[derive(Debug, Clone)]
pub enum Value {
    String(Vec<u8>),
    Number(Vec<u8>),
    Bool(bool),
    Null,
}

impl Value {
    /// Encode value to bytes
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Value::String(s) => {
                let mut out = vec![value_type::STRING];
                out.extend_from_slice(&(s.len() as u16).to_le_bytes());
                out.extend_from_slice(s);
                out
            }
            Value::Number(n) => {
                let mut out = vec![value_type::NUMBER];
                out.extend_from_slice(&(n.len() as u8).to_le_bytes());
                out.extend_from_slice(n);
                out
            }
            Value::Bool(b) => {
                vec![value_type::BOOL, if *b { 1 } else { 0 }]
            }
            Value::Null => {
                vec![value_type::NULL]
            }
        }
    }

    /// Decode value from bytes
    pub fn decode(input: &[u8], pos: &mut usize) -> Option<Self> {
        if *pos >= input.len() {
            return None;
        }

        let typ = input[*pos];
        *pos += 1;

        match typ {
            value_type::STRING => {
                let len = u16::from_le_bytes([input[*pos], input[*pos + 1]]) as usize;
                *pos += 2;
                let s = input[*pos..*pos + len].to_vec();
                *pos += len;
                Some(Value::String(s))
            }
            value_type::NUMBER => {
                let len = input[*pos] as usize;
                *pos += 1;
                let n = input[*pos..*pos + len].to_vec();
                *pos += len;
                Some(Value::Number(n))
            }
            value_type::BOOL => {
                let b = input[*pos] != 0;
                *pos += 1;
                Some(Value::Bool(b))
            }
            value_type::NULL => Some(Value::Null),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple() {
        let mut extractor = TemplateExtractor::new();
        let input = br#"{"name":"alice","age":30}"#;

        let (template, values) = extractor.extract(input);

        assert_eq!(template.slot_count, 2);
        assert_eq!(values.len(), 2);

        // Check structure contains keys
        let has_name_key = template.pattern.iter().any(|t| {
            matches!(t, TemplateToken::Key(k) if k == b"name")
        });
        assert!(has_name_key);
    }

    #[test]
    fn test_same_structure_same_hash() {
        let mut extractor = TemplateExtractor::new();

        let input1 = br#"{"id":1,"name":"alice"}"#;
        let input2 = br#"{"id":2,"name":"bob"}"#;

        let (t1, _) = extractor.extract(input1);
        let (t2, _) = extractor.extract(input2);

        // Same structure should have same hash
        assert_eq!(t1.hash, t2.hash);
    }

    #[test]
    fn test_different_structure_different_hash() {
        let mut extractor = TemplateExtractor::new();

        let input1 = br#"{"id":1,"name":"alice"}"#;
        let input2 = br#"{"id":1,"email":"alice@example.com"}"#;

        let (t1, _) = extractor.extract(input1);
        let (t2, _) = extractor.extract(input2);

        // Different keys should have different hash
        assert_ne!(t1.hash, t2.hash);
    }

    #[test]
    fn test_value_encode_decode() {
        let values = vec![
            Value::String(b"hello".to_vec()),
            Value::Number(b"123".to_vec()),
            Value::Bool(true),
            Value::Null,
        ];

        for original in values {
            let encoded = original.encode();
            let mut pos = 0;
            let decoded = Value::decode(&encoded, &mut pos).unwrap();

            match (&original, &decoded) {
                (Value::String(a), Value::String(b)) => assert_eq!(a, b),
                (Value::Number(a), Value::Number(b)) => assert_eq!(a, b),
                (Value::Bool(a), Value::Bool(b)) => assert_eq!(a, b),
                (Value::Null, Value::Null) => {}
                _ => panic!("Type mismatch"),
            }
        }
    }
}

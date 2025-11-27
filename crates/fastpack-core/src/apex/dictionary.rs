//! Adaptive Dictionary System
//!
//! Multi-level dictionary for pattern learning:
//! - L0: Static (common JSON patterns)
//! - L1: Session (learned across requests)
//! - L2: Message (local patterns)

use std::collections::HashMap;

/// Dictionary entry
#[derive(Debug, Clone)]
pub struct DictEntry {
    /// The pattern bytes
    pub pattern: Vec<u8>,
    /// Usage count
    pub count: u32,
    /// Level (0=static, 1=session, 2=message)
    pub level: DictionaryLevel,
}

/// Dictionary level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictionaryLevel {
    Static = 0,
    Session = 1,
    Message = 2,
}

/// Adaptive dictionary
pub struct Dictionary {
    /// Pattern to ID mapping
    pattern_to_id: HashMap<Vec<u8>, u16>,
    /// ID to entry mapping
    entries: Vec<DictEntry>,
    /// Next available ID
    next_id: u16,
}

impl Dictionary {
    /// Create new dictionary with static entries
    pub fn new() -> Self {
        let mut dict = Self {
            pattern_to_id: HashMap::new(),
            entries: Vec::new(),
            next_id: 0,
        };

        // Add static L0 entries (common JSON patterns)
        dict.add_static_entries();

        dict
    }

    /// Create empty dictionary (no static entries)
    pub fn empty() -> Self {
        Self {
            pattern_to_id: HashMap::new(),
            entries: Vec::new(),
            next_id: 0,
        }
    }

    fn add_static_entries(&mut self) {
        // Common JSON keys
        let static_patterns: &[&[u8]] = &[
            // Common keys
            b"id", b"name", b"type", b"data", b"value", b"error",
            b"message", b"status", b"code", b"result", b"success",
            b"created_at", b"updated_at", b"deleted_at",
            b"user", b"email", b"password", b"token",
            b"items", b"list", b"count", b"total", b"page",
            b"url", b"path", b"method", b"headers", b"body",
            // Common values
            b"true", b"false", b"null",
            b"GET", b"POST", b"PUT", b"DELETE", b"PATCH",
            b"application/json", b"text/plain", b"text/html",
            b"content-type", b"authorization", b"accept",
            // Common structures
            b"{\"", b"\":", b",\"", b"\":\"", b"\",\"",
            b"\"}", b"[{", b"}]", b"},{",
            // Numbers
            b"0", b"1", b"2", b"3", b"4", b"5", b"6", b"7", b"8", b"9",
        ];

        for pattern in static_patterns {
            self.add(pattern.to_vec(), DictionaryLevel::Static);
        }
    }

    /// Add pattern to dictionary
    pub fn add(&mut self, pattern: Vec<u8>, level: DictionaryLevel) -> u16 {
        if let Some(&id) = self.pattern_to_id.get(&pattern) {
            // Increment count
            if let Some(entry) = self.entries.get_mut(id as usize) {
                entry.count += 1;
            }
            return id;
        }

        let id = self.next_id;
        self.next_id += 1;

        self.pattern_to_id.insert(pattern.clone(), id);
        self.entries.push(DictEntry {
            pattern,
            count: 1,
            level,
        });

        id
    }

    /// Look up pattern ID
    pub fn lookup(&self, pattern: &[u8]) -> Option<u16> {
        self.pattern_to_id.get(pattern).copied()
    }

    /// Get pattern by ID
    pub fn get(&self, id: u16) -> Option<&[u8]> {
        self.entries.get(id as usize).map(|e| e.pattern.as_slice())
    }

    /// Get entry by ID
    pub fn get_entry(&self, id: u16) -> Option<&DictEntry> {
        self.entries.get(id as usize)
    }

    /// Merge another dictionary into this one
    pub fn merge(&mut self, other: &Dictionary) {
        for entry in &other.entries {
            if entry.level != DictionaryLevel::Static {
                // Only merge non-static entries
                if !self.pattern_to_id.contains_key(&entry.pattern) {
                    self.add(entry.pattern.clone(), DictionaryLevel::Session);
                }
            }
        }
    }

    /// Get dictionary size
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// Find longest matching pattern at position
    pub fn find_longest_match(&self, input: &[u8], pos: usize) -> Option<(u16, usize)> {
        let mut best_match: Option<(u16, usize)> = None;

        // Try patterns of decreasing length
        let max_len = (input.len() - pos).min(64); // Limit pattern length

        for len in (2..=max_len).rev() {
            let pattern = &input[pos..pos + len];
            if let Some(id) = self.lookup(pattern) {
                if best_match.map_or(true, |(_, l)| len > l) {
                    best_match = Some((id, len));
                    break; // Found longest match
                }
            }
        }

        best_match
    }

    /// Learn patterns from input
    pub fn learn(&mut self, input: &[u8], level: DictionaryLevel) {
        // Simple n-gram learning
        let min_len = 3;
        let max_len = 16;

        // Count occurrences of patterns
        let mut counts: HashMap<&[u8], u32> = HashMap::new();

        for len in min_len..=max_len.min(input.len()) {
            for i in 0..=input.len() - len {
                let pattern = &input[i..i + len];
                *counts.entry(pattern).or_insert(0) += 1;
            }
        }

        // Add patterns that appear multiple times
        for (pattern, count) in counts {
            if count >= 2 && !self.pattern_to_id.contains_key(pattern) {
                self.add(pattern.to_vec(), level);
            }
        }
    }

    /// Encode dictionary for transmission
    pub fn encode(&self, level: DictionaryLevel) -> Vec<u8> {
        let mut output = Vec::new();

        let entries: Vec<_> = self.entries.iter()
            .filter(|e| e.level == level)
            .collect();

        // Entry count
        output.extend_from_slice(&(entries.len() as u16).to_le_bytes());

        for entry in entries {
            // Pattern length
            output.push(entry.pattern.len() as u8);
            // Pattern bytes
            output.extend_from_slice(&entry.pattern);
        }

        output
    }

    /// Decode dictionary from bytes
    pub fn decode(input: &[u8], level: DictionaryLevel) -> Self {
        let mut dict = Self::empty();
        let mut pos = 0;

        if input.len() < 2 {
            return dict;
        }

        let count = u16::from_le_bytes([input[0], input[1]]) as usize;
        pos += 2;

        for _ in 0..count {
            if pos >= input.len() {
                break;
            }

            let len = input[pos] as usize;
            pos += 1;

            if pos + len > input.len() {
                break;
            }

            let pattern = input[pos..pos + len].to_vec();
            pos += len;

            dict.add(pattern, level);
        }

        dict
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_entries() {
        let dict = Dictionary::new();
        assert!(dict.size() > 0);
        assert!(dict.lookup(b"id").is_some());
        assert!(dict.lookup(b"name").is_some());
    }

    #[test]
    fn test_add_and_lookup() {
        let mut dict = Dictionary::empty();
        let id = dict.add(b"custom".to_vec(), DictionaryLevel::Session);

        assert_eq!(dict.lookup(b"custom"), Some(id));
        assert_eq!(dict.get(id), Some(b"custom".as_slice()));
    }

    #[test]
    fn test_find_longest_match() {
        let mut dict = Dictionary::empty();
        dict.add(b"hello".to_vec(), DictionaryLevel::Session);
        dict.add(b"hello world".to_vec(), DictionaryLevel::Session);
        dict.add(b"hel".to_vec(), DictionaryLevel::Session);

        let input = b"hello world!";
        let result = dict.find_longest_match(input, 0);

        // Should find "hello world" (longest)
        assert!(result.is_some());
        let (id, len) = result.unwrap();
        assert_eq!(len, 11); // "hello world"
        assert_eq!(dict.get(id), Some(b"hello world".as_slice()));
    }

    #[test]
    fn test_learn() {
        let mut dict = Dictionary::empty();
        let input = b"abcabc defdef abcabc";

        dict.learn(input, DictionaryLevel::Message);

        // Should have learned "abc" (appears 3 times)
        assert!(dict.lookup(b"abc").is_some());
    }

    #[test]
    fn test_encode_decode() {
        let mut dict = Dictionary::empty();
        dict.add(b"test1".to_vec(), DictionaryLevel::Session);
        dict.add(b"test2".to_vec(), DictionaryLevel::Session);

        let encoded = dict.encode(DictionaryLevel::Session);
        let decoded = Dictionary::decode(&encoded, DictionaryLevel::Session);

        assert_eq!(decoded.size(), 2);
        assert!(decoded.lookup(b"test1").is_some());
        assert!(decoded.lookup(b"test2").is_some());
    }
}

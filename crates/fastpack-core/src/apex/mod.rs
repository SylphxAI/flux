//! APEX - Adaptive Pattern EXtraction
//!
//! A next-generation compression algorithm optimized for JSON/API data.
//!
//! Key innovations:
//! 1. Structural compression - separate structure from values
//! 2. Adaptive dictionary - learns patterns across requests
//! 3. Predictive encoding - uses JSON grammar for prediction
//! 4. Delta streams - efficient encoding of sequential data

mod tokenizer;
mod template;
mod dictionary;
mod encoder;
mod delta;

pub use tokenizer::{Token, Tokenizer};
pub use template::{Template, TemplateExtractor};
pub use dictionary::{Dictionary, DictionaryLevel};
pub use encoder::{ApexEncoder, ApexDecoder};
pub use delta::DeltaEncoder;

use crate::{Error, Result};

/// APEX magic bytes
pub const APEX_MAGIC: [u8; 4] = *b"APEX";

/// APEX version
pub const APEX_VERSION: u8 = 1;

/// APEX compression options
#[derive(Debug, Clone, Default)]
pub struct ApexOptions {
    /// Enable structure detection
    pub structural: bool,
    /// Enable predictive encoding
    pub predictive: bool,
    /// Enable delta encoding
    pub delta: bool,
    /// Compression level (0-3)
    pub level: u8,
}

/// APEX session for stateful compression
pub struct ApexSession {
    dictionary: Dictionary,
    templates: Vec<Template>,
    message_count: u64,
}

impl ApexSession {
    pub fn new() -> Self {
        Self {
            dictionary: Dictionary::new(),
            templates: Vec::new(),
            message_count: 0,
        }
    }

    /// Compress with session learning
    pub fn compress(&mut self, input: &[u8], opts: &ApexOptions) -> Result<Vec<u8>> {
        let mut encoder = ApexEncoder::new(opts.clone(), &self.dictionary);
        let result = encoder.encode(input)?;

        // Update session dictionary
        self.dictionary.merge(&encoder.local_dictionary());
        self.message_count += 1;

        Ok(result)
    }

    /// Decompress with session state
    pub fn decompress(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = ApexDecoder::new(&self.dictionary);
        let result = decoder.decode(input)?;

        // Update session dictionary from received data
        self.dictionary.merge(&decoder.learned_dictionary());

        Ok(result)
    }

    /// Get compression statistics
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            message_count: self.message_count,
            dictionary_size: self.dictionary.size(),
            template_count: self.templates.len(),
        }
    }
}

impl Default for ApexSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Session statistics
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub message_count: u64,
    pub dictionary_size: usize,
    pub template_count: usize,
}

/// Standalone APEX compression (no session)
pub fn apex_compress(input: &[u8], opts: &ApexOptions) -> Result<Vec<u8>> {
    let dict = Dictionary::new();
    let mut encoder = ApexEncoder::new(opts.clone(), &dict);
    encoder.encode(input)
}

/// Standalone APEX decompression
pub fn apex_decompress(input: &[u8]) -> Result<Vec<u8>> {
    let dict = Dictionary::new();
    let mut decoder = ApexDecoder::new(&dict);
    decoder.decode(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apex_roundtrip() {
        let data = br#"{"id":123,"name":"test","values":[1,2,3]}"#;
        let opts = ApexOptions::default();

        let compressed = apex_compress(data, &opts).unwrap();
        let decompressed = apex_decompress(&compressed).unwrap();

        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_session_roundtrip() {
        let mut session = ApexSession::new();
        let opts = ApexOptions {
            structural: true,
            ..Default::default()
        };

        // Similar JSON structures
        let data1 = br#"{"id":1,"name":"alice","score":100}"#;
        let data2 = br#"{"id":2,"name":"bob","score":95}"#;
        let data3 = br#"{"id":3,"name":"charlie","score":88}"#;

        let c1 = session.compress(data1, &opts).unwrap();
        let c2 = session.compress(data2, &opts).unwrap();
        let c3 = session.compress(data3, &opts).unwrap();

        // All should compress
        assert!(c1.len() > 0);
        assert!(c2.len() > 0);
        assert!(c3.len() > 0);

        // Verify roundtrip
        let mut decode_session = ApexSession::new();
        let d1 = decode_session.decompress(&c1).unwrap();
        let d2 = decode_session.decompress(&c2).unwrap();
        let d3 = decode_session.decompress(&c3).unwrap();

        assert_eq!(data1.as_slice(), d1.as_slice());
        assert_eq!(data2.as_slice(), d2.as_slice());
        assert_eq!(data3.as_slice(), d3.as_slice());

        // Session stats
        let stats = session.stats();
        assert_eq!(stats.message_count, 3);
    }
}

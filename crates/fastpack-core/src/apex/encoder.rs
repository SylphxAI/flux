//! APEX Encoder/Decoder
//!
//! Main compression engine combining all APEX features.

use super::{
    dictionary::Dictionary,
    template::{TemplateExtractor, Value},
    tokenizer::is_json,
    ans::{ans_compress, ans_decompress},
    APEX_MAGIC, APEX_VERSION, ApexOptions,
};
use crate::{Result, Error};
use crate::compress::compress as lz4_compress;
use crate::decompress::decompress as lz4_decompress;
use crate::Options as Lz4Options;

/// Flags for APEX frame
#[allow(dead_code)]
mod flags {
    pub const HAS_TEMPLATE: u8 = 0b0000_0001;
    pub const HAS_DICT_UPDATE: u8 = 0b0000_0010;
    pub const DELTA_ENABLED: u8 = 0b0000_0100;
    pub const IS_JSON: u8 = 0b0000_1000;
    pub const LZ4_FALLBACK: u8 = 0b0001_0000;
    pub const ANS_ENCODED: u8 = 0b0010_0000;
}

/// APEX Encoder
pub struct ApexEncoder {
    opts: ApexOptions,
    #[allow(dead_code)]
    session_dict: Dictionary,
    local_dict: Dictionary,
    template_extractor: TemplateExtractor,
}

impl ApexEncoder {
    pub fn new(opts: ApexOptions, _session_dict: &Dictionary) -> Self {
        Self {
            opts,
            session_dict: Dictionary::empty(),
            local_dict: Dictionary::empty(),
            template_extractor: TemplateExtractor::new(),
        }
        // Note: In a real implementation, we'd clone session_dict
        // For simplicity, using empty dicts here
    }

    /// Encode input data
    pub fn encode(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        let mut output = Vec::with_capacity(input.len());

        // Write header
        output.extend_from_slice(&APEX_MAGIC);
        output.push(APEX_VERSION);

        // Determine encoding strategy
        let is_json_input = is_json(input);
        let use_structural = is_json_input && self.opts.structural;

        let mut frame_flags = 0u8;
        if is_json_input {
            frame_flags |= flags::IS_JSON;
        }

        if use_structural && input.len() > 50 {
            // Try structural compression for larger JSON
            match self.encode_structural(input) {
                Ok(structural_data) => {
                    // Apply ANS entropy coding for better compression
                    let ans_data = ans_compress(&structural_data);

                    // Use ANS if it provides benefit
                    let (final_data, use_ans) = if ans_data.len() < structural_data.len() {
                        (ans_data, true)
                    } else {
                        (structural_data, false)
                    };

                    if final_data.len() < input.len() {
                        frame_flags |= flags::HAS_TEMPLATE;
                        if use_ans {
                            frame_flags |= flags::ANS_ENCODED;
                        }
                        output.push(frame_flags);
                        output.extend_from_slice(&(final_data.len() as u32).to_le_bytes());
                        output.extend_from_slice(&final_data);
                        return Ok(output);
                    }
                }
                Err(_) => {
                    // Fall through to LZ4
                }
            }
        }

        // Fallback to LZ4
        frame_flags |= flags::LZ4_FALLBACK;
        output.push(frame_flags);

        let compressed = lz4_compress(input, &Lz4Options::default())?;
        output.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        output.extend_from_slice(&compressed);

        Ok(output)
    }

    /// Structural encoding for JSON
    fn encode_structural(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        let (template, values) = self.template_extractor.extract(input);

        let mut output = Vec::new();

        // Encode template hash (for matching known templates)
        output.extend_from_slice(&template.hash.to_le_bytes());

        // Encode template pattern (simplified - in real impl, use dictionary)
        let template_bytes = self.encode_template(&template);
        output.extend_from_slice(&(template_bytes.len() as u16).to_le_bytes());
        output.extend_from_slice(&template_bytes);

        // Encode values
        let values_bytes = self.encode_values(&values);
        output.extend_from_slice(&(values_bytes.len() as u16).to_le_bytes());
        output.extend_from_slice(&values_bytes);

        Ok(output)
    }

    fn encode_template(&self, template: &super::template::Template) -> Vec<u8> {
        use super::template::TemplateToken;

        let mut output = Vec::new();
        output.push(template.pattern.len() as u8);

        for token in &template.pattern {
            match token {
                TemplateToken::ObjectStart => output.push(1),
                TemplateToken::ObjectEnd => output.push(2),
                TemplateToken::ArrayStart => output.push(3),
                TemplateToken::ArrayEnd => output.push(4),
                TemplateToken::Colon => output.push(5),
                TemplateToken::Comma => output.push(6),
                TemplateToken::Key(k) => {
                    output.push(7);
                    output.push(k.len() as u8);
                    output.extend_from_slice(k);
                }
                TemplateToken::ValueSlot(t) => {
                    output.push(8);
                    output.push(*t);
                }
            }
        }

        output
    }

    fn encode_values(&self, values: &[Value]) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(&(values.len() as u16).to_le_bytes());

        for value in values {
            output.extend_from_slice(&value.encode());
        }

        output
    }

    /// Get learned local dictionary
    pub fn local_dictionary(&self) -> &Dictionary {
        &self.local_dict
    }
}

/// APEX Decoder
pub struct ApexDecoder {
    #[allow(dead_code)]
    session_dict: Dictionary,
    learned_dict: Dictionary,
}

impl ApexDecoder {
    pub fn new(_session_dict: &Dictionary) -> Self {
        Self {
            session_dict: Dictionary::empty(),
            learned_dict: Dictionary::empty(),
        }
    }

    /// Decode APEX compressed data
    pub fn decode(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        if input.len() < 6 {
            return Err(Error::CorruptedData);
        }

        // Verify magic
        if input[0..4] != APEX_MAGIC {
            return Err(Error::InvalidMagic);
        }

        let version = input[4];
        if version > APEX_VERSION {
            return Err(Error::UnsupportedVersion);
        }

        let frame_flags = input[5];
        let mut pos = 6;

        if frame_flags & flags::LZ4_FALLBACK != 0 {
            // LZ4 fallback path
            if pos + 4 > input.len() {
                return Err(Error::CorruptedData);
            }

            let compressed_len = u32::from_le_bytes([
                input[pos], input[pos + 1], input[pos + 2], input[pos + 3]
            ]) as usize;
            pos += 4;

            if pos + compressed_len > input.len() {
                return Err(Error::CorruptedData);
            }

            let compressed = &input[pos..pos + compressed_len];
            return lz4_decompress(compressed);
        }

        if frame_flags & flags::HAS_TEMPLATE != 0 {
            // Structural decompression
            let ans_encoded = frame_flags & flags::ANS_ENCODED != 0;
            return self.decode_structural(&input[pos..], ans_encoded);
        }

        Err(Error::CorruptedData)
    }

    fn decode_structural(&mut self, input: &[u8], ans_encoded: bool) -> Result<Vec<u8>> {
        // First 4 bytes are data length (part of frame format)
        if input.len() < 4 {
            return Err(Error::CorruptedData);
        }
        let data_bytes = &input[4..];

        // If ANS encoded, decode first to get structural data
        let decoded_input;
        let structural_data: &[u8] = if ans_encoded {
            decoded_input = ans_decompress(data_bytes)
                .ok_or(Error::CorruptedData)?;
            &decoded_input[..]
        } else {
            data_bytes
        };

        let mut pos = 0;

        // Read template hash (8 bytes)
        if pos + 8 > structural_data.len() {
            return Err(Error::CorruptedData);
        }
        let _template_hash = u64::from_le_bytes([
            structural_data[pos], structural_data[pos + 1], structural_data[pos + 2], structural_data[pos + 3],
            structural_data[pos + 4], structural_data[pos + 5], structural_data[pos + 6], structural_data[pos + 7]
        ]);
        pos += 8;

        // Read template
        if pos + 2 > structural_data.len() {
            return Err(Error::CorruptedData);
        }
        let template_len = u16::from_le_bytes([structural_data[pos], structural_data[pos + 1]]) as usize;
        pos += 2;

        if pos + template_len > structural_data.len() {
            return Err(Error::CorruptedData);
        }
        let template_bytes = &structural_data[pos..pos + template_len];
        pos += template_len;

        // Read values
        if pos + 2 > structural_data.len() {
            return Err(Error::CorruptedData);
        }
        let values_len = u16::from_le_bytes([structural_data[pos], structural_data[pos + 1]]) as usize;
        pos += 2;

        if pos + values_len > structural_data.len() {
            return Err(Error::CorruptedData);
        }
        let values_bytes = &structural_data[pos..pos + values_len];

        // Reconstruct JSON
        self.reconstruct_json(template_bytes, values_bytes)
    }

    fn reconstruct_json(&self, template: &[u8], values: &[u8]) -> Result<Vec<u8>> {
        use super::template::Value;

        let mut output = Vec::new();
        let mut t_pos = 0;
        let mut v_pos = 0;

        if template.is_empty() {
            return Err(Error::CorruptedData);
        }

        // Skip value count in values
        if values.len() >= 2 {
            v_pos = 2;
        }

        let token_count = template[t_pos] as usize;
        t_pos += 1;

        for _ in 0..token_count {
            if t_pos >= template.len() {
                break;
            }

            let token_type = template[t_pos];
            t_pos += 1;

            match token_type {
                1 => output.push(b'{'),
                2 => output.push(b'}'),
                3 => output.push(b'['),
                4 => output.push(b']'),
                5 => output.push(b':'),
                6 => output.push(b','),
                7 => {
                    // Key
                    if t_pos >= template.len() {
                        break;
                    }
                    let key_len = template[t_pos] as usize;
                    t_pos += 1;

                    output.push(b'"');
                    if t_pos + key_len <= template.len() {
                        output.extend_from_slice(&template[t_pos..t_pos + key_len]);
                    }
                    t_pos += key_len;
                    output.push(b'"');
                }
                8 => {
                    // Value slot
                    if t_pos >= template.len() {
                        break;
                    }
                    let _value_type = template[t_pos];
                    t_pos += 1;

                    // Decode value
                    if let Some(value) = Value::decode(values, &mut v_pos) {
                        match value {
                            Value::String(s) => {
                                output.push(b'"');
                                output.extend_from_slice(&s);
                                output.push(b'"');
                            }
                            Value::Number(n) => {
                                output.extend_from_slice(&n);
                            }
                            Value::Bool(b) => {
                                if b {
                                    output.extend_from_slice(b"true");
                                } else {
                                    output.extend_from_slice(b"false");
                                }
                            }
                            Value::Null => {
                                output.extend_from_slice(b"null");
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(output)
    }

    /// Get learned dictionary from decoding
    pub fn learned_dictionary(&self) -> &Dictionary {
        &self.learned_dict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_simple() {
        let input = br#"{"id":123,"name":"test"}"#;
        let opts = ApexOptions::default();

        let dict = Dictionary::new();
        let mut encoder = ApexEncoder::new(opts, &dict);
        let compressed = encoder.encode(input).unwrap();

        let mut decoder = ApexDecoder::new(&dict);
        let decompressed = decoder.decode(&compressed).unwrap();

        assert_eq!(input.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_encode_decode_structural() {
        let input = br#"{"id":123,"name":"alice","score":100,"active":true}"#;
        let opts = ApexOptions {
            structural: true,
            ..Default::default()
        };

        let dict = Dictionary::new();
        let mut encoder = ApexEncoder::new(opts, &dict);
        let compressed = encoder.encode(input).unwrap();

        let mut decoder = ApexDecoder::new(&dict);
        let decompressed = decoder.decode(&compressed).unwrap();

        assert_eq!(input.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_non_json_fallback() {
        let input = b"This is not JSON, just plain text";
        let opts = ApexOptions {
            structural: true,
            ..Default::default()
        };

        let dict = Dictionary::new();
        let mut encoder = ApexEncoder::new(opts, &dict);
        let compressed = encoder.encode(input).unwrap();

        // Should use LZ4 fallback
        assert!(compressed[5] & flags::LZ4_FALLBACK != 0);

        let mut decoder = ApexDecoder::new(&dict);
        let decompressed = decoder.decode(&compressed).unwrap();

        assert_eq!(input.as_slice(), decompressed.as_slice());
    }
}

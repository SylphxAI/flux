//! Pure JSON surface detectors for FastPack (ADR-168 rust_impl residual).
//!
//! Structural checks only — no I/O. Oracle: tokenizer::is_json + frame flag bits.

use crate::apex::is_json;
use crate::frame::Flags;

/// True when input looks like a JSON object/array after trimming ASCII whitespace.
#[inline]
pub fn looks_like_json(input: &[u8]) -> bool {
    is_json(input)
}

/// Classify pure payload surface for encoder selection heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadSurface {
    Empty,
    JsonObject,
    JsonArray,
    BinaryOrText,
}

pub fn classify_payload_surface(input: &[u8]) -> PayloadSurface {
    if input.is_empty() {
        return PayloadSurface::Empty;
    }
    if !looks_like_json(input) {
        return PayloadSurface::BinaryOrText;
    }
    let mut i = 0;
    while i < input.len() && input[i].is_ascii_whitespace() {
        i += 1;
    }
    match input.get(i).copied() {
        Some(b'{') => PayloadSurface::JsonObject,
        Some(b'[') => PayloadSurface::JsonArray,
        _ => PayloadSurface::BinaryOrText,
    }
}

/// Pure flag bit matrix: checksum bit round-trips via Flags helpers.
pub fn flags_checksum_roundtrip(with_checksum: bool) -> bool {
    let f = if with_checksum {
        Flags::new().with_checksum()
    } else {
        Flags::new()
    };
    let again = Flags::from_byte(f.as_byte());
    again.has_checksum() == with_checksum && again.as_byte() == f.as_byte()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_not_json() {
        assert!(!looks_like_json(b""));
        assert_eq!(classify_payload_surface(b""), PayloadSurface::Empty);
    }

    #[test]
    fn detects_object_and_array() {
        assert_eq!(
            classify_payload_surface(b"  {\"a\":1}"),
            PayloadSurface::JsonObject
        );
        assert_eq!(
            classify_payload_surface(b"\n[1,2]"),
            PayloadSurface::JsonArray
        );
        assert!(looks_like_json(b"{\"x\":true}"));
        assert!(looks_like_json(b"[0]"));
    }

    #[test]
    fn rejects_plain_text() {
        assert!(!looks_like_json(b"hello"));
        assert_eq!(
            classify_payload_surface(b"hello"),
            PayloadSurface::BinaryOrText
        );
        assert_eq!(
            classify_payload_surface(b"123"),
            PayloadSurface::BinaryOrText
        );
    }

    #[test]
    fn checksum_flag_matrix() {
        assert!(flags_checksum_roundtrip(true));
        assert!(flags_checksum_roundtrip(false));
    }
}

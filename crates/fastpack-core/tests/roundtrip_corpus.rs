//! Bounded compress/decompress roundtrip corpus (ADR-168 rust_impl deepen for fastpack).

use fastpack_core::{compress, decompress, Options};

fn cases() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("empty", vec![]),
        ("hello", b"hello world".to_vec()),
        ("zeros", vec![0u8; 64]),
        ("ascii-run", b"aaaaaaaaaaaaaaaaaaaaaaaa".to_vec()),
        ("binary", (0u8..=255).collect()),
    ]
}

#[test]
fn compress_decompress_roundtrip_corpus() {
    let opts = Options::default();
    for (id, input) in cases() {
        let compressed = compress(&input, &opts).unwrap_or_else(|e| panic!("{id}: compress: {e}"));
        let out = decompress(&compressed).unwrap_or_else(|e| panic!("{id}: decompress: {e}"));
        assert_eq!(out, input, "case {id}");
    }
}

//! WebAssembly bindings for FastPack

use wasm_bindgen::prelude::*;
use fastpack_core::{compress as core_compress, decompress as core_decompress, Options, Level};

/// Compress data
#[wasm_bindgen]
pub fn compress(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    core_compress(data, &Options::default())
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Compress data with options
#[wasm_bindgen]
pub fn compress_with_level(data: &[u8], level: u8) -> Result<Vec<u8>, JsValue> {
    let opts = Options {
        level: match level {
            0 => Level::None,
            1 => Level::Fast,
            _ => Level::Better,
        },
        checksum: false,
    };
    core_compress(data, &opts)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Decompress data
#[wasm_bindgen]
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    core_decompress(data)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Get library version
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

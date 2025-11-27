//! Node.js native addon bindings for FastPack

use napi_derive::napi;
use fastpack_core::{compress as core_compress, decompress as core_decompress, Options, Level};

/// Compress data synchronously
#[napi]
pub fn compress_sync(data: napi::bindgen_prelude::Buffer) -> napi::Result<napi::bindgen_prelude::Buffer> {
    let result = core_compress(&data, &Options::default())
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result.into())
}

/// Compress data with level
#[napi]
pub fn compress_sync_with_level(data: napi::bindgen_prelude::Buffer, level: u8) -> napi::Result<napi::bindgen_prelude::Buffer> {
    let opts = Options {
        level: match level {
            0 => Level::None,
            1 => Level::Fast,
            _ => Level::Better,
        },
        checksum: false,
    };
    let result = core_compress(&data, &opts)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result.into())
}

/// Decompress data synchronously
#[napi]
pub fn decompress_sync(data: napi::bindgen_prelude::Buffer) -> napi::Result<napi::bindgen_prelude::Buffer> {
    let result = core_decompress(&data)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result.into())
}

/// Get library version
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

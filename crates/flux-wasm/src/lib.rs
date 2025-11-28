//! WebAssembly bindings for FLUX v2
//!
//! FLUX is a schema-aware JSON compression protocol optimized for API traffic.

use wasm_bindgen::prelude::*;
use flux_core::{
    compress as core_compress,
    decompress as core_decompress,
    FluxSession, FluxConfig, FluxStreamSession,
};
use std::cell::RefCell;
use std::collections::HashMap;

// ============================================================================
// One-shot compression
// ============================================================================

/// Compress JSON data using FLUX
///
/// Best for one-off compression. For repeated compression of similar data,
/// use session-based compression instead.
#[wasm_bindgen]
pub fn flux_compress(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    core_compress(data)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Decompress FLUX data
#[wasm_bindgen]
pub fn flux_decompress(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    core_decompress(data)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// ============================================================================
// Session-based compression (schema caching)
// ============================================================================

thread_local! {
    static FLUX_SESSIONS: RefCell<HashMap<u32, FluxSession>> = RefCell::new(HashMap::new());
    static STREAM_SESSIONS: RefCell<HashMap<u32, FluxStreamSession>> = RefCell::new(HashMap::new());
    static NEXT_SESSION_ID: RefCell<u32> = RefCell::new(1);
}

fn get_next_id() -> u32 {
    NEXT_SESSION_ID.with(|next_id| {
        let id = *next_id.borrow();
        *next_id.borrow_mut() = id + 1;
        id
    })
}

/// Create a new FLUX session for schema-cached compression
/// Returns session ID
#[wasm_bindgen]
pub fn flux_session_create() -> u32 {
    let id = get_next_id();
    FLUX_SESSIONS.with(|sessions| {
        sessions.borrow_mut().insert(id, FluxSession::new());
    });
    id
}

/// Create a FLUX session with custom configuration
#[wasm_bindgen]
pub fn flux_session_create_with_config(
    columnar: bool,
    entropy: bool,
    delta: bool,
    checksum: bool,
) -> u32 {
    let id = get_next_id();
    let config = FluxConfig {
        columnar,
        entropy,
        delta,
        checksum,
        max_dict_size: 65536,
    };
    FLUX_SESSIONS.with(|sessions| {
        sessions.borrow_mut().insert(id, FluxSession::with_config(config));
    });
    id
}

/// Compress using FLUX session (enables schema caching)
#[wasm_bindgen]
pub fn flux_session_compress(session_id: u32, data: &[u8]) -> Result<Vec<u8>, JsValue> {
    FLUX_SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid session ID"))?;

        session.compress(data)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

/// Decompress using FLUX session
#[wasm_bindgen]
pub fn flux_session_decompress(session_id: u32, data: &[u8]) -> Result<Vec<u8>, JsValue> {
    FLUX_SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid session ID"))?;

        session.decompress(data)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

/// Get FLUX session statistics as JSON
#[wasm_bindgen]
pub fn flux_session_stats(session_id: u32) -> Result<String, JsValue> {
    FLUX_SESSIONS.with(|sessions| {
        let sessions = sessions.borrow();
        let session = sessions.get(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid session ID"))?;

        let stats = session.stats();
        let ratio = session.compression_ratio();

        Ok(format!(
            r#"{{"messagesProcessed":{},"bytesIn":{},"bytesOut":{},"schemasCached":{},"cacheHits":{},"cacheMisses":{},"compressionRatio":{:.3}}}"#,
            stats.messages_processed,
            stats.bytes_in,
            stats.bytes_out,
            stats.schemas_cached,
            stats.cache_hits,
            stats.cache_misses,
            ratio
        ))
    })
}

/// Reset FLUX session state
#[wasm_bindgen]
pub fn flux_session_reset(session_id: u32) -> Result<(), JsValue> {
    FLUX_SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid session ID"))?;

        session.reset();
        Ok(())
    })
}

/// Destroy a FLUX session
#[wasm_bindgen]
pub fn flux_session_destroy(session_id: u32) -> bool {
    FLUX_SESSIONS.with(|sessions| {
        sessions.borrow_mut().remove(&session_id).is_some()
    })
}

// ============================================================================
// Streaming delta compression (real-time state updates)
// ============================================================================

/// Create a new streaming session for delta compression
/// Ideal for WebSocket-style real-time state updates
#[wasm_bindgen]
pub fn flux_stream_create() -> u32 {
    let id = get_next_id();
    STREAM_SESSIONS.with(|sessions| {
        sessions.borrow_mut().insert(id, FluxStreamSession::new());
    });
    id
}

/// Send state update, returns compressed delta
/// First call returns full state, subsequent calls return only changes
#[wasm_bindgen]
pub fn flux_stream_update(session_id: u32, json: &[u8]) -> Result<Vec<u8>, JsValue> {
    STREAM_SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid stream session ID"))?;

        session.update(json)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

/// Receive delta and reconstruct full state
#[wasm_bindgen]
pub fn flux_stream_receive(session_id: u32, data: &[u8]) -> Result<Vec<u8>, JsValue> {
    STREAM_SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid stream session ID"))?;

        session.receive(data)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

/// Get streaming session statistics
#[wasm_bindgen]
pub fn flux_stream_stats(session_id: u32) -> Result<String, JsValue> {
    STREAM_SESSIONS.with(|sessions| {
        let sessions = sessions.borrow();
        let session = sessions.get(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid stream session ID"))?;

        let stats = session.stats();
        let efficiency = session.delta_efficiency();

        Ok(format!(
            r#"{{"updatesSent":{},"fullSends":{},"deltaSends":{},"bytesFull":{},"bytesDelta":{},"deltaEfficiency":{:.3}}}"#,
            stats.updates_sent,
            stats.full_sends,
            stats.delta_sends,
            stats.bytes_full,
            stats.bytes_delta,
            efficiency
        ))
    })
}

/// Reset streaming session state
#[wasm_bindgen]
pub fn flux_stream_reset(session_id: u32) -> Result<(), JsValue> {
    STREAM_SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid stream session ID"))?;

        session.reset();
        Ok(())
    })
}

/// Destroy a streaming session
#[wasm_bindgen]
pub fn flux_stream_destroy(session_id: u32) -> bool {
    STREAM_SESSIONS.with(|sessions| {
        sessions.borrow_mut().remove(&session_id).is_some()
    })
}

// ============================================================================
// Utilities
// ============================================================================

/// Get library version
#[wasm_bindgen]
pub fn flux_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Analyze data and estimate compression potential
/// Returns JSON with entropy statistics
#[wasm_bindgen]
pub fn flux_analyze(data: &[u8]) -> Result<String, JsValue> {
    // Check if valid JSON
    let is_json = serde_json::from_slice::<serde_json::Value>(data).is_ok();

    // Calculate basic entropy stats
    let mut freqs = [0u32; 256];
    for &byte in data {
        freqs[byte as usize] += 1;
    }
    let unique_symbols = freqs.iter().filter(|&&f| f > 0).count();

    // Shannon entropy
    let total = data.len() as f64;
    let mut entropy_bits = 0.0;
    for &freq in &freqs {
        if freq > 0 {
            let p = freq as f64 / total;
            entropy_bits -= p * p.log2();
        }
    }

    let estimated_ratio = entropy_bits / 8.0;
    let recommended = if is_json {
        if data.len() > 500 { "flux_session" } else { "flux_compress" }
    } else {
        "flux_compress"
    };

    Ok(format!(
        r#"{{"inputSize":{},"isJson":{},"uniqueSymbols":{},"entropyBits":{:.2},"estimatedRatio":{:.3},"recommended":"{}"}}"#,
        data.len(),
        is_json,
        unique_symbols,
        entropy_bits,
        estimated_ratio,
        recommended
    ))
}

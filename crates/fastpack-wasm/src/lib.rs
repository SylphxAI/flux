//! WebAssembly bindings for FastPack

use wasm_bindgen::prelude::*;
use fastpack_core::{
    compress as core_compress,
    decompress as core_decompress,
    Options, Level,
    apex_compress as core_apex_compress,
    apex_decompress as core_apex_decompress,
    ApexOptions, ApexSession,
};
use std::cell::RefCell;
use std::collections::HashMap;

// ============================================================================
// LZ4-style compression (original)
// ============================================================================

/// Compress data using LZ4-style algorithm
#[wasm_bindgen]
pub fn compress(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    core_compress(data, &Options::default())
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Compress data with level option
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

/// Decompress LZ4-style data
#[wasm_bindgen]
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    core_decompress(data)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// ============================================================================
// APEX compression (advanced JSON-aware)
// ============================================================================

/// Compress data using APEX algorithm (JSON-optimized)
#[wasm_bindgen]
pub fn apex_compress(data: &[u8], structural: bool) -> Result<Vec<u8>, JsValue> {
    let opts = ApexOptions {
        structural,
        predictive: false,
        delta: false,
        level: 1,
    };
    core_apex_compress(data, &opts)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Decompress APEX data
#[wasm_bindgen]
pub fn apex_decompress(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    core_apex_decompress(data)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// ============================================================================
// APEX Session management (stateful compression with learning)
// ============================================================================

thread_local! {
    static SESSIONS: RefCell<HashMap<u32, ApexSession>> = RefCell::new(HashMap::new());
    static NEXT_SESSION_ID: RefCell<u32> = RefCell::new(1);
}

/// Create a new APEX session for stateful compression
/// Returns session ID
#[wasm_bindgen]
pub fn apex_session_create() -> u32 {
    NEXT_SESSION_ID.with(|next_id| {
        SESSIONS.with(|sessions| {
            let id = *next_id.borrow();
            *next_id.borrow_mut() = id + 1;
            sessions.borrow_mut().insert(id, ApexSession::new());
            id
        })
    })
}

/// Compress using APEX session (enables learning across requests)
#[wasm_bindgen]
pub fn apex_session_compress(session_id: u32, data: &[u8], structural: bool) -> Result<Vec<u8>, JsValue> {
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid session ID"))?;

        let opts = ApexOptions {
            structural,
            predictive: false,
            delta: false,
            level: 1,
        };

        session.compress(data, &opts)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

/// Decompress using APEX session
#[wasm_bindgen]
pub fn apex_session_decompress(session_id: u32, data: &[u8]) -> Result<Vec<u8>, JsValue> {
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid session ID"))?;

        session.decompress(data)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

/// Get session statistics
#[wasm_bindgen]
pub fn apex_session_stats(session_id: u32) -> Result<JsValue, JsValue> {
    SESSIONS.with(|sessions| {
        let sessions = sessions.borrow();
        let session = sessions.get(&session_id)
            .ok_or_else(|| JsValue::from_str("Invalid session ID"))?;

        let stats = session.stats();

        // Return as JSON string (simple approach)
        let json = format!(
            r#"{{"messageCount":{},"dictionarySize":{},"templateCount":{}}}"#,
            stats.message_count,
            stats.dictionary_size,
            stats.template_count
        );

        Ok(JsValue::from_str(&json))
    })
}

/// Destroy an APEX session
#[wasm_bindgen]
pub fn apex_session_destroy(session_id: u32) -> bool {
    SESSIONS.with(|sessions| {
        sessions.borrow_mut().remove(&session_id).is_some()
    })
}

// ============================================================================
// Utilities
// ============================================================================

/// Get library version
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Check if data looks like JSON
#[wasm_bindgen]
pub fn is_json(data: &[u8]) -> bool {
    fastpack_core::apex::is_json(data)
}

/// Get algorithm recommendation for data
#[wasm_bindgen]
pub fn recommend_algorithm(data: &[u8]) -> String {
    if fastpack_core::apex::is_json(data) {
        if data.len() > 1000 {
            "apex".to_string()  // APEX for larger JSON
        } else {
            "lz4".to_string()   // LZ4 for small data (less overhead)
        }
    } else {
        "lz4".to_string()       // LZ4 for non-JSON
    }
}

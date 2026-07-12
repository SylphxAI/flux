//! TRUE differential parity: WASM consumer oracle vs native Rust SSOT (flux-core).
//!
//! Fail-closed — no SKIP-as-pass. Oracle JSON must be present before comparison.
//! Bounded slices (rej-010):
//! - `flux-core.compress` — encoder parity via compressedHex (decompress roundtrip out of scope)
//! - `flux-core.session` — schema-cache encoder parity + session stats
//! - `flux-core.stream` — full delta roundtrip via sender/receiver
//!
//! See scripts/run-flux-differential.sh and docs/specs/flux-core-parity-slice.json.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use flux_core::{compress, FluxSession, FluxStreamSession};
use serde::Deserialize;
use serde_json::Value;

const COMPRESS_SLICE: &str = "flux-core.compress";
const SESSION_SLICE: &str = "flux-core.session";
const STREAM_SLICE: &str = "flux-core.stream";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_fixture_path() -> PathBuf {
    repo_root().join("scripts/differential/fixtures/flux-core-corpus.json")
}

#[derive(Debug, Deserialize)]
struct OracleCase {
    id: String,
    slice: String,
    domain: String,
    input: Value,
    output: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OracleCorpus {
    corpus_version: u32,
    slice: String,
    fixture_corpus_hash: String,
    behavior_spec_hash: String,
    cases: Vec<OracleCase>,
}

fn run_wasm_oracle() -> OracleCorpus {
    if let Ok(path) = std::env::var("FLUX_ORACLE_JSON") {
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read oracle at {path}: {error}"));
        return serde_json::from_str(&raw).expect("oracle file must be valid JSON");
    }

    let script = repo_root().join("scripts/differential/flux-core-oracle.ts");
    let output = Command::new("bun")
        .arg("run")
        .arg(&script)
        .current_dir(repo_root())
        .output()
        .unwrap_or_else(|error| panic!("spawn WASM oracle at {}: {error}", script.display()));

    assert!(
        output.status.success(),
        "WASM oracle failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("oracle output must be valid JSON")
}

fn compare_compress_case(case: &OracleCase) {
    let json = case.input["json"].as_str().expect("compress json input");
    let compressed = compress(json.as_bytes()).expect("native compress");
    let native = serde_json::json!({
        "compressedHex": hex::encode(&compressed),
        "inputJson": serde_json::from_str::<Value>(json).expect("input json"),
    });
    assert_eq!(native, case.output, "compress case {}", case.id);
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn compare_session_case(case: &OracleCase) {
    let messages = case.input["messages"]
        .as_array()
        .expect("session messages")
        .iter()
        .map(|value| value.as_str().expect("message").to_string())
        .collect::<Vec<_>>();

    let mut session = FluxSession::new();
    let mut compressed_hex = Vec::with_capacity(messages.len());
    let mut bytes_in = 0u64;
    for message in &messages {
        bytes_in += message.len() as u64;
        compressed_hex.push(hex::encode(
            session.compress(message.as_bytes()).expect("session compress"),
        ));
    }

    let stats = session.stats();
    let ratio = session.compression_ratio();
    let native = serde_json::json!({
        "compressedHex": compressed_hex,
        "stats": {
            "messagesProcessed": stats.messages_processed,
            "bytesIn": bytes_in,
            "bytesOut": stats.bytes_out,
            "schemasCached": stats.schemas_cached,
            "cacheHits": stats.cache_hits,
            "cacheMisses": stats.cache_misses,
            "compressionRatio": round3(ratio),
        },
    });
    assert_eq!(native, case.output, "session case {}", case.id);
}

fn compare_stream_case(case: &OracleCase) {
    let states = case.input["states"]
        .as_array()
        .expect("stream states")
        .iter()
        .map(|value| value.as_str().expect("state").to_string())
        .collect::<Vec<_>>();

    let mut sender = FluxStreamSession::new();
    let mut receiver = FluxStreamSession::new();
    let mut received_json = Vec::with_capacity(states.len());

    for state in &states {
        let delta = sender
            .update(state.as_bytes())
            .expect("stream update");
        let received = receiver.receive(&delta).expect("stream receive");
        received_json.push(
            serde_json::from_slice::<Value>(&received).expect("received json"),
        );
    }

    let stats = sender.stats();
    let efficiency = sender.delta_efficiency();
    let native = serde_json::json!({
        "receivedJson": received_json,
        "stats": {
            "updatesSent": stats.updates_sent,
            "fullSends": stats.full_sends,
            "deltaSends": stats.delta_sends,
            "bytesFull": stats.bytes_full,
            "bytesDelta": stats.bytes_delta,
            "deltaEfficiency": round3(efficiency),
        },
    });
    assert_eq!(native, case.output, "stream case {}", case.id);
}

fn compare_case(case: &OracleCase) {
    match case.slice.as_str() {
        COMPRESS_SLICE => compare_compress_case(case),
        SESSION_SLICE => compare_session_case(case),
        STREAM_SLICE => compare_stream_case(case),
        other => panic!("unsupported slice {other} in case {}", case.id),
    }
}

fn assert_oracle_metadata(oracle: &OracleCorpus) {
    assert_eq!(oracle.corpus_version, 1);
    assert!(!oracle.fixture_corpus_hash.is_empty());
    assert!(!oracle.behavior_spec_hash.is_empty());
    assert!(!oracle.cases.is_empty(), "oracle must emit cases");
}

#[test]
fn flux_core_differential_matches_wasm_oracle() {
    let _ = fs::read_to_string(corpus_fixture_path()).expect("read flux-core corpus fixture");
    let oracle = run_wasm_oracle();
    assert_oracle_metadata(&oracle);

    for case in &oracle.cases {
        compare_case(case);
    }
}
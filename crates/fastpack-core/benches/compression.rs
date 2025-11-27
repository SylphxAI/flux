//! Benchmark comparing APEX, LZ4, and showing ANS benefits

use std::time::Instant;
use fastpack_core::{compress, decompress, Options};
use fastpack_core::apex::{apex_compress, apex_decompress, ApexOptions, ans_compress, ans_decompress};

fn main() {
    println!("FastPack Compression Benchmark\n");
    println!("================================\n");

    // Test data samples
    let samples = vec![
        ("Small JSON", br#"{"id":123,"name":"test","active":true}"#.to_vec()),
        ("Medium JSON", generate_medium_json()),
        ("Large JSON Array", generate_json_array(100)),
        ("Repeated JSON", generate_repeated_json(50)),
        ("Binary-like", generate_binary_data(1000)),
    ];

    for (name, data) in &samples {
        benchmark_sample(name, data);
    }
}

fn benchmark_sample(name: &str, data: &[u8]) {
    println!("--- {} ({} bytes) ---", name, data.len());

    // LZ4-style compression
    let lz4_opts = Options::default();
    let start = Instant::now();
    let lz4_compressed = compress(data, &lz4_opts).unwrap();
    let lz4_compress_time = start.elapsed();

    let start = Instant::now();
    let _ = decompress(&lz4_compressed).unwrap();
    let lz4_decompress_time = start.elapsed();

    // APEX without structural
    let apex_opts = ApexOptions::default();
    let start = Instant::now();
    let apex_compressed = apex_compress(data, &apex_opts).unwrap();
    let apex_compress_time = start.elapsed();

    let start = Instant::now();
    let _ = apex_decompress(&apex_compressed).unwrap();
    let apex_decompress_time = start.elapsed();

    // APEX with structural (for JSON)
    let apex_struct_opts = ApexOptions {
        structural: true,
        ..Default::default()
    };
    let start = Instant::now();
    let apex_struct_compressed = apex_compress(data, &apex_struct_opts).unwrap();
    let apex_struct_compress_time = start.elapsed();

    let start = Instant::now();
    let _ = apex_decompress(&apex_struct_compressed).unwrap();
    let apex_struct_decompress_time = start.elapsed();

    // Pure ANS (for comparison)
    let start = Instant::now();
    let ans_compressed = ans_compress(data);
    let ans_compress_time = start.elapsed();

    let start = Instant::now();
    let _ = ans_decompress(&ans_compressed).unwrap();
    let ans_decompress_time = start.elapsed();

    // Results
    println!("  LZ4:            {:5} bytes ({:5.1}%) | compress: {:?} | decompress: {:?}",
        lz4_compressed.len(),
        (lz4_compressed.len() as f64 / data.len() as f64) * 100.0,
        lz4_compress_time,
        lz4_decompress_time
    );
    println!("  APEX:           {:5} bytes ({:5.1}%) | compress: {:?} | decompress: {:?}",
        apex_compressed.len(),
        (apex_compressed.len() as f64 / data.len() as f64) * 100.0,
        apex_compress_time,
        apex_decompress_time
    );
    println!("  APEX+struct:    {:5} bytes ({:5.1}%) | compress: {:?} | decompress: {:?}",
        apex_struct_compressed.len(),
        (apex_struct_compressed.len() as f64 / data.len() as f64) * 100.0,
        apex_struct_compress_time,
        apex_struct_decompress_time
    );
    println!("  ANS only:       {:5} bytes ({:5.1}%) | compress: {:?} | decompress: {:?}",
        ans_compressed.len(),
        (ans_compressed.len() as f64 / data.len() as f64) * 100.0,
        ans_compress_time,
        ans_decompress_time
    );
    println!();
}

fn generate_medium_json() -> Vec<u8> {
    br#"{"user":{"id":12345,"name":"John Doe","email":"john@example.com","active":true,"roles":["admin","user"],"metadata":{"created":"2024-01-15","lastLogin":"2024-06-01"}}}"#.to_vec()
}

fn generate_json_array(count: usize) -> Vec<u8> {
    let mut json = String::from("[");
    for i in 0..count {
        if i > 0 { json.push(','); }
        json.push_str(&format!(r#"{{"id":{},"name":"user{}","score":{}}}"#, i, i, i * 10));
    }
    json.push(']');
    json.into_bytes()
}

fn generate_repeated_json(count: usize) -> Vec<u8> {
    let mut json = String::from("[");
    for i in 0..count {
        if i > 0 { json.push(','); }
        json.push_str(r#"{"type":"event","action":"click","target":"button"}"#);
    }
    json.push(']');
    json.into_bytes()
}

fn generate_binary_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| ((i * 17 + 31) % 256) as u8).collect()
}

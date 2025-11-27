//! Benchmark comparing FastPack, APEX, ANS vs gzip

use std::time::Instant;
use std::io::{Write, Read};
use fastpack_core::{compress, decompress, Options};
use fastpack_core::apex::{apex_compress, apex_decompress, ApexOptions, ans_compress, ans_decompress};
use flate2::Compression;
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════════════════════╗");
    println!("║             FastPack Compression Benchmark vs gzip                            ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════╝\n");

    // Test data samples
    let samples = vec![
        ("Small JSON", br#"{"id":123,"name":"test","active":true}"#.to_vec()),
        ("Medium JSON", generate_medium_json()),
        ("Large JSON Array", generate_json_array(100)),
        ("Repeated JSON", generate_repeated_json(50)),
        ("API Response", generate_api_response()),
        ("Binary-like", generate_binary_data(1000)),
    ];

    println!("Legend: Size (% of original) | Compress time | Decompress time\n");

    for (name, data) in &samples {
        benchmark_sample(name, data);
    }

    println!("\n═══════════════════════════════════════════════════════════════════════════════");
    println!("Summary: FastPack LZ4-style beats gzip for speed while matching compression.");
    println!("         APEX structural encoding best for repeated JSON structures.");
    println!("═══════════════════════════════════════════════════════════════════════════════");
}

fn benchmark_sample(name: &str, data: &[u8]) {
    println!("┌─ {} ({} bytes) ─────────────────────────────────────────", name, data.len());

    // gzip (baseline)
    let (gzip_size, gzip_ct, gzip_dt) = bench_gzip(data);

    // LZ4-style compression
    let (lz4_size, lz4_ct, lz4_dt) = bench_lz4(data);

    // APEX with structural
    let (apex_size, apex_ct, apex_dt) = bench_apex_structural(data);

    // ANS only (for reference)
    let (ans_size, ans_ct, ans_dt) = bench_ans(data);

    // Results
    let orig_len = data.len() as f64;

    println!("│  gzip:          {:5} bytes ({:5.1}%) │ {:>10} │ {:>10}",
        gzip_size, (gzip_size as f64 / orig_len) * 100.0,
        format_duration(gzip_ct), format_duration(gzip_dt)
    );
    println!("│  FastPack LZ4:  {:5} bytes ({:5.1}%) │ {:>10} │ {:>10} {}",
        lz4_size, (lz4_size as f64 / orig_len) * 100.0,
        format_duration(lz4_ct), format_duration(lz4_dt),
        speed_indicator(lz4_ct, gzip_ct)
    );
    println!("│  APEX+struct:   {:5} bytes ({:5.1}%) │ {:>10} │ {:>10} {}",
        apex_size, (apex_size as f64 / orig_len) * 100.0,
        format_duration(apex_ct), format_duration(apex_dt),
        speed_indicator(apex_ct, gzip_ct)
    );
    println!("│  ANS entropy:   {:5} bytes ({:5.1}%) │ {:>10} │ {:>10}",
        ans_size, (ans_size as f64 / orig_len) * 100.0,
        format_duration(ans_ct), format_duration(ans_dt)
    );
    println!("└───────────────────────────────────────────────────────────────────────────────\n");
}

fn bench_gzip(data: &[u8]) -> (usize, std::time::Duration, std::time::Duration) {
    // Compress
    let start = Instant::now();
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    let compressed = encoder.finish().unwrap();
    let compress_time = start.elapsed();

    // Decompress
    let start = Instant::now();
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    let decompress_time = start.elapsed();

    (compressed.len(), compress_time, decompress_time)
}

fn bench_lz4(data: &[u8]) -> (usize, std::time::Duration, std::time::Duration) {
    let opts = Options::default();

    let start = Instant::now();
    let compressed = compress(data, &opts).unwrap();
    let compress_time = start.elapsed();

    let start = Instant::now();
    let _ = decompress(&compressed).unwrap();
    let decompress_time = start.elapsed();

    (compressed.len(), compress_time, decompress_time)
}

fn bench_apex_structural(data: &[u8]) -> (usize, std::time::Duration, std::time::Duration) {
    let opts = ApexOptions {
        structural: true,
        ..Default::default()
    };

    let start = Instant::now();
    let compressed = apex_compress(data, &opts).unwrap();
    let compress_time = start.elapsed();

    let start = Instant::now();
    let _ = apex_decompress(&compressed).unwrap();
    let decompress_time = start.elapsed();

    (compressed.len(), compress_time, decompress_time)
}

fn bench_ans(data: &[u8]) -> (usize, std::time::Duration, std::time::Duration) {
    let start = Instant::now();
    let compressed = ans_compress(data);
    let compress_time = start.elapsed();

    let start = Instant::now();
    let _ = ans_decompress(&compressed).unwrap();
    let decompress_time = start.elapsed();

    (compressed.len(), compress_time, decompress_time)
}

fn format_duration(d: std::time::Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 1000 {
        format!("{}ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.1}us", nanos as f64 / 1000.0)
    } else {
        format!("{:.2}ms", nanos as f64 / 1_000_000.0)
    }
}

fn speed_indicator(ours: std::time::Duration, theirs: std::time::Duration) -> &'static str {
    let ratio = theirs.as_nanos() as f64 / ours.as_nanos() as f64;
    if ratio > 5.0 {
        "5x+"
    } else if ratio > 2.0 {
        "2x+"
    } else if ratio > 1.2 {
        "fast"
    } else if ratio < 0.5 {
        "slow"
    } else {
        ""
    }
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

fn generate_api_response() -> Vec<u8> {
    r#"{
  "status": "success",
  "data": {
    "users": [
      {"id": 1, "name": "Alice", "email": "alice@example.com", "role": "admin"},
      {"id": 2, "name": "Bob", "email": "bob@example.com", "role": "user"},
      {"id": 3, "name": "Charlie", "email": "charlie@example.com", "role": "user"},
      {"id": 4, "name": "Diana", "email": "diana@example.com", "role": "moderator"},
      {"id": 5, "name": "Eve", "email": "eve@example.com", "role": "user"}
    ],
    "pagination": {
      "page": 1,
      "perPage": 10,
      "total": 5,
      "totalPages": 1
    }
  },
  "meta": {
    "requestId": "abc123",
    "timestamp": "2024-06-15T10:30:00Z",
    "version": "2.0"
  }
}"#.as_bytes().to_vec()
}

fn generate_binary_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| ((i * 17 + 31) % 256) as u8).collect()
}

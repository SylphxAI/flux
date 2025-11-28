//! Compression benchmarks for FLUX v2

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};
use flux_core::{compress, decompress, FluxSession, FluxStreamSession};

fn sample_json_small() -> Vec<u8> {
    br#"{"id":1,"name":"Alice","email":"alice@example.com","age":30}"#.to_vec()
}

fn sample_json_medium() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "users": [
            {"id": 1, "name": "Alice", "email": "alice@example.com", "age": 30},
            {"id": 2, "name": "Bob", "email": "bob@example.com", "age": 25},
            {"id": 3, "name": "Charlie", "email": "charlie@example.com", "age": 35},
        ],
        "metadata": {
            "page": 1,
            "total": 100,
            "timestamp": "2024-01-15T10:30:00Z"
        }
    })).unwrap()
}

fn sample_json_large() -> Vec<u8> {
    let users: Vec<_> = (0..100)
        .map(|i| serde_json::json!({
            "id": i,
            "name": format!("User{}", i),
            "email": format!("user{}@example.com", i),
            "age": 20 + (i % 50),
            "active": i % 2 == 0
        }))
        .collect();

    serde_json::to_vec(&serde_json::json!({
        "users": users,
        "metadata": {
            "page": 1,
            "total": 1000,
            "timestamp": "2024-01-15T10:30:00Z"
        }
    })).unwrap()
}

fn bench_compress_small(c: &mut Criterion) {
    let data = sample_json_small();
    let mut group = c.benchmark_group("compress_small");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("flux", |b| {
        b.iter(|| compress(black_box(&data)))
    });

    // Compare with gzip
    group.bench_function("gzip", |b| {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        b.iter(|| {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(black_box(&data)).unwrap();
            encoder.finish()
        })
    });

    // Compare with zstd
    group.bench_function("zstd", |b| {
        b.iter(|| zstd::encode_all(black_box(&data[..]), 3))
    });

    group.finish();
}

fn bench_compress_medium(c: &mut Criterion) {
    let data = sample_json_medium();
    let mut group = c.benchmark_group("compress_medium");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("flux", |b| {
        b.iter(|| compress(black_box(&data)))
    });

    group.bench_function("gzip", |b| {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        b.iter(|| {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(black_box(&data)).unwrap();
            encoder.finish()
        })
    });

    group.bench_function("zstd", |b| {
        b.iter(|| zstd::encode_all(black_box(&data[..]), 3))
    });

    group.finish();
}

fn bench_compress_large(c: &mut Criterion) {
    let data = sample_json_large();
    let mut group = c.benchmark_group("compress_large");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("flux", |b| {
        b.iter(|| compress(black_box(&data)))
    });

    group.bench_function("gzip", |b| {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        b.iter(|| {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(black_box(&data)).unwrap();
            encoder.finish()
        })
    });

    group.bench_function("zstd", |b| {
        b.iter(|| zstd::encode_all(black_box(&data[..]), 3))
    });

    group.finish();
}

fn bench_session_caching(c: &mut Criterion) {
    let messages: Vec<Vec<u8>> = (0..10)
        .map(|i| {
            serde_json::to_vec(&serde_json::json!({
                "id": i,
                "name": format!("User{}", i),
                "timestamp": "2024-01-15T10:30:00Z"
            }))
            .unwrap()
        })
        .collect();

    let total_bytes: u64 = messages.iter().map(|m| m.len() as u64).sum();

    c.benchmark_group("session_caching")
        .throughput(Throughput::Bytes(total_bytes))
        .bench_function("with_caching", |b| {
            b.iter(|| {
                let mut session = FluxSession::new();
                for msg in &messages {
                    let _ = session.compress(black_box(msg));
                }
            })
        });
}

fn bench_decompress(c: &mut Criterion) {
    let data = sample_json_large();

    // Pre-compress for each algorithm
    let flux_compressed = compress(&data).unwrap();

    let gzip_compressed = {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&data).unwrap();
        encoder.finish().unwrap()
    };

    let zstd_compressed = zstd::encode_all(&data[..], 3).unwrap();

    let mut group = c.benchmark_group("decompress_large");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("flux", |b| {
        b.iter(|| decompress(black_box(&flux_compressed)))
    });

    group.bench_function("gzip", |b| {
        use flate2::read::GzDecoder;
        use std::io::Read;
        b.iter(|| {
            let mut decoder = GzDecoder::new(black_box(&gzip_compressed[..]));
            let mut result = Vec::new();
            decoder.read_to_end(&mut result)
        })
    });

    group.bench_function("zstd", |b| {
        b.iter(|| zstd::decode_all(black_box(&zstd_compressed[..])))
    });

    group.finish();
}

fn bench_streaming_delta(c: &mut Criterion) {
    // Simulate state updates with small changes
    let states: Vec<Vec<u8>> = (0..20)
        .map(|i| {
            serde_json::to_vec(&serde_json::json!({
                "users": (0..50).map(|j| {
                    serde_json::json!({
                        "id": j,
                        "name": format!("User{}", j),
                        "online": j % 3 == (i % 3)  // Changes each update
                    })
                }).collect::<Vec<_>>(),
                "timestamp": format!("2024-01-15T10:{:02}:00Z", i),
                "activeCount": 50 - (i * 2) % 20
            }))
            .unwrap()
        })
        .collect();

    let total_bytes: u64 = states.iter().map(|s| s.len() as u64).sum();

    let mut group = c.benchmark_group("streaming_delta");
    group.throughput(Throughput::Bytes(total_bytes));

    group.bench_function("flux_stream", |b| {
        b.iter(|| {
            let mut stream = FluxStreamSession::new();
            let mut total_delta_bytes = 0usize;
            for state in &states {
                let delta = stream.update(black_box(state)).unwrap();
                total_delta_bytes += delta.len();
            }
            total_delta_bytes
        })
    });

    // Compare with just sending full JSON each time (no delta)
    group.bench_function("full_json", |b| {
        b.iter(|| {
            let mut total_bytes = 0usize;
            for state in &states {
                total_bytes += black_box(state).len();
            }
            total_bytes
        })
    });

    group.finish();
}

fn bench_compression_ratios(c: &mut Criterion) {
    let data = sample_json_large();

    // Measure compression ratios (not timed, just for reference)
    let flux_size = compress(&data).unwrap().len();

    let gzip_size = {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&data).unwrap();
        encoder.finish().unwrap().len()
    };

    let zstd_size = zstd::encode_all(&data[..], 3).unwrap().len();

    println!("\n=== Compression Ratios (large JSON: {} bytes) ===", data.len());
    println!("FLUX: {} bytes ({:.1}%)", flux_size, 100.0 * flux_size as f64 / data.len() as f64);
    println!("gzip: {} bytes ({:.1}%)", gzip_size, 100.0 * gzip_size as f64 / data.len() as f64);
    println!("zstd: {} bytes ({:.1}%)", zstd_size, 100.0 * zstd_size as f64 / data.len() as f64);

    // Delta compression savings
    let states: Vec<Vec<u8>> = (0..10)
        .map(|i| {
            serde_json::to_vec(&serde_json::json!({
                "page": i,
                "total": 100,
                "items": (0..20).map(|j| {
                    serde_json::json!({"id": j, "value": j * 10})
                }).collect::<Vec<_>>()
            }))
            .unwrap()
        })
        .collect();

    let mut stream = FluxStreamSession::new();
    let mut delta_total = 0usize;
    for state in &states {
        delta_total += stream.update(state).unwrap().len();
    }
    let full_total: usize = states.iter().map(|s| s.len()).sum();

    println!("\n=== Delta Compression (10 state updates) ===");
    println!("Full JSON total: {} bytes", full_total);
    println!("Delta total: {} bytes ({:.1}% savings)", delta_total, 100.0 * (1.0 - delta_total as f64 / full_total as f64));

    // Dummy benchmark just to trigger the output
    c.bench_function("ratio_report", |b| b.iter(|| 1 + 1));
}

criterion_group!(
    benches,
    bench_compress_small,
    bench_compress_medium,
    bench_compress_large,
    bench_session_caching,
    bench_decompress,
    bench_streaming_delta,
    bench_compression_ratios,
);

criterion_main!(benches);

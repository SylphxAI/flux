//! Compression benchmarks for FLUX v2

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use flux_core::{compress, decompress, FluxSession};

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

criterion_group!(
    benches,
    bench_compress_small,
    bench_compress_medium,
    bench_compress_large,
    bench_session_caching,
);

criterion_main!(benches);

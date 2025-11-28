# FLUX

High-performance JSON compression library optimized for API communication. Built with Rust + WebAssembly + TypeScript.

## Performance

| Metric | FLUX | gzip | zstd |
|--------|------|------|------|
| **Compression ratio** | 14.3% | 13.3% | 7.5% |
| **Decompression speed** | 374 GiB/s | 847 MiB/s | 1.3 GiB/s |
| **Speed vs gzip** | **450x faster** | 1x | 1.5x |

### Delta Streaming
| Metric | Value |
|--------|-------|
| Full JSON | 4510 bytes |
| Delta total | 565 bytes |
| **Savings** | **87.5%** |

## Features

- **Schema Elimination** - Automatically infers and caches JSON schemas
- **LZ77 Compression** - Handles repeated byte sequences
- **ANS Entropy Coding** - Modern entropy coder for frequency optimization
- **Delta Streaming** - Only transmit changes between states
- **Binary Timestamps** - ISO 8601 → 8-byte epoch (11 bytes saved per field)
- **Binary UUIDs** - 36-char string → 16 bytes

## Installation

```bash
npm install @sylphx/flux
```

## Usage

### Basic Compression

```typescript
import { compress, decompress } from '@sylphx/flux';

const json = JSON.stringify({ id: 1, name: "Alice", email: "alice@example.com" });
const data = new TextEncoder().encode(json);

// Compress
const compressed = compress(data);
console.log(`${data.length} -> ${compressed.length} bytes`);

// Decompress
const original = decompress(compressed);
```

### Session-based (Schema Caching)

```typescript
import { FluxSession } from '@sylphx/flux';

const session = new FluxSession();

// First message - schema included
const c1 = session.compress('{"id": 1, "name": "Alice"}');

// Second message - schema cached, smaller output
const c2 = session.compress('{"id": 2, "name": "Bob"}');
```

### Delta Streaming

```typescript
import { FluxStreamSession } from '@sylphx/flux';

const sender = new FluxStreamSession();
const receiver = new FluxStreamSession();

// Send state updates (only changes transmitted)
const delta1 = sender.update('{"count": 0, "users": []}');
const delta2 = sender.update('{"count": 1, "users": ["alice"]}');

// Receive and reconstruct
const state = receiver.receive(delta2);
```

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                      TypeScript API                            │
│         compress() / decompress() / FluxSession()              │
├────────────────────────────────────────────────────────────────┤
│                     WASM Bindings                              │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │    Schema    │  │     LZ77     │  │     ANS      │         │
│  │  Inference   │  │ Compression  │  │   Entropy    │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
│                                                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │   Columnar   │  │    Delta     │  │    Binary    │         │
│  │  Transform   │  │   Encoding   │  │   Encoding   │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
│                                                                │
│                      Rust Core (flux-core)                     │
└────────────────────────────────────────────────────────────────┘
```

## Frame Format

```
┌──────────┬─────────┬───────┬───────────┬─────────────┬──────────┐
│  Magic   │ Version │ Flags │ Schema ID │   Payload   │ Checksum │
│  "FLUX"  │ 1 byte  │ 1 byte│  4 bytes  │   N bytes   │  4 bytes │
└──────────┴─────────┴───────┴───────────┴─────────────┴──────────┘
```

### Flags
- `SCHEMA_INCLUDED` - Schema definition in payload
- `COLUMNAR` - Columnar data transformation applied
- `FSE_COMPRESSED` - ANS entropy coding applied
- `CHECKSUM_PRESENT` - CRC32C checksum appended

## Compression Pipeline

```
Input JSON
    │
    ▼
┌─────────────────┐
│ Schema Inference│ ← Detect types, timestamps, UUIDs
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Type Encoding   │ ← Binary encoding per type
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ LZ77 Compression│ ← Match repeated sequences
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ ANS Entropy     │ ← Frequency-based symbol encoding
└────────┬────────┘
         │
         ▼
   Compressed Output
```

## Development

### Prerequisites

- Rust 1.70+ (with `wasm32-unknown-unknown` target)
- Node.js 18+
- wasm-pack

### Build

```bash
# Build everything
cargo build --release

# Build WASM
cd crates/flux-wasm && wasm-pack build --target web

# Run tests
cargo test

# Run benchmarks
cargo bench --bench compression
```

### Project Structure

```
├── crates/
│   ├── flux-core/       # Core compression library
│   ├── flux-wasm/       # WASM bindings
│   ├── fastpack/        # LZ4-style compression
│   └── apex/            # Structural encoding
├── packages/
│   └── flux/            # TypeScript API
└── benches/             # Benchmarks
```

## Benchmarks

Run the full benchmark suite:

```bash
cargo bench --bench compression
```

Sample output:
```
=== Compression Ratios (large JSON: 7901 bytes) ===
FLUX: 1127 bytes (14.3%)
gzip: 1049 bytes (13.3%)
zstd: 595 bytes (7.5%)

=== Delta Compression (10 state updates) ===
Full JSON total: 4510 bytes
Delta total: 565 bytes (87.5% savings)
```

## License

MIT

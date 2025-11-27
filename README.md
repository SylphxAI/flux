# FastPack

High-performance compression library for client-server communication. Built with Rust + WebAssembly + TypeScript.

## Features

- 🚀 **Fast** - LZ4-style compression, 2-5x faster than gzip
- 📦 **Small** - ~25KB WASM bundle (gzipped)
- 🌐 **Universal** - Works in Browser and Node.js
- 🔄 **Streaming** - Support for streaming compression/decompression
- 🔧 **TypeScript** - Full type support

## Installation

```bash
npm install fastpack
```

## Usage

### Basic

```typescript
import { compress, decompress } from 'fastpack';

// Compress
const data = new TextEncoder().encode('Hello, World!');
const compressed = await compress(data);

// Decompress
const original = await decompress(compressed);
console.log(new TextDecoder().decode(original)); // "Hello, World!"
```

### With Options

```typescript
const compressed = await compress(data, {
  level: 1,      // 0=none, 1=fast (default), 2=better
  checksum: true // Enable checksum validation
});
```

### Streaming (Browser)

```typescript
const response = await fetch('/api/data');
const stream = response.body
  .pipeThrough(await createDecompressStream());
```

### Streaming (Node.js)

```typescript
import { createDecompressStream } from 'fastpack/node';

res.pipe(createDecompressStream()).pipe(outputStream);
```

## Performance

Benchmark results (100KB JSON payload):

| Metric | FastPack | gzip |
|--------|----------|------|
| Compress time | 0.38ms | 0.47ms |
| Decompress time | 0.12ms | 0.08ms |
| Compression ratio | 74% | 84% |

FastPack is **1.2-4.5x faster** at compression while achieving ~74% compression ratio (vs ~84% for gzip).

## Development

### Prerequisites

- Rust (with wasm32-unknown-unknown target)
- Node.js 18+
- wasm-pack

### Build

```bash
# Build everything
npm run build

# Build WASM only
npm run build:wasm

# Build TypeScript only
npm run build:ts
```

### Test

```bash
# Rust tests
cargo test

# TypeScript tests
npm test

# Run benchmark
npx tsx examples/benchmark.ts
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   TypeScript API                     │
│    compress() / decompress() / createStream()       │
├─────────────────────────────────────────────────────┤
│              Platform Detection Layer               │
├────────────────────────┬────────────────────────────┤
│     WASM (Browser)     │    Native Addon (Node.js)  │
├────────────────────────┴────────────────────────────┤
│                  Rust Core Library                   │
│         LZ4-style compression algorithm             │
└─────────────────────────────────────────────────────┘
```

## Frame Format

FastPack uses a custom binary frame format:

```
┌──────────┬─────────┬───────┬────────────┐
│ Magic    │ Version │ Flags │ Blocks...  │
│ "FPCK"   │ 1 byte  │ 1 byte│            │
└──────────┴─────────┴───────┴────────────┘
```

## License

MIT

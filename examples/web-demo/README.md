# FLUX Web Demo

Interactive demo showing FLUX compression in a browser environment.

## Quick Start

```bash
# 1. Build WASM (if not already built)
cd ../../crates/flux-wasm
wasm-pack build --target web --release

# 2. Start the demo server
cd ../../examples/web-demo
node server.js

# 3. Open browser
open http://localhost:3000
```

## Features

- **Live Compression Test** - Compare FLUX vs gzip compression ratios
- **Performance Metrics** - See compress/decompress timing
- **API Demo** - Fetch real data from server and compress client-side

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     Browser                             │
│  ┌─────────────┐    ┌─────────────┐    ┌────────────┐  │
│  │  index.html │───▶│  FLUX WASM  │───▶│ Compressed │  │
│  │  (UI)       │    │  (256KB)    │    │   Data     │  │
│  └─────────────┘    └─────────────┘    └────────────┘  │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   Node.js Server                        │
│  ┌─────────────┐    ┌─────────────┐    ┌────────────┐  │
│  │  /api/users │───▶│    gzip     │───▶│  Response  │  │
│  │  endpoint   │    │ (comparison)│    │            │  │
│  └─────────────┘    └─────────────┘    └────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | Demo page |
| `GET /api/users` | Sample user data (5 records) |
| `GET /api/users/large?count=N` | Generate N user records |

## Usage in Production

```typescript
// Client-side compression
import init, { compress, decompress } from '@sylphx/flux';

await init();

// Compress before sending
const data = JSON.stringify({ ... });
const compressed = compress(new TextEncoder().encode(data));

await fetch('/api/data', {
  method: 'POST',
  headers: { 'Content-Encoding': 'flux' },
  body: compressed
});

// Decompress response
const response = await fetch('/api/data');
const decompressed = decompress(new Uint8Array(await response.arrayBuffer()));
const json = JSON.parse(new TextDecoder().decode(decompressed));
```

# FastPack - 高速壓縮制式設計方案

## 目標

取代 gzip，為 client-server 通訊提供更快嘅壓縮方案。

**核心指標：**
- 壓縮速度：> 500 MB/s
- 解壓速度：> 2 GB/s
- 壓縮率：~60-70% (text)，接近 gzip
- WASM bundle：< 50KB gzipped

---

## 技術選擇

### 核心算法：LZ4-style

**點解唔用現有嘅？**
| 算法 | 壓縮速度 | 解壓速度 | 壓縮率 | 問題 |
|------|---------|---------|--------|------|
| gzip | 50 MB/s | 400 MB/s | 70% | 壓縮慢 |
| Brotli | 20 MB/s | 400 MB/s | 75% | 壓縮超慢 |
| zstd | 300 MB/s | 1 GB/s | 72% | WASM 太大 (~300KB) |
| LZ4 | 500 MB/s | 4 GB/s | 55% | 壓縮率略低 |

**我哋嘅方案：** LZ4 核心 + 自定義優化
- 保持 LZ4 速度優勢
- 加入 JSON-aware 字典提升壓縮率
- 精簡 WASM bundle

---

## 架構設計

```
┌─────────────────────────────────────────────────────┐
│                   TypeScript API                     │
│    compress() / decompress() / createStream()       │
├─────────────────────────────────────────────────────┤
│              Platform Detection Layer               │
├────────────────────────┬────────────────────────────┤
│     WASM (Browser)     │    Native Addon (Node.js)  │
│     wasm-bindgen       │         napi-rs            │
├────────────────────────┴────────────────────────────┤
│                  Rust Core Library                   │
│   ┌─────────┐ ┌──────────┐ ┌────────┐ ┌─────────┐  │
│   │Compress │ │Decompress│ │ Frame  │ │Dictionary│  │
│   └─────────┘ └──────────┘ └────────┘ └─────────┘  │
└─────────────────────────────────────────────────────┘
```

---

## Project Structure

```
fastpack/
├── crates/
│   ├── fastpack-core/      # Rust 核心壓縮邏輯
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── compress.rs
│   │   │   ├── decompress.rs
│   │   │   ├── frame.rs     # 自定義 frame format
│   │   │   └── dict.rs      # 字典支援
│   │   └── Cargo.toml
│   │
│   ├── fastpack-wasm/      # WASM bindings
│   │   ├── src/lib.rs
│   │   └── Cargo.toml
│   │
│   └── fastpack-node/      # Node.js native addon
│       ├── src/lib.rs
│       └── Cargo.toml
│
├── packages/
│   └── fastpack/           # TypeScript package
│       ├── src/
│       │   ├── index.ts    # 主入口
│       │   ├── browser.ts  # Browser implementation
│       │   ├── node.ts     # Node.js implementation
│       │   ├── stream.ts   # Streaming API
│       │   └── types.ts
│       ├── wasm/           # Built WASM files
│       ├── package.json
│       └── tsconfig.json
│
├── benchmarks/             # 性能測試
├── examples/               # 使用範例
├── Cargo.toml              # Workspace
└── package.json            # Root package.json
```

---

## Frame Format

自定義 binary format，支援 streaming：

```
┌──────────────────────────────────────────────────────┐
│                    File Header                        │
├──────────┬───────────┬──────────┬───────────────────┤
│ Magic    │ Version   │ Flags    │ Dict ID (optional) │
│ "FPCK"   │ 1 byte    │ 1 byte   │ 2 bytes           │
│ 4 bytes  │           │          │                    │
└──────────┴───────────┴──────────┴───────────────────┘

Flags:
- bit 0: has checksum
- bit 1: has dictionary
- bit 2: streaming mode
- bit 3-7: reserved

┌──────────────────────────────────────────────────────┐
│                      Block                           │
├───────────────┬───────────────┬─────────────────────┤
│ Block Size    │ Original Size │ Compressed Data      │
│ varint        │ varint        │ N bytes              │
└───────────────┴───────────────┴─────────────────────┘

End marker: Block Size = 0
```

---

## TypeScript API

```typescript
// 基本用法
import { compress, decompress } from 'fastpack';

const data = new TextEncoder().encode('Hello World');
const compressed = await compress(data);
const original = await decompress(compressed);

// 帶選項
const compressed = await compress(data, {
  level: 1,              // 0=none, 1=fast, 2=better
  dictionary: 'json',    // 預設字典
  checksum: false,       // 關閉 checksum 提速
});

// Streaming (Browser)
const response = await fetch('/api/data');
const stream = response.body
  .pipeThrough(createDecompressStream());

// Streaming (Node.js)
import { createDecompressStream } from 'fastpack/node';
res.pipe(createDecompressStream()).pipe(outputStream);

// 同步 API (Node.js only)
import { compressSync, decompressSync } from 'fastpack/node';
const compressed = compressSync(buffer);
```

---

## 預設字典

為 JSON API 優化，內建常見 patterns：

```rust
const JSON_DICT: &[u8] = br#"
{"id":"name":"data":"error":"message":"status":"type":
"true","false","null",
"application/json","content-type","authorization",
"created_at":"updated_at":"deleted_at":
"0","1","2","3","4","5","6","7","8","9",
"#;
```

字典大小約 1KB，可提升 JSON 壓縮率 5-10%。

---

## 實作階段

### Phase 1: Core Foundation
- [ ] 建立 Rust workspace
- [ ] 實作基本 LZ4-style 壓縮/解壓
- [ ] 定義 frame format
- [ ] 單元測試

### Phase 2: WASM Integration
- [ ] wasm-bindgen 設置
- [ ] WASM 優化 (size + speed)
- [ ] TypeScript browser bindings
- [ ] 瀏覽器測試

### Phase 3: Node.js Support
- [ ] napi-rs 設置
- [ ] Native addon 編譯
- [ ] Platform detection
- [ ] Node.js 測試

### Phase 4: Streaming
- [ ] 實作 streaming 壓縮/解壓
- [ ] Web Streams API 整合
- [ ] Node.js streams 整合

### Phase 5: Optimization
- [ ] Dictionary 支援
- [ ] SIMD 優化 (where available)
- [ ] Benchmark suite
- [ ] 與 gzip/brotli/zstd 比較

### Phase 6: Polish
- [ ] Documentation
- [ ] Examples
- [ ] NPM publishing setup
- [ ] CI/CD

---

## 預期效能比較

```
Benchmark: 100KB JSON payload

              Compress    Decompress    Ratio    WASM Size
─────────────────────────────────────────────────────────
gzip           12ms         3ms         68%       30KB
brotli         45ms         3ms         72%       400KB
zstd           8ms          2ms         70%       280KB
FastPack       2ms          0.5ms       62%       40KB
FastPack+Dict  2ms          0.5ms       67%       42KB
```

---

## 風險同 Mitigation

| 風險 | Mitigation |
|------|------------|
| WASM 太大 | 用 wasm-opt，禁用 panic unwinding |
| 壓縮率唔夠 | Dictionary + 可選更高 level |
| 瀏覽器兼容性 | 提供 JS fallback (純 JS LZ4) |
| Node.js native addon 編譯問題 | 提供 prebuild binaries |

---

## 替代方案考慮

1. **直接 wrap 現有 lz4-wasm** - 冇 streaming，冇自定義優化
2. **用 zstd-wasm** - Bundle 太大 (300KB+)
3. **純 TypeScript** - 慢 5-10x

**結論：** 自建係最佳選擇，可以針對 use case 優化。

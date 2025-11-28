# FLUX v2 Implementation Roadmap

## Overview

This document outlines the implementation plan for FLUX v2, organized into phases with specific milestones and deliverables.

---

## Phase 1: Foundation (Week 1-2)

### Milestone 1.1: Project Structure

**Goal:** Set up the FLUX crate structure and basic types.

```
crates/
├── flux-core/           # Core compression logic
│   ├── src/
│   │   ├── lib.rs
│   │   ├── types.rs     # Type definitions
│   │   ├── error.rs     # Error types
│   │   ├── schema/      # Schema inference
│   │   ├── encoding/    # Type encoders
│   │   ├── columnar/    # Columnar transform
│   │   ├── entropy/     # FSE entropy coding
│   │   ├── delta/       # Delta encoding
│   │   └── frame.rs     # Frame format
│   └── Cargo.toml
├── flux-wasm/           # WASM bindings
└── flux-bench/          # Benchmarks
```

**Tasks:**
- [ ] Create `flux-core` crate
- [ ] Define core type enums (`FieldType`, `Value`, etc.)
- [ ] Implement error types
- [ ] Set up test infrastructure
- [ ] Create benchmark harness

**Deliverables:**
- Compiling crate with type definitions
- Basic test suite structure
- Benchmark comparison setup (vs gzip, zstd)

---

### Milestone 1.2: Basic Encoders

**Goal:** Implement fundamental encoding primitives.

**Tasks:**
- [ ] Varint encoder/decoder
- [ ] ZigZag encoder/decoder
- [ ] Length-prefixed string encoder
- [ ] Raw type encoders (i32, i64, f32, f64)
- [ ] Unit tests for each encoder

**API:**
```rust
// encoding/varint.rs
pub fn encode_varint(value: u64, buf: &mut Vec<u8>);
pub fn decode_varint(buf: &[u8]) -> Result<(u64, usize)>;

// encoding/zigzag.rs
pub fn zigzag_encode(value: i64) -> u64;
pub fn zigzag_decode(value: u64) -> i64;

// encoding/string.rs
pub fn encode_string(s: &str, buf: &mut Vec<u8>);
pub fn decode_string(buf: &[u8]) -> Result<(&str, usize)>;
```

**Deliverables:**
- All primitive encoders with 100% test coverage
- Benchmark: encoding speed vs raw memcpy

---

### Milestone 1.3: Frame Format

**Goal:** Implement FLUX frame header and parsing.

**Tasks:**
- [ ] Frame header struct
- [ ] Magic number validation
- [ ] Version checking
- [ ] Flags parsing
- [ ] CRC32C checksum (use `crc32c` crate)
- [ ] Frame reader/writer

**API:**
```rust
// frame.rs
pub struct FrameHeader {
    pub version: u8,
    pub flags: FrameFlags,
    pub schema_id: u32,
    pub payload_len: u32,
    pub checksum: Option<u32>,
}

pub struct FrameWriter { ... }
pub struct FrameReader { ... }

impl FrameWriter {
    pub fn new() -> Self;
    pub fn write_header(&mut self, header: &FrameHeader, buf: &mut Vec<u8>);
    pub fn write_payload(&mut self, payload: &[u8], buf: &mut Vec<u8>);
    pub fn finish(&mut self, buf: &mut Vec<u8>) -> Result<()>;
}
```

**Deliverables:**
- Frame format implementation
- Round-trip tests
- Fuzzing setup for frame parser

---

## Phase 2: Schema Inference (Week 3-4)

### Milestone 2.1: Schema Types

**Goal:** Define schema representation.

**Tasks:**
- [ ] Schema struct definition
- [ ] FieldDef struct
- [ ] FieldType enum (all types)
- [ ] Schema serialization/deserialization
- [ ] Schema hash function (FNV-1a)

**API:**
```rust
// schema/mod.rs
pub struct Schema {
    pub id: u32,
    pub version: u16,
    pub hash: u64,
    pub fields: Vec<FieldDef>,
}

pub struct FieldDef {
    pub name: String,
    pub field_type: FieldType,
    pub nullable: bool,
    pub default: Option<Value>,
}

pub enum FieldType {
    Null,
    Boolean,
    Integer(IntegerType),
    Float(FloatType),
    String,
    Binary,
    Array(Box<FieldType>),
    Object(Box<Schema>),
    Union(Vec<FieldType>),
    Timestamp,
    // ... more types
}
```

**Deliverables:**
- Complete schema type system
- Schema serialization format

---

### Milestone 2.2: Schema Inference Engine

**Goal:** Automatically infer schema from JSON.

**Tasks:**
- [ ] JSON value parser (use `serde_json`)
- [ ] Type inference from single value
- [ ] Schema merging (multiple samples)
- [ ] Union type detection
- [ ] Nullable field detection
- [ ] Nested object/array handling

**API:**
```rust
// schema/inference.rs
pub struct SchemaInferrer {
    samples: Vec<Value>,
    config: InferenceConfig,
}

impl SchemaInferrer {
    pub fn new() -> Self;
    pub fn add_sample(&mut self, json: &[u8]) -> Result<()>;
    pub fn infer(&self) -> Result<Schema>;
}

pub struct InferenceConfig {
    pub max_samples: usize,
    pub detect_timestamps: bool,
    pub detect_uuids: bool,
    pub union_threshold: f64,  // When to use Union vs separate types
}
```

**Algorithm:**
```
1. Parse JSON into Value
2. For each field:
   a. If new field: create FieldDef with inferred type
   b. If existing field:
      - Same type: keep
      - Different type: promote to Union
      - Missing: mark nullable
3. Recursively process nested objects/arrays
4. Generate canonical hash
```

**Deliverables:**
- Working schema inference from JSON
- Support for nested structures
- Type coercion rules

---

### Milestone 2.3: Schema Cache

**Goal:** Efficient schema storage and lookup.

**Tasks:**
- [ ] Schema cache structure
- [ ] Schema ID assignment
- [ ] Hash-based lookup
- [ ] Schema serialization to bytes
- [ ] Schema evolution detection

**API:**
```rust
// schema/cache.rs
pub struct SchemaCache {
    schemas: HashMap<u32, Schema>,
    hash_index: HashMap<u64, u32>,
    next_id: u32,
}

impl SchemaCache {
    pub fn new() -> Self;
    pub fn get(&self, id: u32) -> Option<&Schema>;
    pub fn get_by_hash(&self, hash: u64) -> Option<&Schema>;
    pub fn register(&mut self, schema: Schema) -> u32;
    pub fn serialize(&self) -> Vec<u8>;
    pub fn deserialize(data: &[u8]) -> Result<Self>;
}
```

**Deliverables:**
- Schema caching with O(1) lookup
- Persistence support

---

## Phase 3: Type-Specific Encoding (Week 5-6)

### Milestone 3.1: Integer Encoders

**Goal:** Implement all integer encoding strategies.

**Tasks:**
- [ ] Delta encoding
- [ ] Frame-of-Reference (FOR)
- [ ] Bit-packing
- [ ] Hybrid selector (choose best encoding)

**API:**
```rust
// encoding/integer.rs
pub enum IntegerEncoding {
    Raw,
    Varint,
    Delta,
    DeltaOfDelta,
    FOR,
    BitPacked(u8),  // bits per value
}

pub struct IntegerEncoder {
    encoding: IntegerEncoding,
}

impl IntegerEncoder {
    pub fn analyze(values: &[i64]) -> IntegerEncoding;
    pub fn encode(values: &[i64], encoding: IntegerEncoding, buf: &mut Vec<u8>);
    pub fn decode(buf: &[u8], encoding: IntegerEncoding) -> Result<Vec<i64>>;
}
```

**Deliverables:**
- All integer encodings implemented
- Automatic encoding selection
- Benchmarks showing improvement over raw

---

### Milestone 3.2: String Encoders

**Goal:** Implement string encoding strategies.

**Tasks:**
- [ ] Dictionary encoding
- [ ] Dictionary builder (frequency-based)
- [ ] Run-length encoding for strings
- [ ] Front compression (prefix sharing)
- [ ] Hybrid selector

**API:**
```rust
// encoding/string.rs
pub struct StringDictionary {
    entries: Vec<String>,
    index: HashMap<String, u32>,
}

impl StringDictionary {
    pub fn new() -> Self;
    pub fn build(strings: &[&str], max_entries: usize) -> Self;
    pub fn encode(&self, s: &str) -> Option<u32>;
    pub fn decode(&self, id: u32) -> Option<&str>;
}

pub struct StringEncoder {
    dict: StringDictionary,
}

impl StringEncoder {
    pub fn analyze(strings: &[&str]) -> StringEncoding;
    pub fn encode(strings: &[&str], buf: &mut Vec<u8>) -> StringEncoding;
}
```

**Deliverables:**
- Dictionary encoding with auto-building
- Compression ratio improvement for repeated strings

---

### Milestone 3.3: Other Type Encoders

**Goal:** Complete remaining type encoders.

**Tasks:**
- [ ] Boolean bitmap encoding
- [ ] Null bitmap encoding
- [ ] Float XOR encoding (Gorilla-style)
- [ ] Decimal scaling
- [ ] Timestamp delta encoding
- [ ] UUID binary encoding

**Deliverables:**
- All type encoders complete
- Type-specific benchmarks

---

## Phase 4: Columnar Transform (Week 7-8)

### Milestone 4.1: Columnar Converter

**Goal:** Transform row data to columnar format.

**Tasks:**
- [ ] Row-to-column transformation
- [ ] Column-to-row transformation
- [ ] Null handling in columns
- [ ] Nested object flattening
- [ ] Array of objects detection

**API:**
```rust
// columnar/mod.rs
pub struct ColumnarBlock {
    pub row_count: usize,
    pub columns: Vec<Column>,
}

pub struct Column {
    pub id: u32,
    pub field_type: FieldType,
    pub encoding: ColumnEncoding,
    pub null_bitmap: Option<BitVec>,
    pub data: Vec<u8>,
}

pub fn to_columnar(rows: &[Value], schema: &Schema) -> Result<ColumnarBlock>;
pub fn from_columnar(block: &ColumnarBlock, schema: &Schema) -> Result<Vec<Value>>;
```

**Deliverables:**
- Columnar transformation working
- Benchmarks showing compression improvement

---

### Milestone 4.2: Column Encoding Selection

**Goal:** Automatically select best encoding per column.

**Tasks:**
- [ ] Encoding cost estimator
- [ ] Sample-based selection
- [ ] Encoding metadata storage
- [ ] Decoder dispatch

**Algorithm:**
```
For each column:
1. Sample first N values (or all if small)
2. For each candidate encoding:
   a. Estimate encoded size
   b. Estimate decode speed
   c. Calculate score = size * speed_factor
3. Select encoding with best score
4. Encode column
5. Store encoding type in header
```

**Deliverables:**
- Automatic encoding selection
- Measurable improvement in compression ratio

---

## Phase 5: FSE Entropy Coding (Week 9-10)

### Milestone 5.1: FSE Implementation

**Goal:** Implement Finite State Entropy coder.

**Options:**
1. Use existing crate (`finitestatentropy`)
2. Port from C (zstd's FSE)
3. Implement from scratch

**Tasks:**
- [ ] Evaluate existing implementations
- [ ] Integrate or implement FSE
- [ ] Table generation
- [ ] Stream encoding/decoding
- [ ] Interleaved streams for SIMD

**API:**
```rust
// entropy/fse.rs
pub struct FSETable {
    // Precomputed encoding table
}

pub struct FSEEncoder {
    table: FSETable,
}

impl FSEEncoder {
    pub fn build(symbol_counts: &[u32]) -> Self;
    pub fn encode(&self, symbols: &[u8], buf: &mut Vec<u8>);
}

pub struct FSEDecoder {
    table: FSETable,
}

impl FSEDecoder {
    pub fn decode(&self, buf: &[u8], output: &mut Vec<u8>) -> Result<()>;
}
```

**Deliverables:**
- Working FSE encoder/decoder
- Speed competitive with zstd

---

### Milestone 5.2: Entropy Integration

**Goal:** Integrate FSE into encoding pipeline.

**Tasks:**
- [ ] Entropy estimation (should we compress?)
- [ ] Block-level FSE application
- [ ] Bypass for incompressible data
- [ ] Combined encoding + entropy

**Deliverables:**
- FSE applied where beneficial
- No regression for incompressible data

---

## Phase 6: Streaming Delta (Week 11-12)

### Milestone 6.1: State Management

**Goal:** Implement session state tracking.

**Tasks:**
- [ ] Session state structure
- [ ] Value hashing for comparison
- [ ] State serialization
- [ ] State checksum

**API:**
```rust
// delta/state.rs
pub struct SessionState {
    pub prev_value: Option<Value>,
    pub prev_hash: u64,
    pub schema_cache: SchemaCache,
    pub string_dict: StringDictionary,
}

impl SessionState {
    pub fn new() -> Self;
    pub fn update(&mut self, value: &Value);
    pub fn hash(&self) -> u64;
}
```

**Deliverables:**
- Session state management
- Efficient state hashing

---

### Milestone 6.2: Delta Computation

**Goal:** Compute minimal delta between states.

**Tasks:**
- [ ] Value diff algorithm
- [ ] Path encoding
- [ ] Operation types (SET, DELETE, etc.)
- [ ] Array diff (LCS-based or simpler)

**API:**
```rust
// delta/diff.rs
pub enum DeltaOp {
    Set { path: Path, value: Value },
    Delete { path: Path },
    Append { path: Path, value: Value },
    Insert { path: Path, index: usize, value: Value },
    Remove { path: Path, index: usize },
}

pub fn compute_delta(old: &Value, new: &Value) -> Vec<DeltaOp>;
pub fn apply_delta(base: &Value, ops: &[DeltaOp]) -> Result<Value>;
```

**Deliverables:**
- Delta computation algorithm
- Delta application with verification

---

### Milestone 6.3: Delta Protocol

**Goal:** Implement delta wire protocol.

**Tasks:**
- [ ] Delta message format
- [ ] Hash verification
- [ ] Resync on mismatch
- [ ] Compression of delta operations

**Deliverables:**
- Complete delta protocol
- Error recovery handling

---

## Phase 7: Integration & Optimization (Week 13-14)

### Milestone 7.1: WASM Bindings

**Goal:** Create WASM interface for browser/Node.js.

**Tasks:**
- [ ] Create `flux-wasm` crate
- [ ] Export core functions
- [ ] Session management API
- [ ] TypeScript type definitions
- [ ] Build configuration

**Deliverables:**
- Working WASM package
- TypeScript API

---

### Milestone 7.2: Performance Optimization

**Goal:** Optimize for production performance.

**Tasks:**
- [ ] Profile hot paths
- [ ] SIMD optimizations
- [ ] Memory allocation reduction
- [ ] Zero-copy where possible
- [ ] Benchmark suite

**Targets:**
- Compression: >400 MB/s
- Decompression: >1 GB/s
- Memory: <2x input size

**Deliverables:**
- Optimized implementation
- Performance regression tests

---

### Milestone 7.3: Documentation & Testing

**Goal:** Production-ready documentation.

**Tasks:**
- [ ] API documentation
- [ ] Usage examples
- [ ] Integration guide
- [ ] Fuzzing coverage
- [ ] Edge case tests

**Deliverables:**
- Complete documentation
- 90%+ test coverage

---

## Phase 8: Production Release (Week 15-16)

### Milestone 8.1: Beta Testing

**Tasks:**
- [ ] Internal testing with real data
- [ ] Performance validation
- [ ] Bug fixes
- [ ] API refinement

### Milestone 8.2: Release

**Tasks:**
- [ ] Version 2.0.0 release
- [ ] Publish to crates.io
- [ ] NPM package
- [ ] Announcement

---

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Compression Ratio | <15% | Benchmark suite |
| Compression Speed | >400 MB/s | Criterion benchmarks |
| Decompression Speed | >1 GB/s | Criterion benchmarks |
| Memory Usage | <2x input | Valgrind/heaptrack |
| Test Coverage | >90% | cargo-tarpaulin |
| Documentation | 100% public API | cargo doc |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| FSE complexity | Use existing implementation if needed |
| Performance targets | Profile early, optimize incrementally |
| Schema evolution edge cases | Extensive fuzzing |
| WASM size | Use wasm-opt, feature flags |
| Backward compatibility | Version negotiation in protocol |

---

## Dependencies

### Required Crates
```toml
[dependencies]
serde_json = "1.0"     # JSON parsing
crc32c = "0.6"         # Checksum
bitvec = "1.0"         # Bit manipulation

[dev-dependencies]
criterion = "0.5"      # Benchmarks
proptest = "1.0"       # Property testing
arbitrary = "1.0"      # Fuzzing
```

### Optional (for comparison)
```toml
[dev-dependencies]
flate2 = "1.0"         # gzip comparison
zstd = "0.12"          # zstd comparison
lz4 = "1.24"           # lz4 comparison
```

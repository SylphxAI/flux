# FLUX v2 - Fast Lightweight Universal eXchange

## Overview

FLUX is a next-generation compression protocol designed specifically for JSON-based API communication. It combines schema inference, columnar transformation, type-specific encoding, and streaming delta updates to achieve unprecedented compression ratios while maintaining high throughput.

### Design Goals

| Goal | Target | Rationale |
|------|--------|-----------|
| Compression Ratio | <15% of original | Beat gzip (45%) and zstd (40%) significantly |
| Compression Speed | >400 MB/s | Match or exceed LZ4 |
| Decompression Speed | >1 GB/s | Critical for client-side performance |
| First Byte Latency | <1ms | Real-time API requirements |
| Schema Learning | Automatic | No manual schema definition needed |
| Streaming Support | Native | Incremental updates without full retransmission |

### Key Innovations

1. **Schema Elimination** - Automatically infer and cache schemas, transmit only schema IDs
2. **Columnar Transform** - Reorganize data for better compression locality
3. **Type-Aware Encoding** - Use optimal encoding per data type (not generic compression)
4. **FSE Entropy Coding** - Modern entropy coder (faster than Huffman, used by zstd)
5. **Streaming Delta** - Only transmit changes between states

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              FLUX v2 Stack                               │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌──────────┐ │
│  │   Input     │───▶│   Schema    │───▶│  Columnar   │───▶│  Type    │ │
│  │   JSON      │    │  Inference  │    │  Transform  │    │ Encoding │ │
│  └─────────────┘    └─────────────┘    └─────────────┘    └──────────┘ │
│                                                                    │     │
│                                                                    ▼     │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌──────────┐ │
│  │   Output    │◀───│   Frame     │◀───│    FSE      │◀───│  Delta   │ │
│  │   Bytes     │    │  Assembly   │    │  Entropy    │    │ Encoding │ │
│  └─────────────┘    └─────────────┘    └─────────────┘    └──────────┘ │
│                                                                          │
├─────────────────────────────────────────────────────────────────────────┤
│                           Session State                                  │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                  │
│  │   Schema    │    │   String    │    │   Previous  │                  │
│  │   Cache     │    │ Dictionary  │    │    State    │                  │
│  └─────────────┘    └─────────────┘    └─────────────┘                  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Layer Specifications

### Layer 1: Schema Inference

#### Purpose
Eliminate redundant key names by inferring and caching JSON schemas.

#### Schema Definition
```rust
struct Schema {
    id: u32,                      // Unique schema identifier
    version: u16,                 // Schema version for evolution
    hash: u64,                    // Fast comparison hash
    fields: Vec<FieldDef>,        // Ordered field definitions
}

struct FieldDef {
    name: String,                 // Original field name
    field_type: FieldType,        // Inferred type
    nullable: bool,               // Can be null/missing
    children: Option<Schema>,     // For nested objects/arrays
}

enum FieldType {
    Null,
    Boolean,
    Integer,                      // i64
    Float,                        // f64
    String,
    Array(Box<FieldType>),        // Homogeneous array
    Object(SchemaId),             // Nested object with schema
    Union(Vec<FieldType>),        // Multiple possible types
}
```

#### Schema Inference Algorithm
```
1. Parse JSON value
2. For each key-value pair:
   a. Record key name and order
   b. Infer value type recursively
   c. Track nullability across samples
3. Generate canonical schema hash
4. Check schema cache:
   - Hit: Return cached schema ID
   - Miss: Assign new ID, cache schema
5. Handle schema evolution:
   - New fields: Add as nullable
   - Type changes: Promote to Union type
```

#### Wire Format
```
First message with new schema:
┌──────┬─────────┬──────────────────┬──────────┐
│ 0x01 │ SchemaID│ Schema Definition│  Data    │
│ (1B) │  (4B)   │   (variable)     │(variable)│
└──────┴─────────┴──────────────────┴──────────┘

Subsequent messages (cached schema):
┌──────┬─────────┬──────────┐
│ 0x02 │ SchemaID│  Data    │
│ (1B) │  (4B)   │(variable)│
└──────┴─────────┴──────────┘
```

#### Expected Savings
- Typical API response: 40-60% of size is keys
- With schema caching: Keys reduced to 4 bytes (schema ID)

---

### Layer 2: Columnar Transform

#### Purpose
Reorganize arrays of objects into columns for better compression locality.

#### Transformation
```
Input (Row-oriented):
[
  {"id": 1, "name": "alice", "score": 95},
  {"id": 2, "name": "bob", "score": 87},
  {"id": 3, "name": "charlie", "score": 92}
]

Output (Column-oriented):
{
  "id": [1, 2, 3],
  "name": ["alice", "bob", "charlie"],
  "score": [95, 87, 92]
}
```

#### Benefits
- Same-type values adjacent → better compression
- Enables type-specific encoding per column
- Enables delta encoding for sequential values

#### Algorithm
```
1. Detect array of homogeneous objects
2. Extract field names from schema
3. For each field:
   a. Collect all values into column array
   b. Track null positions in bitmap
4. Output: Column metadata + column data
```

#### Wire Format
```
Columnar Block:
┌────────┬───────────┬─────────────────────────────┐
│RowCount│ NullBitmap│        Column Data          │
│  (4B)  │ (variable)│        (variable)           │
└────────┴───────────┴─────────────────────────────┘

Column Data (repeated for each column):
┌──────────┬──────────┬─────────────┐
│ColumnType│ Encoding │  Values     │
│   (1B)   │   (1B)   │ (variable)  │
└──────────┴──────────┴─────────────┘
```

#### Expected Savings
- 20-40% additional compression from locality
- Enables efficient SIMD processing

---

### Layer 3: Type-Specific Encoding

#### Purpose
Use optimal encoding for each data type instead of generic compression.

#### Integer Encoding

##### Varint (for small positive integers)
```
Value Range         | Bytes | Format
--------------------|-------|------------------
0-127               | 1     | 0xxxxxxx
128-16383           | 2     | 10xxxxxx xxxxxxxx
16384-2097151       | 3     | 110xxxxx xxxxxxxx xxxxxxxx
2097152-268435455   | 4     | 1110xxxx xxxxxxxx xxxxxxxx xxxxxxxx
Larger              | 5     | 11110000 + 4 bytes (raw i64)
```

##### ZigZag (for signed integers)
```rust
fn zigzag_encode(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}
// -1 → 1, 1 → 2, -2 → 3, 2 → 4, ...
```

##### Delta Encoding (for sequential values)
```
Input:  [1000, 1001, 1002, 1005, 1006]
Delta:  [1000, 1, 1, 3, 1]  // First value + deltas
Varint: [0x87 0x68, 0x01, 0x01, 0x03, 0x01]  // 7 bytes vs 20+ bytes
```

##### Frame-of-Reference (FOR) (for clustered values)
```
Input:  [1000, 1002, 1001, 1003, 1005]
Min:    1000
Offset: [0, 2, 1, 3, 5]  // Each fits in 1 byte
```

##### Bit-Packing (for small ranges)
```
If max - min < 256: use 8 bits per value
If max - min < 65536: use 16 bits per value
Otherwise: use 32/64 bits
```

#### String Encoding

##### Dictionary Encoding
```
Strings: ["apple", "banana", "apple", "cherry", "banana"]
Dictionary: {0: "apple", 1: "banana", 2: "cherry"}
Encoded: [0, 1, 0, 2, 1]  // 5 bytes vs 30 bytes

Dictionary Wire Format:
┌───────────┬────────────────────────────────┐
│ DictSize  │ Entries (length-prefixed)      │
│   (2B)    │                                │
└───────────┴────────────────────────────────┘
```

##### Run-Length Encoding (for repeated strings)
```
Input:  ["a", "a", "a", "b", "b", "a"]
RLE:    [(3, "a"), (2, "b"), (1, "a")]
```

##### Front Compression (for sorted strings)
```
Input:  ["application", "apply", "apple"]
Compressed:
  - "application" (full)
  - (4, "y")      // 4 chars shared, then "y"
  - (4, "le")     // 4 chars shared, then "le"
```

#### Boolean Encoding

##### Bitmap Packing
```
Input:  [true, false, true, true, false, true, true, false]
Bitmap: 0b01101101 = 0x6D (1 byte vs 8 bytes)
```

#### Float Encoding

##### XOR Encoding (for similar floats)
```
Gorilla compression: XOR consecutive values
If XOR has many leading/trailing zeros, encode compactly
```

##### Decimal Scaling (for money/fixed-point)
```
Input:  [19.99, 29.99, 9.99]
Scale:  100
Encoded: [1999, 2999, 999] as integers
```

#### Null Handling

##### Null Bitmap
```
Values: [1, null, 3, null, 5]
Bitmap: 0b10101 = 0x15
Data:   [1, 3, 5]  // Only non-null values
```

#### Encoding Selection Algorithm
```
For each column:
1. Analyze value distribution
2. Calculate entropy
3. Try candidate encodings (sample)
4. Select best encoding by: size * decode_speed_factor
5. Store encoding type in column header
```

---

### Layer 4: FSE Entropy Coding

#### Purpose
Apply entropy coding to achieve near-optimal compression on encoded data.

#### Why FSE over Huffman/ANS?
| Property | Huffman | rANS | FSE (tANS) |
|----------|---------|------|------------|
| Speed | Medium | Medium | Fast |
| Compression | Good | Optimal | Near-optimal |
| Decoding | Symbol-by-symbol | Reverse order | Fast table lookup |
| SIMD | Limited | Good | Excellent |

#### FSE Basics
```
FSE uses a finite-state machine:
- State transitions encode symbols
- Table-based: only additions, masks, shifts (no division)
- Interleaved streams for SIMD parallelism
```

#### When to Apply
```
Apply FSE when:
- Block size > 256 bytes (overhead threshold)
- Estimated entropy gain > 10%
- Data is not already low-entropy

Skip FSE when:
- Small data (overhead > gain)
- Already dictionary-encoded with small dict
- Random/encrypted data
```

#### Wire Format
```
FSE Block:
┌──────┬───────────┬──────────────┬─────────────┐
│ Flag │ TableDesc │ CompressedLen│ FSE Stream  │
│ (1B) │ (variable)│     (4B)     │ (variable)  │
└──────┴───────────┴──────────────┴─────────────┘

Flag:
  0x00 = Raw (no FSE)
  0x01 = FSE compressed
  0x02 = RLE (single repeated byte)
```

---

### Layer 5: Streaming Delta

#### Purpose
For real-time updates, only transmit changes instead of full state.

#### Delta Types

##### Value Delta
```
Previous: {"count": 100, "status": "active"}
Current:  {"count": 105, "status": "active"}
Delta:    {"count": +5}  // Only changed field
```

##### Array Delta
```
Previous: [{"id": 1}, {"id": 2}, {"id": 3}]
Current:  [{"id": 1}, {"id": 2, "score": 50}, {"id": 3}, {"id": 4}]
Delta:    {
  "modify": [{index: 1, changes: {"score": 50}}],
  "append": [{"id": 4}]
}
```

##### Structural Delta
```
Operations:
- SET: Set field value
- DELETE: Remove field
- APPEND: Add to array
- INSERT: Insert at index
- REMOVE: Remove from array
- MOVE: Reorder array element
```

#### State Synchronization

##### Checksum Protocol
```
Client                          Server
   │                               │
   │◀──── State + Checksum ────────│
   │                               │
   │───── ACK(Checksum) ──────────▶│
   │                               │
   │◀──── Delta + NewChecksum ─────│
   │                               │
   │───── ACK(NewChecksum) ───────▶│
   │                               │

On checksum mismatch: Full state sync
```

##### Wire Format
```
Delta Message:
┌──────┬──────────┬──────────┬──────────────┐
│ Type │ PrevHash │ NewHash  │ Operations   │
│ (1B) │   (8B)   │   (8B)   │  (variable)  │
└──────┴──────────┴──────────┴──────────────┘

Operation:
┌────────┬─────────┬───────────┐
│ OpCode │  Path   │  Value    │
│  (1B)  │(variable│(variable) │
└────────┴─────────┴───────────┘
```

#### Expected Savings
- Incremental updates: 90-99% reduction
- Real-time sync: <100 bytes per update typically

---

## Frame Format

### FLUX Frame Structure
```
┌────────────────────────────────────────────────────────────┐
│                      FLUX Frame                             │
├────────┬─────────┬───────┬──────────┬─────────┬───────────┤
│ Magic  │ Version │ Flags │ SchemaID │ Length  │  Payload  │
│ "FLUX" │  (1B)   │ (1B)  │   (4B)   │  (4B)   │(variable) │
│  (4B)  │         │       │          │         │           │
└────────┴─────────┴───────┴──────────┴─────────┴───────────┘
```

### Flags
```
Bit 0: Has schema definition (first message)
Bit 1: Columnar format
Bit 2: FSE compressed
Bit 3: Delta message
Bit 4: Has checksum
Bit 5-7: Reserved
```

### Magic Bytes
```
FLUX = [0x46, 0x4C, 0x55, 0x58]
```

### Version
```
0x01 = FLUX v2.0
```

---

## Session Management

### Session State
```rust
struct FluxSession {
    // Schema management
    schema_cache: HashMap<u64, Schema>,
    next_schema_id: u32,

    // String dictionary (shared)
    global_dict: StringDictionary,

    // Delta state
    prev_state: Option<Value>,
    prev_hash: u64,

    // Statistics
    messages_sent: u64,
    bytes_saved: u64,
}
```

### Client-Server Handshake
```
Client                          Server
   │                               │
   │──── HELLO(capabilities) ─────▶│
   │                               │
   │◀─── HELLO(capabilities) ──────│
   │                               │
   │──── SYNC(dict_hash) ─────────▶│
   │                               │
   │◀─── SYNC(dict_delta) ─────────│
   │                               │
   │◀─── DATA ─────────────────────│
   │                               │
```

---

## Performance Targets

### Benchmarks to Beat

| Metric | gzip | zstd | LZ4 | FLUX Target |
|--------|------|------|-----|-------------|
| Ratio (JSON) | 45% | 40% | 55% | **12-15%** |
| Ratio (cached) | 45% | 40% | 55% | **3-5%** |
| Compress | 50 MB/s | 300 MB/s | 500 MB/s | **400 MB/s** |
| Decompress | 200 MB/s | 800 MB/s | 2 GB/s | **1.5 GB/s** |

### Test Cases
1. Small JSON (<100 bytes) - API request
2. Medium JSON (1-10 KB) - API response
3. Large JSON array (100+ KB) - Batch data
4. Repeated structure - Paginated results
5. Incremental update - Real-time sync

---

## Implementation Phases

### Phase 1: Foundation (Week 1-2)
- [ ] Schema inference engine
- [ ] Basic type encoders (varint, zigzag)
- [ ] Frame format implementation
- [ ] Basic benchmarks

### Phase 2: Core Encoding (Week 3-4)
- [ ] All type-specific encoders
- [ ] Columnar transformation
- [ ] Dictionary encoding
- [ ] Null handling

### Phase 3: Entropy & Optimization (Week 5-6)
- [ ] FSE integration
- [ ] Encoding selection algorithm
- [ ] SIMD optimizations
- [ ] Performance tuning

### Phase 4: Streaming (Week 7-8)
- [ ] Delta encoding
- [ ] State synchronization
- [ ] Session management
- [ ] Error recovery

### Phase 5: Integration (Week 9-10)
- [ ] WASM bindings
- [ ] TypeScript API
- [ ] Documentation
- [ ] Production benchmarks

---

## References

1. [Columnar Storage Formats Evaluation (VLDB 2024)](https://www.vldb.org/pvldb/vol17/p148-zeng.pdf)
2. [JSON Tiles (SIGMOD 2021)](https://dl.acm.org/doi/10.1145/3448016.3452809)
3. [Finite State Entropy](https://github.com/Cyan4973/FiniteStateEntropy)
4. [Zstandard](https://github.com/facebook/zstd)
5. [Uber JSON Compression](https://www.uber.com/blog/trip-data-squeeze-json-encoding-compression/)
6. [Gorilla Time Series Compression](http://www.vldb.org/pvldb/vol8/p1816-teller.pdf)

---

## Appendix A: Encoding Quick Reference

| Data Type | Primary Encoding | Fallback |
|-----------|------------------|----------|
| Small int (0-127) | Varint | - |
| Signed int | ZigZag + Varint | Raw i64 |
| Sequential int | Delta + Varint | FOR |
| Float | XOR / Decimal | Raw f64 |
| Short string | Dictionary | Length-prefixed |
| Long string | Front compression | Raw |
| Boolean | Bitmap | - |
| Null | Bitmap | - |
| Array | Columnar | Row-wise |

## Appendix B: Compression Ratio Examples

```
Example 1: User List
Input:  [{"id":1,"name":"alice","email":"alice@example.com","active":true}, ...]
Size:   10 users × 70 bytes = 700 bytes

FLUX breakdown:
- Schema: 4 bytes (cached ID)
- IDs: 10 × 1 byte (varint) = 10 bytes
- Names: Dict(5) + 10 indices = 35 bytes
- Emails: Dict(5) + 10 indices = 85 bytes
- Active: 2 bytes (bitmap)
- Overhead: 20 bytes
Total: ~156 bytes (22% of original)

With FSE: ~130 bytes (18% of original)

Example 2: Incremental Update
Previous: 1000 users
Changed:  3 users updated

FLUX delta:
- Header: 17 bytes
- 3 × change operation: 30 bytes
Total: ~47 bytes (vs retransmitting 70KB)
```

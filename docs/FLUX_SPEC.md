# FLUX v2 Technical Specification

> Version: 2.0.0-draft
> Status: Design Phase
> Last Updated: 2024

## 1. Data Types

### 1.1 Primitive Types

| Type ID | Name | Description | Size |
|---------|------|-------------|------|
| 0x00 | Null | JSON null | 0 bytes |
| 0x01 | Boolean | true/false | 1 bit (packed) |
| 0x02 | Int8 | Signed 8-bit | 1 byte |
| 0x03 | Int16 | Signed 16-bit | 2 bytes |
| 0x04 | Int32 | Signed 32-bit | 4 bytes |
| 0x05 | Int64 | Signed 64-bit | 8 bytes |
| 0x06 | Varint | Variable-length int | 1-9 bytes |
| 0x07 | Float32 | IEEE 754 single | 4 bytes |
| 0x08 | Float64 | IEEE 754 double | 8 bytes |
| 0x09 | String | UTF-8 string | variable |
| 0x0A | Binary | Raw bytes | variable |
| 0x0B | Array | Homogeneous array | variable |
| 0x0C | Object | Key-value map | variable |
| 0x0D | Union | Multiple types | variable |

### 1.2 Extended Types

| Type ID | Name | Description |
|---------|------|-------------|
| 0x10 | Timestamp | Unix epoch millis (varint) |
| 0x11 | UUID | 128-bit UUID |
| 0x12 | Decimal | Fixed-point decimal |
| 0x13 | Date | Days since epoch |
| 0x14 | Time | Milliseconds since midnight |

---

## 2. Encoding Specifications

### 2.1 Varint Encoding

Variable-length unsigned integer encoding (similar to Protocol Buffers).

```
Encoding:
- Use 7 bits per byte for data
- MSB = 1 means more bytes follow
- MSB = 0 means final byte

Examples:
  0         → 0x00
  127       → 0x7F
  128       → 0x80 0x01
  16383     → 0xFF 0x7F
  16384     → 0x80 0x80 0x01
```

```rust
fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value >= 0x80 {
        buf.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

fn decode_varint(buf: &[u8]) -> (u64, usize) {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut pos = 0;

    loop {
        let byte = buf[pos];
        result |= ((byte & 0x7F) as u64) << shift;
        pos += 1;

        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    (result, pos)
}
```

### 2.2 ZigZag Encoding

Signed integer to unsigned conversion for efficient varint encoding.

```rust
fn zigzag_encode(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

fn zigzag_decode(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

// Examples:
//  0 → 0
// -1 → 1
//  1 → 2
// -2 → 3
//  2 → 4
```

### 2.3 Delta Encoding

For sequences of related values.

```
Format:
┌────────────┬───────────┬───────────┬─────┐
│ BaseValue  │  Delta1   │  Delta2   │ ... │
│  (varint)  │ (zigzag)  │ (zigzag)  │     │
└────────────┴───────────┴───────────┴─────┘

Algorithm:
  deltas[0] = values[0]  // First value as-is
  deltas[i] = values[i] - values[i-1]  // Subsequent as deltas
```

### 2.4 Frame-of-Reference (FOR)

For clustered integer values.

```
Format:
┌──────────┬──────────┬────────────────────────────┐
│ MinValue │ BitWidth │ Packed Offsets             │
│ (varint) │  (1B)    │ (bitwidth × count / 8)     │
└──────────┴──────────┴────────────────────────────┘

Algorithm:
  1. Find min and max values
  2. Calculate required bit width: ceil(log2(max - min + 1))
  3. Store min as base
  4. Store each value as (value - min) using bit width
```

### 2.5 Dictionary Encoding

For repeated string values.

```
Format:
┌─────────────────────────────────────────────┐
│                Dictionary                    │
├────────────┬────────────────────────────────┤
│ EntryCount │ Entries                         │
│  (varint)  │                                 │
└────────────┴────────────────────────────────┘

Entry:
┌────────┬─────────┐
│ Length │  UTF-8  │
│(varint)│ (bytes) │
└────────┴─────────┘

Values:
┌────────────────────────────────────┐
│ Indices (varint per value)         │
└────────────────────────────────────┘
```

### 2.6 Run-Length Encoding (RLE)

For repeated consecutive values.

```
Format:
┌──────────────────────────────────────┐
│ Runs                                  │
├───────────┬──────────┬───────────────┤
│ RunLength │  Value   │ (repeated)    │
│ (varint)  │(type-dep)│               │
└───────────┴──────────┴───────────────┘

Optimization:
- If run length = 1, encode value directly (no length prefix)
- Use special marker for literal runs vs repeated runs
```

### 2.7 Boolean Bitmap

Pack 8 booleans per byte.

```
Format:
┌────────────┬────────────────────────┐
│ BitCount   │ Packed Bits            │
│  (varint)  │ (ceil(count/8) bytes)  │
└────────────┴────────────────────────┘

Bit order: LSB first within each byte
Example: [true, false, true, true] → 0b00001101 = 0x0D
```

### 2.8 Null Bitmap

Track null positions in nullable columns.

```
Format:
┌────────────┬────────────────────────┬─────────────┐
│ TotalCount │ Null Bitmap            │ Non-null    │
│  (varint)  │ (ceil(count/8) bytes)  │ Values      │
└────────────┴────────────────────────┴─────────────┘

Bitmap: 1 = value present, 0 = null
```

---

## 3. Schema Format

### 3.1 Schema Definition

```
Schema:
┌──────────┬─────────┬──────────┬────────────────┐
│ SchemaID │ Version │ FieldCnt │ Fields         │
│  (4B)    │  (2B)   │ (varint) │ (repeated)     │
└──────────┴─────────┴──────────┴────────────────┘

Field:
┌───────────┬──────────┬──────────┬──────────────┐
│ NameLen   │ Name     │ TypeInfo │ Flags        │
│ (varint)  │ (UTF-8)  │ (var)    │ (1B)         │
└───────────┴──────────┴──────────┴──────────────┘

TypeInfo:
┌──────────┬────────────────────────────────────┐
│ TypeID   │ TypeParams (type-specific)         │
│  (1B)    │ (variable)                         │
└──────────┴────────────────────────────────────┘

Flags:
  Bit 0: Nullable
  Bit 1: Has default value
  Bit 2: Deprecated
  Bit 3-7: Reserved
```

### 3.2 Schema Hash

Fast comparison using FNV-1a hash.

```rust
fn schema_hash(schema: &Schema) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis

    for field in &schema.fields {
        // Hash field name
        for byte in field.name.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3); // FNV prime
        }

        // Hash field type
        hash ^= field.field_type as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }

    hash
}
```

### 3.3 Schema Evolution Rules

| Change | Backward Compatible | Forward Compatible |
|--------|--------------------|--------------------|
| Add optional field | ✅ | ✅ |
| Add required field | ❌ | ✅ |
| Remove optional field | ✅ | ❌ |
| Remove required field | ❌ | ❌ |
| Widen type (int32→int64) | ✅ | ❌ |
| Narrow type (int64→int32) | ❌ | ✅ |
| Rename field | ❌ | ❌ |

---

## 4. Frame Format

### 4.1 Frame Header

```
┌────────┬─────────┬───────┬──────────┬──────────┬──────────┐
│ Magic  │ Version │ Flags │ SchemaID │ PayloadLen│ Checksum │
│ (4B)   │  (1B)   │ (1B)  │  (4B)    │   (4B)   │  (4B)    │
└────────┴─────────┴───────┴──────────┴──────────┴──────────┘
Total: 18 bytes header
```

### 4.2 Magic Number

```
ASCII: "FLUX"
Bytes: [0x46, 0x4C, 0x55, 0x58]
```

### 4.3 Version Byte

```
High nibble: Major version (0-15)
Low nibble:  Minor version (0-15)

0x20 = Version 2.0
```

### 4.4 Flags Byte

```
Bit 0: SCHEMA_INCLUDED    - Schema definition in payload
Bit 1: COLUMNAR           - Data in columnar format
Bit 2: FSE_COMPRESSED     - FSE entropy coding applied
Bit 3: DELTA_MESSAGE      - Payload is delta update
Bit 4: CHECKSUM_PRESENT   - CRC32 checksum included
Bit 5: DICTIONARY_UPDATE  - Contains dictionary entries
Bit 6: STREAMING          - Part of streaming session
Bit 7: Reserved
```

### 4.5 Checksum

CRC32C (Castagnoli) of payload.

```rust
fn crc32c(data: &[u8]) -> u32 {
    // Use hardware CRC32C if available (SSE4.2)
    // Otherwise software implementation
}
```

---

## 5. Columnar Format

### 5.1 Columnar Block

```
┌───────────┬────────────┬────────────────────────┐
│ RowCount  │ ColumnCnt  │ Columns                │
│ (varint)  │ (varint)   │ (repeated)             │
└───────────┴────────────┴────────────────────────┘
```

### 5.2 Column Format

```
┌───────────┬───────────┬────────────┬────────────────┐
│ ColumnID  │ Encoding  │ NullBitmap │ EncodedData    │
│ (varint)  │  (1B)     │ (optional) │ (variable)     │
└───────────┴───────────┴────────────┴────────────────┘

Encoding byte:
  0x00: Raw (no encoding)
  0x01: Varint
  0x02: Delta + Varint
  0x03: FOR (Frame-of-Reference)
  0x04: Dictionary
  0x05: RLE
  0x06: Bitmap (booleans)
  0x07: FSE compressed
  0x08-0xFF: Reserved
```

---

## 6. Delta Protocol

### 6.1 Delta Message

```
┌──────────┬───────────┬───────────┬──────────────┐
│ MsgType  │ BaseHash  │ NewHash   │ Operations   │
│  (1B)    │   (8B)    │   (8B)    │ (variable)   │
└──────────┴───────────┴───────────┴──────────────┘

MsgType:
  0x01: DELTA      - Incremental update
  0x02: FULL_SYNC  - Complete state
  0x03: RESET      - Clear state
```

### 6.2 Delta Operation

```
┌──────────┬──────────┬───────────┬──────────────┐
│ OpCode   │ PathLen  │ Path      │ Value        │
│  (1B)    │ (varint) │ (bytes)   │ (variable)   │
└──────────┴──────────┴───────────┴──────────────┘

OpCode:
  0x01: SET        - Set field value
  0x02: DELETE     - Remove field
  0x03: APPEND     - Append to array
  0x04: INSERT     - Insert at index
  0x05: REMOVE     - Remove from array
  0x06: MOVE       - Move array element
  0x07: INCREMENT  - Atomic increment
  0x08: DECREMENT  - Atomic decrement
```

### 6.3 Path Encoding

JSON Pointer-like path encoding.

```
Path: /users/0/name
Encoded: [5, "users", 0, 4, "name"]
         │    │      │  │    │
         │    │      │  │    └─ Field name
         │    │      │  └────── Name length
         │    │      └───────── Array index (special marker + varint)
         │    └──────────────── Field name
         └───────────────────── Name length

Special markers:
  0x00: End of path
  0xFF: Array index follows (varint)
```

---

## 7. Session Protocol

### 7.1 Handshake

```
Client → Server: HELLO
┌──────────┬─────────┬────────────────┐
│ MsgType  │ Version │ Capabilities   │
│  0x01    │  (2B)   │    (4B)        │
└──────────┴─────────┴────────────────┘

Server → Client: HELLO_ACK
┌──────────┬─────────┬────────────────┬───────────┐
│ MsgType  │ Version │ Capabilities   │ SessionID │
│  0x02    │  (2B)   │    (4B)        │   (8B)    │
└──────────┴─────────┴────────────────┴───────────┘

Capabilities (bitfield):
  Bit 0: COLUMNAR_SUPPORT
  Bit 1: FSE_SUPPORT
  Bit 2: DELTA_SUPPORT
  Bit 3: DICTIONARY_SHARING
  Bit 4-31: Reserved
```

### 7.2 Dictionary Sync

```
DICT_UPDATE:
┌──────────┬───────────┬────────────────────────┐
│ MsgType  │ EntryCount│ Entries                │
│  0x10    │ (varint)  │ (repeated DictEntry)   │
└──────────┴───────────┴────────────────────────┘

DictEntry:
┌───────────┬───────────┬──────────────┐
│ GlobalID  │ StringLen │ StringData   │
│ (varint)  │ (varint)  │ (UTF-8)      │
└───────────┴───────────┴──────────────┘
```

### 7.3 Acknowledgment

```
ACK:
┌──────────┬───────────┬───────────┐
│ MsgType  │ SeqNum    │ StateHash │
│  0x20    │ (varint)  │   (8B)    │
└──────────┴───────────┴───────────┘
```

---

## 8. Error Codes

| Code | Name | Description |
|------|------|-------------|
| 0x00 | OK | Success |
| 0x01 | INVALID_MAGIC | Bad magic number |
| 0x02 | VERSION_MISMATCH | Unsupported version |
| 0x03 | SCHEMA_NOT_FOUND | Unknown schema ID |
| 0x04 | CHECKSUM_MISMATCH | Data corruption |
| 0x05 | DECODE_ERROR | Malformed data |
| 0x06 | STATE_DESYNC | Delta base mismatch |
| 0x07 | BUFFER_OVERFLOW | Output buffer too small |
| 0x08 | UNSUPPORTED_ENCODING | Unknown encoding type |

---

## 9. Constants

```rust
// Limits
const MAX_SCHEMA_FIELDS: usize = 1024;
const MAX_STRING_LENGTH: usize = 16 * 1024 * 1024; // 16 MB
const MAX_ARRAY_LENGTH: usize = 1024 * 1024; // 1M elements
const MAX_NESTING_DEPTH: usize = 64;
const MAX_DICTIONARY_SIZE: usize = 65536; // 64K entries

// Thresholds
const FSE_MIN_BLOCK_SIZE: usize = 256;
const COLUMNAR_MIN_ROWS: usize = 4;
const DICTIONARY_MIN_OCCURRENCES: usize = 2;

// Buffer sizes
const DEFAULT_BUFFER_SIZE: usize = 64 * 1024; // 64 KB
const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024; // 64 MB
```

---

## 10. Security Considerations

### 10.1 Input Validation

- Validate all lengths before allocation
- Check nesting depth to prevent stack overflow
- Limit dictionary size to prevent memory exhaustion
- Verify checksums before processing

### 10.2 Denial of Service Prevention

- Set maximum decompression ratio (e.g., 1000:1)
- Timeout on decoding operations
- Rate limit schema registrations

### 10.3 Data Integrity

- Always verify CRC32C checksum
- Validate schema consistency
- Check delta base hash before applying

---

## Appendix A: Test Vectors

### A.1 Varint Encoding

```
Input    | Encoded (hex)
---------|---------------
0        | 00
1        | 01
127      | 7F
128      | 80 01
255      | FF 01
256      | 80 02
16383    | FF 7F
16384    | 80 80 01
```

### A.2 ZigZag Encoding

```
Input    | Encoded
---------|--------
0        | 0
-1       | 1
1        | 2
-2       | 3
2        | 4
-64      | 127
64       | 128
```

### A.3 Sample Frame

```
Input JSON:
{"id": 123, "name": "test"}

FLUX Frame (hex):
46 4C 55 58  // Magic: "FLUX"
20           // Version: 2.0
01           // Flags: SCHEMA_INCLUDED
00 00 00 01  // SchemaID: 1
00 00 00 0F  // PayloadLen: 15
[schema + data bytes]
12 34 56 78  // CRC32C checksum
```

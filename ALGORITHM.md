# APEX - Adaptive Pattern EXtraction

## 設計理念

現有算法嘅問題：
- **LZ4/LZ77**: 只睇 local patterns，錯過 global 重複
- **gzip/zstd**: 通用設計，唔識 JSON 結構
- **Brotli**: 有 dictionary，但係 static

APEX 專為 **client-server JSON API** 設計：
1. 兩端可以 share state
2. 連續 request 有 similar structure
3. JSON 有 predictable patterns

---

## 核心創新

### 1. Structural Compression (結構壓縮)

JSON 有兩層：**結構** 同 **數據**

```json
{"id":123,"name":"alice","score":95}
{"id":456,"name":"bob","score":87}
```

傳統壓縮睇到嘅：一堆 bytes
APEX 睇到嘅：
- Structure template: `{"id":_,"name":"_","score":_}`
- Values: `[123,"alice",95]`, `[456,"bob",87]`

**結構只需傳一次，之後只傳數據！**

### 2. Adaptive Dictionary (自適應字典)

```
┌─────────────────────────────────────────────────┐
│                 Dictionary Levels               │
├─────────────────────────────────────────────────┤
│ L0: Static    │ Common JSON: "id", "name", ... │
│ L1: Session   │ Learned from this connection   │
│ L2: Message   │ Built during this message      │
└─────────────────────────────────────────────────┘
```

- L0: 內建常見 patterns (免費)
- L1: 連接開始時 learn，越用越準
- L2: 單個 message 內嘅 local patterns

### 3. Predictive Encoding (預測編碼)

利用 JSON 語法規則預測下一個 byte：

```
After `{` → expect `"` or `}`  (2 choices = 1 bit)
After `"key":` → expect value type (4-5 choices = 2-3 bits)
After `[` → expect value or `]` (limited choices)
```

**Context-aware prediction = fewer bits needed**

### 4. Delta Streams (差異流)

API response 通常有 arrays of similar objects：

```json
[
  {"id":1,"ts":1000000001,"val":100},
  {"id":2,"ts":1000000002,"val":102},
  {"id":3,"ts":1000000003,"val":99}
]
```

APEX detects:
- `id`: sequential (+1, +1, +1) → encode as "delta +1"
- `ts`: sequential (+1, +1, +1) → encode as "delta +1"
- `val`: varying → encode normally

**Sequential patterns = nearly zero cost**

---

## Frame Format v2

```
┌────────────────────────────────────────────────────────────┐
│                      APEX Frame                            │
├──────────┬─────────┬──────────┬─────────┬─────────────────┤
│ Magic    │ Version │ Flags    │ Dict ID │ Payload         │
│ "APEX"   │ 1 byte  │ 2 bytes  │ 2 bytes │ Variable        │
└──────────┴─────────┴──────────┴─────────┴─────────────────┘

Flags:
- bit 0: has structure template
- bit 1: has session dictionary update
- bit 2: delta encoding enabled
- bit 3: predictive encoding enabled
- bit 4-5: compression level (0-3)
- bit 6-15: reserved

Payload (when structural compression):
┌──────────────────┬───────────────────┬──────────────────┐
│ Template (once)  │ Value Stream      │ LZ4 Residual     │
│ Compressed keys  │ Type-optimized    │ Fallback data    │
└──────────────────┴───────────────────┴──────────────────┘
```

---

## 預期效能

```
Benchmark: API response (100KB JSON, array of 1000 objects)

Algorithm       Ratio    Compress    Decompress
─────────────────────────────────────────────────
gzip            84%      12ms        3ms
LZ4             62%      2ms         0.5ms
zstd            86%      5ms         1ms
─────────────────────────────────────────────────
APEX (cold)     85%      3ms         1ms      ← First request
APEX (warm)     92%      1ms         0.3ms    ← After learning
APEX (delta)    97%      0.5ms       0.2ms    ← Similar structure
```

**Key insight**: 第一個 request 同 gzip 差唔多，但 subsequent requests 可以達到 **97% 壓縮率**！

---

## Implementation Phases

### Phase 1: Structure Detection
- [ ] JSON tokenizer
- [ ] Template extraction
- [ ] Structure hashing

### Phase 2: Adaptive Dictionary
- [ ] Static dictionary (L0)
- [ ] Session dictionary protocol
- [ ] Dictionary sync between client/server

### Phase 3: Predictive Encoder
- [ ] Context model for JSON
- [ ] Arithmetic/ANS coder
- [ ] Fast decode path

### Phase 4: Delta Streams
- [ ] Type detection (int, float, string)
- [ ] Delta encoder per type
- [ ] Automatic pattern detection

---

## API Design

```typescript
// Basic (auto-detect mode)
const compressed = await apex.compress(jsonData);

// With session (learning mode)
const session = apex.createSession();
const c1 = await session.compress(request1);  // Cold
const c2 = await session.compress(request2);  // Warm - better ratio!
const c3 = await session.compress(request3);  // Hot - best ratio!

// Server side
const serverSession = apex.createSession();
const d1 = await serverSession.decompress(c1);
// Session automatically syncs dictionary
```

---

## vs Other Algorithms

| Feature | gzip | LZ4 | zstd | Brotli | **APEX** |
|---------|------|-----|------|--------|----------|
| Speed | Slow | Fast | Medium | Slow | Fast |
| Ratio | Good | Low | Best | Best | **Adaptive** |
| JSON-aware | ❌ | ❌ | ❌ | ❌ | ✅ |
| Learning | ❌ | ❌ | Dict | Static | ✅ Session |
| Delta | ❌ | ❌ | ❌ | ❌ | ✅ |
| Streaming | ✅ | ✅ | ✅ | ❌ | ✅ |

# ADR-PROPOSED — Fleet Flux Rust North Star architecture

- **Status:** Proposed
- **Date:** 2026-07-10
- **Relates to:** ADR-167 (SylphxAI/doctrine)
- **Change class:** `required-future` for Flux compression foundation; `advisory` for fleet

## Context

Flux (`@sylphx/flux`) is a high-performance JSON compression library for API
communication: schema elimination, LZ77, ANS entropy coding, and delta streaming.
Rust crates `flux-core`, `flux-wasm`, `fastpack-core`, `fastpack-wasm`, and
`fastpack-node` own the codec hot path; npm packages (`packages/flux`,
`packages/fastpack`) are thin WASM/NAPI loaders only.

Unlike application repos, Flux is a **published framework foundation** — Rust has
always been runtime authority for compress/decompress, session schema cache, and
stream deltas. The migration ledger tracks **8 capabilities** across FLUX and
FastPack cores. rej-010 re-audit downgraded prior `ts_deleted` claims pending
SHA-bound differential proof between native SSOT and WASM consumer paths. Central
doctrine
[ADR-167](https://github.com/SylphxAI/doctrine/blob/main/docs/adr/ADR-167-boundary-contract-stack-and-platform-pillars.md)
classifies Flux as foundation infrastructure with Rust-first default.

Cutover posture here is **proof hardening**, not greenfield rewrite: preserve npm
public API contracts, benchmark claims, and WASM/native boundary separation.

## Decision

### 1. North Star production stack (Flux repo)

| Layer | North Star | Transitional (until sunset slice) |
| --- | --- | --- |
| Cross-boundary contract | Rust crate public API + npm type exports | None (framework surface is the contract) |
| FLUX compress/decompress | Rust `flux-core` | None — always Rust authority |
| FLUX session schema + delta | Rust `flux-core/src/schema/`, `delta/` | None |
| WASM portable runtime | Rust `flux-wasm` compiled module | TS `packages/flux` WASM consumer |
| FastPack codec | Rust `fastpack-core` + `fastpack-wasm` | TS `packages/fastpack` FFI bridge |
| Node native bindings | Rust `fastpack-node` (NAPI) | None |
| Proto/admin surfaces | N/A (no cross-repo proto today) | Add Buf only if ops/admin RPC needed |
| Publish artifacts | npm packages + WASM prebuilds per CI | unchanged release path |

### 2. Ownership matrix

| Concern | Owner | Flux may | Flux must not |
| --- | --- | --- | --- |
| Compression framework, benchmarks, codecs | **SylphxAI/flux** | Own crates, npm packages, CI gates | Embed product-specific payload semantics |
| Consumer app business logic | Product repos | Export `compress`/`decompress` primitives | Own tenant/auth/billing semantics |
| Engineering doctrine, fleet audits | **SylphxAI/doctrine** | Run conformance audits | Fork standards into repo prose |

### 3. Strangler-fig cutover posture

Foundation repo — slices are **differential-proof** not runtime migration:

- **S0:** `cargo test`/`clippy` workspace + `scripts/check-rust-authority.sh` green.
- **S1:** FLUX core differential — native `flux-core` SSOT vs `flux-wasm` consumer path.
- **S2:** Session schema + stream delta differential corpus.
- **S3:** FastPack native vs WASM/NAPI differential round-trip.
- **S4:** Re-claim `ts_deleted` only after `differential_green` + npm publish proof.
- Each slice requires N4 (`cargo test` + `cargo clippy -D warnings`) per fleet cutover registry.

### 4. Contract stack (ADR-167 alignment)

- **Rust crates** are SSOT for compression hot path; WASM is compiled projection of `flux-core`.
- **Connect/gRPC** not required unless future cross-repo ops surfaces land.
- **TypeScript npm packages** remain portable edge pillar — not backend authority.
- Hand-written duplicate codecs in TS are rejected (`scripts/check-rust-authority.sh`).

## Alternatives considered

| Alternative | Why rejected |
| --- | --- |
| TypeScript compression backend revival | Violates foundation design; fails rust-authority gate |
| Skip differential proof | rej-010 promotion freeze requires SHA-bound harness for authority claims |
| Product logic in `flux-core` | Violates framework boundary; consumers own semantics |

## Consequences

- All compression authority remains in `crates/flux-*` and `crates/fastpack-*`; TS is bindings only.
- `docs/specs/migration-ledger.json` tracks proof status, not greenfield migration percent.
- Prior `ts_deleted` claims stay reverted until differential_green at bound SHAs.
- npm published versions are forward-fix-only; benchmark claims require CI reproduction.

## Validation

- `docs/specs/migration-ledger.json` — capability `state`, `proof.status`, `promotionHold`
- Differential harness: `scripts/run-flux-differential.sh` + `crates/flux-core/tests/flux_core_differential.rs`
- `scripts/check-rust-authority.sh` — hard gate against TS codec regression
- `.github/workflows/ci.yml` — native/WASM publish proof
- `python3 $DOCTRINE/scripts/project-control-plane-audit.py --local . --fail-on-drift`
- `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` (N4 gate)
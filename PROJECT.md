# FLUX

FLUX is a tenant-neutral JSON compression foundation for API payloads. The
repository owns the Rust compression core, WebAssembly bindings, TypeScript
package surfaces, frame/spec documentation, and benchmark/test harnesses needed
to evolve the compression format independently of any product.

## Lifecycle

- State: `active`
- Layer: `foundation`
- Machine manifest: [`.doctrine/project.json`](./.doctrine/project.json)

## Goals

- Provide schema-aware JSON compression primitives for API communication.
- Ship stable Rust, WebAssembly, and TypeScript package surfaces for the FLUX
  format.
- Keep the compression core product-neutral, deterministic, benchmarkable, and
  independently releasable.

## Non-Goals

- This repository does not own application transport policy, customer schemas,
  routing, authentication, billing, telemetry, or product-specific behavior.
- This repository does not own organization-wide CI, release governance, or
  downstream deployment policy.
- This repository must not embed sibling-project assumptions into the
  compression format or package APIs.

## Boundary

FLUX owns the compression frame format, Rust core crates under `crates/`,
WebAssembly bindings, package exports under `packages/`, examples, specs, and
benchmarks. Product repositories consume FLUX only through published package
exports or documented specs; they must not reach into this repository's
internals or alter core behavior for a single application.

## Public Surfaces

- Rust core crate metadata: `crates/flux-core/Cargo.toml`
- WebAssembly binding crate metadata: `crates/flux-wasm/Cargo.toml`
- TypeScript package metadata: `packages/flux/package.json`
- Compression specification: `docs/FLUX_SPEC.md`
- Human project orientation: `PROJECT.md`
- Machine-readable project manifest: `.doctrine/project.json`

## Delivery

- CI model: GitHub Actions (`.github/workflows/`)
- Required contexts: `rust-parity-differential`
- Deploy/release path: npm packages `@sylphx/flux` + `@sylphx/flux-wasm` via
  `.github/workflows/release.yml` on `main` (trusted publishing / NPM_TOKEN)
- Production proof: `scripts/run-flux-differential.sh` main-bound green + npm
  registry readback (`npm view @sylphx/flux gitHead` == publish commit) +
  install smoke (`npm pack` / consumer compress roundtrip)
- Recovery class: `forward-fix-only`

Release safety: packages are public-scoped (`@sylphx/*`) with versioned
`@sylphx/flux` → `@sylphx/flux-wasm@0.1.0` dependency (no `workspace:*` on the
publish surface). First publish still requires org npm credentials or OIDC
trusted publishing secrets on this repository.

Production defect (2026-07-15): `@sylphx/flux-wasm@0.1.0` published package.json-only
because wasm-pack emits `wasm/.gitignore` with `*`, which npm pack honors — consumer
install smoke fails (`Cannot find module .../flux_wasm.js`). Fix: strip that
gitignore in build, fail-closed `scripts/check-flux-pack-integrity.sh` + consumer
smoke, bump to `0.1.1`. Registry re-publish still requires `NPM_TOKEN` / trusted
publishing.

Adoption gaps remaining: publish `0.1.1` with non-empty wasm readback, fastpack
differential + npm surface, completeness beyond flux-core compress/session/stream.
Tracked in `.doctrine/project.json`.

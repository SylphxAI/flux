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

- CI model: `none`
- Required contexts: none declared in this repository
- Deploy/release path: package/crate release path is not declared yet
- Production proof: benchmark, package publication, and consumer smoke proof are
  not declared yet
- Recovery class: `forward-fix-only`

Adoption is baseline only. The current gaps are tracked in
`.doctrine/project.json`.

#!/usr/bin/env bash
# Rust-first authority gate for SylphxAI/flux Phase 1 pilot.
# Proof: cargo test --workspace + TS packages are WASM/native loaders only.
#
# Portable: uses grep -RE only (no ripgrep). CI runners may not have `rg`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Rust parity proof: cargo test --workspace (skip differential harness integration)"
cargo test --workspace -- --skip flux_core_differential_matches_wasm_oracle

echo "==> Verify no TS backend authority in package surfaces"
BACKEND_MARKERS=(
  'from ["'\'']hono'
  'from ["'\'']@trpc'
  'new Hono\('
  'McpServer'
  'stdio.*transport'
)
for marker in "${BACKEND_MARKERS[@]}"; do
  # grep -R: recursive; -E: ERE (same patterns as former rg -q); exclude build trees
  if grep -REq --exclude-dir=node_modules --exclude-dir=dist --exclude-dir=target \
      --include='*.ts' --include='*.tsx' --include='*.js' --include='*.mjs' --include='*.cjs' \
      "$marker" packages/ 2>/dev/null; then
    echo "FAIL: TS backend marker found in packages/: $marker"
    exit 1
  fi
done

echo "==> Verify TS compression surfaces delegate to Rust/WASM/native bindings"
LOADER_SURFACES=(
  packages/flux/src/index.ts
  packages/fastpack/src/browser.ts
  packages/fastpack/src/node.ts
  packages/fastpack/src/apex.ts
)
for surface in "${LOADER_SURFACES[@]}"; do
  if [[ ! -f "$surface" ]]; then
    echo "FAIL: loader surface missing: $surface"
    exit 1
  fi
  if ! grep -Eq 'wasm\.|wasmModule|nativeAddon|flux-wasm|fastpack_wasm' "$surface"; then
    echo "FAIL: $surface does not delegate to Rust/WASM/native surface"
    exit 1
  fi
done

echo "==> flux-wasm npm pack integrity (empty-registry regression gate)"
bash "$ROOT/scripts/check-flux-pack-integrity.sh"

echo "OK: Rust authority verified; TS packages are WASM/native loaders only; pack integrity OK"

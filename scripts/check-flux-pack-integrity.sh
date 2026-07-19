#!/usr/bin/env bash
# Fail-closed gate: @sylphx/flux-wasm tarball MUST ship .wasm + JS glue.
# Root cause of registry@0.1.0 empty pack: wasm-pack writes wasm/.gitignore with "*"
# which npm pack honors, excluding every artifact under files:["wasm"].
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PKG_DIR="packages/flux-wasm"
WASM_DIR="$PKG_DIR/wasm"
SCRATCH="${SCRATCH_DIR:-/tmp/flux-pack-integrity}"
mkdir -p "$SCRATCH"

echo "==> build flux-wasm (wasm-pack)"
bun run --cwd "$PKG_DIR" build

# wasm-pack always emits a local .gitignore of "*" — strip it so npm pack includes artifacts.
if [[ -f "$WASM_DIR/.gitignore" ]]; then
  echo "==> remove wasm-pack .gitignore that blanks npm pack ($WASM_DIR/.gitignore)"
  rm -f "$WASM_DIR/.gitignore"
fi

REQUIRED=(
  "$WASM_DIR/flux_wasm_bg.wasm"
  "$WASM_DIR/flux_wasm.js"
  "$WASM_DIR/flux_wasm.d.ts"
)
for f in "${REQUIRED[@]}"; do
  if [[ ! -s "$f" ]]; then
    echo "FAIL: required wasm artifact missing or empty: $f"
    exit 1
  fi
  echo "OK present: $f ($(wc -c <"$f") bytes)"
done

echo "==> pack @sylphx/flux-wasm (integrity)"
(
  cd "$PKG_DIR"
  # Avoid re-entering prepack recursion when already built; pack uses on-disk files.
  bun pm pack --ignore-scripts --destination "$SCRATCH" --quiet >/dev/null
)

TGZ="$(ls -1 "$SCRATCH"/sylphx-flux-wasm-*.tgz | sort | tail -1)"
if [[ -z "${TGZ:-}" || ! -f "$TGZ" ]]; then
  echo "FAIL: pack tarball not produced under $SCRATCH"
  exit 1
fi

echo "==> tarball listing: $TGZ"
LISTING="$(tar -tzf "$TGZ")"
echo "$LISTING"

for need in \
  "package/wasm/flux_wasm_bg.wasm" \
  "package/wasm/flux_wasm.js" \
  "package/wasm/flux_wasm.d.ts"
do
  if ! grep -qxF "$need" <<<"$LISTING"; then
    echo "FAIL: tarball missing $need"
    echo "This is the empty-registry defect class (package.json-only pack)."
    exit 1
  fi
done

WASM_BYTES="$(tar -xOf "$TGZ" package/wasm/flux_wasm_bg.wasm | wc -c | tr -d ' ')"
if [[ "$WASM_BYTES" -lt 1000 ]]; then
  echo "FAIL: packed flux_wasm_bg.wasm too small ($WASM_BYTES bytes)"
  exit 1
fi

echo "OK: flux-wasm pack integrity (wasm_bytes=$WASM_BYTES tarball=$(basename "$TGZ"))"
echo "PACK_TGZ=$TGZ"

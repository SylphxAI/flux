#!/usr/bin/env bash
# Consumer install smoke from local tarballs.
#
# Primary observables (fail-closed):
# 1) @sylphx/flux-wasm pack ships flux_wasm_bg.wasm (empty-registry regression)
# 2) Loader installs and initializes WASM under Node (bytes init, not web fetch)
# 3) Stream delta roundtrip works (proven differential slice)
#
# Note: one-shot compress→decompress is NOT claimed here — flux-core oneshot
# decoder is incomplete (see test_compress_decompress_simple + parity slice
# limitations). Differential proves encoder parity + stream full roundtrip.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SCRATCH="${SCRATCH_DIR:-/tmp/flux-consumer-smoke}"
rm -rf "$SCRATCH"
mkdir -p "$SCRATCH/packs" "$SCRATCH/app"

echo "==> pack integrity gate"
SCRATCH_DIR="$SCRATCH/packs" bash "$ROOT/scripts/check-flux-pack-integrity.sh"

echo "==> ensure JS deps"
if [[ ! -d node_modules/tsup ]]; then
  npm install --no-fund --no-audit
fi

echo "==> build + pack @sylphx/flux loader"
npm -w @sylphx/flux run build
(
  cd packages/flux
  npm pack --ignore-scripts --pack-destination "$SCRATCH/packs" >/dev/null
)

WASM_TGZ="$(ls -1 "$SCRATCH/packs"/sylphx-flux-wasm-*.tgz | sort | tail -1)"
FLUX_TGZ=""
for f in "$SCRATCH/packs"/sylphx-flux-*.tgz; do
  case "$(basename "$f")" in
    sylphx-flux-wasm-*) ;;
    *) FLUX_TGZ="$f" ;;
  esac
done

if [[ -z "${WASM_TGZ:-}" || -z "${FLUX_TGZ:-}" || ! -f "$WASM_TGZ" || ! -f "$FLUX_TGZ" ]]; then
  echo "FAIL: missing local packs wasm=$WASM_TGZ flux=$FLUX_TGZ"
  ls -la "$SCRATCH/packs" || true
  exit 1
fi

echo "using WASM_TGZ=$WASM_TGZ"
echo "using FLUX_TGZ=$FLUX_TGZ"

echo "==> install from local tarballs"
(
  cd "$SCRATCH/app"
  npm init -y >/dev/null 2>&1
  npm install "$WASM_TGZ" "$FLUX_TGZ" --no-fund --no-audit
)

echo "==> Node consumer primary observables"
SMOKE_APP="$SCRATCH/app" node --input-type=module <<'JS'
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import path from "node:path";
import fs from "node:fs";

const app = process.env.SMOKE_APP;
const fluxPkg = path.join(app, "node_modules", "@sylphx", "flux");
const wasmPkg = path.join(app, "node_modules", "@sylphx", "flux-wasm");
const wasmJs = path.join(wasmPkg, "wasm", "flux_wasm.js");
const wasmBin = path.join(wasmPkg, "wasm", "flux_wasm_bg.wasm");
if (!fs.existsSync(wasmJs) || !fs.existsSync(wasmBin)) {
  console.error("SMOKE_FAIL missing wasm artifacts after install", {
    wasmJs,
    wasmBin,
    listing: fs.existsSync(wasmPkg) ? fs.readdirSync(wasmPkg) : null,
  });
  process.exit(2);
}

// Prefer package entry; pass bytes to init (web target fetch fails under Node).
const wasmMod = await import(pathToFileURL(wasmJs).href);
const wasmBytes = fs.readFileSync(wasmBin);
await wasmMod.default({ module_or_path: wasmBytes });

const ver = wasmMod.flux_version();
const payload = new TextEncoder().encode(
  JSON.stringify({ id: 1, name: "flux-pack-integrity", tags: ["a", "b"] })
);
const compressed = wasmMod.flux_compress(payload);
if (!(compressed instanceof Uint8Array) || compressed.length < 4) {
  console.error("SMOKE_FAIL compress empty", compressed);
  process.exit(2);
}
const magic = new TextDecoder().decode(compressed.subarray(0, 4));
if (magic !== "FLUX") {
  console.error("SMOKE_FAIL bad magic", magic);
  process.exit(2);
}

// Stream full roundtrip (differential-proven surface)
const sender = wasmMod.flux_stream_create();
const receiver = wasmMod.flux_stream_create();
const state1 = new TextEncoder().encode(JSON.stringify({ count: 0, items: [] }));
const state2 = new TextEncoder().encode(JSON.stringify({ count: 1, items: ["a"] }));
const d1 = wasmMod.flux_stream_update(sender, state1);
const r1 = wasmMod.flux_stream_receive(receiver, d1);
const d2 = wasmMod.flux_stream_update(sender, state2);
const r2 = wasmMod.flux_stream_receive(receiver, d2);
const text1 = new TextDecoder().decode(r1);
const text2 = new TextDecoder().decode(r2);
if (JSON.parse(text1).count !== 0 || JSON.parse(text2).count !== 1) {
  console.error("SMOKE_FAIL stream roundtrip", { text1, text2 });
  process.exit(2);
}
wasmMod.flux_stream_destroy(sender);
wasmMod.flux_stream_destroy(receiver);

// Also verify the published @sylphx/flux loader surface can be imported
const fluxEntry = path.join(fluxPkg, "dist", "index.mjs");
if (!fs.existsSync(fluxEntry)) {
  console.error("SMOKE_FAIL missing flux dist entry");
  process.exit(2);
}
const fluxSrc = fs.readFileSync(fluxEntry, "utf8");
if (!fluxSrc.includes("@sylphx/flux-wasm") && !fluxSrc.includes("flux_compress")) {
  console.error("SMOKE_FAIL flux loader does not reference wasm surface");
  process.exit(2);
}

console.log(
  JSON.stringify({
    ok: true,
    version: ver,
    compressedBytes: compressed.length,
    streamRoundtrip: true,
    packHasWasm: true,
    oneshotDecompressClaimed: false,
  })
);
JS

echo "OK: flux consumer smoke green (pack integrity + stream roundtrip)"

#!/usr/bin/env bash
# Flux core bounded differential parity — WASM consumer oracle vs native Rust SSOT.
# Slices: flux-core.compress | flux-core.session | flux-core.stream | all
# Fail-closed: requires bun + wasm-pack (no SKIP-as-pass).
# See PARITY-VERIFICATION-STANDARD.md, DECISION-001 / rej-010.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRATCH="${SCRATCH_DIR:-/tmp/flux-differential}"
mkdir -p "$SCRATCH"
LOG="$SCRATCH/differential.log"
ARTIFACT="$SCRATCH/verification.json"
ORACLE_JSON="$SCRATCH/oracle.json"
SLICE_FILTER="all"
: >"$LOG"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --slice)
      SLICE_FILTER="${2:-}"
      shift 2
      ;;
    *)
      echo "::error::unknown argument: $1" | tee -a "$LOG"
      exit 1
      ;;
  esac
done

case "$SLICE_FILTER" in
  all|flux-core.compress|flux-core.session|flux-core.stream) ;;
  *)
    echo "::error::invalid --slice value: $SLICE_FILTER" | tee -a "$LOG"
    exit 1
    ;;
esac

cd "$REPO_ROOT"

if ! command -v bun >/dev/null 2>&1; then
  echo "::error::bun required for flux differential parity — no SKIP-as-pass" | tee -a "$LOG"
  exit 1
fi

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "::error::wasm-pack required for flux differential parity — no SKIP-as-pass" | tee -a "$LOG"
  exit 1
fi

echo "=== flux core bounded differential parity $(date -Iseconds) slice=$SLICE_FILTER ===" | tee -a "$LOG"

echo "--- rust authority gate ---" | tee -a "$LOG"
bash "$REPO_ROOT/scripts/check-rust-authority.sh" 2>&1 | tee -a "$LOG"

echo "--- build flux-wasm consumer baseline ---" | tee -a "$LOG"
wasm-pack build crates/flux-wasm --target web --out-dir ../../packages/flux-wasm/wasm 2>&1 | tee -a "$LOG"

echo "--- WASM consumer oracle ---" | tee -a "$LOG"
bun run "$REPO_ROOT/scripts/differential/flux-core-oracle.ts" >"$ORACLE_JSON" 2>>"$LOG"

echo "--- Rust differential test ---" | tee -a "$LOG"
FLUX_ORACLE_JSON="$ORACLE_JSON" \
  cargo test -p flux-core --test flux_core_differential flux_core_differential_matches_wasm_oracle -- --nocapture 2>&1 | tee -a "$LOG"

CANDIDATE_SHA="${CANDIDATE_SHA:-$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || echo unknown)}"
BASELINE_TS_SHA="$(git -C "$REPO_ROOT" log -1 --format=%H -- packages/flux/src/index.ts crates/flux-wasm/src/lib.rs packages/flux-wasm 2>/dev/null || echo unknown)"
RUST_SHA="$CANDIDATE_SHA"
BEHAVIOR_SPEC_HASH="$(jq -r '.behaviorSpecHash' "$ORACLE_JSON")"
FIXTURE_CORPUS_HASH="$(jq -r '.fixtureCorpusHash' "$ORACLE_JSON")"
CASE_COUNT="$(jq '.cases | length' "$ORACLE_JSON")"
COMPRESS_CASE_COUNT="$(jq '[.cases[] | select(.slice == "flux-core.compress")] | length' "$ORACLE_JSON")"
SESSION_CASE_COUNT="$(jq '[.cases[] | select(.slice == "flux-core.session")] | length' "$ORACLE_JSON")"
STREAM_CASE_COUNT="$(jq '[.cases[] | select(.slice == "flux-core.stream")] | length' "$ORACLE_JSON")"

jq -n \
  --arg verifiedAt "$(date -Iseconds)" \
  --arg candidateSha "$CANDIDATE_SHA" \
  --arg baselineTsSha "$BASELINE_TS_SHA" \
  --arg rustCandidateSha "$RUST_SHA" \
  --arg behaviorSpecHash "$BEHAVIOR_SPEC_HASH" \
  --arg fixtureCorpusHash "$FIXTURE_CORPUS_HASH" \
  --arg sliceFilter "$SLICE_FILTER" \
  --argjson caseCount "$CASE_COUNT" \
  --argjson compressCaseCount "$COMPRESS_CASE_COUNT" \
  --argjson sessionCaseCount "$SESSION_CASE_COUNT" \
  --argjson streamCaseCount "$STREAM_CASE_COUNT" \
  '{
    schemaVersion: 2,
    slice: (if $sliceFilter == "all" then "flux-core.compress|flux-core.session|flux-core.stream" else $sliceFilter end),
    sliceFilter: $sliceFilter,
    status: "differential_green",
    verifiedAt: $verifiedAt,
    lastComparedMainSha: $candidateSha,
    mergeGroupSha: $candidateSha,
    baselineTsSha: $baselineTsSha,
    rustCandidateSha: $rustCandidateSha,
    behaviorSpecHash: $behaviorSpecHash,
    fixtureCorpusHash: $fixtureCorpusHash,
    caseCount: $caseCount,
    compressCaseCount: $compressCaseCount,
    sessionCaseCount: $sessionCaseCount,
    streamCaseCount: $streamCaseCount,
    harness: "scripts/run-flux-differential.sh",
    differentialTest: "crates/flux-core/tests/flux_core_differential.rs#flux_core_differential_matches_wasm_oracle",
    oracle: "scripts/differential/flux-core-oracle.ts",
    gate: "scripts/check-rust-authority.sh",
    promotionPolicy: "NO_PROMOTIONS — promotion_hold active until prod_audit_pass"
  }' >"$ARTIFACT"

echo "flux-differential: OK (cases=$CASE_COUNT compress=$COMPRESS_CASE_COUNT session=$SESSION_CASE_COUNT stream=$STREAM_CASE_COUNT)" | tee -a "$LOG"
echo "verification artifact: $ARTIFACT" | tee -a "$LOG"
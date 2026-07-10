#!/usr/bin/env bun
/**
 * WASM consumer oracle for flux-core bounded differential parity (rej-010).
 *
 * Runs packages/flux-wasm consumer path (same surface as packages/flux) against the
 * frozen flux-core corpus. Emits canonical JSON consumed by
 * crates/flux-core/tests/flux_core_differential.rs.
 *
 * Compress/session slices prove encoder parity (compressedHex). Stream slice proves
 * full delta roundtrip. One-shot decompress roundtrip is intentionally out of scope
 * at this bound — see docs/specs/flux-core-parity-slice.json.
 *
 * Fail-closed: requires bun + built flux-wasm — no SKIP-as-pass.
 */
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, '../..');
const CORPUS_PATH = join(__dirname, 'fixtures/flux-core-corpus.json');
const WASM_PKG = join(REPO_ROOT, 'packages/flux-wasm/wasm/flux_wasm.js');

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

interface CorpusCase {
  readonly id: string;
  readonly slice: 'flux-core.compress' | 'flux-core.session' | 'flux-core.stream';
  readonly domain: string;
  readonly input: Record<string, Json>;
}

interface CorpusManifest {
  readonly corpusVersion: number;
  readonly slice: string;
  readonly cases: readonly CorpusCase[];
}

interface FluxWasm {
  default?: () => Promise<void>;
  flux_compress: (data: Uint8Array) => Uint8Array;
  flux_session_create: () => number;
  flux_session_compress: (sessionId: number, data: Uint8Array) => Uint8Array;
  flux_session_stats: (sessionId: number) => string;
  flux_session_destroy: (sessionId: number) => boolean;
  flux_stream_create: () => number;
  flux_stream_update: (sessionId: number, data: Uint8Array) => Uint8Array;
  flux_stream_receive: (sessionId: number, data: Uint8Array) => Uint8Array;
  flux_stream_stats: (sessionId: number) => string;
  flux_stream_destroy: (sessionId: number) => boolean;
}

export interface DifferentialCase {
  readonly id: string;
  readonly slice: CorpusCase['slice'];
  readonly domain: string;
  readonly input: Record<string, Json>;
  readonly output: Json;
}

export interface DifferentialCorpus {
  readonly corpusVersion: number;
  readonly slice: string;
  readonly fixtureCorpusHash: string;
  readonly behaviorSpecHash: string;
  readonly cases: readonly DifferentialCase[];
}

function sha256Hex(content: string): string {
  return createHash('sha256').update(content).digest('hex');
}

function toHex(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString('hex');
}

function parseJsonBytes(bytes: Uint8Array): Json {
  return JSON.parse(new TextDecoder().decode(bytes)) as Json;
}

async function loadWasm(): Promise<FluxWasm> {
  const wasm = (await import(WASM_PKG)) as FluxWasm;
  if (wasm.default) {
    await wasm.default();
  }
  return wasm;
}

function runCompressCase(wasm: FluxWasm, corpusCase: CorpusCase): DifferentialCase {
  const json = corpusCase.input.json as string;
  const inputBytes = new TextEncoder().encode(json);
  const compressed = wasm.flux_compress(inputBytes);
  return {
    ...corpusCase,
    output: {
      compressedHex: toHex(compressed),
      inputJson: JSON.parse(json) as Json,
    },
  };
}

function runSessionCase(wasm: FluxWasm, corpusCase: CorpusCase): DifferentialCase {
  const messages = corpusCase.input.messages as string[];
  const sessionId = wasm.flux_session_create();
  try {
    const compressedHex = messages.map((message) => {
      const bytes = new TextEncoder().encode(message);
      return toHex(wasm.flux_session_compress(sessionId, bytes));
    });
    const stats = JSON.parse(wasm.flux_session_stats(sessionId)) as Json;
    return {
      ...corpusCase,
      output: {
        compressedHex,
        stats,
      },
    };
  } finally {
    wasm.flux_session_destroy(sessionId);
  }
}

function runStreamCase(wasm: FluxWasm, corpusCase: CorpusCase): DifferentialCase {
  const states = corpusCase.input.states as string[];
  const senderId = wasm.flux_stream_create();
  const receiverId = wasm.flux_stream_create();
  try {
    const receivedJson = states.map((state) => {
      const delta = wasm.flux_stream_update(senderId, new TextEncoder().encode(state));
      const received = wasm.flux_stream_receive(receiverId, delta);
      return parseJsonBytes(received);
    });
    const stats = JSON.parse(wasm.flux_stream_stats(senderId)) as Json;
    return {
      ...corpusCase,
      output: {
        receivedJson,
        stats,
      },
    };
  } finally {
    wasm.flux_stream_destroy(senderId);
    wasm.flux_stream_destroy(receiverId);
  }
}

async function buildOracle(): Promise<DifferentialCorpus> {
  const corpusRaw = readFileSync(CORPUS_PATH, 'utf8');
  const corpus = JSON.parse(corpusRaw) as CorpusManifest;
  if (corpus.corpusVersion !== 1) {
    throw new Error(`unsupported corpusVersion: ${corpus.corpusVersion}`);
  }

  const wasm = await loadWasm();
  const cases = corpus.cases.map((corpusCase) => {
    switch (corpusCase.slice) {
      case 'flux-core.compress':
        return runCompressCase(wasm, corpusCase);
      case 'flux-core.session':
        return runSessionCase(wasm, corpusCase);
      case 'flux-core.stream':
        return runStreamCase(wasm, corpusCase);
      default:
        throw new Error(`unsupported slice: ${(corpusCase as CorpusCase).slice}`);
    }
  });

  const fixtureCorpusHash = sha256Hex(corpusRaw);
  const behaviorSpecHash = sha256Hex(corpusRaw + JSON.stringify(cases));

  return {
    corpusVersion: 1,
    slice: corpus.slice,
    fixtureCorpusHash,
    behaviorSpecHash,
    cases,
  };
}

const oracle = await buildOracle();
process.stdout.write(`${JSON.stringify(oracle)}\n`);
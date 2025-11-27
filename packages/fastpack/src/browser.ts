/**
 * Browser implementation using WebAssembly
 */

import type { CompressInput, CompressOptions, CompressResult } from './types';
import { normalizeInput } from './types';

// WASM module - will be loaded lazily
let wasmModule: typeof import('../wasm/fastpack_wasm') | null = null;
let wasmInitPromise: Promise<void> | null = null;

/**
 * Initialize WASM module
 */
async function initWasm(): Promise<void> {
  if (wasmModule) return;

  if (!wasmInitPromise) {
    wasmInitPromise = (async () => {
      // Dynamic import of WASM module
      const wasm = await import('../wasm/fastpack_wasm');
      await wasm.default();
      wasmModule = wasm;
    })();
  }

  await wasmInitPromise;
}

/**
 * Compress data
 */
export async function compress(
  input: CompressInput,
  options: CompressOptions = {}
): Promise<CompressResult> {
  await initWasm();

  const data = normalizeInput(input);
  const level = options.level ?? 1;

  return wasmModule!.compress_with_level(data, level);
}

/**
 * Decompress data
 */
export async function decompress(input: Uint8Array): Promise<CompressResult> {
  await initWasm();
  return wasmModule!.decompress(input);
}

/**
 * Create a compression TransformStream
 */
export function createCompressStream(
  options: CompressOptions = {}
): TransformStream<Uint8Array, Uint8Array> {
  const level = options.level ?? 1;
  let initialized = false;

  return new TransformStream({
    async start() {
      await initWasm();
      initialized = true;
    },
    async transform(chunk, controller) {
      if (!initialized) await initWasm();
      const compressed = wasmModule!.compress_with_level(chunk, level);
      controller.enqueue(compressed);
    },
  });
}

/**
 * Create a decompression TransformStream
 */
export function createDecompressStream(): TransformStream<Uint8Array, Uint8Array> {
  let initialized = false;

  return new TransformStream({
    async start() {
      await initWasm();
      initialized = true;
    },
    async transform(chunk, controller) {
      if (!initialized) await initWasm();
      const decompressed = wasmModule!.decompress(chunk);
      controller.enqueue(decompressed);
    },
  });
}

/**
 * Get library version
 */
export async function version(): Promise<string> {
  await initWasm();
  return wasmModule!.version();
}

// Re-export types
export type { CompressInput, CompressOptions, CompressResult } from './types';

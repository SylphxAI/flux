/**
 * Node.js implementation
 *
 * Uses native addon when available, falls back to WASM
 */

import type { CompressInput, CompressOptions, CompressResult } from './types';
import { normalizeInput } from './types';

// Try to load native addon, fall back to WASM
let nativeAddon: {
  compressSync: (data: Buffer) => Buffer;
  decompressSyncWithLevel: (data: Buffer, level: number) => Buffer;
  decompressSync: (data: Buffer) => Buffer;
  version: () => string;
} | null = null;

let wasmModule: typeof import('../wasm/fastpack_wasm') | null = null;

// Try to load native addon
try {
  // This will be replaced with actual native addon import
  // nativeAddon = require('../native/fastpack.node');
} catch {
  // Native addon not available
}

async function getWasm() {
  if (!wasmModule) {
    const wasm = await import('../wasm/fastpack_wasm');

    // In Node.js, we need to load the WASM file manually
    const fs = await import('fs');
    const path = await import('path');
    const url = await import('url');

    // Get the directory of the WASM file
    const __dirname = path.dirname(url.fileURLToPath(import.meta.url));
    const wasmPath = path.join(__dirname, '..', 'wasm', 'fastpack_wasm_bg.wasm');

    const wasmBytes = fs.readFileSync(wasmPath);
    await wasm.default(wasmBytes);
    wasmModule = wasm;
  }
  return wasmModule;
}

/**
 * Compress data
 */
export async function compress(
  input: CompressInput,
  options: CompressOptions = {}
): Promise<CompressResult> {
  const data = normalizeInput(input);
  const level = options.level ?? 1;

  if (nativeAddon) {
    return new Uint8Array(nativeAddon.decompressSyncWithLevel(Buffer.from(data), level));
  }

  const wasm = await getWasm();
  return wasm.compress_with_level(data, level);
}

/**
 * Compress data synchronously (Node.js only)
 */
export function compressSync(
  input: CompressInput,
  options: CompressOptions = {}
): CompressResult {
  if (!nativeAddon) {
    throw new Error('Sync operations require native addon. Use async compress() instead.');
  }

  const data = normalizeInput(input);
  const level = options.level ?? 1;
  return new Uint8Array(nativeAddon.decompressSyncWithLevel(Buffer.from(data), level));
}

/**
 * Decompress data
 */
export async function decompress(input: Uint8Array): Promise<CompressResult> {
  if (nativeAddon) {
    return new Uint8Array(nativeAddon.decompressSync(Buffer.from(input)));
  }

  const wasm = await getWasm();
  return wasm.decompress(input);
}

/**
 * Decompress data synchronously (Node.js only)
 */
export function decompressSync(input: Uint8Array): CompressResult {
  if (!nativeAddon) {
    throw new Error('Sync operations require native addon. Use async decompress() instead.');
  }

  return new Uint8Array(nativeAddon.decompressSync(Buffer.from(input)));
}

/**
 * Create a compression Transform stream
 */
export function createCompressStream(options: CompressOptions = {}) {
  const { Transform } = require('stream');
  const level = options.level ?? 1;

  return new Transform({
    async transform(
      chunk: Buffer,
      _encoding: string,
      callback: (error?: Error | null, data?: Buffer) => void
    ) {
      try {
        if (nativeAddon) {
          callback(null, nativeAddon.decompressSyncWithLevel(chunk, level));
        } else {
          const wasm = await getWasm();
          callback(null, Buffer.from(wasm.compress_with_level(new Uint8Array(chunk), level)));
        }
      } catch (err) {
        callback(err as Error);
      }
    },
  });
}

/**
 * Create a decompression Transform stream
 */
export function createDecompressStream() {
  const { Transform } = require('stream');

  return new Transform({
    async transform(
      chunk: Buffer,
      _encoding: string,
      callback: (error?: Error | null, data?: Buffer) => void
    ) {
      try {
        if (nativeAddon) {
          callback(null, nativeAddon.decompressSync(chunk));
        } else {
          const wasm = await getWasm();
          callback(null, Buffer.from(wasm.decompress(new Uint8Array(chunk))));
        }
      } catch (err) {
        callback(err as Error);
      }
    },
  });
}

/**
 * Get library version
 */
export async function version(): Promise<string> {
  if (nativeAddon) {
    return nativeAddon.version();
  }
  const wasm = await getWasm();
  return wasm.version();
}

// Re-export types
export type { CompressInput, CompressOptions, CompressResult } from './types';

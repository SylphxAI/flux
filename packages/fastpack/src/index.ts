/**
 * FastPack - High-performance compression library
 *
 * Auto-detects environment and uses the appropriate implementation:
 * - Browser: WebAssembly
 * - Node.js: Native addon (with WASM fallback)
 *
 * @example
 * ```typescript
 * import { compress, decompress } from 'fastpack';
 *
 * const data = new TextEncoder().encode('Hello, World!');
 * const compressed = await compress(data);
 * const original = await decompress(compressed);
 * ```
 */

import type { CompressInput, CompressOptions, CompressResult } from './types';

// Detect environment
const isBrowser = typeof window !== 'undefined' && typeof window.document !== 'undefined';
const isNode = typeof process !== 'undefined' && process.versions?.node;

// Lazy load implementation
let impl: {
  compress: (input: CompressInput, options?: CompressOptions) => Promise<CompressResult>;
  decompress: (input: Uint8Array) => Promise<CompressResult>;
  createCompressStream: (options?: CompressOptions) => unknown;
  createDecompressStream: () => unknown;
  version: () => Promise<string>;
} | null = null;

async function getImpl() {
  if (impl) return impl;

  if (isBrowser) {
    impl = await import('./browser');
  } else if (isNode) {
    impl = await import('./node');
  } else {
    // Fallback to browser (WASM)
    impl = await import('./browser');
  }

  return impl;
}

/**
 * Compress data
 *
 * @param input - Data to compress (Uint8Array, ArrayBuffer, or string)
 * @param options - Compression options
 * @returns Compressed data
 *
 * @example
 * ```typescript
 * const compressed = await compress('Hello, World!');
 * const compressed = await compress(data, { level: 2 });
 * ```
 */
export async function compress(
  input: CompressInput,
  options: CompressOptions = {}
): Promise<CompressResult> {
  const i = await getImpl();
  return i.compress(input, options);
}

/**
 * Decompress data
 *
 * @param input - Compressed data
 * @returns Original data
 *
 * @example
 * ```typescript
 * const original = await decompress(compressed);
 * ```
 */
export async function decompress(input: Uint8Array): Promise<CompressResult> {
  const i = await getImpl();
  return i.decompress(input);
}

/**
 * Create a compression stream
 *
 * @param options - Compression options
 * @returns TransformStream (browser) or Transform (Node.js)
 *
 * @example
 * ```typescript
 * // Browser
 * const stream = createCompressStream();
 * response.body.pipeThrough(stream);
 *
 * // Node.js
 * const stream = createCompressStream();
 * inputStream.pipe(stream).pipe(outputStream);
 * ```
 */
export async function createCompressStream(options: CompressOptions = {}) {
  const i = await getImpl();
  return i.createCompressStream(options);
}

/**
 * Create a decompression stream
 *
 * @returns TransformStream (browser) or Transform (Node.js)
 */
export async function createDecompressStream() {
  const i = await getImpl();
  return i.createDecompressStream();
}

/**
 * Get library version
 */
export async function version(): Promise<string> {
  const i = await getImpl();
  return i.version();
}

// Re-export types
export type { CompressInput, CompressOptions, CompressResult } from './types';
export { normalizeInput } from './types';

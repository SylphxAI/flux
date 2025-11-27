/**
 * APEX - Advanced JSON-aware compression
 *
 * Features:
 * - Structural compression (separates JSON keys from values)
 * - Session-based learning (improves ratio over time)
 * - Optimized for API responses
 */

import type { CompressInput, CompressResult } from './types';
import { normalizeInput } from './types';

// WASM module
let wasmModule: typeof import('../wasm/fastpack_wasm') | null = null;
let wasmInitPromise: Promise<void> | null = null;

async function getWasm() {
  if (wasmModule) return wasmModule;

  if (!wasmInitPromise) {
    wasmInitPromise = (async () => {
      const wasm = await import('../wasm/fastpack_wasm');

      // Handle Node.js vs Browser
      if (typeof window === 'undefined') {
        const fs = await import('fs');
        const path = await import('path');
        const url = await import('url');
        const __dirname = path.dirname(url.fileURLToPath(import.meta.url));
        const wasmPath = path.join(__dirname, '..', 'wasm', 'fastpack_wasm_bg.wasm');
        const wasmBytes = fs.readFileSync(wasmPath);
        await wasm.default(wasmBytes);
      } else {
        await wasm.default();
      }

      wasmModule = wasm;
    })();
  }

  await wasmInitPromise;
  return wasmModule!;
}

/**
 * APEX compression options
 */
export interface ApexOptions {
  /**
   * Enable structural compression (better for JSON)
   * @default true
   */
  structural?: boolean;
}

/**
 * APEX session statistics
 */
export interface ApexSessionStats {
  messageCount: number;
  dictionarySize: number;
  templateCount: number;
}

/**
 * Compress data using APEX algorithm
 *
 * @example
 * ```typescript
 * import { apexCompress, apexDecompress } from 'fastpack/apex';
 *
 * const json = JSON.stringify({ id: 1, name: 'test' });
 * const compressed = await apexCompress(json);
 * const original = await apexDecompress(compressed);
 * ```
 */
export async function apexCompress(
  input: CompressInput,
  options: ApexOptions = {}
): Promise<CompressResult> {
  const wasm = await getWasm();
  const data = normalizeInput(input);
  const structural = options.structural ?? true;
  return wasm.apex_compress(data, structural);
}

/**
 * Decompress APEX data
 */
export async function apexDecompress(input: Uint8Array): Promise<CompressResult> {
  const wasm = await getWasm();
  return wasm.apex_decompress(input);
}

/**
 * Check if data looks like JSON
 */
export async function isJson(input: CompressInput): Promise<boolean> {
  const wasm = await getWasm();
  const data = normalizeInput(input);
  return wasm.is_json(data);
}

/**
 * Get recommended algorithm for data
 * @returns 'apex' for larger JSON, 'lz4' otherwise
 */
export async function recommendAlgorithm(input: CompressInput): Promise<'apex' | 'lz4'> {
  const wasm = await getWasm();
  const data = normalizeInput(input);
  return wasm.recommend_algorithm(data) as 'apex' | 'lz4';
}

/**
 * APEX Session for stateful compression with learning
 *
 * Sessions learn patterns across multiple compress calls,
 * improving compression ratio over time for similar data.
 *
 * @example
 * ```typescript
 * const session = await ApexSession.create();
 *
 * // Each call learns from previous data
 * const c1 = await session.compress(request1);
 * const c2 = await session.compress(request2); // Better ratio!
 * const c3 = await session.compress(request3); // Even better!
 *
 * // Get stats
 * const stats = await session.stats();
 * console.log(`Processed ${stats.messageCount} messages`);
 *
 * // Clean up when done
 * await session.destroy();
 * ```
 */
export class ApexSession {
  private sessionId: number;
  private destroyed = false;

  private constructor(sessionId: number) {
    this.sessionId = sessionId;
  }

  /**
   * Create a new APEX session
   */
  static async create(): Promise<ApexSession> {
    const wasm = await getWasm();
    const id = wasm.apex_session_create();
    return new ApexSession(id);
  }

  /**
   * Compress data using this session
   */
  async compress(input: CompressInput, options: ApexOptions = {}): Promise<CompressResult> {
    if (this.destroyed) {
      throw new Error('Session has been destroyed');
    }
    const wasm = await getWasm();
    const data = normalizeInput(input);
    const structural = options.structural ?? true;
    return wasm.apex_session_compress(this.sessionId, data, structural);
  }

  /**
   * Decompress data using this session
   */
  async decompress(input: Uint8Array): Promise<CompressResult> {
    if (this.destroyed) {
      throw new Error('Session has been destroyed');
    }
    const wasm = await getWasm();
    return wasm.apex_session_decompress(this.sessionId, input);
  }

  /**
   * Get session statistics
   */
  async stats(): Promise<ApexSessionStats> {
    if (this.destroyed) {
      throw new Error('Session has been destroyed');
    }
    const wasm = await getWasm();
    const json = wasm.apex_session_stats(this.sessionId);
    return JSON.parse(json as string);
  }

  /**
   * Destroy this session and free resources
   */
  async destroy(): Promise<void> {
    if (this.destroyed) return;
    const wasm = await getWasm();
    wasm.apex_session_destroy(this.sessionId);
    this.destroyed = true;
  }
}

/**
 * Auto-select best algorithm and compress
 *
 * Uses APEX for larger JSON, LZ4 for other data
 */
export async function autoCompress(input: CompressInput): Promise<CompressResult> {
  const algorithm = await recommendAlgorithm(input);

  if (algorithm === 'apex') {
    return apexCompress(input);
  } else {
    // Use standard LZ4
    const { compress } = await import('./index');
    return compress(input);
  }
}

// ============================================================================
// ANS Entropy Coding (standalone)
// ============================================================================

/**
 * Compress data using ANS (Asymmetric Numeral Systems) entropy coding
 *
 * ANS provides near-optimal compression for data with skewed byte
 * frequency distributions. Best for:
 * - Pre-processed data (after structural extraction)
 * - Data with many repeated byte values
 * - Low-entropy binary data
 *
 * @example
 * ```typescript
 * import { ansCompress, ansDecompress } from 'fastpack/apex';
 *
 * const data = new Uint8Array([1, 1, 1, 2, 2, 3]);
 * const compressed = await ansCompress(data);
 * const original = await ansDecompress(compressed);
 * ```
 */
export async function ansCompress(input: CompressInput): Promise<CompressResult> {
  const wasm = await getWasm();
  const data = normalizeInput(input);
  return wasm.ans_compress(data);
}

/**
 * Decompress ANS-encoded data
 */
export async function ansDecompress(input: Uint8Array): Promise<CompressResult> {
  const wasm = await getWasm();
  return wasm.ans_decompress(input);
}

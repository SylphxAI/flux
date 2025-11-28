/**
 * FLUX v2 - Schema-aware JSON compression
 *
 * FLUX is optimized for JSON-based API traffic with features like:
 * - Schema inference and caching
 * - Columnar transformation
 * - Type-specific encoding
 * - Streaming delta compression
 *
 * @example
 * ```typescript
 * import { compress, decompress, FluxSession, FluxStream } from 'flux';
 *
 * // One-shot compression
 * const data = JSON.stringify({ id: 1, name: 'test' });
 * const compressed = await compress(data);
 * const original = await decompress(compressed);
 *
 * // Session-based (schema caching)
 * const session = await FluxSession.create();
 * const c1 = await session.compress({ id: 1, name: 'alice' });
 * const c2 = await session.compress({ id: 2, name: 'bob' }); // Uses cached schema
 *
 * // Streaming delta (real-time updates)
 * const stream = await FluxStream.create();
 * const delta1 = await stream.update({ count: 0 });  // Full state
 * const delta2 = await stream.update({ count: 1 });  // Delta only
 * ```
 */

import type {
  FluxConfig,
  FluxStats,
  FluxStreamStats,
  FluxAnalysis,
  FluxInput,
  FluxResult,
} from './types';
import { normalizeInput } from './types';

// WASM module type
interface FluxWasm {
  flux_compress(data: Uint8Array): Uint8Array;
  flux_decompress(data: Uint8Array): Uint8Array;
  flux_session_create(): number;
  flux_session_create_with_config(
    columnar: boolean,
    entropy: boolean,
    delta: boolean,
    checksum: boolean
  ): number;
  flux_session_compress(sessionId: number, data: Uint8Array): Uint8Array;
  flux_session_decompress(sessionId: number, data: Uint8Array): Uint8Array;
  flux_session_stats(sessionId: number): string;
  flux_session_reset(sessionId: number): void;
  flux_session_destroy(sessionId: number): boolean;
  flux_stream_create(): number;
  flux_stream_update(sessionId: number, data: Uint8Array): Uint8Array;
  flux_stream_receive(sessionId: number, data: Uint8Array): Uint8Array;
  flux_stream_stats(sessionId: number): string;
  flux_stream_reset(sessionId: number): void;
  flux_stream_destroy(sessionId: number): boolean;
  flux_version(): string;
  flux_analyze(data: Uint8Array): string;
}

// Lazy-loaded WASM module
let wasmModule: FluxWasm | null = null;
let wasmLoadPromise: Promise<FluxWasm> | null = null;

async function loadWasm(): Promise<FluxWasm> {
  if (wasmModule) return wasmModule;
  if (wasmLoadPromise) return wasmLoadPromise;

  wasmLoadPromise = (async () => {
    // @ts-ignore - dynamic import of WASM
    const wasm = await import('flux-wasm');
    await wasm.default?.();
    wasmModule = wasm as unknown as FluxWasm;
    return wasmModule;
  })();

  return wasmLoadPromise;
}

// ============================================================================
// One-shot API
// ============================================================================

/**
 * Compress JSON data using FLUX
 *
 * @param input - JSON data to compress (string, Uint8Array, or ArrayBuffer)
 * @returns Compressed data
 *
 * @example
 * ```typescript
 * const compressed = await compress('{"id": 1, "name": "test"}');
 * const compressed = await compress(jsonBytes);
 * ```
 */
export async function compress(input: FluxInput): Promise<FluxResult> {
  const wasm = await loadWasm();
  const data = normalizeInput(input);
  return wasm.flux_compress(data);
}

/**
 * Decompress FLUX data
 *
 * @param data - Compressed FLUX data
 * @returns Original JSON data
 *
 * @example
 * ```typescript
 * const original = await decompress(compressed);
 * const json = new TextDecoder().decode(original);
 * ```
 */
export async function decompress(data: Uint8Array): Promise<FluxResult> {
  const wasm = await loadWasm();
  return wasm.flux_decompress(data);
}

/**
 * Analyze data and estimate compression potential
 *
 * @param input - Data to analyze
 * @returns Analysis results with recommendations
 *
 * @example
 * ```typescript
 * const analysis = await analyze('{"users": [...]}');
 * console.log(analysis.recommended); // 'flux_session'
 * console.log(analysis.estimatedRatio); // 0.45
 * ```
 */
export async function analyze(input: FluxInput): Promise<FluxAnalysis> {
  const wasm = await loadWasm();
  const data = normalizeInput(input);
  const json = wasm.flux_analyze(data);
  return JSON.parse(json);
}

/**
 * Get FLUX library version
 */
export async function version(): Promise<string> {
  const wasm = await loadWasm();
  return wasm.flux_version();
}

// ============================================================================
// Session API (schema caching)
// ============================================================================

/**
 * FLUX compression session with schema caching
 *
 * Use sessions when compressing multiple similar JSON objects.
 * The session caches schemas to avoid retransmitting structure.
 *
 * @example
 * ```typescript
 * const session = await FluxSession.create();
 *
 * // First message includes schema
 * const c1 = await session.compress({ id: 1, name: 'alice' });
 *
 * // Subsequent messages use cached schema
 * const c2 = await session.compress({ id: 2, name: 'bob' });
 *
 * console.log(session.stats()); // { cacheHits: 1, ... }
 *
 * session.destroy();
 * ```
 */
export class FluxSession {
  private wasm: FluxWasm;
  private sessionId: number;

  private constructor(wasm: FluxWasm, sessionId: number) {
    this.wasm = wasm;
    this.sessionId = sessionId;
  }

  /**
   * Create a new FLUX session
   */
  static async create(config?: FluxConfig): Promise<FluxSession> {
    const wasm = await loadWasm();
    const sessionId = config
      ? wasm.flux_session_create_with_config(
          config.columnar ?? true,
          config.entropy ?? true,
          config.delta ?? true,
          config.checksum ?? true
        )
      : wasm.flux_session_create();
    return new FluxSession(wasm, sessionId);
  }

  /**
   * Compress JSON data using session schema cache
   */
  compress(input: FluxInput): FluxResult {
    const data = normalizeInput(input);
    return this.wasm.flux_session_compress(this.sessionId, data);
  }

  /**
   * Decompress FLUX data using session schema cache
   */
  decompress(data: Uint8Array): FluxResult {
    return this.wasm.flux_session_decompress(this.sessionId, data);
  }

  /**
   * Get session statistics
   */
  stats(): FluxStats {
    const json = this.wasm.flux_session_stats(this.sessionId);
    return JSON.parse(json);
  }

  /**
   * Reset session state (clears schema cache)
   */
  reset(): void {
    this.wasm.flux_session_reset(this.sessionId);
  }

  /**
   * Destroy session and free resources
   */
  destroy(): void {
    this.wasm.flux_session_destroy(this.sessionId);
  }
}

// ============================================================================
// Streaming API (delta compression)
// ============================================================================

/**
 * FLUX streaming session for delta compression
 *
 * Use streaming sessions for real-time state updates where only
 * changes between states need to be transmitted.
 *
 * @example
 * ```typescript
 * // Sender
 * const sender = await FluxStream.create();
 * const delta1 = sender.update({ count: 0, items: [] });  // Full state
 * const delta2 = sender.update({ count: 1, items: ['a'] }); // Delta only
 * ws.send(delta2);
 *
 * // Receiver
 * const receiver = await FluxStream.create();
 * ws.onmessage = (event) => {
 *   const state = receiver.receive(event.data);
 *   console.log(JSON.parse(new TextDecoder().decode(state)));
 * };
 * ```
 */
export class FluxStream {
  private wasm: FluxWasm;
  private sessionId: number;

  private constructor(wasm: FluxWasm, sessionId: number) {
    this.wasm = wasm;
    this.sessionId = sessionId;
  }

  /**
   * Create a new streaming session
   */
  static async create(): Promise<FluxStream> {
    const wasm = await loadWasm();
    const sessionId = wasm.flux_stream_create();
    return new FluxStream(wasm, sessionId);
  }

  /**
   * Send state update, returns compressed delta
   *
   * First call returns full state, subsequent calls return only changes.
   */
  update(input: FluxInput): FluxResult {
    const data = normalizeInput(input);
    return this.wasm.flux_stream_update(this.sessionId, data);
  }

  /**
   * Receive delta and reconstruct full state
   */
  receive(data: Uint8Array): FluxResult {
    return this.wasm.flux_stream_receive(this.sessionId, data);
  }

  /**
   * Get streaming session statistics
   */
  stats(): FluxStreamStats {
    const json = this.wasm.flux_stream_stats(this.sessionId);
    return JSON.parse(json);
  }

  /**
   * Reset streaming session state
   */
  reset(): void {
    this.wasm.flux_stream_reset(this.sessionId);
  }

  /**
   * Destroy streaming session and free resources
   */
  destroy(): void {
    this.wasm.flux_stream_destroy(this.sessionId);
  }
}

// Re-export types
export type {
  FluxConfig,
  FluxStats,
  FluxStreamStats,
  FluxAnalysis,
  FluxInput,
  FluxResult,
} from './types';
export { normalizeInput } from './types';

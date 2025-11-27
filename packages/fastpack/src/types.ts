/**
 * Compression options
 */
export interface CompressOptions {
  /**
   * Compression level
   * - 0: No compression (passthrough)
   * - 1: Fast compression (default)
   * - 2: Better compression ratio
   */
  level?: 0 | 1 | 2;

  /**
   * Enable checksum for data integrity
   * @default false
   */
  checksum?: boolean;
}

/**
 * Compression/decompression result type
 */
export type CompressResult = Uint8Array;

/**
 * Input types that can be compressed
 */
export type CompressInput = Uint8Array | ArrayBuffer | string;

/**
 * Normalize input to Uint8Array
 */
export function normalizeInput(input: CompressInput): Uint8Array {
  if (typeof input === 'string') {
    return new TextEncoder().encode(input);
  }
  if (input instanceof ArrayBuffer) {
    return new Uint8Array(input);
  }
  return input;
}

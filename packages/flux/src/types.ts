/**
 * FLUX session configuration
 */
export interface FluxConfig {
  /**
   * Enable columnar transformation
   * @default true
   */
  columnar?: boolean;

  /**
   * Enable entropy coding
   * @default true
   */
  entropy?: boolean;

  /**
   * Enable delta encoding
   * @default true
   */
  delta?: boolean;

  /**
   * Enable checksum
   * @default true
   */
  checksum?: boolean;
}

/**
 * FLUX session statistics
 */
export interface FluxStats {
  messagesProcessed: number;
  bytesIn: number;
  bytesOut: number;
  schemasCached: number;
  cacheHits: number;
  cacheMisses: number;
  compressionRatio: number;
}

/**
 * FLUX streaming session statistics
 */
export interface FluxStreamStats {
  updatesSent: number;
  fullSends: number;
  deltaSends: number;
  bytesFull: number;
  bytesDelta: number;
  deltaEfficiency: number;
}

/**
 * Data analysis result
 */
export interface FluxAnalysis {
  inputSize: number;
  isJson: boolean;
  uniqueSymbols: number;
  entropyBits: number;
  estimatedRatio: number;
  recommended: 'flux_compress' | 'flux_session';
}

/**
 * Input types that can be compressed
 */
export type FluxInput = Uint8Array | ArrayBuffer | string;

/**
 * Compression result type
 */
export type FluxResult = Uint8Array;

/**
 * Normalize input to Uint8Array
 */
export function normalizeInput(input: FluxInput): Uint8Array {
  if (typeof input === 'string') {
    return new TextEncoder().encode(input);
  }
  if (input instanceof ArrayBuffer) {
    return new Uint8Array(input);
  }
  return input;
}

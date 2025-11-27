import { describe, it, expect } from 'vitest';
import { compress, decompress, normalizeInput } from '../src/index';
import type { CompressOptions } from '../src/types';

describe('FastPack', () => {
  describe('normalizeInput', () => {
    it('should handle string input', () => {
      const result = normalizeInput('hello');
      expect(result).toBeInstanceOf(Uint8Array);
      expect(new TextDecoder().decode(result)).toBe('hello');
    });

    it('should handle Uint8Array input', () => {
      const input = new Uint8Array([1, 2, 3]);
      const result = normalizeInput(input);
      expect(result).toBe(input);
    });

    it('should handle ArrayBuffer input', () => {
      const buffer = new ArrayBuffer(4);
      new Uint8Array(buffer).set([1, 2, 3, 4]);
      const result = normalizeInput(buffer);
      expect(result).toBeInstanceOf(Uint8Array);
      expect(Array.from(result)).toEqual([1, 2, 3, 4]);
    });
  });

  describe('compress/decompress', () => {
    it('should roundtrip empty data', async () => {
      const data = new Uint8Array(0);
      const compressed = await compress(data);
      const decompressed = await decompress(compressed);
      expect(Array.from(decompressed)).toEqual([]);
    });

    it('should roundtrip small data', async () => {
      const data = new TextEncoder().encode('Hello, FastPack!');
      const compressed = await compress(data);
      const decompressed = await decompress(compressed);
      expect(new TextDecoder().decode(decompressed)).toBe('Hello, FastPack!');
    });

    it('should roundtrip repeated data', async () => {
      const data = new TextEncoder().encode('abc'.repeat(100));
      const compressed = await compress(data);
      const decompressed = await decompress(compressed);
      expect(new TextDecoder().decode(decompressed)).toBe('abc'.repeat(100));
      // Repeated data should compress well
      expect(compressed.length).toBeLessThan(data.length);
    });

    it('should roundtrip JSON data', async () => {
      const json = JSON.stringify({
        id: 123,
        name: 'test',
        data: [1, 2, 3],
        nested: { key: 'value' },
      });
      const data = new TextEncoder().encode(json);
      const compressed = await compress(data);
      const decompressed = await decompress(compressed);
      expect(JSON.parse(new TextDecoder().decode(decompressed))).toEqual({
        id: 123,
        name: 'test',
        data: [1, 2, 3],
        nested: { key: 'value' },
      });
    });

    it('should roundtrip with string input', async () => {
      const compressed = await compress('Hello, World!');
      const decompressed = await decompress(compressed);
      expect(new TextDecoder().decode(decompressed)).toBe('Hello, World!');
    });

    it('should handle different compression levels', async () => {
      const data = new TextEncoder().encode('test data '.repeat(50));

      const levels: CompressOptions['level'][] = [0, 1, 2];
      for (const level of levels) {
        const compressed = await compress(data, { level });
        const decompressed = await decompress(compressed);
        expect(new TextDecoder().decode(decompressed)).toBe('test data '.repeat(50));
      }
    });

    it('should roundtrip large data', async () => {
      const data = new Uint8Array(100000);
      for (let i = 0; i < data.length; i++) {
        data[i] = i % 256;
      }
      const compressed = await compress(data);
      const decompressed = await decompress(compressed);
      expect(Array.from(decompressed)).toEqual(Array.from(data));
    });
  });
});

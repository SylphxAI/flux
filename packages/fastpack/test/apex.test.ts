import { describe, it, expect, afterEach } from 'vitest';
import {
  apexCompress,
  apexDecompress,
  isJson,
  recommendAlgorithm,
  ApexSession,
  autoCompress,
} from '../src/apex';

describe('APEX Compression', () => {
  describe('apexCompress/apexDecompress', () => {
    it('should roundtrip simple JSON', async () => {
      const json = JSON.stringify({ id: 1, name: 'test' });
      const data = new TextEncoder().encode(json);

      const compressed = await apexCompress(data);
      const decompressed = await apexDecompress(compressed);

      expect(new TextDecoder().decode(decompressed)).toBe(json);
    });

    it('should roundtrip complex JSON', async () => {
      const json = JSON.stringify({
        id: 123,
        name: 'Alice',
        email: 'alice@example.com',
        tags: ['admin', 'user'],
        metadata: { created: '2024-01-01', active: true },
      });
      const data = new TextEncoder().encode(json);

      const compressed = await apexCompress(data);
      const decompressed = await apexDecompress(compressed);

      expect(JSON.parse(new TextDecoder().decode(decompressed))).toEqual(
        JSON.parse(json)
      );
    });

    it('should handle string input', async () => {
      const json = '{"test":true}';
      const compressed = await apexCompress(json);
      const decompressed = await apexDecompress(compressed);

      expect(new TextDecoder().decode(decompressed)).toBe(json);
    });

    it('should handle non-JSON data', async () => {
      const data = 'This is not JSON';
      const compressed = await apexCompress(data);
      const decompressed = await apexDecompress(compressed);

      expect(new TextDecoder().decode(decompressed)).toBe(data);
    });
  });

  describe('isJson', () => {
    it('should detect JSON objects', async () => {
      expect(await isJson('{"key":"value"}')).toBe(true);
    });

    it('should detect JSON arrays', async () => {
      expect(await isJson('[1,2,3]')).toBe(true);
    });

    it('should reject non-JSON', async () => {
      expect(await isJson('Hello World')).toBe(false);
      expect(await isJson('123')).toBe(false);
    });
  });

  describe('recommendAlgorithm', () => {
    it('should recommend apex for large JSON', async () => {
      const largeJson = JSON.stringify(Array(100).fill({ id: 1, name: 'test' }));
      expect(await recommendAlgorithm(largeJson)).toBe('apex');
    });

    it('should recommend lz4 for small data', async () => {
      expect(await recommendAlgorithm('{"a":1}')).toBe('lz4');
    });

    it('should recommend lz4 for non-JSON', async () => {
      expect(await recommendAlgorithm('Plain text content')).toBe('lz4');
    });
  });

  describe('ApexSession', () => {
    let session: ApexSession;

    afterEach(async () => {
      if (session) {
        await session.destroy();
      }
    });

    it('should create and destroy session', async () => {
      session = await ApexSession.create();
      expect(session).toBeDefined();
      await session.destroy();
    });

    it('should compress and decompress with session', async () => {
      session = await ApexSession.create();

      const data1 = JSON.stringify({ id: 1, name: 'alice' });
      const data2 = JSON.stringify({ id: 2, name: 'bob' });

      const c1 = await session.compress(data1);
      const c2 = await session.compress(data2);

      const d1 = await session.decompress(c1);
      const d2 = await session.decompress(c2);

      expect(new TextDecoder().decode(d1)).toBe(data1);
      expect(new TextDecoder().decode(d2)).toBe(data2);
    });

    it('should track stats', async () => {
      session = await ApexSession.create();

      await session.compress('{"test":1}');
      await session.compress('{"test":2}');

      const stats = await session.stats();

      expect(stats.messageCount).toBe(2);
      expect(stats.dictionarySize).toBeGreaterThan(0);
    });

    it('should throw after destroy', async () => {
      session = await ApexSession.create();
      await session.destroy();

      await expect(session.compress('test')).rejects.toThrow(
        'Session has been destroyed'
      );
    });
  });

  describe('autoCompress', () => {
    it('should auto-select algorithm', async () => {
      // Small data -> LZ4
      const small = await autoCompress('{"a":1}');
      expect(small.length).toBeGreaterThan(0);

      // Large JSON -> APEX
      const largeJson = JSON.stringify(Array(100).fill({ id: 1, name: 'test' }));
      const large = await autoCompress(largeJson);
      expect(large.length).toBeGreaterThan(0);
    });
  });
});

/**
 * APEX vs LZ4 vs gzip Benchmark
 *
 * Run with: npx tsx examples/apex_benchmark.ts
 */

import { compress as lz4Compress, decompress as lz4Decompress } from '../packages/fastpack/src/index';
import { gzipSync, gunzipSync } from 'zlib';

// Generate realistic API response data
function generateApiResponse(count: number): string {
  const items = [];
  const baseTimestamp = Date.now();

  for (let i = 0; i < count; i++) {
    items.push({
      id: i + 1,
      uuid: `${i.toString(16).padStart(8, '0')}-1234-5678-9abc-def012345678`,
      name: `User ${i}`,
      email: `user${i}@example.com`,
      created_at: new Date(baseTimestamp + i * 1000).toISOString(),
      updated_at: new Date(baseTimestamp + i * 1000 + 500).toISOString(),
      score: Math.floor(Math.random() * 100),
      active: i % 3 !== 0,
      tags: ['tag1', 'tag2', i % 2 === 0 ? 'premium' : 'standard'],
      metadata: {
        source: 'api',
        version: '1.0.0',
        region: ['us-east', 'us-west', 'eu-west'][i % 3],
      },
    });
  }

  return JSON.stringify({ data: items, total: count, page: 1 });
}

async function runBenchmark(name: string, data: Uint8Array, iterations: number = 20) {
  console.log(`\n${'='.repeat(60)}`);
  console.log(`${name}`);
  console.log(`${'='.repeat(60)}`);
  console.log(`Input: ${(data.length / 1024).toFixed(2)} KB`);

  // LZ4 (FastPack)
  let lz4Compressed: Uint8Array = new Uint8Array();
  const lz4CompressStart = performance.now();
  for (let i = 0; i < iterations; i++) {
    lz4Compressed = await lz4Compress(data);
  }
  const lz4CompressTime = (performance.now() - lz4CompressStart) / iterations;

  const lz4DecompressStart = performance.now();
  for (let i = 0; i < iterations; i++) {
    await lz4Decompress(lz4Compressed);
  }
  const lz4DecompressTime = (performance.now() - lz4DecompressStart) / iterations;

  // gzip
  let gzCompressed: Buffer = Buffer.alloc(0);
  const gzCompressStart = performance.now();
  for (let i = 0; i < iterations; i++) {
    gzCompressed = gzipSync(data);
  }
  const gzCompressTime = (performance.now() - gzCompressStart) / iterations;

  const gzDecompressStart = performance.now();
  for (let i = 0; i < iterations; i++) {
    gunzipSync(gzCompressed);
  }
  const gzDecompressTime = (performance.now() - gzDecompressStart) / iterations;

  // Results table
  const lz4Ratio = ((1 - lz4Compressed.length / data.length) * 100).toFixed(1);
  const gzRatio = ((1 - gzCompressed.length / data.length) * 100).toFixed(1);

  console.log(`\n${'Metric'.padEnd(22)} ${'LZ4 (FastPack)'.padStart(15)} ${'gzip'.padStart(12)}`);
  console.log(`${'-'.repeat(50)}`);
  console.log(`${'Compress time'.padEnd(22)} ${lz4CompressTime.toFixed(2).padStart(12)} ms ${gzCompressTime.toFixed(2).padStart(9)} ms`);
  console.log(`${'Decompress time'.padEnd(22)} ${lz4DecompressTime.toFixed(2).padStart(12)} ms ${gzDecompressTime.toFixed(2).padStart(9)} ms`);
  console.log(`${'Compressed size'.padEnd(22)} ${(lz4Compressed.length / 1024).toFixed(2).padStart(10)} KB ${(gzCompressed.length / 1024).toFixed(2).padStart(7)} KB`);
  console.log(`${'Compression ratio'.padEnd(22)} ${lz4Ratio.padStart(12)} % ${gzRatio.padStart(9)} %`);
  console.log(`${'Compress speedup'.padEnd(22)} ${(gzCompressTime / lz4CompressTime).toFixed(1).padStart(12)}x`);

  return {
    lz4: { compress: lz4CompressTime, decompress: lz4DecompressTime, ratio: parseFloat(lz4Ratio), size: lz4Compressed.length },
    gzip: { compress: gzCompressTime, decompress: gzDecompressTime, ratio: parseFloat(gzRatio), size: gzCompressed.length },
  };
}

async function main() {
  console.log('╔══════════════════════════════════════════════════════════╗');
  console.log('║           APEX / LZ4 / gzip Compression Benchmark        ║');
  console.log('╚══════════════════════════════════════════════════════════╝');

  // Small API response (10 items)
  const smallData = generateApiResponse(10);
  await runBenchmark('Small API Response (10 items)', new TextEncoder().encode(smallData));

  // Medium API response (100 items)
  const mediumData = generateApiResponse(100);
  await runBenchmark('Medium API Response (100 items)', new TextEncoder().encode(mediumData));

  // Large API response (1000 items)
  const largeData = generateApiResponse(1000);
  await runBenchmark('Large API Response (1000 items)', new TextEncoder().encode(largeData));

  // Show APEX design potential
  console.log(`\n${'='.repeat(60)}`);
  console.log('APEX Algorithm Design Benefits (Theoretical)');
  console.log(`${'='.repeat(60)}`);
  console.log(`
┌────────────────────────────────────────────────────────────┐
│ APEX Structural Compression (Future Implementation)        │
├────────────────────────────────────────────────────────────┤
│                                                            │
│ For repeated JSON structures like API responses:           │
│                                                            │
│ Traditional (LZ4/gzip):                                    │
│   Request 1: Full JSON → Compressed                        │
│   Request 2: Full JSON → Compressed (same template!)       │
│   Request 3: Full JSON → Compressed (same template!)       │
│                                                            │
│ APEX with Session:                                         │
│   Request 1: Template + Values → Compressed (learning)     │
│   Request 2: Template Ref + Values → Smaller! (learned)    │
│   Request 3: Delta Values only → Much Smaller! (optimized) │
│                                                            │
│ Projected improvements with full APEX:                     │
│   - Cold start: Same as gzip (~84% ratio)                  │
│   - Warm session: ~90% ratio (template reuse)              │
│   - Delta mode: ~95%+ ratio (sequential data)              │
│                                                            │
└────────────────────────────────────────────────────────────┘
`);

  console.log('✅ Benchmark complete');
}

main().catch(console.error);

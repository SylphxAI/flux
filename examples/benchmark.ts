/**
 * FastPack Benchmark
 *
 * Run with: npx tsx examples/benchmark.ts
 */

import { compress, decompress } from '../packages/fastpack/src/index';
import { gzipSync, gunzipSync } from 'zlib';

// Generate test data
function generateJsonData(size: number): string {
  const items = [];
  for (let i = 0; i < size; i++) {
    items.push({
      id: i,
      name: `Item ${i}`,
      value: Math.random(),
      tags: ['tag1', 'tag2', 'tag3'],
      nested: { key: `value-${i}` },
    });
  }
  return JSON.stringify(items);
}

function generateRepeatedData(size: number): string {
  return 'Hello, World! '.repeat(size);
}

async function benchmark(name: string, data: Uint8Array, iterations: number = 100) {
  console.log(`\n=== ${name} ===`);
  console.log(`Input size: ${(data.length / 1024).toFixed(2)} KB`);

  // FastPack
  const fpStart = performance.now();
  let fpCompressed: Uint8Array = new Uint8Array();
  for (let i = 0; i < iterations; i++) {
    fpCompressed = await compress(data);
  }
  const fpCompressTime = (performance.now() - fpStart) / iterations;

  const fpDecompStart = performance.now();
  for (let i = 0; i < iterations; i++) {
    await decompress(fpCompressed);
  }
  const fpDecompressTime = (performance.now() - fpDecompStart) / iterations;

  const fpRatio = ((1 - fpCompressed.length / data.length) * 100).toFixed(1);

  // gzip
  const gzStart = performance.now();
  let gzCompressed: Buffer = Buffer.alloc(0);
  for (let i = 0; i < iterations; i++) {
    gzCompressed = gzipSync(data);
  }
  const gzCompressTime = (performance.now() - gzStart) / iterations;

  const gzDecompStart = performance.now();
  for (let i = 0; i < iterations; i++) {
    gunzipSync(gzCompressed);
  }
  const gzDecompressTime = (performance.now() - gzDecompStart) / iterations;

  const gzRatio = ((1 - gzCompressed.length / data.length) * 100).toFixed(1);

  // Results
  console.log(`\n${''.padEnd(20)} ${'FastPack'.padStart(12)} ${'gzip'.padStart(12)}`);
  console.log(`${'Compress time'.padEnd(20)} ${fpCompressTime.toFixed(2).padStart(10)} ms ${gzCompressTime.toFixed(2).padStart(10)} ms`);
  console.log(`${'Decompress time'.padEnd(20)} ${fpDecompressTime.toFixed(2).padStart(10)} ms ${gzDecompressTime.toFixed(2).padStart(10)} ms`);
  console.log(`${'Compressed size'.padEnd(20)} ${(fpCompressed.length / 1024).toFixed(2).padStart(8)} KB ${(gzCompressed.length / 1024).toFixed(2).padStart(8)} KB`);
  console.log(`${'Compression ratio'.padEnd(20)} ${fpRatio.padStart(10)} % ${gzRatio.padStart(10)} %`);
  console.log(`${'Speedup (compress)'.padEnd(20)} ${(gzCompressTime / fpCompressTime).toFixed(1).padStart(10)}x`);
  console.log(`${'Speedup (decompress)'.padEnd(20)} ${(gzDecompressTime / fpDecompressTime).toFixed(1).padStart(10)}x`);
}

async function main() {
  console.log('FastPack Benchmark');
  console.log('==================');

  // Test with JSON data
  const jsonData = generateJsonData(1000);
  await benchmark('JSON Data (1000 items)', new TextEncoder().encode(jsonData), 50);

  // Test with repeated data
  const repeatedData = generateRepeatedData(1000);
  await benchmark('Repeated Text', new TextEncoder().encode(repeatedData), 50);

  // Test with larger JSON
  const largeJson = generateJsonData(5000);
  await benchmark('Large JSON (5000 items)', new TextEncoder().encode(largeJson), 20);

  console.log('\n✅ Benchmark complete');
}

main().catch(console.error);

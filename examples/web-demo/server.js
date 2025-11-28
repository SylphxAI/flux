/**
 * FLUX Demo Server
 *
 * Simple Express server demonstrating FLUX compression for API responses
 */

const http = require('http');
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

const PORT = 3000;

// Sample data - simulates real API responses
const sampleData = {
  users: [
    { id: 1, name: "Alice Chen", email: "alice@example.com", created: "2024-01-15T10:30:00Z", role: "admin" },
    { id: 2, name: "Bob Smith", email: "bob@example.com", created: "2024-01-16T14:20:00Z", role: "user" },
    { id: 3, name: "Carol Wang", email: "carol@example.com", created: "2024-01-17T09:15:00Z", role: "user" },
    { id: 4, name: "David Lee", email: "david@example.com", created: "2024-01-18T16:45:00Z", role: "moderator" },
    { id: 5, name: "Eve Zhang", email: "eve@example.com", created: "2024-01-19T11:00:00Z", role: "user" },
  ],
  metadata: {
    total: 5,
    page: 1,
    perPage: 10,
    timestamp: new Date().toISOString()
  }
};

// Large dataset for compression comparison
function generateLargeData(count = 100) {
  const users = [];
  const roles = ['admin', 'user', 'moderator', 'guest'];

  for (let i = 1; i <= count; i++) {
    users.push({
      id: i,
      uuid: `550e8400-e29b-41d4-a716-${String(i).padStart(12, '0')}`,
      name: `User ${i}`,
      email: `user${i}@example.com`,
      created: new Date(Date.now() - i * 86400000).toISOString(),
      role: roles[i % roles.length],
      score: Math.floor(Math.random() * 1000),
      active: i % 3 !== 0,
      tags: ['tag1', 'tag2', 'tag3'].slice(0, (i % 3) + 1)
    });
  }

  return {
    users,
    metadata: {
      total: count,
      generated: new Date().toISOString()
    }
  };
}

// MIME types
const mimeTypes = {
  '.html': 'text/html',
  '.js': 'application/javascript',
  '.wasm': 'application/wasm',
  '.css': 'text/css',
  '.json': 'application/json'
};

const server = http.createServer((req, res) => {
  // CORS headers
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Accept-Encoding');

  if (req.method === 'OPTIONS') {
    res.writeHead(204);
    res.end();
    return;
  }

  const url = new URL(req.url, `http://localhost:${PORT}`);

  // API endpoints
  if (url.pathname === '/api/users') {
    const json = JSON.stringify(sampleData);
    res.setHeader('Content-Type', 'application/json');
    res.setHeader('X-Original-Size', json.length);
    res.end(json);
    return;
  }

  if (url.pathname === '/api/users/large') {
    const count = parseInt(url.searchParams.get('count') || '100');
    const data = generateLargeData(count);
    const json = JSON.stringify(data);

    // Check if client accepts gzip
    const acceptEncoding = req.headers['accept-encoding'] || '';

    if (acceptEncoding.includes('gzip')) {
      zlib.gzip(json, (err, compressed) => {
        if (err) {
          res.writeHead(500);
          res.end('Compression error');
          return;
        }

        res.setHeader('Content-Type', 'application/json');
        res.setHeader('Content-Encoding', 'gzip');
        res.setHeader('X-Original-Size', json.length);
        res.setHeader('X-Compressed-Size', compressed.length);
        res.setHeader('X-Compression-Ratio', (compressed.length / json.length * 100).toFixed(1) + '%');
        res.end(compressed);
      });
    } else {
      res.setHeader('Content-Type', 'application/json');
      res.setHeader('X-Original-Size', json.length);
      res.end(json);
    }
    return;
  }

  // Serve static files
  let filePath = url.pathname;
  if (filePath === '/') filePath = '/index.html';

  // Map paths
  const fullPath = filePath.startsWith('/pkg/')
    ? path.join(__dirname, '../../crates/flux-wasm', filePath)
    : path.join(__dirname, filePath);

  const ext = path.extname(fullPath);
  const contentType = mimeTypes[ext] || 'application/octet-stream';

  fs.readFile(fullPath, (err, content) => {
    if (err) {
      if (err.code === 'ENOENT') {
        res.writeHead(404);
        res.end('Not found: ' + filePath);
      } else {
        res.writeHead(500);
        res.end('Server error');
      }
      return;
    }

    res.setHeader('Content-Type', contentType);
    res.end(content);
  });
});

server.listen(PORT, () => {
  console.log(`
╔═══════════════════════════════════════════════════════════╗
║                   FLUX Demo Server                        ║
╠═══════════════════════════════════════════════════════════╣
║  Server running at: http://localhost:${PORT}                 ║
║                                                           ║
║  Endpoints:                                               ║
║    GET /              - Demo page                         ║
║    GET /api/users     - Sample user data                  ║
║    GET /api/users/large?count=100 - Large dataset         ║
╚═══════════════════════════════════════════════════════════╝
  `);
});

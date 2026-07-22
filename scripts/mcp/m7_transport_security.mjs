#!/usr/bin/env node

import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import http from 'node:http';
import { dirname, resolve } from 'node:path';
import { mkdirSync } from 'node:fs';

const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index], process.argv[index + 1]);
}
const endpoint = new URL(args.get('--mcp-url') ?? process.env.ORDIVON_M7_MCP_URL ?? 'http://127.0.0.1:8897/mcp');
const outputPath = resolve(args.get('--output') ?? '/tmp/ordivon-m7-security.json');
const token = process.env.ORDIVON_M7_BEARER_TOKEN ?? '';
const sourceRevision = process.env.ORDIVON_M7_SOURCE_REVISION ?? '';
if (token.length < 32) throw new Error('ORDIVON_M7_BEARER_TOKEN must be at least 32 characters');

const initialize = JSON.stringify({
  jsonrpc: '2.0',
  id: 1,
  method: 'initialize',
  params: {
    protocolVersion: '2025-11-25',
    capabilities: {},
    clientInfo: { name: 'm7-security', version: '0.1.0' }
  }
});
async function post(headers, body = initialize) {
  return fetch(endpoint, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      accept: 'application/json, text/event-stream',
      ...headers
    },
    body
  });
}

const missing = await post({});
assert.equal(missing.status, 401);
const invalid = await post({ authorization: 'Bearer invalid' });
assert.equal(invalid.status, 401);
const origin = await post({
  authorization: `Bearer ${token}`,
  origin: 'https://evil.example'
});
assert.equal(origin.status, 403);
const oversized = await post(
  { authorization: `Bearer ${token}` },
  JSON.stringify({ payload: 'x'.repeat(1_100_000) })
);
assert.equal(oversized.status, 413);
const badHost = await new Promise((resolveStatus, reject) => {
  const request = http.request({
    host: endpoint.hostname,
    port: Number(endpoint.port),
    path: endpoint.pathname,
    method: 'POST',
    headers: {
      host: 'evil.example',
      authorization: `Bearer ${token}`,
      'content-type': 'application/json',
      accept: 'application/json, text/event-stream',
      'content-length': Buffer.byteLength(initialize)
    }
  }, response => {
    response.resume();
    response.on('end', () => resolveStatus(response.statusCode));
  });
  request.on('error', reject);
  request.end(initialize);
});
assert.equal(badHost, 403);

const evidence = {
  schemaVersion: 1,
  phase: 'ORDIVON-MIGRATION-M7-TRANSPORT-SECURITY-2026-07-22',
  generatedAt: new Date().toISOString(),
  sourceRevision,
  endpoint: endpoint.toString(),
  results: {
    missingAuthorization: missing.status,
    invalidAuthorization: invalid.status,
    disallowedOrigin: origin.status,
    disallowedHost: badHost,
    oversizedBody: oversized.status
  }
};
mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify(evidence, null, 2));

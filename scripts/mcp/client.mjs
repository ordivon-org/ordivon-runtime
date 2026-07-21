import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const sdkRoot = process.env.ORDIVON_MCP_SDK_ROOT;
if (!sdkRoot) throw new Error('ORDIVON_MCP_SDK_ROOT is required');

const { Client } = await import(
  pathToFileURL(join(sdkRoot, 'dist/esm/client/index.js')).href
);
const { StreamableHTTPClientTransport } = await import(
  pathToFileURL(join(sdkRoot, 'dist/esm/client/streamableHttp.js')).href
);
const types = await import(pathToFileURL(join(sdkRoot, 'dist/esm/types.js')).href);

export const CallToolResultSchema = types.CallToolResultSchema;
export const ListToolsResultSchema = types.ListToolsResultSchema;

export function sha256(value) {
  const material = Buffer.isBuffer(value) ? value : Buffer.from(String(value));
  return `sha256:${createHash('sha256').update(material).digest('hex')}`;
}

export function sha256File(path) {
  return sha256(readFileSync(path));
}

export function byteLength(value) {
  if (value === undefined || value === null) return 0;
  return Buffer.byteLength(typeof value === 'string' ? value : JSON.stringify(value));
}


export function requiredEnvironment(name) {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
}

export function m5Config() {
  return {
    endpoint: new URL(requiredEnvironment('ORDIVON_M5_MCP_URL')),
    token: requiredEnvironment('ORDIVON_M4_BEARER_TOKEN'),
    repoRoot: requiredEnvironment('ORDIVON_M5_REPO_ROOT'),
    sourceRevision: requiredEnvironment('ORDIVON_M5_SOURCE_REVISION'),
    storeRoot: requiredEnvironment('ORDIVON_M5_STORE_ROOT'),
    tracePath: requiredEnvironment('ORDIVON_M5_TRACE_PATH'),
    httpTracePath: requiredEnvironment('ORDIVON_M5_HTTP_TRACE_PATH')
  };
}

export async function connectM5(name, measuredFetch = fetch) {
  const config = m5Config();
  const client = new Client({ name, version: '1.0.0' });
  const traceIds = [];
  const wrappedFetch = async (input, init = {}) => {
    const response = await measuredFetch(input, init);
    const traceId = response.headers.get('x-ordivon-trace-id');
    if (traceId) traceIds.push(traceId);
    return response;
  };
  const transport = new StreamableHTTPClientTransport(config.endpoint, {
    requestInit: {
      headers: { Authorization: `Bearer ${config.token}` }
    },
    fetch: wrappedFetch
  });
  await client.connect(transport);
  return { client, transport, traceIds, config };
}

export async function callTool(connection, name, args) {
  const result = await connection.client.callTool(
    { name, arguments: args },
    CallToolResultSchema
  );
  if (result.isError) {
    const error = result.structuredContent?.error ?? result;
    const failure = new Error(`${name}: ${JSON.stringify(error)}`);
    failure.toolResult = result;
    throw failure;
  }
  return result;
}

export function structured(result) {
  if (result.structuredContent === undefined) {
    throw new Error('tool result omitted structuredContent');
  }
  return result.structuredContent;
}

export async function closeConnection(connection) {
  await connection.transport.close().catch(() => {});
}

export function assertCompactSuccess(result, budgetBytes, label) {
  if (result.isError) throw new Error(`${label} unexpectedly failed`);
  if (!result.structuredContent) throw new Error(`${label} has no structuredContent`);
  const serialized = JSON.stringify(result);
  if (serialized.includes('traceId') || serialized.includes('coreMs')) {
    throw new Error(`${label} leaked trace data into model context`);
  }
  if ((result.content ?? []).length !== 0) {
    throw new Error(`${label} duplicated success content outside structuredContent`);
  }
  const bytes = byteLength(result);
  if (bytes > budgetBytes) {
    throw new Error(`${label} response ${bytes} exceeds budget ${budgetBytes}`);
  }
  return bytes;
}

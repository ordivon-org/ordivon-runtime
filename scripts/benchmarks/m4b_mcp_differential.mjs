#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const SCHEMA_VERSION = 1;
const DEFAULT_ITERATIONS = 20;
const DEFAULT_WARMUPS = 3;
const TOOL_TIMEOUT_MS = 30_000;
const SCRIPT_SLEEP_SECONDS = 0.35;

function parseArgs(values) {
  const result = new Map();
  for (let index = 0; index < values.length; index += 2) {
    const key = values[index];
    const value = values[index + 1];
    if (!key?.startsWith('--') || value === undefined) {
      throw new Error('arguments must be --name value pairs');
    }
    result.set(key, value);
  }
  return result;
}

function discoverSdkRoot() {
  if (process.env.ORDIVON_M2_MCP_SDK_ROOT) {
    return resolve(process.env.ORDIVON_M2_MCP_SDK_ROOT);
  }
  const output = spawnSync('find', [
    '/root/.npm/_npx',
    '-path',
    '*/node_modules/@modelcontextprotocol/sdk/package.json',
    '-print'
  ], { encoding: 'utf8', timeout: 10_000 });
  if (output.status !== 0) throw new Error(`cannot discover MCP SDK: ${output.stderr}`);
  const candidates = output.stdout.trim().split('\n').filter(Boolean);
  if (candidates.length === 0) throw new Error('no MCP SDK installation found');
  return dirname(candidates[0]);
}

const args = parseArgs(process.argv.slice(2));
const config = {
  sdkRoot: discoverSdkRoot(),
  legacyMcpUrl: args.get('--legacy-mcp-url') ?? 'http://127.0.0.1:8811/mcp',
  m4McpUrl: args.get('--m4-mcp-url') ?? 'http://127.0.0.1:8895/mcp',
  m4Token: process.env.ORDIVON_M4_BEARER_TOKEN ?? '',
  m4StoreRoot: resolve(args.get('--m4-store-root') ?? '/root/.local/share/ordivon-m4-formal'),
  repoRoot: resolve(args.get('--repo-root') ?? process.cwd()),
  sourceRevision: args.get('--source-revision') ?? '4592689dc9183fcb08f4828d3d752a4cf57e318f',
  m4Binary: resolve(args.get('--m4-binary') ?? 'target/debug/ordivon-m4-http'),
  m4Runner: resolve(args.get('--m4-runner') ?? 'target/debug/ordivon-task-runner'),
  outputPath: resolve(args.get('--output') ?? '/tmp/ordivon-m4b-evidence.json'),
  iterations: Number(args.get('--iterations') ?? DEFAULT_ITERATIONS),
  warmups: Number(args.get('--warmups') ?? DEFAULT_WARMUPS)
};

if (config.m4Token.length < 32) {
  throw new Error('ORDIVON_M4_BEARER_TOKEN must be at least 32 characters');
}
if (!Number.isInteger(config.iterations) || config.iterations < 1 || config.iterations > 20) {
  throw new Error('ORDIVON_M2_ITERATIONS must be in 1..=20');
}
if (!Number.isInteger(config.warmups) || config.warmups < 0 || config.warmups > 5) {
  throw new Error('ORDIVON_M2_WARMUPS must be in 0..=5');
}

const clientModuleUrl = pathToFileURL(join(config.sdkRoot, 'dist/esm/client/index.js'));
const transportModuleUrl = pathToFileURL(
  join(config.sdkRoot, 'dist/esm/client/streamableHttp.js')
);
const { Client } = await import(clientModuleUrl.href);
const { StreamableHTTPClientTransport } = await import(transportModuleUrl.href);
const { CallToolResultSchema } = await import(
  pathToFileURL(join(config.sdkRoot, 'dist/esm/types.js')).href
);

function byteLength(value) {
  return Buffer.byteLength(typeof value === 'string' ? value : JSON.stringify(value));
}

function sha256(value) {
  return `sha256:${createHash('sha256').update(value).digest('hex')}`;
}

function sha256File(path) {
  return sha256(readFileSync(path));
}

function median(values) {
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1
    ? sorted[middle]
    : Math.round((sorted[middle - 1] + sorted[middle]) / 2);
}

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  const index = Math.max(0, Math.ceil(sorted.length * fraction) - 1);
  return sorted[index];
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}
function execLocal(program, args, options = {}) {
  const result = spawnSync(program, args, {
    cwd: options.cwd,
    env: options.env ?? process.env,
    encoding: 'utf8',
    timeout: options.timeout ?? TOOL_TIMEOUT_MS,
    maxBuffer: 16 * 1024 * 1024
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      `${program} failed (${result.status}): ${result.stderr || result.stdout}`
    );
  }
  return result.stdout;
}

function removeWorktree(workspacePath) {
  try {
    execLocal('git', ['-C', config.repoRoot, 'worktree', 'remove', '--force', workspacePath]);
  } catch {
    rmSync(workspacePath, { recursive: true, force: true });
    execLocal('git', ['-C', config.repoRoot, 'worktree', 'prune']);
  }
}

function makeMetrics() {
  return {
    logicalCalls: 0,
    contextBytes: 0,
    outputBytes: 0,
    requestBytes: 0,
    responseBytes: 0,
    httpRequests: 0,
    sseResponseBytesUnmeasured: false,
    calls: []
  };
}

function transportSnapshot(metrics) {
  return {
    requestBytes: metrics.requestBytes,
    responseBytes: metrics.responseBytes,
    httpRequests: metrics.httpRequests,
    sseResponseBytesUnmeasured: metrics.sseResponseBytesUnmeasured
  };
}

function transportDelta(after, before) {
  return {
    requestBytes: after.requestBytes - before.requestBytes,
    responseBytes: after.responseBytes - before.responseBytes,
    httpRequests: after.httpRequests - before.httpRequests,
    sseResponseBytesUnmeasured:
      after.sseResponseBytesUnmeasured || before.sseResponseBytesUnmeasured
  };
}

function textContent(result) {
  return (result.content ?? [])
    .filter(item => item.type === 'text')
    .map(item => item.text)
    .join('\n');
}
async function connectLegacy(metrics, clientName) {
  const measuredFetch = async (input, init = {}) => {
    metrics.httpRequests += 1;
    if (typeof init.body === 'string') metrics.requestBytes += byteLength(init.body);
    const response = await fetch(input, init);
    const contentType = response.headers.get('content-type') ?? '';
    if (contentType.includes('text/event-stream')) {
      metrics.sseResponseBytesUnmeasured = true;
    } else {
      const clone = response.clone();
      metrics.responseBytes += (await clone.arrayBuffer()).byteLength;
    }
    return response;
  };
  const client = new Client({ name: clientName, version: '0.1.0' });
  const transport = new StreamableHTTPClientTransport(new URL(config.legacyMcpUrl), {
    fetch: measuredFetch
  });
  await client.connect(transport);
  return { client, transport };
}

async function legacyCall(connection, metrics, name, args) {
  metrics.logicalCalls += 1;
  const result = await connection.client.callTool({ name, arguments: args });
  const responseBytes = byteLength(result);
  metrics.contextBytes += responseBytes;
  metrics.calls.push({ name, responseBytes });
  if (result.isError) throw new Error(`${name} returned an MCP tool error: ${textContent(result)}`);
  return result;
}

function legacyReadBody(result) {
  const text = textContent(result);
  const separator = text.indexOf('\n\n');
  return separator >= 0 ? text.slice(separator + 2) : text;
}

function requireSentinel(result, operation) {
  const text = textContent(result);
  if (!text.includes('__ORDIVON_M2_OK__')) {
    throw new Error(`${operation} did not produce completion sentinel: ${text}`);
  }
  return text;
}
function journeyMaterial(marker) {
  const appendedLine = `M3B targeted marker ${marker}`;
  const script = [
    'from pathlib import Path',
    'import sys,time',
    "text=Path('crates/ordivon-exec/README.md').read_text()",
    `marker=${JSON.stringify(appendedLine)}`,
    "observed='marker=' + str(marker in text)",
    "Path('m3b-output.txt').write_text(observed)",
    "print('M3B_STDOUT ' + observed, flush=True)",
    "print('M3B_STDERR diagnostic', file=sys.stderr, flush=True)",
    `time.sleep(${SCRIPT_SLEEP_SECONDS})`,
    ''
  ].join('\n');
  return {
    appendedLine,
    script,
    expectedOutput: 'marker=True',
    expectedStdout: 'M3B_STDOUT marker=True',
    expectedStderr: 'M3B_STDERR diagnostic'
  };
}

async function legacyExec(connection, metrics, command, operation) {
  const wrapped = `set -euo pipefail; ${command}; printf '\n__ORDIVON_M2_OK__\n'`;
  const result = await legacyCall(connection, metrics, 'start_process', {
    command: wrapped,
    timeout_ms: TOOL_TIMEOUT_MS,
    origin: 'llm'
  });
  return requireSentinel(result, operation);
}

async function runLegacyJourney(pairId, marker) {
  const metrics = makeMetrics();
  const bootstrap = makeMetrics();
  const workspacePath = join('/root/.local/share/ordivon-m3', `legacy-targeted-${pairId}`);
  mkdirSync(dirname(workspacePath), { recursive: true });
  removeWorktree(workspacePath);
  const connection = await connectLegacy(bootstrap, `ordivon-m3b-legacy-${pairId}`);
  const material = journeyMaterial(marker);
  const transportAtTaskStart = transportSnapshot(bootstrap);
  const started = process.hrtime.bigint();
  let succeeded = false;
  let semanticDigest = null;
  let caughtError = null;
  let elapsedMs = 0;
  try {
    await legacyExec(
      connection,
      metrics,
      `git -C ${shellQuote(config.repoRoot)} worktree add --detach ${shellQuote(workspacePath)} ${shellQuote(config.sourceRevision)}`,
      'git worktree add'
    );
    const readCode = [
      'from pathlib import Path',
      'import hashlib,json',
      `p=Path(${JSON.stringify(join(workspacePath, 'crates/ordivon-exec/README.md'))})`,
      'b=p.read_bytes()',
      "print('__M3B_READ__' + json.dumps({'prefix': b[:64].decode(), 'digest': 'sha256:' + hashlib.sha256(b).hexdigest()}))"
    ].join(';');
    const readText = await legacyExec(
      connection,
      metrics,
      `/usr/bin/python3 -c ${shellQuote(readCode)}`,
      'targeted read'
    );
    const readMatch = readText.match(/__M3B_READ__(\{[^\n]+\})/);
    if (!readMatch) throw new Error(`Legacy targeted read missing JSON: ${readText}`);
    const read = JSON.parse(readMatch[1]);
    if (byteLength(read.prefix) > 64) throw new Error('Legacy prefix exceeded 64 bytes');

    const marker64 = Buffer.from(`\n${material.appendedLine}\n`).toString('base64');
    const script64 = Buffer.from(material.script).toString('base64');
    const mutateCode = [
      'from pathlib import Path',
      'import base64,hashlib',
      `p=Path(${JSON.stringify(join(workspacePath, 'crates/ordivon-exec/README.md'))})`,
      'b=p.read_bytes()',
      `assert 'sha256:' + hashlib.sha256(b).hexdigest() == ${JSON.stringify(read.digest)}`,
      `p.write_bytes(b + base64.b64decode(${JSON.stringify(marker64)}))`,
      `Path(${JSON.stringify(join(workspacePath, 'm3b_tool.py'))}).write_bytes(base64.b64decode(${JSON.stringify(script64)}))`
    ].join(';');
    await legacyExec(connection, metrics, `/usr/bin/python3 -c ${shellQuote(mutateCode)}`, 'batch mutation');
    const runText = await legacyExec(
      connection,
      metrics,
      `cd ${shellQuote(workspacePath)} && /usr/bin/python3 m3b_tool.py`,
      'model-authored tool'
    );
    if (!runText.includes(material.expectedStdout) || !runText.includes(material.expectedStderr)) {
      throw new Error(`Legacy compact output mismatch: ${runText}`);
    }
    metrics.outputBytes += byteLength(material.expectedStdout) + byteLength(material.expectedStderr);
    const diffText = await legacyExec(
      connection,
      metrics,
      `cd ${shellQuote(workspacePath)} && git diff -- crates/ordivon-exec/README.md && printf '\n__M3B_UNTRACKED__\n' && git ls-files --others --exclude-standard`,
      'git diff and untracked files'
    );
    if (!diffText.includes(material.appendedLine)) throw new Error('Legacy M3B diff lost marker');
    if (!diffText.includes('m3b_tool.py') || !diffText.includes('m3b-output.txt')) {
      throw new Error('Legacy M3B untracked paths incomplete');
    }
    semanticDigest = sha256(JSON.stringify({
      output: material.expectedOutput,
      stdout: material.expectedStdout,
      stderr: material.expectedStderr,
      marker: material.appendedLine,
      untracked: ['m3b-output.txt', 'm3b_tool.py']
    }));
    succeeded = true;
  } catch (error) {
    caughtError = error;
  } finally {
    elapsedMs = Number((process.hrtime.bigint() - started) / 1_000_000n);
    await connection.transport.terminateSession().catch(() => {});
    await connection.transport.close().catch(() => {});
    removeWorktree(workspacePath);
  }
  if (caughtError) throw caughtError;
  return {
    backend: 'LEGACY_DESKTOP_COMMANDER', pairId, succeeded, elapsedMs,
    toolCalls: metrics.logicalCalls, remoteRoundTrips: transportDelta(transportSnapshot(bootstrap), transportAtTaskStart).httpRequests,
    contextBytes: metrics.contextBytes, outputBytes: metrics.outputBytes,
    recoveredAfterDisconnect: false, fallbackCount: 0, semanticDigest,
    callBreakdown: metrics.calls,
    serverVersion: connection.client.getServerVersion(),
    transport: {
      bootstrap: transportAtTaskStart,
      task: transportDelta(transportSnapshot(bootstrap), transportAtTaskStart),
      total: transportSnapshot(bootstrap)
    }
  };
}

async function connectM4(metrics, clientName) {
  const measuredFetch = async (input, init = {}) => {
    metrics.httpRequests += 1;
    if (typeof init.body === 'string') metrics.requestBytes += byteLength(init.body);
    const response = await fetch(input, init);
    const clone = response.clone();
    metrics.responseBytes += (await clone.arrayBuffer()).byteLength;
    return response;
  };
  const client = new Client({ name: clientName, version: '0.1.0' });
  const transport = new StreamableHTTPClientTransport(new URL(config.m4McpUrl), {
    fetch: measuredFetch,
    requestInit: {
      headers: { Authorization: `Bearer ${config.m4Token}` }
    }
  });
  await client.connect(transport);
  return { client, transport };
}

async function m4Call(connection, metrics, name, args) {
  metrics.logicalCalls += 1;
  const result = await connection.client.callTool(
    { name, arguments: args },
    CallToolResultSchema
  );
  const responseBytes = byteLength(result);
  metrics.contextBytes += responseBytes;
  metrics.calls.push({ name, responseBytes });
  if (result.isError) {
    throw new Error(`${name} returned MCP tool error: ${JSON.stringify(result.structuredContent)}`);
  }
  if (!result.structuredContent) {
    throw new Error(`${name} omitted structuredContent`);
  }
  return result.structuredContent;
}

function cleanupM4(workspaceId, taskId, workspacePath) {
  removeWorktree(workspacePath);
  rmSync(join(config.m4StoreRoot, 'workspace-records', `${workspaceId}.json`), { force: true });
  rmSync(join(config.m4StoreRoot, 'tasks', taskId), { recursive: true, force: true });
}

async function runOrdivonJourney(pairId, marker) {
  const metrics = makeMetrics();
  const bootstrap = makeMetrics();
  const workspaceId = `m4b-workspace-${pairId}`;
  const taskId = `m4b-task-${pairId}`;
  const workspacePath = join(config.m4StoreRoot, 'workspaces', workspaceId);
  const material = journeyMaterial(marker);
  removeWorktree(workspacePath);
  const connection = await connectM4(bootstrap, `ordivon-m4b-${pairId}`);
  const transportAtTaskStart = transportSnapshot(bootstrap);
  const started = process.hrtime.bigint();
  let succeeded = false;
  let semanticDigest = null;
  let caughtError = null;
  let elapsedMs = 0;
  try {
    await m4Call(connection, metrics, 'workspace.open', {
      schemaVersion: 1,
      workspaceId,
      sourceRepo: config.repoRoot,
      sourceRevision: config.sourceRevision
    });
    const read = await m4Call(connection, metrics, 'workspace.read', {
      schemaVersion: 1,
      workspaceId,
      relativePath: 'crates/ordivon-exec/README.md',
      mode: 'SLICE',
      offset: 0,
      maxBytes: 64
    });
    if (byteLength(read.content) > 64) throw new Error('M4B slice exceeded 64 bytes');
    await m4Call(connection, metrics, 'workspace.mutate', {
      schemaVersion: 1,
      workspaceId,
      mutations: [
        {
          relativePath: 'crates/ordivon-exec/README.md',
          mode: 'APPEND',
          content: `\n${material.appendedLine}\n`,
          expectedDigest: read.digest
        },
        {
          relativePath: 'm3b_tool.py',
          mode: 'WRITE',
          content: material.script,
          expectedDigest: null
        }
      ]
    });
    const compact = await m4Call(connection, metrics, 'workspace.exec', {
      schemaVersion: 1,
      execution: {
        schemaVersion: 1,
        taskId,
        workspaceId,
        executable: '/usr/bin/python3.14',
        args: ['m3b_tool.py'],
        cwdRelative: '.',
        env: { PYTHONUNBUFFERED: '1' },
        timeoutMs: 10_000,
        stdoutLimitBytes: 65_536,
        stderrLimitBytes: 65_536
      },
      waitMs: 5000,
      stdoutTailBytes: 1024,
      stderrTailBytes: 1024
    });
    if (compact.status !== 'COMPLETED') throw new Error(`M4B task failed: ${JSON.stringify(compact)}`);
    if (!compact.stdoutTail.includes(material.expectedStdout)) throw new Error('M4B stdout mismatch');
    if (!compact.stderrTail.includes(material.expectedStderr)) throw new Error('M4B stderr mismatch');
    metrics.outputBytes += byteLength(compact.stdoutTail) + byteLength(compact.stderrTail);
    const diff = await m4Call(connection, metrics, 'workspace.diff', {
      schemaVersion: 1,
      workspaceId,
      maxBytes: 1_048_576
    });
    if (!diff.diff.includes(material.appendedLine)) throw new Error('M4B diff lost marker');
    if (!diff.untrackedPaths.includes('m3b_tool.py') || !diff.untrackedPaths.includes('m3b-output.txt')) {
      throw new Error('M4B untracked paths incomplete');
    }
    semanticDigest = sha256(JSON.stringify({
      output: material.expectedOutput,
      stdout: material.expectedStdout,
      stderr: material.expectedStderr,
      marker: material.appendedLine,
      untracked: ['m3b-output.txt', 'm3b_tool.py']
    }));
    succeeded = true;
  } catch (error) {
    caughtError = error;
  } finally {
    elapsedMs = Number((process.hrtime.bigint() - started) / 1_000_000n);
    await connection.transport.close().catch(() => {});
    cleanupM4(workspaceId, taskId, workspacePath);
  }
  if (caughtError) throw caughtError;
  return {
    backend: 'ORDIVON_M4', pairId, succeeded, elapsedMs,
    toolCalls: metrics.logicalCalls, remoteRoundTrips: transportDelta(transportSnapshot(bootstrap), transportAtTaskStart).httpRequests,
    contextBytes: metrics.contextBytes, outputBytes: metrics.outputBytes,
    recoveredAfterDisconnect: true, fallbackCount: 0, semanticDigest,
    callBreakdown: metrics.calls,
    serverVersion: connection.client.getServerVersion(),
    transport: {
      bootstrap: transportAtTaskStart,
      task: transportDelta(transportSnapshot(bootstrap), transportAtTaskStart),
      total: transportSnapshot(bootstrap)
    }
  };
}

async function probeM4Disconnect(label) {
  const metrics = makeMetrics();
  const workspaceId = `m4-probe-workspace-${label}`;
  const taskId = `m4-probe-task-${label}`;
  const workspacePath = join(config.m4StoreRoot, 'workspaces', workspaceId);
  removeWorktree(workspacePath);
  const first = await connectM4(metrics, `m4-probe-a-${label}`);
  await m4Call(first, metrics, 'workspace.open', {
    schemaVersion: 1,
    workspaceId,
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision
  });
  await m4Call(first, metrics, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'm4_probe.py',
      mode: 'WRITE',
      content: "import time\nprint('M4_PROBE_START', flush=True)\ntime.sleep(1.2)\nprint('M4_PROBE_DONE', flush=True)\n",
      expectedDigest: null
    }]
  });
  const stream = first.client.experimental.tasks.callToolStream(
    { name: 'workspace.exec', arguments: {
      schemaVersion: 1,
      execution: {
        schemaVersion: 1,
        taskId,
        workspaceId,
        executable: '/usr/bin/python3.14',
        args: ['m4_probe.py'],
        cwdRelative: '.',
        env: { PYTHONUNBUFFERED: '1' },
        timeoutMs: 10_000,
        stdoutLimitBytes: 65_536,
        stderrLimitBytes: 65_536
      },
      waitMs: 0,
      stdoutTailBytes: 1024,
      stderrTailBytes: 1024
    }},
    CallToolResultSchema,
    { task: { ttl: 60_000 } }
  );
  let created = null;
  for await (const message of stream) {
    if (message.type === 'taskCreated') {
      created = message.task.taskId;
      break;
    }
  }
  if (created !== taskId) throw new Error('M4 native task creation mismatch');
  await first.transport.close().catch(() => {});
  await new Promise(resolve => setTimeout(resolve, 1500));
  const second = await connectM4(metrics, `m4-probe-b-${label}`);
  const task = await second.client.experimental.tasks.getTask(taskId);
  const result = await second.client.experimental.tasks.getTaskResult(
    taskId,
    CallToolResultSchema
  );
  const recovered = task.status === 'completed' &&
    result.structuredContent?.stdoutTail?.includes('M4_PROBE_DONE');
  await second.transport.close().catch(() => {});
  cleanupM4(workspaceId, taskId, workspacePath);
  rmSync(join(config.m4StoreRoot, 'm4-native-task-projections', `${taskId}.json`), { force: true });
  return {
    recovered,
    taskId,
    observation: result.structuredContent,
    metrics
  };
}

function parseLegacyPid(result) {
  const text = textContent(result);
  const match = text.match(/PID\s+(\d+)/i);
  if (!match) throw new Error(`cannot parse legacy PID: ${text}`);
  return Number(match[1]);
}

async function probeLegacyDisconnect() {
  const metrics = makeMetrics();
  const first = await connectLegacy(metrics, 'ordivon-m2-disconnect-a');
  const command = [
    '/usr/bin/python3',
    '-c',
    "import time; print('M2_RECOVERY_START', flush=True); time.sleep(1.5); print('M2_RECOVERY_DONE', flush=True)"
  ].map(shellQuote).join(' ');
  const started = await legacyCall(first, metrics, 'start_process', {
    command,
    timeout_ms: 100,
    origin: 'llm'
  });
  const pid = parseLegacyPid(started);
  await first.transport.terminateSession().catch(() => {});
  await first.transport.close().catch(() => {});
  await new Promise(resolve => setTimeout(resolve, 200));

  const second = await connectLegacy(metrics, 'ordivon-m2-disconnect-b');
  let readText = '';
  try {
    const read = await legacyCall(second, metrics, 'read_process_output', {
      pid,
      timeout_ms: 3_000,
      offset: 0,
      length: 100
    });
    readText = textContent(read);
  } catch (error) {
    readText = String(error);
  }
  await second.transport.terminateSession().catch(() => {});
  await second.transport.close().catch(() => {});
  try {
    process.kill(pid, 'SIGKILL');
  } catch {
    // The process may already have exited.
  }
  return {
    recovered: readText.includes('M2_RECOVERY_DONE'),
    pid,
    observation: readText,
    metrics
  };
}

function summarize(samples) {
  const fields = [
    'elapsedMs',
    'toolCalls',
    'remoteRoundTrips',
    'contextBytes',
    'outputBytes'
  ];
  const result = {};
  for (const field of fields) result[field] = median(samples.map(sample => sample[field]));
  result.elapsedP50Ms = percentile(samples.map(sample => sample.elapsedMs), 0.50);
  result.elapsedP95Ms = percentile(samples.map(sample => sample.elapsedMs), 0.95);
  result.taskHttpRequests = median(samples.map(sample => sample.transport.task.httpRequests));
  result.taskRequestBytes = median(samples.map(sample => sample.transport.task.requestBytes));
  result.taskResponseBytes = median(samples.map(sample => sample.transport.task.responseBytes));
  result.succeeded = samples.every(sample => sample.succeeded);
  result.recoveredAfterDisconnect = samples.every(sample => sample.recoveredAfterDisconnect);
  result.fallbackCount = samples.reduce((total, sample) => total + sample.fallbackCount, 0);
  result.semanticDigests = [...new Set(samples.map(sample => sample.semanticDigest))].sort();
  result.callBreakdown = samples[0].callBreakdown.map((call, index) => ({
    sequence: index + 1,
    name: call.name,
    medianResponseBytes: median(
      samples.map(sample => sample.callBreakdown[index].responseBytes)
    )
  }));
  return result;
}

function reductionPercent(legacy, ordivon) {
  if (legacy === 0) return ordivon === 0 ? 0 : Number.NEGATIVE_INFINITY;
  return ((legacy - ordivon) / legacy) * 100;
}
function assessCutover(legacy, ordivon) {
  const gates = {
    completionNotWorse: !legacy.succeeded || ordivon.succeeded,
    toolCallsAtMostFive: ordivon.toolCalls <= 5,
    contextAtMost3220: ordivon.contextBytes <= 3220,
    elapsedWithin10Percent: ordivon.elapsedMs <= Math.ceil(legacy.elapsedMs * 1.10),
    p95Within10Percent: ordivon.elapsedP95Ms <= Math.ceil(legacy.elapsedP95Ms * 1.10),
    httpRequestsNotWorse: ordivon.taskHttpRequests <= legacy.taskHttpRequests,
    disconnectRecovery: ordivon.recoveredAfterDisconnect,
    noFallback: ordivon.fallbackCount === 0,
    semanticEquivalence:
      legacy.semanticDigests.length === ordivon.semanticDigests.length &&
      legacy.semanticDigests.every((digest, index) => digest === ordivon.semanticDigests[index])
  };
  return {
    eligible: Object.values(gates).every(Boolean), gates,
    reductions: {
      toolCallsPercent: reductionPercent(legacy.toolCalls, ordivon.toolCalls),
      contextBytesPercent: reductionPercent(legacy.contextBytes, ordivon.contextBytes),
      outputBytesPercent: reductionPercent(legacy.outputBytes, ordivon.outputBytes)
    },
    elapsedRatio: ordivon.elapsedMs / legacy.elapsedMs,
    p95Ratio: ordivon.elapsedP95Ms / legacy.elapsedP95Ms
  };
}

function asContractSample(sampleId, summary, backend) {
  return {
    schemaVersion: SCHEMA_VERSION,
    sampleId,
    capability: 'WORKSPACE_EXEC',
    backend,
    succeeded: summary.succeeded,
    elapsedMs: summary.elapsedMs,
    toolCalls: summary.toolCalls,
    remoteRoundTrips: summary.remoteRoundTrips,
    contextBytes: summary.contextBytes,
    outputBytes: summary.outputBytes,
    recoveredAfterDisconnect: summary.recoveredAfterDisconnect,
    fallbackCount: summary.fallbackCount
  };
}
const legacySamples = [];
const ordivonSamples = [];
const totalPairs = config.warmups + config.iterations;

for (let index = 0; index < totalPairs; index += 1) {
  const pairId = `${process.pid}-${Date.now()}-${index}`;
  const marker = `pair-${index}-${config.sourceRevision.slice(0, 12)}`;
  const order = index % 2 === 0 ? ['legacy', 'ordivon'] : ['ordivon', 'legacy'];
  const pairResults = {};
  for (const backend of order) {
    pairResults[backend] = backend === 'legacy'
      ? await runLegacyJourney(pairId, marker)
      : await runOrdivonJourney(pairId, marker);
  }
  if (pairResults.legacy.semanticDigest !== pairResults.ordivon.semanticDigest) {
    throw new Error(`semantic mismatch in pair ${pairId}`);
  }
  if (index >= config.warmups) {
    legacySamples.push(pairResults.legacy);
    ordivonSamples.push(pairResults.ordivon);
  }
  console.error(
    `M4B_PAIR index=${index} warmup=${index < config.warmups} ` +
    `legacyMs=${pairResults.legacy.elapsedMs} ordivonMs=${pairResults.ordivon.elapsedMs} ` +
    `legacyCalls=${pairResults.legacy.toolCalls} ordivonCalls=${pairResults.ordivon.toolCalls}`
  );
}

const disconnectProbe = await probeLegacyDisconnect();
const m4DisconnectProbe = await probeM4Disconnect(`${process.pid}-${Date.now()}`);
for (const sample of legacySamples) sample.recoveredAfterDisconnect = disconnectProbe.recovered;
for (const sample of ordivonSamples) sample.recoveredAfterDisconnect = m4DisconnectProbe.recovered;
const legacySummary = summarize(legacySamples);
const ordivonSummary = summarize(ordivonSamples);
const cutover = assessCutover(legacySummary, ordivonSummary);
const evidence = {
  schemaVersion: SCHEMA_VERSION,
  phase: 'ORDIVON-MIGRATION-M4B-2026-07-22',
  evidenceClass: 'MCP_TO_MCP_M4B_DIFFERENTIAL_BENCHMARK',
  generatedAt: new Date().toISOString(),
  sourceRevision: config.sourceRevision,
  taskJourney: 'M4B_TARGETED_READ_MCP_TASK_JOURNEY',
  metricSemantics: {
    elapsedMs: 'Median task journey wall time; adapter bootstrap and cleanup excluded.',
    toolCalls: 'Logical model-facing adapter calls.',
    remoteRoundTrips: 'Actual task-phase HTTP request count measured by the shared client fetch wrapper.',
    contextBytes: 'Serialized tool or CLI responses returned to the model-facing caller.',
    outputBytes: 'Task stdout and stderr bytes deliberately consumed by the caller.',
    legacyTransport: 'Actual HTTP metrics are supplementary.',
    ordivonTransport: 'Actual stateless Streamable HTTP JSON metrics measured by the same client.'
  },
  runtimeIdentity: {
    branch: execLocal('git', ['-C', config.repoRoot, 'branch', '--show-current']).trim(),
    gitHeadAtRun: execLocal('git', ['-C', config.repoRoot, 'rev-parse', 'HEAD']).trim(),
    desktopCommander: legacySamples[0].serverVersion,
    mcpSdkVersion: JSON.parse(
      readFileSync(join(config.sdkRoot, 'package.json'), 'utf8')
    ).version,
    nodeVersion: process.version,
    kernel: execLocal('uname', ['-r']).trim(),
    systemd: execLocal('systemctl', ['--version']).split('\n')[0],
    m4Binary: { path: config.m4Binary, digest: sha256File(config.m4Binary) },
    m4Runner: { path: config.m4Runner, digest: sha256File(config.m4Runner) },
    m4Server: ordivonSamples[0].serverVersion
  },
  configuration: {
    iterations: config.iterations,
    warmups: config.warmups,
    alternatingOrder: true,
    scriptSleepSeconds: SCRIPT_SLEEP_SECONDS,
    legacyMcpUrl: config.legacyMcpUrl,
    m4McpUrl: config.m4McpUrl,
    sdkRootProvidedExternally: true
  },
  rawSamples: {
    legacy: legacySamples,
    ordivon: ordivonSamples
  },
  summaries: {
    legacy: legacySummary,
    ordivon: ordivonSummary
  },
  contractSamples: {
    legacy: asContractSample(
      'm2-legacy-median',
      legacySummary,
      'LEGACY_DESKTOP_COMMANDER'
    ),
    ordivon: asContractSample(
      'm2-ordivon-median',
      ordivonSummary,
      'ORDIVON'
    )
  },
  performanceDelta: {
    elapsedMs: ordivonSummary.elapsedMs - legacySummary.elapsedMs,
    toolCalls: ordivonSummary.toolCalls - legacySummary.toolCalls,
    remoteRoundTrips:
      ordivonSummary.remoteRoundTrips - legacySummary.remoteRoundTrips,
    contextBytes: ordivonSummary.contextBytes - legacySummary.contextBytes,
    outputBytes: ordivonSummary.outputBytes - legacySummary.outputBytes,
    disconnectRecoveryImproved:
      !legacySummary.recoveredAfterDisconnect &&
      ordivonSummary.recoveredAfterDisconnect
  },
  m4DisconnectProbe: {
    recovered: m4DisconnectProbe.recovered,
    observation: m4DisconnectProbe.observation,
    logicalCalls: m4DisconnectProbe.metrics.logicalCalls,
    httpRequests: m4DisconnectProbe.metrics.httpRequests
  },
  disconnectProbe: {
    backend: 'LEGACY_DESKTOP_COMMANDER',
    recovered: disconnectProbe.recovered,
    observation: disconnectProbe.observation,
    logicalCalls: disconnectProbe.metrics.logicalCalls,
    httpRequests: disconnectProbe.metrics.httpRequests
  },
  cutover,
  claimsNotMade: [
    'Both backends use real Streamable HTTP MCP through the same JS SDK.',
    'This benchmark covers one local Python task journey, not all coding work.',
    'The result can authorize limited dogfood only, not production routing.'
  ]
};
mkdirSync(dirname(config.outputPath), { recursive: true });
writeFileSync(config.outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify({
  outputPath: config.outputPath,
  legacy: legacySummary,
  ordivon: ordivonSummary,
  cutover
}, null, 2));

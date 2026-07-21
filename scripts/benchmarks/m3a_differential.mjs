#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const SCHEMA_VERSION = 1;
const DEFAULT_ITERATIONS = 5;
const DEFAULT_WARMUPS = 1;
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
  mcpUrl: args.get('--mcp-url') ?? 'http://127.0.0.1:8811/mcp',
  repoRoot: resolve(args.get('--repo-root') ?? process.cwd()),
  sourceRevision: args.get('--source-revision') ?? 'e1a40df3878881e47a56fd428ec6eb316301799b',
  m1Cli: resolve(args.get('--m1-cli') ?? 'target/debug/ordivon-m1-cli'),
  m1Runner: resolve(args.get('--m1-runner') ?? 'target/debug/ordivon-task-runner'),
  outputPath: resolve(args.get('--output') ?? '/tmp/ordivon-m3a-evidence.json'),
  iterations: Number(args.get('--iterations') ?? DEFAULT_ITERATIONS),
  warmups: Number(args.get('--warmups') ?? DEFAULT_WARMUPS)
};

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
  const transport = new StreamableHTTPClientTransport(new URL(config.mcpUrl), {
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
  const appendedLine = `M2 differential marker ${marker}`;
  const script = [
    'from pathlib import Path',
    'import sys,time',
    "text=Path('crates/ordivon-exec/README.md').read_text()",
    `marker=${JSON.stringify(appendedLine)}`,
    "Path('m2-output.txt').write_text('marker=' + str(marker in text))",
    "print('M2_STDOUT marker observed', flush=True)",
    "print('M2_STDERR diagnostic', file=sys.stderr, flush=True)",
    `time.sleep(${SCRIPT_SLEEP_SECONDS})`,
    ''
  ].join('\n');
  return {
    appendedLine,
    script,
    expectedOutput: 'marker=True',
    expectedStdout: 'M2_STDOUT marker observed',
    expectedStderr: 'M2_STDERR diagnostic'
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
  const workspacePath = join('/root/.local/share/ordivon-m2', `legacy-${pairId}`);
  mkdirSync(dirname(workspacePath), { recursive: true });
  removeWorktree(workspacePath);
  const connection = await connectLegacy(bootstrap, `ordivon-m2-legacy-${pairId}`);
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
    const read = await legacyCall(connection, metrics, 'read_file', {
      path: join(workspacePath, 'crates/ordivon-exec/README.md'),
      offset: 0,
      length: 1000,
      origin: 'llm'
    });
    const original = legacyReadBody(read);
    const updated = `${original}\n${material.appendedLine}\n`;
    await legacyCall(connection, metrics, 'write_file', {
      path: join(workspacePath, 'crates/ordivon-exec/README.md'),
      content: updated,
      mode: 'rewrite',
      origin: 'llm'
    });
    await legacyCall(connection, metrics, 'write_file', {
      path: join(workspacePath, 'm2_tool.py'),
      content: material.script,
      mode: 'rewrite',
      origin: 'llm'
    });
    const executionText = await legacyExec(
      connection,
      metrics,
      `cd ${shellQuote(workspacePath)} && /usr/bin/python3 m2_tool.py 2>&1`,
      'model-authored tool'
    );
    if (!executionText.includes(material.expectedStdout) || !executionText.includes(material.expectedStderr)) {
      throw new Error(`legacy execution output mismatch: ${executionText}`);
    }
    metrics.outputBytes += byteLength(material.expectedStdout) + byteLength(material.expectedStderr);
    const generated = await legacyCall(connection, metrics, 'read_file', {
      path: join(workspacePath, 'm2-output.txt'),
      offset: 0,
      length: 20,
      origin: 'llm'
    });
    const generatedBody = legacyReadBody(generated);
    if (generatedBody !== material.expectedOutput) {
      throw new Error(`legacy generated output mismatch: ${generatedBody}`);
    }
    const diffText = await legacyExec(
      connection,
      metrics,
      `cd ${shellQuote(workspacePath)} && git diff -- crates/ordivon-exec/README.md && printf '\n__M2_UNTRACKED__\n' && git ls-files --others --exclude-standard`,
      'git diff and untracked files'
    );
    if (!diffText.includes(material.appendedLine)) throw new Error('legacy diff lost marker');
    if (!diffText.includes('m2_tool.py') || !diffText.includes('m2-output.txt')) {
      throw new Error('legacy untracked paths incomplete');
    }
    semanticDigest = sha256(
      JSON.stringify({
        output: generatedBody,
        stdout: material.expectedStdout,
        stderr: material.expectedStderr,
        marker: material.appendedLine,
        untracked: ['m2-output.txt', 'm2_tool.py']
      })
    );
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
    backend: 'LEGACY_DESKTOP_COMMANDER',
    pairId,
    succeeded,
    elapsedMs,
    toolCalls: metrics.logicalCalls,
    remoteRoundTrips: metrics.logicalCalls,
    contextBytes: metrics.contextBytes,
    outputBytes: metrics.outputBytes,
    recoveredAfterDisconnect: false,
    fallbackCount: 0,
    semanticDigest,
    callBreakdown: metrics.calls,
    serverVersion: connection.client.getServerVersion(),
    transport: {
      bootstrap: transportAtTaskStart,
      task: transportDelta(transportSnapshot(bootstrap), transportAtTaskStart),
      total: transportSnapshot(bootstrap)
    }
  };
}
function m1Call(metrics, storeRoot, command, request) {
  metrics.logicalCalls += 1;
  const result = spawnSync(config.m1Cli, [command], {
    input: JSON.stringify(request),
    encoding: 'utf8',
    timeout: TOOL_TIMEOUT_MS,
    maxBuffer: 16 * 1024 * 1024,
    env: {
      ...process.env,
      ORDIVON_M1_STORE_ROOT: storeRoot,
      ORDIVON_M1_RUNNER_PATH: config.m1Runner,
      ORDIVON_M1_ALLOWED_EXECUTABLE_ROOTS: '/usr/bin'
    }
  });
  if (result.error) throw result.error;
  const responseBytes = byteLength(result.stdout);
  metrics.contextBytes += responseBytes;
  metrics.calls.push({ name: command, responseBytes });
  const body = JSON.parse(result.stdout);
  if (result.status !== 0 || !body.ok) {
    throw new Error(`${command} failed: ${JSON.stringify(body.error ?? body)}`);
  }
  return body.result;
}

function cleanupM1(storeRoot, workspacePath) {
  removeWorktree(workspacePath);
  rmSync(storeRoot, { recursive: true, force: true });
}

function runOrdivonJourney(pairId, marker) {
  const metrics = makeMetrics();
  const storeRoot = join('/root/.local/share/ordivon-m3', `m3a-store-${pairId}`);
  const workspaceId = `m3a-workspace-${pairId}`;
  const taskId = `m3a-task-${pairId}`;
  const workspacePath = join(storeRoot, 'workspaces', workspaceId);
  rmSync(storeRoot, { recursive: true, force: true });
  mkdirSync(dirname(storeRoot), { recursive: true });
  const material = journeyMaterial(marker);
  const started = process.hrtime.bigint();
  let succeeded = false;
  let semanticDigest = null;
  let caughtError = null;
  let elapsedMs = 0;
  try {
    m1Call(metrics, storeRoot, 'workspace-open', {
      schemaVersion: 1,
      workspaceId,
      sourceRepo: config.repoRoot,
      sourceRevision: config.sourceRevision
    });
    const read = m1Call(metrics, storeRoot, 'workspace-read-compact', {
      schemaVersion: 1,
      workspaceId,
      relativePath: 'crates/ordivon-exec/README.md',
      maxBytes: 1_048_576
    });
    m1Call(metrics, storeRoot, 'workspace-mutate', {
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
          relativePath: 'm2_tool.py',
          mode: 'WRITE',
          content: material.script,
          expectedDigest: null
        }
      ]
    });
    const compact = m1Call(metrics, storeRoot, 'task-run', {
      schemaVersion: 1,
      execution: {
        schemaVersion: 1,
        taskId,
        workspaceId,
        executable: '/usr/bin/python3.14',
        args: ['m2_tool.py'],
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
    if (compact.status !== 'COMPLETED') throw new Error(`M3A task failed: ${JSON.stringify(compact)}`);
    if (!compact.stdoutTail.includes(material.expectedStdout)) throw new Error('M3A stdout mismatch');
    if (!compact.stderrTail.includes(material.expectedStderr)) throw new Error('M3A stderr mismatch');
    metrics.outputBytes += byteLength(compact.stdoutTail) + byteLength(compact.stderrTail);
    const generated = m1Call(metrics, storeRoot, 'workspace-read-slice-compact', {
      schemaVersion: 1,
      workspaceId,
      relativePath: 'm2-output.txt',
      offset: 0,
      maxBytes: 1024
    });
    if (generated.content !== material.expectedOutput) throw new Error('M3A generated output mismatch');
    const diff = m1Call(metrics, storeRoot, 'workspace-diff-compact', {
      schemaVersion: 1,
      workspaceId,
      maxBytes: 1_048_576
    });
    if (!diff.diff.includes(material.appendedLine)) throw new Error('M3A diff lost marker');
    if (!diff.untrackedPaths.includes('m2_tool.py') || !diff.untrackedPaths.includes('m2-output.txt')) {
      throw new Error('M3A untracked paths incomplete');
    }
    semanticDigest = sha256(JSON.stringify({
      output: generated.content,
      stdout: material.expectedStdout,
      stderr: material.expectedStderr,
      marker: material.appendedLine,
      untracked: ['m2-output.txt', 'm2_tool.py']
    }));
    succeeded = true;
  } catch (error) {
    caughtError = error;
  } finally {
    elapsedMs = Number((process.hrtime.bigint() - started) / 1_000_000n);
    cleanupM1(storeRoot, workspacePath);
  }
  if (caughtError) throw caughtError;
  return {
    backend: 'ORDIVON', pairId, succeeded, elapsedMs,
    toolCalls: metrics.logicalCalls,
    remoteRoundTrips: metrics.logicalCalls,
    contextBytes: metrics.contextBytes,
    outputBytes: metrics.outputBytes,
    recoveredAfterDisconnect: true,
    fallbackCount: 0,
    semanticDigest,
    callBreakdown: metrics.calls,
    transport: { mode: 'LOCAL_CLI', actualHttpRequests: null }
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
    toolCallsAtMostSix: ordivon.toolCalls <= 6,
    contextNotWorse: ordivon.contextBytes <= legacy.contextBytes,
    elapsedWithin10Percent: ordivon.elapsedMs <= Math.ceil(legacy.elapsedMs * 1.10),
    disconnectRecovery: ordivon.recoveredAfterDisconnect,
    noFallback: ordivon.fallbackCount === 0,
    semanticEquivalence:
      legacy.semanticDigests.length === ordivon.semanticDigests.length &&
      legacy.semanticDigests.every((digest, index) => digest === ordivon.semanticDigests[index])
  };
  return {
    eligible: Object.values(gates).every(Boolean),
    gates,
    reductions: {
      toolCallsPercent: reductionPercent(legacy.toolCalls, ordivon.toolCalls),
      contextBytesPercent: reductionPercent(legacy.contextBytes, ordivon.contextBytes),
      outputBytesPercent: reductionPercent(legacy.outputBytes, ordivon.outputBytes)
    },
    elapsedRatio: ordivon.elapsedMs / legacy.elapsedMs
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
      : runOrdivonJourney(pairId, marker);
  }
  if (pairResults.legacy.semanticDigest !== pairResults.ordivon.semanticDigest) {
    throw new Error(`semantic mismatch in pair ${pairId}`);
  }
  if (index >= config.warmups) {
    legacySamples.push(pairResults.legacy);
    ordivonSamples.push(pairResults.ordivon);
  }
  console.error(
    `M3A_PAIR index=${index} warmup=${index < config.warmups} ` +
    `legacyMs=${pairResults.legacy.elapsedMs} ordivonMs=${pairResults.ordivon.elapsedMs} ` +
    `legacyCalls=${pairResults.legacy.toolCalls} ordivonCalls=${pairResults.ordivon.toolCalls}`
  );
}

const disconnectProbe = await probeLegacyDisconnect();
for (const sample of legacySamples) sample.recoveredAfterDisconnect = disconnectProbe.recovered;
const legacySummary = summarize(legacySamples);
const ordivonSummary = summarize(ordivonSamples);
const cutover = assessCutover(legacySummary, ordivonSummary);
const evidence = {
  schemaVersion: SCHEMA_VERSION,
  phase: 'ORDIVON-MIGRATION-M3A-2026-07-22',
  evidenceClass: 'LOCAL_M3A_DIFFERENTIAL_BENCHMARK',
  generatedAt: new Date().toISOString(),
  sourceRevision: config.sourceRevision,
  taskJourney: 'M3A_FULL_READ_COMPACT_TASK_JOURNEY',
  metricSemantics: {
    elapsedMs: 'Median task journey wall time; adapter bootstrap and cleanup excluded.',
    toolCalls: 'Logical model-facing adapter calls.',
    remoteRoundTrips: 'Transport-normalized logical calls for both backends.',
    contextBytes: 'Serialized tool or CLI responses returned to the model-facing caller.',
    outputBytes: 'Task stdout and stderr bytes deliberately consumed by the caller.',
    legacyTransport: 'Actual HTTP metrics are supplementary.',
    ordivonTransport: 'M3 is local CLI; no MCP transport result is fabricated.'
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
    m1Cli: {
      path: config.m1Cli,
      digest: sha256File(config.m1Cli)
    },
    m1Runner: {
      path: config.m1Runner,
      digest: sha256File(config.m1Runner)
    }
  },
  configuration: {
    iterations: config.iterations,
    warmups: config.warmups,
    alternatingOrder: true,
    scriptSleepSeconds: SCRIPT_SLEEP_SECONDS,
    mcpUrl: config.mcpUrl,
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
  disconnectProbe: {
    backend: 'LEGACY_DESKTOP_COMMANDER',
    recovered: disconnectProbe.recovered,
    observation: disconnectProbe.observation,
    logicalCalls: disconnectProbe.metrics.logicalCalls,
    httpRequests: disconnectProbe.metrics.httpRequests
  },
  cutover,
  claimsNotMade: [
    'Ordivon M3 has no MCP transport, so no Ordivon HTTP byte claim is made.',
    'This benchmark covers one local Python task journey, not all coding work.',
    'The result does not authorize a production route switch.'
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

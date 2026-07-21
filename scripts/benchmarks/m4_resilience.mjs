#!/usr/bin/env node

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

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
  const output = spawnSync('find', [
    '/root/.npm/_npx', '-path',
    '*/node_modules/@modelcontextprotocol/sdk/package.json', '-print'
  ], { encoding: 'utf8', timeout: 10_000 });
  if (output.status !== 0) throw new Error(output.stderr);
  const packagePath = output.stdout.trim().split('\n').filter(Boolean)[0];
  if (!packagePath) throw new Error('MCP SDK not found');
  return dirname(packagePath);
}
const args = parseArgs(process.argv.slice(2));
const config = {
  sdkRoot: discoverSdkRoot(),
  endpoint: new URL(args.get('--mcp-url') ?? 'http://127.0.0.1:8895/mcp'),
  repoRoot: resolve(args.get('--repo-root') ?? process.cwd()),
  sourceRevision: args.get('--source-revision') ?? '4592689dc9183fcb08f4828d3d752a4cf57e318f',
  storeRoot: resolve(args.get('--store-root') ?? '/root/.local/share/ordivon-m4-formal'),
  serverUnit: args.get('--server-unit') ?? 'ordivon-m4-formal-http.service',
  outputPath: resolve(args.get('--output') ?? '/tmp/ordivon-m4-resilience.json'),
  token: process.env.ORDIVON_M4_BEARER_TOKEN ?? ''
};
if (config.token.length < 32) throw new Error('ORDIVON_M4_BEARER_TOKEN must be at least 32 characters');

const { Client } = await import(
  pathToFileURL(join(config.sdkRoot, 'dist/esm/client/index.js')).href
);
const { StreamableHTTPClientTransport } = await import(
  pathToFileURL(join(config.sdkRoot, 'dist/esm/client/streamableHttp.js')).href
);
const { CallToolResultSchema } = await import(
  pathToFileURL(join(config.sdkRoot, 'dist/esm/types.js')).href
);

async function connect(name) {
  const client = new Client({ name, version: '0.1.0' });
  const transport = new StreamableHTTPClientTransport(config.endpoint, {
    requestInit: { headers: { Authorization: `Bearer ${config.token}` } }
  });
  await client.connect(transport);
  return { client, transport };
}
async function call(connection, name, argumentsValue) {
  const result = await connection.client.callTool(
    { name, arguments: argumentsValue },
    CallToolResultSchema
  );
  if (result.isError) throw new Error(`${name}: ${JSON.stringify(result.structuredContent)}`);
  return result.structuredContent;
}

async function startNative(connection, taskId, workspaceId, scriptName) {
  const stream = connection.client.experimental.tasks.callToolStream(
    { name: 'workspace.exec', arguments: {
      schemaVersion: 1,
      execution: {
        schemaVersion: 1,
        taskId,
        workspaceId,
        executable: '/usr/bin/python3.14',
        args: [scriptName],
        cwdRelative: '.',
        env: { PYTHONUNBUFFERED: '1' },
        timeoutMs: 60_000,
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
  for await (const message of stream) {
    if (message.type === 'taskCreated') return message.task.taskId;
  }
  throw new Error('native task creation did not return taskCreated');
}
async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      const connection = await connect(`m4-restart-wait-${attempt}`);
      await connection.transport.close();
      return;
    } catch (error) {
      lastError = error;
      await new Promise(resolveWait => setTimeout(resolveWait, 100));
    }
  }
  throw lastError ?? new Error('M4 server did not return');
}

function run(program, argsValue) {
  const result = spawnSync(program, argsValue, { encoding: 'utf8', timeout: 15_000 });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${program} failed: ${result.stderr}`);
  }
  return result.stdout;
}

function cleanup(workspaceId, taskIds) {
  const workspacePath = join(config.storeRoot, 'workspaces', workspaceId);
  if (existsSync(workspacePath)) {
    spawnSync('git', [
      '-C', config.repoRoot, 'worktree', 'remove', '--force', workspacePath
    ], { encoding: 'utf8', timeout: 10_000 });
  }
  rmSync(join(config.storeRoot, 'workspace-records', `${workspaceId}.json`), { force: true });
  for (const taskId of taskIds) {
    rmSync(join(config.storeRoot, 'tasks', taskId), { recursive: true, force: true });
    rmSync(join(config.storeRoot, 'm4-native-task-projections', `${taskId}.json`), { force: true });
  }
}
const suffix = `${process.pid}-${Date.now()}`;
const workspaceId = `m4-resilience-workspace-${suffix}`;
const restartTaskId = `m4-restart-task-${suffix}`;
const cancelTaskId = `m4-cancel-task-${suffix}`;
const first = await connect('m4-resilience-a');
await call(first, 'workspace.open', {
  schemaVersion: 1,
  workspaceId,
  sourceRepo: config.repoRoot,
  sourceRevision: config.sourceRevision
});
await call(first, 'workspace.mutate', {
  schemaVersion: 1,
  workspaceId,
  mutations: [
    {
      relativePath: 'm4_restart.py',
      mode: 'WRITE',
      content: "import time\nprint('RESTART_START', flush=True)\ntime.sleep(2)\nprint('RESTART_DONE', flush=True)\n",
      expectedDigest: null
    },
    {
      relativePath: 'm4_cancel.py',
      mode: 'WRITE',
      content: "import time\nprint('CANCEL_START', flush=True)\ntime.sleep(30)\n",
      expectedDigest: null
    }
  ]
});
const createdRestart = await startNative(
  first,
  restartTaskId,
  workspaceId,
  'm4_restart.py'
);
assert.equal(createdRestart, restartTaskId);
await first.transport.close();

run('systemctl', ['restart', config.serverUnit]);
await waitForServer();
await new Promise(resolveWait => setTimeout(resolveWait, 2200));

const second = await connect('m4-resilience-b');
const restartedTask = await second.client.experimental.tasks.getTask(restartTaskId);
assert.equal(restartedTask.status, 'completed');
const restartedResult = await second.client.experimental.tasks.getTaskResult(
  restartTaskId,
  CallToolResultSchema
);
assert.equal(restartedResult.isError, false);
assert.match(restartedResult.structuredContent.stdoutTail, /RESTART_DONE/);
const createdCancel = await startNative(
  second,
  cancelTaskId,
  workspaceId,
  'm4_cancel.py'
);
assert.equal(createdCancel, cancelTaskId);
await new Promise(resolveWait => setTimeout(resolveWait, 200));
const cancelled = await second.client.experimental.tasks.cancelTask(cancelTaskId);
assert.equal(cancelled.status, 'cancelled');
const cancelledTask = await second.client.experimental.tasks.getTask(cancelTaskId);
assert.equal(cancelledTask.status, 'cancelled');
const cancelledResult = await second.client.experimental.tasks.getTaskResult(
  cancelTaskId,
  CallToolResultSchema
);
assert.equal(cancelledResult.isError, true);
assert.equal(cancelledResult.structuredContent.error.code, 'TASK_CANCELLED');
assert.match(cancelledResult.structuredContent.error.message, /cancelled/i);
await second.transport.close();
const units = spawnSync('systemctl', [
  'list-units', '--all', `ordivon-m1-${restartTaskId}.service`,
  `ordivon-m1-${cancelTaskId}.service`, '--no-legend', '--plain'
], { encoding: 'utf8', timeout: 10_000 });
if (units.error) throw units.error;
const processes = spawnSync('pgrep', [
  '-af', 'm4_restart.py|m4_cancel.py'
], { encoding: 'utf8', timeout: 10_000 });
const residualUnits = units.stdout.trim();
const residualProcesses = processes.status === 0
  ? processes.stdout.split('\n').filter(line => line && !line.includes('m4_resilience.mjs'))
  : [];
assert.equal(residualUnits, '');
assert.deepEqual(residualProcesses, []);

const evidence = {
  schemaVersion: 1,
  phase: 'ORDIVON-MIGRATION-M4-RESILIENCE-2026-07-22',
  generatedAt: new Date().toISOString(),
  sourceRevision: config.sourceRevision,
  serverUnit: config.serverUnit,
  serverRestartRecovery: {
    taskId: restartTaskId,
    status: restartedTask.status,
    stdoutTail: restartedResult.structuredContent.stdoutTail,
    recovered: true
  },
  cancellation: {
    taskId: cancelTaskId,
    status: cancelledTask.status,
    resultIsError: cancelledResult.isError,
    errorCode: cancelledResult.structuredContent.error.code,
    stdoutTail: cancelledResult.structuredContent.error.message,
    cgroupClean: true
  },
  cleanup: {
    residualUnits,
    residualProcesses
  }
};
cleanup(workspaceId, [restartTaskId, cancelTaskId]);
mkdirSync(dirname(config.outputPath), { recursive: true });
writeFileSync(config.outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify(evidence, null, 2));

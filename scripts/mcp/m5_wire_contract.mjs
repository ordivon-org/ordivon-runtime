#!/usr/bin/env node

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  CallToolResultSchema,
  assertCompactSuccess,
  byteLength,
  callTool,
  closeConnection,
  connectM5,
  m5Config,
  structured
} from './client.mjs';

const config = m5Config();
const suffix = `${process.pid}-${Date.now()}`;
const workspaceId = `m5-wire-workspace-${suffix}`;
const taskId = `m5-wire-task-${suffix}`;
const connection = await connectM5('ordivon-m5-wire-contract');
const observations = [];

async function measured(name, args, budget) {
  const result = await callTool(connection, name, args);
  const bytes = assertCompactSuccess(result, budget, name);
  observations.push({ name, bytes });
  return structured(result);
}

try {
  const listed = await connection.client.listTools({}, undefined);
  const names = listed.tools.map(tool => tool.name).sort();
  assert.deepEqual(names, [
    'artifact.read',
    'task.cancel',
    'task.observe',
    'workspace.diff',
    'workspace.exec',
    'workspace.mutate',
    'workspace.open',
    'workspace.read'
  ]);

  const opened = await measured('workspace.open', {
    schemaVersion: 1,
    workspaceId,
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision
  }, 220);
  assert.equal(opened.workspaceId, workspaceId);

  const deniedRevision = await connection.client.callTool(
    {
      name: 'workspace.open',
      arguments: {
        schemaVersion: 1,
        workspaceId: `${workspaceId}-denied-revision`,
        sourceRepo: config.repoRoot,
        sourceRevision: `${config.sourceRevision}-other`
      }
    },
    CallToolResultSchema
  );
  assert.equal(deniedRevision.isError, true);
  assert.equal(
    deniedRevision.structuredContent.error.code,
    'SOURCE_REVISION_NOT_ALLOWED'
  );
  assert.ok(byteLength(deniedRevision) <= 360);

  const read = await measured('workspace.read', {
    schemaVersion: 1,
    workspaceId,
    relativePath: 'Cargo.toml',
    mode: 'SLICE',
    offset: 0,
    maxBytes: 64
  }, 360);
  assert.ok(read.content.length > 0);

  await measured('workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'm5_wire.py',
      mode: 'WRITE',
      content: "print('M5_WIRE_OK', flush=True)\n",
      expectedDigest: null
    }]
  }, 460);

  const executed = await measured('workspace.exec', {
    schemaVersion: 1,
    execution: {
      schemaVersion: 1,
      taskId,
      workspaceId,
      executable: '/usr/bin/python3.14',
      args: ['m5_wire.py'],
      cwdRelative: '.',
      env: { PYTHONUNBUFFERED: '1' },
      timeoutMs: 10_000,
      stdoutLimitBytes: 65_536,
      stderrLimitBytes: 65_536
    },
    waitMs: 5000,
    stdoutTailBytes: 1024,
    stderrTailBytes: 1024
  }, 420);
  assert.equal(executed.status, 'COMPLETED');
  assert.match(executed.stdoutTail, /M5_WIRE_OK/);

  const observed = await measured('task.observe', {
    schemaVersion: 1,
    taskId,
    waitMs: 0,
    stdoutTailBytes: 1024,
    stderrTailBytes: 1024
  }, 420);
  assert.equal(observed.status, 'COMPLETED');

  const artifact = await measured('artifact.read', {
    schemaVersion: 1,
    taskId,
    artifactId: `${taskId}.stdout`,
    offset: 0,
    maxBytes: 1024
  }, 560);
  assert.match(artifact.content, /M5_WIRE_OK/);

  const diff = await measured('workspace.diff', {
    schemaVersion: 1,
    workspaceId,
    maxBytes: 4096
  }, 900);
  assert.ok(diff.untrackedPaths.includes('m5_wire.py'));

  const concurrentResults = await Promise.all(
    Array.from({ length: 64 }, () => callTool(connection, 'workspace.read', {
      schemaVersion: 1,
      workspaceId,
      relativePath: 'Cargo.toml',
      mode: 'SLICE',
      offset: 0,
      maxBytes: 32
    }))
  );
  for (const result of concurrentResults) {
    assertCompactSuccess(result, 320, 'workspace.read.concurrent');
  }
  await new Promise(resolve => setTimeout(resolve, 100));

  const coreRows = readFileSync(config.tracePath, 'utf8')
    .trim().split('\n').filter(Boolean).map(JSON.parse);
  const httpRows = readFileSync(config.httpTracePath, 'utf8')
    .trim().split('\n').filter(Boolean).map(JSON.parse);
  assert.equal(new Set(coreRows.map(row => row.traceId)).size, coreRows.length);
  assert.equal(new Set(httpRows.map(row => row.traceId)).size, httpRows.length);
  assert.equal(new Set(connection.traceIds).size, connection.traceIds.length);
  assert.ok(coreRows.length >= 71);
  assert.ok(httpRows.length >= 72);

  console.log(JSON.stringify({
    schemaVersion: 1,
    phase: 'ORDIVON-M5-WIRE-CONTRACT',
    workspaceId,
    taskId,
    observations,
    concurrentReads: concurrentResults.length,
    coreTraceRows: coreRows.length,
    httpTraceRows: httpRows.length,
    clientTraceIds: connection.traceIds.length,
    passed: true
  }, null, 2));
} finally {
  await closeConnection(connection);
}

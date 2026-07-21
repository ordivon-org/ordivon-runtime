#!/usr/bin/env node

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  CallToolResultSchema,
  assertCompactSuccess,
  byteLength,
  callTool,
  closeConnection,
  connectM6,
  m6Config,
  sha256,
  structured
} from './client.mjs';

const config = m6Config();
const suffix = `${process.pid}-${Date.now()}`;
const workspaceId = `m6-wire-workspace-${suffix}`;
const clientRequestId = `m6-wire-request-${suffix}`;
const connection = await connectM6('ordivon-m6-wire-contract');
const observations = [];

async function measured(name, args, budget) {
  const result = await callTool(connection, name, args);
  const bytes = assertCompactSuccess(result, budget, name);
  observations.push({ name, bytes });
  return structured(result);
}

function execution(script, waitMs = 10_000) {
  return {
    schemaVersion: 1,
    clientRequestId,
    principal: 'principal:m6-wire',
    authorityRef: 'authority:m6-local-dogfood',
    policyId: 'policy:m6-local-dogfood',
    policyVersion: '1',
    policyDigest: sha256('policy:m6-local-dogfood:1'),
    globalLimit: 4,
    execution: {
      workspaceId,
      executable: '/usr/bin/python3.14',
      args: [script],
      cwdRelative: '.',
      env: { PYTHONUNBUFFERED: '1' },
      timeoutMs: 30_000,
      stdoutLimitBytes: 65_536,
      stderrLimitBytes: 65_536
    },
    waitMs,
    stdoutTailBytes: 4096,
    stderrTailBytes: 4096
  };
}

try {
  const listed = await connection.client.listTools({}, undefined);
  const names = listed.tools.map(tool => tool.name).sort();
  assert.deepEqual(names, [
    'artifact.read',
    'task.cancel',
    'task.list',
    'task.observe',
    'workspace.diff',
    'workspace.exec',
    'workspace.mutate',
    'workspace.open',
    'workspace.read'
  ]);
  const execTool = listed.tools.find(tool => tool.name === 'workspace.exec');
  const execSchema = JSON.stringify(execTool.inputSchema);
  assert.match(execSchema, /clientRequestId/);
  assert.doesNotMatch(execSchema, /taskId/);

  const opened = await measured('workspace.open', {
    schemaVersion: 1,
    workspaceId,
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision
  }, 240);
  assert.equal(opened.workspaceId, workspaceId);

  const read = await measured('workspace.read', {
    schemaVersion: 1,
    workspaceId,
    relativePath: 'Cargo.toml',
    mode: 'SLICE',
    offset: 0,
    maxBytes: 64
  }, 380);
  assert.match(read.content, /\[workspace\]/);

  await measured('workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'm6_wire.py',
      mode: 'WRITE',
      content: "print('M6_WIRE_OK', flush=True)\n",
      expectedDigest: null
    }]
  }, 480);

  const first = await measured('workspace.exec', execution('m6_wire.py'), 640);
  assert.equal(first.status, 'succeeded');
  assert.match(first.stdoutTail, /M6_WIRE_OK/);
  assert.match(first.jobId, /^job-/);
  assert.match(first.attemptId, /^attempt-/);

  const replay = await measured('workspace.exec', execution('m6_wire.py'), 640);
  assert.equal(replay.jobId, first.jobId);
  assert.equal(replay.attemptId, first.attemptId);
  assert.equal(replay.status, 'succeeded');

  const page = await measured('task.list', { limit: 10 }, 1200);
  assert.ok(page.jobs.some(job => job.jobId === first.jobId));

  const artifact = await measured('artifact.read', {
    schemaVersion: 1,
    jobId: first.jobId,
    artifactId: `${first.attemptId}.stdout`,
    offset: 0,
    maxBytes: 1024
  }, 620);
  assert.match(artifact.content, /M6_WIRE_OK/);
  assert.equal(artifact.eof, true);

  const diff = await measured('workspace.diff', {
    schemaVersion: 1,
    workspaceId,
    maxBytes: 4096
  }, 920);
  assert.ok(diff.untrackedPaths.includes('m6_wire.py'));

  const concurrent = await Promise.all(
    Array.from({ length: 64 }, () => callTool(connection, 'workspace.read', {
      schemaVersion: 1,
      workspaceId,
      relativePath: 'Cargo.toml',
      mode: 'SLICE',
      offset: 0,
      maxBytes: 32
    }))
  );
  for (const result of concurrent) {
    assertCompactSuccess(result, 340, 'workspace.read.concurrent');
  }

  await measured('workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [
      {
        relativePath: 'm6_native.py',
        mode: 'WRITE',
        content: "import time\nprint('M6_NATIVE_START', flush=True)\ntime.sleep(1.0)\nprint('M6_NATIVE_DONE', flush=True)\n",
        expectedDigest: null
      },
      {
        relativePath: 'm6_cancel.py',
        mode: 'WRITE',
        content: "import time\nprint('M6_CANCEL_START', flush=True)\ntime.sleep(30)\n",
        expectedDigest: null
      }
    ]
  }, 560);

  const startNative = async (activeConnection, script, requestId) => {
    const request = execution(script, 0);
    request.clientRequestId = requestId;
    const stream = activeConnection.client.experimental.tasks.callToolStream(
      { name: 'workspace.exec', arguments: request },
      CallToolResultSchema,
      { task: { ttl: 60_000 } }
    );
    for await (const message of stream) {
      if (message.type === 'taskCreated') {
        return message.task.taskId;
      }
    }
    throw new Error('native task creation did not return taskCreated');
  };

  const nativeJobId = await startNative(
    connection,
    'm6_native.py',
    `m6-native-request-${suffix}`
  );
  assert.match(nativeJobId, /^job-/);
  await closeConnection(connection);
  await new Promise(resolve => setTimeout(resolve, 1300));

  const resumed = await connectM6('ordivon-m6-wire-resumed');
  try {
    const nativeTask = await resumed.client.experimental.tasks.getTask(nativeJobId);
    assert.equal(nativeTask.status, 'completed');
    const nativeResult = await resumed.client.experimental.tasks.getTaskResult(
      nativeJobId,
      CallToolResultSchema
    );
    assert.equal(nativeResult.isError, false);
    assert.match(nativeResult.structuredContent.stdoutTail, /M6_NATIVE_DONE/);

    const cancelJobId = await startNative(
      resumed,
      'm6_cancel.py',
      `m6-cancel-request-${suffix}`
    );
    await new Promise(resolve => setTimeout(resolve, 200));
    const cancelled = await resumed.client.experimental.tasks.cancelTask(cancelJobId);
    assert.equal(cancelled.status, 'cancelled');
    const cancelledResult = await resumed.client.experimental.tasks.getTaskResult(
      cancelJobId,
      CallToolResultSchema
    );
    assert.equal(cancelledResult.isError, true);
    assert.equal(cancelledResult.structuredContent.error.code, 'TASK_CANCELLED');

    await new Promise(resolve => setTimeout(resolve, 100));
    const coreRows = readFileSync(config.tracePath, 'utf8')
      .trim().split('\n').filter(Boolean).map(JSON.parse);
    const httpRows = readFileSync(config.httpTracePath, 'utf8')
      .trim().split('\n').filter(Boolean).map(JSON.parse);
    assert.equal(new Set(coreRows.map(row => row.traceId)).size, coreRows.length);
    assert.equal(new Set(httpRows.map(row => row.traceId)).size, httpRows.length);
    const clientTraceIds = [...connection.traceIds, ...resumed.traceIds];
    assert.equal(new Set(clientTraceIds).size, clientTraceIds.length);

    console.log(JSON.stringify({
      schemaVersion: 1,
      phase: 'ORDIVON-M6-WIRE-CONTRACT',
      workspaceId,
      replayJobId: first.jobId,
      nativeJobId,
      cancelJobId,
      observations,
      concurrentReads: concurrent.length,
      coreTraceRows: coreRows.length,
      httpTraceRows: httpRows.length,
      clientTraceIds: clientTraceIds.length,
      passed: true
    }, null, 2));
  } finally {
    await closeConnection(resumed);
  }
} finally {
  await closeConnection(connection);
}

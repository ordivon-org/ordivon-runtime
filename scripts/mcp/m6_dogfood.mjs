#!/usr/bin/env node

import assert from 'node:assert/strict';
import { dirname, resolve } from 'node:path';
import { mkdirSync, writeFileSync } from 'node:fs';
import {
  CallToolResultSchema,
  byteLength,
  callTool,
  closeConnection,
  connectM6,
  m6Config,
  requiredEnvironment,
  sha256,
  structured
} from './client.mjs';

const config = m6Config();
const cargoBinary = requiredEnvironment('ORDIVON_M6_CARGO_BINARY');
const rustcBinary = requiredEnvironment('ORDIVON_M6_RUSTC_BINARY');
const outputIndex = process.argv.indexOf('--output');
const outputPath = resolve(
  outputIndex >= 0 ? process.argv[outputIndex + 1] : '/tmp/ordivon-m6-dogfood.json'
);
const runSuffix = `${process.pid}-${Date.now()}`;
const policyDigest = sha256('policy:m6-limited-dogfood:1');

function metrics() {
  return { calls: 0, contextBytes: 0, outputBytes: 0 };
}

async function measured(connection, state, name, args) {
  const result = await callTool(connection, name, args);
  state.calls += 1;
  state.contextBytes += byteLength(result);
  return { result, value: structured(result) };
}

function execRequest(workspaceId, clientRequestId, executable, args, env = {}, waitMs = 30_000) {
  return {
    schemaVersion: 1,
    clientRequestId,
    principal: 'principal:m6-dogfood',
    authorityRef: 'authority:m6-local-dogfood',
    policyId: 'policy:m6-limited-dogfood',
    policyVersion: '1',
    policyDigest,
    globalLimit: 8,
    execution: {
      workspaceId,
      executable,
      args,
      cwdRelative: '.',
      env,
      timeoutMs: 120_000,
      stdoutLimitBytes: 1_048_576,
      stderrLimitBytes: 1_048_576
    },
    waitMs,
    stdoutTailBytes: 8192,
    stderrTailBytes: 8192
  };
}

async function openWorkspace(connection, state, label) {
  const workspaceId = `m6-${label}-${runSuffix}`;
  await measured(connection, state, 'workspace.open', {
    schemaVersion: 1,
    workspaceId,
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision
  });
  return workspaceId;
}

async function runJourney(name, operation) {
  const connection = await connectM6(`ordivon-m6-${name}`);
  const state = metrics();
  const started = process.hrtime.bigint();
  try {
    const result = await operation(connection, state);
    return {
      name,
      succeeded: true,
      elapsedMs: Number((process.hrtime.bigint() - started) / 1_000_000n),
      toolCalls: state.calls,
      contextBytes: state.contextBytes,
      outputBytes: state.outputBytes,
      fallbackCount: 0,
      ...result
    };
  } finally {
    await closeConnection(connection);
  }
}

async function startNative(connection, state, workspaceId, script, clientRequestId) {
  const stream = connection.client.experimental.tasks.callToolStream(
    {
      name: 'workspace.exec',
      arguments: execRequest(
        workspaceId,
        clientRequestId,
        '/usr/bin/python3.14',
        [script],
        { PYTHONUNBUFFERED: '1' },
        0
      )
    },
    CallToolResultSchema,
    { task: { ttl: 60_000 } }
  );
  for await (const message of stream) {
    if (message.type === 'taskCreated') {
      state.calls += 1;
      state.contextBytes += byteLength(message);
      return message.task.taskId;
    }
  }
  throw new Error('native task creation did not return taskCreated');
}

const journeys = [];

journeys.push(await runJourney('readonly-audit', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'readonly');
  const read = (await measured(connection, state, 'workspace.read', {
    schemaVersion: 1,
    workspaceId,
    relativePath: 'Cargo.toml',
    mode: 'SLICE',
    offset: 0,
    maxBytes: 512
  })).value;
  assert.match(read.content, /\[workspace\]/);
  const audit = (await measured(connection, state, 'workspace.exec', execRequest(
    workspaceId,
    `m6-readonly-${runSuffix}`,
    '/usr/bin/rg',
    ['-n', 'transactional-registry-m6', 'crates/ordivon-exec/Cargo.toml']
  ))).value;
  assert.equal(audit.status, 'succeeded');
  assert.match(audit.stdoutTail, /transactional-registry-m6/);
  state.outputBytes += byteLength(audit.stdoutTail) + byteLength(audit.stderrTail);
  const diff = (await measured(connection, state, 'workspace.diff', {
    schemaVersion: 1,
    workspaceId,
    maxBytes: 4096
  })).value;
  assert.equal(diff.diff, '');
  assert.deepEqual(diff.untrackedPaths, []);
  return {
    workspaceId,
    jobId: audit.jobId,
    semanticDigest: sha256(`${read.digest}:${audit.stdoutTail.trim()}`),
    repairRounds: 0
  };
}));

journeys.push(await runJourney('single-file-edit', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'single');
  const read = (await measured(connection, state, 'workspace.read', {
    schemaVersion: 1,
    workspaceId,
    relativePath: 'scripts/README.md',
    mode: 'FULL',
    offset: 0,
    maxBytes: 65_536
  })).value;
  const marker = `<!-- M6_SINGLE_${runSuffix} -->`;
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'scripts/README.md',
      mode: 'APPEND',
      content: `\n${marker}\n`,
      expectedDigest: read.digest
    }]
  });
  const check = (await measured(connection, state, 'workspace.exec', execRequest(
    workspaceId,
    `m6-single-${runSuffix}`,
    '/usr/bin/python3.14',
    ['-c', `from pathlib import Path; assert ${JSON.stringify(marker)} in Path('scripts/README.md').read_text()`]
  ))).value;
  assert.equal(check.status, 'succeeded');
  const diff = (await measured(connection, state, 'workspace.diff', {
    schemaVersion: 1,
    workspaceId,
    maxBytes: 8192
  })).value;
  assert.match(diff.diff, new RegExp(marker));
  return {
    workspaceId,
    jobId: check.jobId,
    semanticDigest: sha256(`${marker}:${diff.diff}`),
    repairRounds: 0
  };
}));

journeys.push(await runJourney('multi-file-test', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'multi');
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [
      {
        relativePath: 'm6calc.py',
        mode: 'WRITE',
        content: 'def multiply(left, right):\n    return left * right\n',
        expectedDigest: null
      },
      {
        relativePath: 'test_m6calc.py',
        mode: 'WRITE',
        content: "import unittest\nfrom m6calc import multiply\n\nclass TestCalc(unittest.TestCase):\n    def test_multiply(self):\n        self.assertEqual(multiply(6, 7), 42)\n\nif __name__ == '__main__':\n    unittest.main()\n",
        expectedDigest: null
      }
    ]
  });
  const test = (await measured(connection, state, 'workspace.exec', execRequest(
    workspaceId,
    `m6-multi-${runSuffix}`,
    '/usr/bin/python3.14',
    ['-m', 'unittest', '-v', 'test_m6calc.py'],
    { PYTHONDONTWRITEBYTECODE: '1' }
  ))).value;
  assert.equal(test.status, 'succeeded');
  assert.match(test.stderrTail, /OK/);
  state.outputBytes += byteLength(test.stdoutTail) + byteLength(test.stderrTail);
  const diff = (await measured(connection, state, 'workspace.diff', {
    schemaVersion: 1,
    workspaceId,
    maxBytes: 8192
  })).value;
  assert.ok(diff.untrackedPaths.includes('m6calc.py'));
  assert.ok(diff.untrackedPaths.includes('test_m6calc.py'));
  return {
    workspaceId,
    jobId: test.jobId,
    semanticDigest: sha256(`${test.status}:${diff.untrackedPaths.sort().join(',')}`),
    repairRounds: 0
  };
}));

journeys.push(await runJourney('failure-repair-loop', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'repair');
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [
      {
        relativePath: 'm6_bug.py',
        mode: 'WRITE',
        content: 'def add(left, right):\n    return left - right\n',
        expectedDigest: null
      },
      {
        relativePath: 'test_m6_bug.py',
        mode: 'WRITE',
        content: "import unittest\nfrom m6_bug import add\n\nclass TestBug(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2, 3), 5)\n\nif __name__ == '__main__':\n    unittest.main()\n",
        expectedDigest: null
      }
    ]
  });
  const failed = (await measured(connection, state, 'workspace.exec', execRequest(
    workspaceId,
    `m6-repair-fail-${runSuffix}`,
    '/usr/bin/python3.14',
    ['-m', 'unittest', '-v', 'test_m6_bug.py'],
    { PYTHONDONTWRITEBYTECODE: '1' }
  ))).value;
  assert.equal(failed.status, 'failed');
  assert.match(failed.stderrTail, /AssertionError/);
  state.outputBytes += byteLength(failed.stdoutTail) + byteLength(failed.stderrTail);

  const buggy = (await measured(connection, state, 'workspace.read', {
    schemaVersion: 1,
    workspaceId,
    relativePath: 'm6_bug.py',
    mode: 'FULL',
    offset: 0,
    maxBytes: 4096
  })).value;
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'm6_bug.py',
      mode: 'REPLACE_EXACT',
      expectedDigest: buggy.digest,
      expectedText: 'return left - right',
      content: 'return left + right'
    }]
  });
  const repaired = (await measured(connection, state, 'workspace.exec', execRequest(
    workspaceId,
    `m6-repair-pass-${runSuffix}`,
    '/usr/bin/python3.14',
    ['-m', 'unittest', '-v', 'test_m6_bug.py'],
    { PYTHONDONTWRITEBYTECODE: '1' }
  ))).value;
  assert.equal(repaired.status, 'succeeded');
  assert.match(repaired.stderrTail, /OK/);
  state.outputBytes += byteLength(repaired.stdoutTail) + byteLength(repaired.stderrTail);
  return {
    workspaceId,
    failedJobId: failed.jobId,
    repairedJobId: repaired.jobId,
    semanticDigest: sha256(`${failed.status}:${repaired.status}:1`),
    repairRounds: 1,
    failureObserved: true
  };
}));

journeys.push(await runJourney('rust-target-test', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'rust');
  const rust = (await measured(connection, state, 'workspace.exec', execRequest(
    workspaceId,
    `m6-rust-${runSuffix}`,
    cargoBinary,
    [
      'test',
      '-p',
      'ordivon-exec',
      '--features',
      'transactional-registry-m6',
      'simultaneous_same_key_creates_one_job_and_one_attempt'
    ],
    {
      PATH: `${dirname(cargoBinary)}:/usr/bin`,
      RUSTC: rustcBinary,
      CARGO_HOME: '/root/.cargo',
      CARGO_NET_OFFLINE: 'true',
      CARGO_TARGET_DIR: '.m6-target',
      HOME: '/tmp'
    }
  ))).value;
  assert.equal(rust.status, 'succeeded');
  assert.match(`${rust.stdoutTail}\n${rust.stderrTail}`, /test result: ok/);
  state.outputBytes += byteLength(rust.stdoutTail) + byteLength(rust.stderrTail);
  return {
    workspaceId,
    jobId: rust.jobId,
    semanticDigest: sha256(`${rust.status}:m6-idempotency-race-test`),
    repairRounds: 0,
    realRepositoryTest: true
  };
}));

journeys.push(await runJourney('bounded-log-artifact', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'log');
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'm6_log.py',
      mode: 'WRITE',
      content: "for i in range(20000):\n    print(f'M6_LOG_{i:05d}_' + 'x' * 32)\n",
      expectedDigest: null
    }]
  });
  const request = execRequest(
    workspaceId,
    `m6-log-${runSuffix}`,
    '/usr/bin/python3.14',
    ['m6_log.py'],
    { PYTHONUNBUFFERED: '1' }
  );
  request.execution.stdoutLimitBytes = 4096;
  request.stdoutTailBytes = 512;
  const log = (await measured(connection, state, 'workspace.exec', request)).value;
  assert.equal(log.status, 'succeeded');
  assert.equal(log.stdoutTruncated, true);
  assert.equal(log.artifactsAvailable, true);
  const artifact = (await measured(connection, state, 'artifact.read', {
    schemaVersion: 1,
    jobId: log.jobId,
    artifactId: `${log.attemptId}.stdout`,
    offset: 0,
    maxBytes: 1024
  })).value;
  assert.match(artifact.content, /M6_LOG_00000/);
  assert.equal(artifact.eof, false);
  state.outputBytes += byteLength(log.stdoutTail) + byteLength(log.stderrTail);
  return {
    workspaceId,
    jobId: log.jobId,
    semanticDigest: sha256(`${log.stdoutTruncated}:${artifact.digest}`),
    repairRounds: 0,
    truncatedOutput: true,
    artifactDigest: artifact.digest
  };
}));

journeys.push(await runJourney('durable-recovery-cancel', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'durable');
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [
      {
        relativePath: 'm6_recover.py',
        mode: 'WRITE',
        content: "import time\nprint('M6_RECOVER_START', flush=True)\ntime.sleep(1.2)\nprint('M6_RECOVER_DONE', flush=True)\n",
        expectedDigest: null
      },
      {
        relativePath: 'm6_cancel.py',
        mode: 'WRITE',
        content: "import time\nprint('M6_CANCEL_START', flush=True)\ntime.sleep(30)\n",
        expectedDigest: null
      }
    ]
  });
  const recoverJobId = await startNative(
    connection,
    state,
    workspaceId,
    'm6_recover.py',
    `m6-recover-${runSuffix}`
  );
  await closeConnection(connection);
  await new Promise(resolve => setTimeout(resolve, 1500));

  const resumed = await connectM6('ordivon-m6-durable-resumed');
  let cancelJobId;
  try {
    const task = await resumed.client.experimental.tasks.getTask(recoverJobId);
    state.calls += 1;
    state.contextBytes += byteLength(task);
    assert.equal(task.status, 'completed');
    const result = await resumed.client.experimental.tasks.getTaskResult(
      recoverJobId,
      CallToolResultSchema
    );
    state.calls += 1;
    state.contextBytes += byteLength(result);
    assert.equal(result.isError, false);
    assert.match(result.structuredContent.stdoutTail, /M6_RECOVER_DONE/);
    state.outputBytes += byteLength(result.structuredContent.stdoutTail);

    cancelJobId = await startNative(
      resumed,
      state,
      workspaceId,
      'm6_cancel.py',
      `m6-cancel-${runSuffix}`
    );
    await new Promise(resolve => setTimeout(resolve, 200));
    const cancelled = await resumed.client.experimental.tasks.cancelTask(cancelJobId);
    state.calls += 1;
    state.contextBytes += byteLength(cancelled);
    assert.equal(cancelled.status, 'cancelled');
    const cancelledResult = await resumed.client.experimental.tasks.getTaskResult(
      cancelJobId,
      CallToolResultSchema
    );
    state.calls += 1;
    state.contextBytes += byteLength(cancelledResult);
    assert.equal(cancelledResult.isError, true);
    assert.equal(cancelledResult.structuredContent.error.code, 'TASK_CANCELLED');
  } finally {
    await closeConnection(resumed);
  }
  return {
    workspaceId,
    recoverJobId,
    cancelJobId,
    semanticDigest: sha256(`${recoverJobId}:succeeded:${cancelJobId}:cancelled`),
    repairRounds: 0,
    recoveredAfterDisconnect: true,
    cancellationClean: true
  };
}));

journeys.push(await runJourney('idempotency-and-list', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'idempotency');
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'm6_idempotent.py',
      mode: 'WRITE',
      content: "print('M6_IDEMPOTENT_OK', flush=True)\n",
      expectedDigest: null
    }]
  });
  const requestId = `m6-idempotency-${runSuffix}`;
  const request = execRequest(
    workspaceId,
    requestId,
    '/usr/bin/python3.14',
    ['m6_idempotent.py']
  );
  const first = (await measured(connection, state, 'workspace.exec', request)).value;
  const replay = (await measured(connection, state, 'workspace.exec', request)).value;
  assert.equal(first.jobId, replay.jobId);
  assert.equal(first.attemptId, replay.attemptId);
  assert.match(first.stdoutTail, /M6_IDEMPOTENT_OK/);

  const page = (await measured(connection, state, 'task.list', { limit: 100 })).value;
  assert.equal(page.jobs.filter(job => job.jobId === first.jobId).length, 1);

  const conflict = structuredClone(request);
  conflict.execution.timeoutMs += 1;
  let conflictCode;
  try {
    await callTool(connection, 'workspace.exec', conflict);
    assert.fail('changed idempotent request unexpectedly executed');
  } catch (error) {
    state.calls += 1;
    state.contextBytes += byteLength(error.toolResult);
    conflictCode = error.toolResult?.structuredContent?.error?.code;
    assert.equal(conflictCode, 'IDEMPOTENCY_CONFLICT');
  }
  return {
    workspaceId,
    jobId: first.jobId,
    semanticDigest: sha256(`${first.jobId}:${first.attemptId}:one-job`),
    repairRounds: 0,
    idempotentReplay: true,
    idempotencyConflictCode: conflictCode,
    listIdentityUnique: true
  };
}));

const summary = {
  journeyCount: journeys.length,
  succeeded: journeys.every(journey => journey.succeeded),
  totalToolCalls: journeys.reduce((sum, journey) => sum + journey.toolCalls, 0),
  totalContextBytes: journeys.reduce((sum, journey) => sum + journey.contextBytes, 0),
  totalOutputBytes: journeys.reduce((sum, journey) => sum + journey.outputBytes, 0),
  totalRepairRounds: journeys.reduce((sum, journey) => sum + journey.repairRounds, 0),
  fallbackCount: journeys.reduce((sum, journey) => sum + journey.fallbackCount, 0),
  recoveredAfterDisconnect: journeys.some(journey => journey.recoveredAfterDisconnect === true),
  cancellationClean: journeys.some(journey => journey.cancellationClean === true),
  idempotentReplay: journeys.some(journey => journey.idempotentReplay === true)
};
assert.equal(summary.journeyCount, 8);
assert.equal(summary.succeeded, true);
assert.equal(summary.totalRepairRounds, 1);
assert.equal(summary.fallbackCount, 0);
assert.equal(summary.recoveredAfterDisconnect, true);
assert.equal(summary.cancellationClean, true);
assert.equal(summary.idempotentReplay, true);

const evidence = {
  schemaVersion: 1,
  phase: 'ORDIVON-MIGRATION-M6-LIMITED-DOGFOOD-2026-07-22',
  generatedAt: new Date().toISOString(),
  sourceRevision: config.sourceRevision,
  routePolicy: {
    primaryBackend: 'ORDIVON_M6_TRANSACTIONAL_MCP',
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision,
    jobIdentity: 'server-generated UUIDv7',
    idempotencyScope: '(principal, clientRequestId)',
    automaticLegacyFallback: false,
    allowedCapabilities: [
      'isolated workspace read',
      'isolated workspace mutation',
      'sandboxed transactional Job execution',
      'Job observation, listing, recovery, and cancellation',
      'bounded digest-verified Artifact read'
    ],
    forbiddenCapabilities: [
      'git push',
      'merge',
      'production worktree mutation',
      'system service modification',
      'Cloudflare modification',
      'credential access',
      'network delegation',
      'deployment'
    ]
  },
  journeys,
  summary,
  claimsNotMade: [
    'The scripted Dogfood harness is not an autonomous model-planning evaluation.',
    'Eight local journeys do not establish production reliability or broad workload coverage.',
    'The result does not authorize production routing, remote exposure, credentials, or external side effects.',
    'M6 does not yet provide a dedicated non-root Worker.'
  ]
};
mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify({ outputPath, summary }, null, 2));

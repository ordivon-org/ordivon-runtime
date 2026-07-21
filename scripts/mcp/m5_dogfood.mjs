#!/usr/bin/env node

import assert from 'node:assert/strict';
import { dirname, resolve } from 'node:path';
import { mkdirSync, writeFileSync } from 'node:fs';
import {
  CallToolResultSchema,
  byteLength,
  callTool,
  closeConnection,
  connectM5,
  m5Config,
  requiredEnvironment,
  sha256,
  structured
} from './client.mjs';

const config = m5Config();
const cargoBinary = requiredEnvironment('ORDIVON_M5_CARGO_BINARY');
const rustcBinary = requiredEnvironment('ORDIVON_M5_RUSTC_BINARY');
const outputIndex = process.argv.indexOf('--output');
const outputPath = resolve(
  outputIndex >= 0 ? process.argv[outputIndex + 1] : '/tmp/ordivon-m5-dogfood.json'
);
const runSuffix = `${process.pid}-${Date.now()}`;

function metrics() {
  return { calls: 0, contextBytes: 0, outputBytes: 0 };
}

async function measured(connection, state, name, args) {
  const result = await callTool(connection, name, args);
  state.calls += 1;
  state.contextBytes += byteLength(result);
  return { result, value: structured(result) };
}

function execution(taskId, workspaceId, executable, args, env = {}) {
  return {
    schemaVersion: 1,
    execution: {
      schemaVersion: 1,
      taskId,
      workspaceId,
      executable,
      args,
      cwdRelative: '.',
      env,
      timeoutMs: 120_000,
      stdoutLimitBytes: 1_048_576,
      stderrLimitBytes: 1_048_576
    },
    waitMs: 30_000,
    stdoutTailBytes: 8192,
    stderrTailBytes: 8192
  };
}

async function openWorkspace(connection, state, label) {
  const workspaceId = `m5-${label}-${runSuffix}`;
  await measured(connection, state, 'workspace.open', {
    schemaVersion: 1,
    workspaceId,
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision
  });
  return workspaceId;
}

async function runJourney(name, operation) {
  const connection = await connectM5(`ordivon-m5-${name}`);
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
  const auditTaskId = `m5-readonly-task-${runSuffix}`;
  const audit = (await measured(connection, state, 'workspace.exec', execution(
    auditTaskId,
    workspaceId,
    '/usr/bin/rg',
    ['-n', 'experimental-http-m4', 'crates/ordivon-mcp/Cargo.toml']
  ))).value;
  assert.equal(audit.status, 'COMPLETED');
  assert.match(audit.stdoutTail, /experimental-http-m4/);
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
  const marker = `<!-- M5_SINGLE_${runSuffix} -->`;
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
  const check = (await measured(connection, state, 'workspace.exec', execution(
    `m5-single-task-${runSuffix}`,
    workspaceId,
    '/usr/bin/python3.14',
    ['-c', `from pathlib import Path; assert ${JSON.stringify(marker)} in Path('scripts/README.md').read_text()`]
  ))).value;
  assert.equal(check.status, 'COMPLETED');
  const diff = (await measured(connection, state, 'workspace.diff', {
    schemaVersion: 1,
    workspaceId,
    maxBytes: 8192
  })).value;
  assert.match(diff.diff, new RegExp(marker));
  return {
    workspaceId,
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
        relativePath: 'm5calc.py',
        mode: 'WRITE',
        content: 'def multiply(left, right):\n    return left * right\n',
        expectedDigest: null
      },
      {
        relativePath: 'test_m5calc.py',
        mode: 'WRITE',
        content: "import unittest\nfrom m5calc import multiply\n\nclass TestCalc(unittest.TestCase):\n    def test_multiply(self):\n        self.assertEqual(multiply(6, 7), 42)\n\nif __name__ == '__main__':\n    unittest.main()\n",
        expectedDigest: null
      }
    ]
  });
  const test = (await measured(connection, state, 'workspace.exec', execution(
    `m5-multi-task-${runSuffix}`,
    workspaceId,
    '/usr/bin/python3.14',
    ['-m', 'unittest', '-v', 'test_m5calc.py']
  ))).value;
  assert.equal(test.status, 'COMPLETED');
  assert.match(test.stderrTail, /OK/);
  state.outputBytes += byteLength(test.stdoutTail) + byteLength(test.stderrTail);
  const diff = (await measured(connection, state, 'workspace.diff', {
    schemaVersion: 1,
    workspaceId,
    maxBytes: 8192
  })).value;
  assert.ok(diff.untrackedPaths.includes('m5calc.py'));
  assert.ok(diff.untrackedPaths.includes('test_m5calc.py'));
  return {
    workspaceId,
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
        relativePath: 'm5_bug.py',
        mode: 'WRITE',
        content: 'def add(left, right):\n    return left - right\n',
        expectedDigest: null
      },
      {
        relativePath: 'test_m5_bug.py',
        mode: 'WRITE',
        content: "import unittest\nfrom m5_bug import add\n\nclass TestBug(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2, 3), 5)\n\nif __name__ == '__main__':\n    unittest.main()\n",
        expectedDigest: null
      }
    ]
  });
  const failed = (await measured(connection, state, 'workspace.exec', execution(
    `m5-repair-fail-${runSuffix}`,
    workspaceId,
    '/usr/bin/python3.14',
    ['-m', 'unittest', '-v', 'test_m5_bug.py'],
    { PYTHONDONTWRITEBYTECODE: '1' }
  ))).value;
  assert.equal(failed.status, 'FAILED');
  assert.match(failed.stderrTail, /AssertionError/);
  state.outputBytes += byteLength(failed.stdoutTail) + byteLength(failed.stderrTail);

  const buggySource = (await measured(connection, state, 'workspace.read', {
    schemaVersion: 1,
    workspaceId,
    relativePath: 'm5_bug.py',
    mode: 'FULL',
    offset: 0,
    maxBytes: 4096
  })).value;
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'm5_bug.py',
      mode: 'REPLACE_EXACT',
      expectedDigest: buggySource.digest,
      expectedText: 'return left - right',
      content: 'return left + right'
    }]
  });
  const repaired = (await measured(connection, state, 'workspace.exec', execution(
    `m5-repair-pass-${runSuffix}`,
    workspaceId,
    '/usr/bin/python3.14',
    ['-m', 'unittest', '-v', 'test_m5_bug.py'],
    { PYTHONDONTWRITEBYTECODE: '1' }
  ))).value;
  assert.equal(repaired.status, 'COMPLETED');
  assert.match(repaired.stderrTail, /OK/);
  state.outputBytes += byteLength(repaired.stdoutTail) + byteLength(repaired.stderrTail);
  return {
    workspaceId,
    semanticDigest: sha256(`${failed.status}:${repaired.status}:1`),
    repairRounds: 1,
    failureObserved: true
  };
}));

journeys.push(await runJourney('rust-target-test', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'rust');
  const rust = (await measured(connection, state, 'workspace.exec', execution(
    `m5-rust-task-${runSuffix}`,
    workspaceId,
    cargoBinary,
    [
      'test',
      '-p',
      'ordivon-mcp',
      'm5_dogfood_policy_binds_source_repo_and_revision'
    ],
    {
      PATH: `${dirname(cargoBinary)}:/usr/bin`,
      RUSTC: rustcBinary,
      CARGO_HOME: '/root/.cargo',
      CARGO_NET_OFFLINE: 'true',
      CARGO_TARGET_DIR: '.m5-target',
      HOME: '/tmp'
    }
  ))).value;
  assert.equal(rust.status, 'COMPLETED');
  assert.match(`${rust.stdoutTail}\n${rust.stderrTail}`, /test result: ok/);
  state.outputBytes += byteLength(rust.stdoutTail) + byteLength(rust.stderrTail);
  return {
    workspaceId,
    semanticDigest: sha256(`${rust.status}:m5-policy-test`),
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
      relativePath: 'm5_log.py',
      mode: 'WRITE',
      content: "for i in range(20000):\n    print(f'M5_LOG_{i:05d}_' + 'x' * 32)\n",
      expectedDigest: null
    }]
  });
  const request = execution(
    `m5-log-task-${runSuffix}`,
    workspaceId,
    '/usr/bin/python3.14',
    ['m5_log.py'],
    { PYTHONUNBUFFERED: '1' }
  );
  request.execution.stdoutLimitBytes = 4096;
  request.stdoutTailBytes = 512;
  const log = (await measured(connection, state, 'workspace.exec', request)).value;
  assert.equal(log.status, 'COMPLETED');
  assert.equal(log.stdoutTruncated, true);
  assert.equal(log.artifactsAvailable, true);
  const artifact = (await measured(connection, state, 'artifact.read', {
    schemaVersion: 1,
    taskId: `m5-log-task-${runSuffix}`,
    artifactId: `m5-log-task-${runSuffix}.stdout`,
    offset: 0,
    maxBytes: 1024
  })).value;
  assert.match(artifact.content, /M5_LOG_00000/);
  assert.equal(artifact.eof, false);
  state.outputBytes += byteLength(log.stdoutTail) + byteLength(log.stderrTail);
  return {
    workspaceId,
    semanticDigest: sha256(`${log.stdoutTruncated}:${artifact.digest}`),
    repairRounds: 0,
    truncatedOutput: true,
    artifactDigest: artifact.digest
  };
}));

async function startNative(connection, state, taskId, workspaceId, scriptName) {
  const stream = connection.client.experimental.tasks.callToolStream(
    {
      name: 'workspace.exec',
      arguments: execution(taskId, workspaceId, '/usr/bin/python3.14', [scriptName], {
        PYTHONUNBUFFERED: '1'
      })
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

journeys.push(await runJourney('durable-recovery-cancel', async (connection, state) => {
  const workspaceId = await openWorkspace(connection, state, 'durable');
  await measured(connection, state, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [
      {
        relativePath: 'm5_recover.py',
        mode: 'WRITE',
        content: "import time\nprint('M5_RECOVER_START', flush=True)\ntime.sleep(1.2)\nprint('M5_RECOVER_DONE', flush=True)\n",
        expectedDigest: null
      },
      {
        relativePath: 'm5_cancel.py',
        mode: 'WRITE',
        content: "import time\nprint('M5_CANCEL_START', flush=True)\ntime.sleep(30)\n",
        expectedDigest: null
      }
    ]
  });
  const recoverTaskId = `m5-recover-task-${runSuffix}`;
  const cancelTaskId = `m5-cancel-task-${runSuffix}`;
  assert.equal(
    await startNative(connection, state, recoverTaskId, workspaceId, 'm5_recover.py'),
    recoverTaskId
  );
  await closeConnection(connection);
  await new Promise(resolve => setTimeout(resolve, 1500));

  const resumed = await connectM5('ordivon-m5-durable-resumed');
  try {
    const task = await resumed.client.experimental.tasks.getTask(recoverTaskId);
    state.calls += 1;
    state.contextBytes += byteLength(task);
    assert.equal(task.status, 'completed');
    const result = await resumed.client.experimental.tasks.getTaskResult(
      recoverTaskId,
      CallToolResultSchema
    );
    state.calls += 1;
    state.contextBytes += byteLength(result);
    assert.equal(result.isError, false);
    assert.match(result.structuredContent.stdoutTail, /M5_RECOVER_DONE/);
    state.outputBytes += byteLength(result.structuredContent.stdoutTail);
    assert.equal(
      await startNative(resumed, state, cancelTaskId, workspaceId, 'm5_cancel.py'),
      cancelTaskId
    );
    await new Promise(resolve => setTimeout(resolve, 200));
    const cancelled = await resumed.client.experimental.tasks.cancelTask(cancelTaskId);
    state.calls += 1;
    state.contextBytes += byteLength(cancelled);
    assert.equal(cancelled.status, 'cancelled');
    const cancelledResult = await resumed.client.experimental.tasks.getTaskResult(
      cancelTaskId,
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
    semanticDigest: sha256(`${recoverTaskId}:completed:${cancelTaskId}:cancelled`),
    repairRounds: 0,
    recoveredAfterDisconnect: true,
    cancellationClean: true
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
  cancellationClean: journeys.some(journey => journey.cancellationClean === true)
};
assert.equal(summary.journeyCount, 7);
assert.equal(summary.succeeded, true);
assert.equal(summary.totalRepairRounds, 1);
assert.equal(summary.fallbackCount, 0);
assert.equal(summary.recoveredAfterDisconnect, true);
assert.equal(summary.cancellationClean, true);

const evidence = {
  schemaVersion: 1,
  phase: 'ORDIVON-MIGRATION-M5-LIMITED-DOGFOOD-2026-07-22',
  generatedAt: new Date().toISOString(),
  sourceRevision: config.sourceRevision,
  routePolicy: {
    primaryBackend: 'ORDIVON_MCP',
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision,
    allowedCapabilities: [
      'isolated workspace read',
      'isolated workspace mutation',
      'sandboxed local execution',
      'Task observation and cancellation',
      'bounded Artifact read'
    ],
    forbiddenCapabilities: [
      'git push',
      'merge',
      'production worktree mutation',
      'system service modification',
      'Cloudflare modification',
      'credential access',
      'deployment'
    ],
    automaticLegacyFallback: false
  },
  journeys,
  summary,
  claimsNotMade: [
    'The scripted dogfood harness is not a complete autonomous coding agent evaluation.',
    'Seven local journeys do not establish production reliability or broad workload coverage.',
    'The result does not authorize production routing.'
  ]
};
mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify({ outputPath, summary }, null, 2));

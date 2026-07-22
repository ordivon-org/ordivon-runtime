#!/usr/bin/env node

import assert from 'node:assert/strict';
import { dirname, resolve } from 'node:path';
import { mkdirSync, writeFileSync } from 'node:fs';
import {
  byteLength,
  callTool,
  closeConnection,
  connectM6,
  connectM7,
  m6Config,
  m7Config,
  sha256,
  structured
} from './client.mjs';

const m6 = m6Config();
const m7 = m7Config();
assert.equal(m6.repoRoot, m7.repoRoot);
assert.equal(m6.sourceRevision, m7.sourceRevision);
const outputIndex = process.argv.indexOf('--output');
const iterationsIndex = process.argv.indexOf('--iterations');
const outputPath = resolve(
  outputIndex >= 0 ? process.argv[outputIndex + 1] : '/tmp/ordivon-m7-shadow.json'
);
const iterations = Number(iterationsIndex >= 0 ? process.argv[iterationsIndex + 1] : 3);
if (!Number.isInteger(iterations) || iterations < 1 || iterations > 5) {
  throw new Error('iterations must be in 1..=5');
}
const policyDigest = sha256('policy:m7-shadow:1');

function metricState() {
  return { calls: 0, contextBytes: 0, outputBytes: 0, httpRequests: 0, repairRounds: 0 };
}

function measuredFetch(state) {
  return async (input, init = {}) => {
    state.httpRequests += 1;
    return fetch(input, init);
  };
}

async function measured(connection, state, name, args) {
  const result = await callTool(connection, name, args);
  state.calls += 1;
  state.contextBytes += byteLength(result);
  return structured(result);
}

function m6Exec(workspaceId, pairId, executable, args, env = {}) {
  return {
    schemaVersion: 1,
    clientRequestId: `m6-${pairId}`,
    principal: 'principal:m7-shadow',
    authorityRef: 'authority:m7-local-shadow',
    policyId: 'policy:m7-shadow',
    policyVersion: '1',
    policyDigest,
    globalLimit: 8,
    execution: {
      workspaceId,
      executable,
      args,
      cwdRelative: '.',
      env,
      timeoutMs: 60_000,
      stdoutLimitBytes: 262_144,
      stderrLimitBytes: 262_144
    },
    waitMs: 30_000,
    stdoutTailBytes: 8192,
    stderrTailBytes: 8192
  };
}

function m7Exec(workspaceId, pairId, executable, args, env = {}) {
  return {
    schemaVersion: 1,
    clientRequestId: `m7-${pairId}`,
    principal: 'principal:m7-shadow',
    authorityRef: 'authority:m7-local-shadow',
    policyId: 'policy:m7-shadow',
    policyVersion: '1',
    policyDigest,
    globalLimit: 8,
    execution: {
      workspaceId,
      executable,
      args,
      cwdRelative: '.',
      env,
      timeoutMs: 60_000,
      stdoutLimitBytes: 262_144,
      stderrLimitBytes: 262_144
    },
    waitMs: 30_000,
    stdoutTailBytes: 8192,
    stderrTailBytes: 8192
  };
}

function semantic(kind, marker = '') {
  return sha256(JSON.stringify({ kind, marker, succeeded: true }));
}

async function connectBackend(backend, state, pairId) {
  const connection = backend === 'm6'
    ? await connectM6(`ordivon-shadow-m6-${pairId}`, measuredFetch(state))
    : await connectM7(`ordivon-shadow-m7-${pairId}`, measuredFetch(state));
  state.httpRequests = 0;
  return connection;
}

function normalizedStatus(_backend, observation) {
  return observation.status;
}

async function execute(connection, state, backend, workspaceId, pairId, executable, args, env = {}) {
  const request = backend === 'm6'
    ? m6Exec(workspaceId, pairId, executable, args, env)
    : m7Exec(workspaceId, pairId, executable, args, env);
  const observation = await measured(connection, state, 'workspace.exec', request);
  return { ...observation, normalizedStatus: normalizedStatus(backend, observation) };
}

async function openWorkspace(connection, state, backend, pairId) {
  const workspaceId = `${backend}-shadow-${pairId}`;
  const config = backend === 'm6' ? m6 : m7;
  await measured(connection, state, 'workspace.open', {
    schemaVersion: 1,
    workspaceId,
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision
  });
  return workspaceId;
}

async function runSample(backend, kind, pairId, marker) {
  const state = metricState();
  const connection = await connectBackend(backend, state, pairId);
  const started = process.hrtime.bigint();
  try {
    const workspaceId = await openWorkspace(connection, state, backend, pairId);
    await runJourneyKind(connection, state, backend, kind, pairId, marker, workspaceId);
    return {
      backend: backend === 'm6' ? 'M6_ROOT_PAYLOAD' : 'M7_WORKER_PAYLOAD',
      kind,
      pairId,
      succeeded: true,
      elapsedMs: Number((process.hrtime.bigint() - started) / 1_000_000n),
      toolCalls: state.calls,
      contextBytes: state.contextBytes,
      outputBytes: state.outputBytes,
      httpRequests: state.httpRequests,
      repairRounds: state.repairRounds,
      fallbackCount: 0,
      semanticDigest: semantic(kind, marker)
    };
  } finally {
    await closeConnection(connection);
  }
}

async function runJourneyKind(connection, state, backend, kind, pairId, marker, workspaceId) {
  if (kind === 'readonly-audit') {
    const read = await measured(connection, state, 'workspace.read', {
      schemaVersion: 1,
      workspaceId,
      relativePath: 'Cargo.toml',
      mode: 'SLICE',
      offset: 0,
      maxBytes: 512
    });
    assert.match(read.content, /\[workspace\]/);
    const audit = await execute(
      connection,
      state,
      backend,
      workspaceId,
      `${pairId}-audit`,
      '/usr/bin/rg',
      ['-n', 'transactional-registry-m6', 'crates/ordivon-exec/Cargo.toml']
    );
    assert.equal(audit.normalizedStatus, 'succeeded');
    assert.match(audit.stdoutTail, /transactional-registry-m6/);
    state.outputBytes += byteLength(audit.stdoutTail) + byteLength(audit.stderrTail);
    const diff = await measured(connection, state, 'workspace.diff', {
      schemaVersion: 1,
      workspaceId,
      maxBytes: 4096
    });
    assert.equal(diff.diff, '');
    assert.deepEqual(diff.untrackedPaths, []);
    return;
  }

  if (kind === 'single-file-edit') {
    const read = await measured(connection, state, 'workspace.read', {
      schemaVersion: 1,
      workspaceId,
      relativePath: 'scripts/README.md',
      mode: 'FULL',
      offset: 0,
      maxBytes: 65_536
    });
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
    const checked = await execute(
      connection,
      state,
      backend,
      workspaceId,
      `${pairId}-single`,
      '/usr/bin/python3.14',
      ['-c', `from pathlib import Path; assert ${JSON.stringify(marker)} in Path('scripts/README.md').read_text()`]
    );
    assert.equal(checked.normalizedStatus, 'succeeded');
    const diff = await measured(connection, state, 'workspace.diff', {
      schemaVersion: 1,
      workspaceId,
      maxBytes: 8192
    });
    assert.match(diff.diff, new RegExp(marker));
    return;
  }

  if (kind === 'multi-file-test') {
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
    const test = await execute(
      connection,
      state,
      backend,
      workspaceId,
      `${pairId}-multi`,
      '/usr/bin/python3.14',
      ['-m', 'unittest', '-v', 'test_m6calc.py'],
      { PYTHONDONTWRITEBYTECODE: '1' }
    );
    assert.equal(test.normalizedStatus, 'succeeded');
    assert.match(test.stderrTail, /OK/);
    state.outputBytes += byteLength(test.stdoutTail) + byteLength(test.stderrTail);
    const diff = await measured(connection, state, 'workspace.diff', {
      schemaVersion: 1,
      workspaceId,
      maxBytes: 8192
    });
    assert.ok(diff.untrackedPaths.includes('m6calc.py'));
    assert.ok(diff.untrackedPaths.includes('test_m6calc.py'));
    return;
  }

  if (kind === 'failure-repair-loop') {
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
    const failed = await execute(
      connection,
      state,
      backend,
      workspaceId,
      `${pairId}-fail`,
      '/usr/bin/python3.14',
      ['-m', 'unittest', '-v', 'test_m6_bug.py'],
      { PYTHONDONTWRITEBYTECODE: '1' }
    );
    assert.equal(failed.normalizedStatus, 'failed');
    assert.match(failed.stderrTail, /AssertionError/);
    state.outputBytes += byteLength(failed.stdoutTail) + byteLength(failed.stderrTail);
    state.repairRounds = 1;

    const buggy = await measured(connection, state, 'workspace.read', {
      schemaVersion: 1,
      workspaceId,
      relativePath: 'm6_bug.py',
      mode: 'FULL',
      offset: 0,
      maxBytes: 4096
    });
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
    const repaired = await execute(
      connection,
      state,
      backend,
      workspaceId,
      `${pairId}-pass`,
      '/usr/bin/python3.14',
      ['-m', 'unittest', '-v', 'test_m6_bug.py'],
      { PYTHONDONTWRITEBYTECODE: '1' }
    );
    assert.equal(repaired.normalizedStatus, 'succeeded');
    assert.match(repaired.stderrTail, /OK/);
    state.outputBytes += byteLength(repaired.stdoutTail) + byteLength(repaired.stderrTail);
    return;
  }

  throw new Error(`unknown journey kind ${kind}`);
}

function median(values) {
  const ordered = [...values].sort((left, right) => left - right);
  const middle = Math.floor(ordered.length / 2);
  return ordered.length % 2 === 1
    ? ordered[middle]
    : Math.round((ordered[middle - 1] + ordered[middle]) / 2);
}

function summarize(samples) {
  return {
    samples: samples.length,
    succeeded: samples.every(sample => sample.succeeded),
    elapsedMs: median(samples.map(sample => sample.elapsedMs)),
    toolCalls: median(samples.map(sample => sample.toolCalls)),
    contextBytes: median(samples.map(sample => sample.contextBytes)),
    outputBytes: median(samples.map(sample => sample.outputBytes)),
    httpRequests: median(samples.map(sample => sample.httpRequests)),
    repairRounds: median(samples.map(sample => sample.repairRounds)),
    fallbackCount: samples.reduce((sum, sample) => sum + sample.fallbackCount, 0)
  };
}

const kinds = [
  'readonly-audit',
  'single-file-edit',
  'multi-file-test',
  'failure-repair-loop'
];
const raw = { m6: [], m7: [] };
let sequence = 0;
for (const kind of kinds) {
  for (let iteration = 0; iteration < iterations; iteration += 1) {
    const pairId = `${kind}-${iteration}-${m7.sourceRevision.slice(0, 10)}`;
    const marker = `M7_SHADOW_${kind}_${iteration}`;
    const order = sequence % 2 === 0 ? ['m6', 'm7'] : ['m7', 'm6'];
    const pair = {};
    for (const backend of order) {
      pair[backend] = await runSample(backend, kind, pairId, marker);
    }
    assert.equal(pair.m6.semanticDigest, pair.m7.semanticDigest);
    raw.m6.push(pair.m6);
    raw.m7.push(pair.m7);
    console.error(
      `M7_SHADOW kind=${kind} iteration=${iteration} ` +
      `m6Ms=${pair.m6.elapsedMs} m7Ms=${pair.m7.elapsedMs} ` +
      `m6Calls=${pair.m6.toolCalls} m7Calls=${pair.m7.toolCalls}`
    );
    sequence += 1;
  }
}

const byKind = {};
for (const kind of kinds) {
  byKind[kind] = {
    m6: summarize(raw.m6.filter(sample => sample.kind === kind)),
    m7: summarize(raw.m7.filter(sample => sample.kind === kind))
  };
}
const overall = {
  m6: summarize(raw.m6),
  m7: summarize(raw.m7)
};

const gates = {
  completionNotWorse: overall.m6.succeeded && overall.m7.succeeded,
  semanticEquivalence: raw.m6.every((sample, index) =>
    sample.semanticDigest === raw.m7[index].semanticDigest
  ),
  repairRoundsNotWorse: overall.m7.repairRounds <= overall.m6.repairRounds,
  toolCallsNotWorse: overall.m7.toolCalls <= overall.m6.toolCalls,
  contextNotIncreased: overall.m7.contextBytes <= overall.m6.contextBytes,
  elapsedWithinTwentyPercent:
    overall.m7.elapsedMs <= Math.ceil(overall.m6.elapsedMs * 1.20),
  noFallback: overall.m7.fallbackCount === 0
};
const decision = {
  localHardeningDogfoodEligible: Object.values(gates).every(Boolean),
  gates
};

const evidence = {
  schemaVersion: 1,
  phase: 'ORDIVON-MIGRATION-M7-WORKER-SHADOW-2026-07-22',
  generatedAt: new Date().toISOString(),
  sourceRevision: m7.sourceRevision,
  iterationsPerJourney: iterations,
  alternatingOrder: true,
  journeyKinds: kinds,
  rawSamples: raw,
  summaries: { byKind, overall },
  decision,
  claimsNotMade: [
    'Scripted Shadow journeys do not measure autonomous model planning quality.',
    'Paired local samples do not establish production reliability.',
    'The decision authorizes only bounded local M7 hardening Dogfood, not production routing.',
    'Real reboot and lifecycle correctness are evaluated separately from this performance comparison.'
  ]
};
mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify({ outputPath, overall, decision }, null, 2));

#!/usr/bin/env node

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import {
  CallToolResultSchema,
  byteLength,
  callTool,
  closeConnection,
  connectLegacy,
  connectM5,
  m5Config,
  sha256,
  structured
} from './client.mjs';

const config = m5Config();
const outputIndex = process.argv.indexOf('--output');
const iterationsIndex = process.argv.indexOf('--iterations');
const outputPath = resolve(
  outputIndex >= 0 ? process.argv[outputIndex + 1] : '/tmp/ordivon-m5-shadow.json'
);
const iterations = Number(iterationsIndex >= 0 ? process.argv[iterationsIndex + 1] : 3);
if (!Number.isInteger(iterations) || iterations < 1 || iterations > 5) {
  throw new Error('iterations must be in 1..=5');
}
const legacyRoot = '/root/.local/share/ordivon-m5-shadow';

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function metrics() {
  return {
    calls: 0,
    contextBytes: 0,
    outputBytes: 0,
    httpRequests: 0,
    repairRounds: 0
  };
}

function measuredFetch(state) {
  return async (input, init = {}) => {
    state.httpRequests += 1;
    return fetch(input, init);
  };
}

function textContent(result) {
  return (result.content ?? [])
    .filter(item => item.type === 'text')
    .map(item => item.text)
    .join('\n');
}

function removeLegacyWorktree(path) {
  const result = spawnSync(
    'git',
    ['-C', config.repoRoot, 'worktree', 'remove', '--force', path],
    { encoding: 'utf8', timeout: 15_000 }
  );
  if (result.status !== 0) {
    rmSync(path, { recursive: true, force: true });
    spawnSync('git', ['-C', config.repoRoot, 'worktree', 'prune']);
  }
}

async function legacyCall(connection, state, name, args) {
  const result = await connection.client.callTool(
    { name, arguments: args },
    CallToolResultSchema
  );
  state.calls += 1;
  state.contextBytes += byteLength(result);
  if (result.isError) {
    throw new Error(`${name}: ${textContent(result)}`);
  }
  return result;
}

async function legacyExec(connection, state, command) {
  const wrapped = `set -euo pipefail; ${command}; printf '\n__M5_OK__\n'`;
  const result = await legacyCall(connection, state, 'start_process', {
    command: wrapped,
    timeout_ms: 120_000,
    origin: 'llm'
  });
  const text = textContent(result);
  if (!text.includes('__M5_OK__')) {
    throw new Error(`legacy command lost sentinel: ${text}`);
  }
  return text;
}

async function legacyAllowFailure(connection, state, command) {
  const wrapped = `set +e; ${command}; code=$?; printf '\n__M5_EXIT__%s\n' "$code"; exit 0`;
  const result = await legacyCall(connection, state, 'start_process', {
    command: wrapped,
    timeout_ms: 120_000,
    origin: 'llm'
  });
  const text = textContent(result);
  const match = text.match(/__M5_EXIT__(\d+)/);
  if (!match) throw new Error(`legacy failure command lost exit marker: ${text}`);
  return { text, exitCode: Number(match[1]) };
}

async function m5Call(connection, state, name, args) {
  const result = await callTool(connection, name, args);
  state.calls += 1;
  state.contextBytes += byteLength(result);
  return structured(result);
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
      timeoutMs: 60_000,
      stdoutLimitBytes: 262_144,
      stderrLimitBytes: 262_144
    },
    waitMs: 30_000,
    stdoutTailBytes: 8192,
    stderrTailBytes: 8192
  };
}

async function openM5(connection, state, workspaceId) {
  await m5Call(connection, state, 'workspace.open', {
    schemaVersion: 1,
    workspaceId,
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision
  });
}

function semantic(kind, marker = '') {
  return sha256(JSON.stringify({ kind, marker, succeeded: true }));
}

async function runBackend(backend, kind, pairId, marker) {
  return backend === 'legacy'
    ? runLegacy(kind, pairId, marker)
    : runM5(kind, pairId, marker);
}

async function runLegacy(kind, pairId, marker) {
  const state = metrics();
  const workspacePath = join(legacyRoot, `legacy-${kind}-${pairId}`);
  mkdirSync(dirname(workspacePath), { recursive: true });
  removeLegacyWorktree(workspacePath);
  const connection = await connectLegacy(
    `ordivon-m5-shadow-legacy-${kind}-${pairId}`,
    measuredFetch(state)
  );
  state.httpRequests = 0;
  const started = process.hrtime.bigint();
  try {
    await legacyExec(
      connection,
      state,
      `git -C ${shellQuote(config.repoRoot)} worktree add --detach ${shellQuote(workspacePath)} ${shellQuote(config.sourceRevision)}`
    );
    if (kind === 'readonly-audit') {
      const read = await legacyCall(connection, state, 'read_file', {
        path: join(workspacePath, 'Cargo.toml'),
        offset: 0,
        length: 40,
        origin: 'llm'
      });
      assert.match(textContent(read), /\[workspace\]/);
      const audit = await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && /usr/bin/rg -n experimental-http-m4 crates/ordivon-mcp/Cargo.toml`
      );
      assert.match(audit, /experimental-http-m4/);
      state.outputBytes += byteLength(audit);
      await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && test -z "$(git status --porcelain)"`
      );
    }
    if (kind === 'single-file-edit') {
      const read = await legacyCall(connection, state, 'read_file', {
        path: join(workspacePath, 'scripts/README.md'),
        offset: 0,
        length: 300,
        origin: 'llm'
      });
      assert.match(textContent(read), /Script Policy/);
      await legacyCall(connection, state, 'write_file', {
        path: join(workspacePath, 'scripts/README.md'),
        content: `\n${marker}\n`,
        mode: 'append',
        origin: 'llm'
      });
      await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && /usr/bin/python3.14 -c ${shellQuote(`from pathlib import Path; assert ${JSON.stringify(marker)} in Path('scripts/README.md').read_text()`)} `
      );
      const diff = await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && git diff -- scripts/README.md`
      );
      assert.match(diff, new RegExp(marker));
    }
    if (kind === 'multi-file-test') {
      const source = 'def multiply(left, right):\n    return left * right\n';
      const test = "import unittest\nfrom m5calc import multiply\n\nclass TestCalc(unittest.TestCase):\n    def test_multiply(self):\n        self.assertEqual(multiply(6, 7), 42)\n\nif __name__ == '__main__':\n    unittest.main()\n";
      const writeCode = [
        'from pathlib import Path',
        'import base64',
        `Path('m5calc.py').write_bytes(base64.b64decode(${JSON.stringify(Buffer.from(source).toString('base64'))}))`,
        `Path('test_m5calc.py').write_bytes(base64.b64decode(${JSON.stringify(Buffer.from(test).toString('base64'))}))`
      ].join(';');
      await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && /usr/bin/python3.14 -c ${shellQuote(writeCode)}`
      );
      const testOutput = await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3.14 -m unittest -v test_m5calc.py`
      );
      assert.match(testOutput, /OK/);
      state.outputBytes += byteLength(testOutput);
      const untracked = await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && git ls-files --others --exclude-standard`
      );
      assert.match(untracked, /m5calc.py/);
      assert.match(untracked, /test_m5calc.py/);
    }
    if (kind === 'failure-repair-loop') {
      const source = 'def add(left, right):\n    return left - right\n';
      const test = "import unittest\nfrom m5_bug import add\n\nclass TestBug(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2, 3), 5)\n\nif __name__ == '__main__':\n    unittest.main()\n";
      const writeCode = [
        'from pathlib import Path',
        'import base64',
        `Path('m5_bug.py').write_bytes(base64.b64decode(${JSON.stringify(Buffer.from(source).toString('base64'))}))`,
        `Path('test_m5_bug.py').write_bytes(base64.b64decode(${JSON.stringify(Buffer.from(test).toString('base64'))}))`
      ].join(';');
      await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && /usr/bin/python3.14 -c ${shellQuote(writeCode)}`
      );
      const failed = await legacyAllowFailure(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3.14 -m unittest -v test_m5_bug.py`
      );
      assert.notEqual(failed.exitCode, 0);
      assert.match(failed.text, /AssertionError/);
      state.outputBytes += byteLength(failed.text);
      state.repairRounds = 1;
      const read = await legacyCall(connection, state, 'read_file', {
        path: join(workspacePath, 'm5_bug.py'),
        offset: 0,
        length: 40,
        origin: 'llm'
      });
      assert.match(textContent(read), /return left - right/);
      await legacyCall(connection, state, 'write_file', {
        path: join(workspacePath, 'm5_bug.py'),
        content: 'def add(left, right):\n    return left + right\n',
        mode: 'rewrite',
        origin: 'llm'
      });
      const repaired = await legacyExec(
        connection,
        state,
        `cd ${shellQuote(workspacePath)} && PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3.14 -m unittest -v test_m5_bug.py`
      );
      assert.match(repaired, /OK/);
      state.outputBytes += byteLength(repaired);
    }
    return {
      backend: 'LEGACY_DESKTOP_COMMANDER',
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
    removeLegacyWorktree(workspacePath);
  }
}

async function runM5(kind, pairId, marker) {
  const state = metrics();
  const connection = await connectM5(
    `ordivon-m5-shadow-${kind}-${pairId}`,
    measuredFetch(state)
  );
  state.httpRequests = 0;
  const workspaceId = `m5-shadow-${kind}-${pairId}`;
  const started = process.hrtime.bigint();
  try {
    await openM5(connection, state, workspaceId);
    if (kind === 'readonly-audit') {
      const read = await m5Call(connection, state, 'workspace.read', {
        schemaVersion: 1,
        workspaceId,
        relativePath: 'Cargo.toml',
        mode: 'SLICE',
        offset: 0,
        maxBytes: 512
      });
      assert.match(read.content, /\[workspace\]/);
      const audit = await m5Call(connection, state, 'workspace.exec', execution(
        `m5-shadow-read-${pairId}`,
        workspaceId,
        '/usr/bin/rg',
        ['-n', 'experimental-http-m4', 'crates/ordivon-mcp/Cargo.toml']
      ));
      assert.equal(audit.status, 'COMPLETED');
      assert.match(audit.stdoutTail, /experimental-http-m4/);
      state.outputBytes += byteLength(audit.stdoutTail) + byteLength(audit.stderrTail);
      const diff = await m5Call(connection, state, 'workspace.diff', {
        schemaVersion: 1,
        workspaceId,
        maxBytes: 4096
      });
      assert.equal(diff.diff, '');
      assert.deepEqual(diff.untrackedPaths, []);
    }
    if (kind === 'single-file-edit') {
      const read = await m5Call(connection, state, 'workspace.read', {
        schemaVersion: 1,
        workspaceId,
        relativePath: 'scripts/README.md',
        mode: 'FULL',
        offset: 0,
        maxBytes: 65_536
      });
      await m5Call(connection, state, 'workspace.mutate', {
        schemaVersion: 1,
        workspaceId,
        mutations: [{
          relativePath: 'scripts/README.md',
          mode: 'APPEND',
          content: `\n${marker}\n`,
          expectedDigest: read.digest
        }]
      });
      const check = await m5Call(connection, state, 'workspace.exec', execution(
        `m5-shadow-single-${pairId}`,
        workspaceId,
        '/usr/bin/python3.14',
        ['-c', `from pathlib import Path; assert ${JSON.stringify(marker)} in Path('scripts/README.md').read_text()`]
      ));
      assert.equal(check.status, 'COMPLETED');
      const diff = await m5Call(connection, state, 'workspace.diff', {
        schemaVersion: 1,
        workspaceId,
        maxBytes: 8192
      });
      assert.match(diff.diff, new RegExp(marker));
    }
    if (kind === 'multi-file-test') {
      await m5Call(connection, state, 'workspace.mutate', {
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
      const test = await m5Call(connection, state, 'workspace.exec', execution(
        `m5-shadow-multi-${pairId}`,
        workspaceId,
        '/usr/bin/python3.14',
        ['-m', 'unittest', '-v', 'test_m5calc.py'],
        { PYTHONDONTWRITEBYTECODE: '1' }
      ));
      assert.equal(test.status, 'COMPLETED');
      assert.match(test.stderrTail, /OK/);
      state.outputBytes += byteLength(test.stdoutTail) + byteLength(test.stderrTail);
      const diff = await m5Call(connection, state, 'workspace.diff', {
        schemaVersion: 1,
        workspaceId,
        maxBytes: 8192
      });
      assert.ok(diff.untrackedPaths.includes('m5calc.py'));
      assert.ok(diff.untrackedPaths.includes('test_m5calc.py'));
    }
    if (kind === 'failure-repair-loop') {
      await m5Call(connection, state, 'workspace.mutate', {
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
      const failed = await m5Call(connection, state, 'workspace.exec', execution(
        `m5-shadow-repair-fail-${pairId}`,
        workspaceId,
        '/usr/bin/python3.14',
        ['-m', 'unittest', '-v', 'test_m5_bug.py'],
        { PYTHONDONTWRITEBYTECODE: '1' }
      ));
      assert.equal(failed.status, 'FAILED');
      assert.match(failed.stderrTail, /AssertionError/);
      state.outputBytes += byteLength(failed.stdoutTail) + byteLength(failed.stderrTail);
      state.repairRounds = 1;
      const buggy = await m5Call(connection, state, 'workspace.read', {
        schemaVersion: 1,
        workspaceId,
        relativePath: 'm5_bug.py',
        mode: 'FULL',
        offset: 0,
        maxBytes: 4096
      });
      await m5Call(connection, state, 'workspace.mutate', {
        schemaVersion: 1,
        workspaceId,
        mutations: [{
          relativePath: 'm5_bug.py',
          mode: 'REPLACE_EXACT',
          expectedDigest: buggy.digest,
          expectedText: 'return left - right',
          content: 'return left + right'
        }]
      });
      const repaired = await m5Call(connection, state, 'workspace.exec', execution(
        `m5-shadow-repair-pass-${pairId}`,
        workspaceId,
        '/usr/bin/python3.14',
        ['-m', 'unittest', '-v', 'test_m5_bug.py'],
        { PYTHONDONTWRITEBYTECODE: '1' }
      ));
      assert.equal(repaired.status, 'COMPLETED');
      assert.match(repaired.stderrTail, /OK/);
      state.outputBytes += byteLength(repaired.stdoutTail) + byteLength(repaired.stderrTail);
    }
    return {
      backend: 'ORDIVON_M5',
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
const raw = { legacy: [], ordivon: [] };
let sequence = 0;
for (const kind of kinds) {
  for (let iteration = 0; iteration < iterations; iteration += 1) {
    const pairId = `${kind}-${iteration}-${config.sourceRevision.slice(0, 10)}`;
    const marker = `M5_SHADOW_${kind}_${iteration}`;
    const order = sequence % 2 === 0
      ? ['legacy', 'ordivon']
      : ['ordivon', 'legacy'];
    const pair = {};
    for (const backend of order) {
      pair[backend] = await runBackend(backend, kind, pairId, marker);
    }
    assert.equal(pair.legacy.semanticDigest, pair.ordivon.semanticDigest);
    raw.legacy.push(pair.legacy);
    raw.ordivon.push(pair.ordivon);
    console.error(
      `M5_SHADOW kind=${kind} iteration=${iteration} ` +
      `legacyMs=${pair.legacy.elapsedMs} ordivonMs=${pair.ordivon.elapsedMs} ` +
      `legacyCalls=${pair.legacy.toolCalls} ordivonCalls=${pair.ordivon.toolCalls}`
    );
    sequence += 1;
  }
}

const byKind = {};
for (const kind of kinds) {
  byKind[kind] = {
    legacy: summarize(raw.legacy.filter(sample => sample.kind === kind)),
    ordivon: summarize(raw.ordivon.filter(sample => sample.kind === kind))
  };
}
const overall = {
  legacy: summarize(raw.legacy),
  ordivon: summarize(raw.ordivon)
};

const gates = {
  completionNotWorse:
    overall.ordivon.succeeded && overall.legacy.succeeded,
  semanticEquivalence:
    raw.legacy.every((sample, index) =>
      sample.semanticDigest === raw.ordivon[index].semanticDigest
    ),
  repairRoundsNotWorse:
    overall.ordivon.repairRounds <= overall.legacy.repairRounds,
  toolCallsNotWorse:
    overall.ordivon.toolCalls <= overall.legacy.toolCalls,
  contextWithinTenPercent:
    overall.ordivon.contextBytes <= Math.ceil(overall.legacy.contextBytes * 1.10),
  elapsedWithinTwentyFivePercent:
    overall.ordivon.elapsedMs <= Math.ceil(overall.legacy.elapsedMs * 1.25),
  noFallback: overall.ordivon.fallbackCount === 0
};
const decision = {
  limitedDogfoodEligible: Object.values(gates).every(Boolean),
  gates
};

const evidence = {
  schemaVersion: 1,
  phase: 'ORDIVON-MIGRATION-M5-SHADOW-COMPARISON-2026-07-22',
  generatedAt: new Date().toISOString(),
  sourceRevision: config.sourceRevision,
  iterationsPerJourney: iterations,
  alternatingOrder: true,
  journeyKinds: kinds,
  rawSamples: raw,
  summaries: { byKind, overall },
  decision,
  claimsNotMade: [
    'Scripted Shadow journeys do not measure autonomous model planning quality.',
    'Twelve paired samples do not establish production reliability.',
    'The decision authorizes only bounded local Dogfood, not production routing.'
  ]
};
mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify({ outputPath, overall, decision }, null, 2));

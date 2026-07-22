#!/usr/bin/env node

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { mkdirSync, writeFileSync } from 'node:fs';
import {
  byteLength,
  callTool,
  closeConnection,
  connectM6,
  m6Config,
  sha256,
  structured
} from './client.mjs';

const config = m6Config();
const outputIndex = process.argv.indexOf('--output');
const outputPath = resolve(
  outputIndex >= 0 ? process.argv[outputIndex + 1] : '/tmp/ordivon-m6-concurrency.json'
);
const suffix = `${process.pid}-${Date.now()}`;
const policyDigest = sha256('policy:m6-concurrency:1');

function percentile(values, fraction) {
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.min(ordered.length - 1, Math.ceil(ordered.length * fraction) - 1)];
}

function registryCounts() {
  const code = `
import json,sqlite3,sys
c=sqlite3.connect(sys.argv[1])
print(json.dumps({
  "jobs": c.execute("select count(*) from jobs").fetchone()[0],
  "attempts": c.execute("select count(*) from attempts").fetchone()[0],
  "active": c.execute("select count(*) from concurrency_reservations where state in ('active','held_orphaned')").fetchone()[0],
  "released": c.execute("select count(*) from concurrency_reservations where state='released'").fetchone()[0]
}))
`;
  return JSON.parse(execFileSync('python3', ['-c', code, config.registryDb], { encoding: 'utf8' }));
}

function runRequest(workspaceId, level, index, waitMs = 0) {
  return {
    schemaVersion: 1,
    clientRequestId: `m6-concurrency-${level}-${index}-${suffix}`,
    principal: 'principal:m6-concurrency',
    authorityRef: 'authority:m6-local-concurrency',
    policyId: 'policy:m6-concurrency',
    policyVersion: '1',
    policyDigest,
    globalLimit: level,
    execution: {
      workspaceId,
      executable: '/usr/bin/python3.14',
      args: ['m6_concurrency.py', String(level), String(index)],
      cwdRelative: '.',
      env: { PYTHONUNBUFFERED: '1' },
      timeoutMs: 20_000,
      stdoutLimitBytes: 65_536,
      stderrLimitBytes: 65_536
    },
    waitMs,
    stdoutTailBytes: 1024,
    stderrTailBytes: 1024
  };
}

const connection = await connectM6('ordivon-m6-concurrency');
const workspaceId = `m6-concurrency-workspace-${suffix}`;
const results = [];
let totalContextBytes = 0;
let totalToolCalls = 0;

try {
  await callTool(connection, 'workspace.open', {
    schemaVersion: 1,
    workspaceId,
    sourceRepo: config.repoRoot,
    sourceRevision: config.sourceRevision
  });
  totalToolCalls += 1;
  await callTool(connection, 'workspace.mutate', {
    schemaVersion: 1,
    workspaceId,
    mutations: [{
      relativePath: 'm6_concurrency.py',
      mode: 'WRITE',
      content: "import sys,time\nlevel,index=sys.argv[1:]\nprint(f'M6_CONCURRENCY_START_{level}_{index}', flush=True)\ntime.sleep(3)\nprint(f'M6_CONCURRENCY_DONE_{level}_{index}', flush=True)\n",
      expectedDigest: null
    }]
  });
  totalToolCalls += 1;

  for (const level of [2, 4, 8]) {
    const admissionStarted = process.hrtime.bigint();
    const admissions = await Promise.all(
      Array.from({ length: level }, async (_, index) => {
        const started = process.hrtime.bigint();
        const result = await callTool(
          connection,
          'workspace.exec',
          runRequest(workspaceId, level, index)
        );
        totalToolCalls += 1;
        totalContextBytes += byteLength(result);
        const observation = structured(result);
        assert.equal(observation.status, 'working');
        return {
          index,
          jobId: observation.jobId,
          attemptId: observation.attemptId,
          admissionMs: Number((process.hrtime.bigint() - started) / 1_000_000n)
        };
      })
    );
    const admissionWallMs = Number(
      (process.hrtime.bigint() - admissionStarted) / 1_000_000n
    );
    assert.equal(new Set(admissions.map(item => item.jobId)).size, level);
    const activeAfterAdmission = registryCounts();
    assert.equal(activeAfterAdmission.active, level);

    let overflowCode;
    try {
      await callTool(
        connection,
        'workspace.exec',
        runRequest(workspaceId, level, 'overflow')
      );
      assert.fail('overflow Job unexpectedly acquired capacity');
    } catch (error) {
      totalToolCalls += 1;
      totalContextBytes += byteLength(error.toolResult);
      overflowCode = error.toolResult?.structuredContent?.error?.code;
      assert.equal(overflowCode, 'CONCURRENCY_LIMIT');
    }

    const completionStarted = process.hrtime.bigint();
    const completions = await Promise.all(
      admissions.map(async admission => {
        const result = await callTool(connection, 'task.observe', {
          schemaVersion: 1,
          jobId: admission.jobId,
          waitMs: 10_000,
          stdoutTailBytes: 1024,
          stderrTailBytes: 1024
        });
        totalToolCalls += 1;
        totalContextBytes += byteLength(result);
        const observation = structured(result);
        assert.equal(observation.status, 'succeeded');
        assert.match(
          observation.stdoutTail,
          new RegExp(`M6_CONCURRENCY_DONE_${level}_${admission.index}`)
        );
        return observation;
      })
    );
    const completionWallMs = Number(
      (process.hrtime.bigint() - completionStarted) / 1_000_000n
    );
    assert.equal(completions.length, level);
    const countsAfterCompletion = registryCounts();
    assert.equal(countsAfterCompletion.active, 0);

    results.push({
      level,
      jobs: admissions.length,
      uniqueJobs: new Set(admissions.map(item => item.jobId)).size,
      admissionWallMs,
      admissionP50Ms: percentile(admissions.map(item => item.admissionMs), 0.50),
      admissionP95Ms: percentile(admissions.map(item => item.admissionMs), 0.95),
      completionWallMs,
      overflowCode,
      activeAfterAdmission: activeAfterAdmission.active,
      activeAfterCompletion: countsAfterCompletion.active,
      releasedAfterCompletion: countsAfterCompletion.released,
      semanticDigest: sha256(
        completions.map(item => `${item.jobId}:${item.status}`).sort().join('|')
      )
    });
  }

  const evidence = {
    schemaVersion: 1,
    phase: 'ORDIVON-MIGRATION-M6-CONCURRENCY-2026-07-22',
    generatedAt: new Date().toISOString(),
    sourceRevision: config.sourceRevision,
    levels: results,
    totals: {
      toolCalls: totalToolCalls,
      contextBytes: totalContextBytes,
      finalRegistry: registryCounts()
    },
    gates: {
      allJobsCompleted: results.every(result => result.activeAfterCompletion === 0),
      uniqueJobIdentity: results.every(result => result.uniqueJobs === result.jobs),
      lastSlotProtected: results.every(result => result.overflowCode === 'CONCURRENCY_LIMIT'),
      noResidualCapacity: registryCounts().active === 0
    },
    claimsNotMade: [
      'The 2/4/8 local matrix does not establish production throughput or multi-host scheduling.',
      'The test uses one WSL instance and one SQLite database.',
      'The result does not authorize production routing.'
    ]
  };
  assert.ok(Object.values(evidence.gates).every(Boolean));
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(JSON.stringify({ outputPath, gates: evidence.gates, results }, null, 2));
} finally {
  await closeConnection(connection);
}

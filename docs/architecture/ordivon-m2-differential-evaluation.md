# Ordivon M2 Differential Evaluation

Date: 2026-07-21
Phase: `ORDIVON-MIGRATION-M2-2026-07-21`
Branch: `agent/m2-differential-evaluation`
Base: `59e59254316e7cdcb52e8c824f8c7955f8808e8f`

## Status

M2 is a local differential evaluation, not a route switch.

It compares the current Legacy Desktop Commander MCP route with the
feature-gated Ordivon M1 Universal Executor on one identical task journey.
Both backends operate on detached Git worktrees created from the exact M1
commit. Neither backend touches the production repository worktree.

The measured conclusion is:

> Ordivon is faster and recoverable, but currently requires more model-facing
> calls and returns more context. The journey is not eligible for default
> routing yet.

No production MCP tool, Bridge setting, Cloudflare route, service, or port was
changed by M2.

## Compared journey

Each measured pair performs the same semantic work:

1. Create a detached Git worktree from the exact source revision.
2. Read `crates/ordivon-exec/README.md`.
3. Append one pair-specific marker.
4. Write one model-authored Python tool.
5. Execute that tool and consume its stdout and stderr.
6. Read the generated `m2-output.txt` file.
7. Inspect the tracked diff and untracked output paths.
8. Verify one backend-neutral semantic digest.
9. Remove the temporary worktree and runtime state.

The Python program, marker, expected output, stdout, stderr, and untracked path
set are identical inside each pair. A pair fails if the two semantic digests
differ.

Cancellation is not included in this benchmark. M1 already proved cancellation;
M2 isolates normal completion performance and tests disconnect recovery in a
separate probe.

## Measurement method

The harness executes one warm-up pair and five measured pairs. Backend order
alternates by pair to reduce cache and ordering bias. Reported values are
medians of the five measured samples.

Legacy is measured through the active Streamable HTTP MCP endpoint at
`127.0.0.1:8811/mcp`, using the same TypeScript MCP SDK installed beside the
running Desktop Commander package. It calls the real `start_process`,
`read_file`, and `write_file` tools.

Ordivon is measured through separate invocations of the feature-gated M1 CLI.
Each invocation is a new process. Task completion is recovered through the
filesystem Task registry and systemd-owned Runner.

The benchmark reports two distinct surfaces:

- task-level logical calls, context bytes, output bytes, and wall time;
- supplementary Legacy HTTP request counts and measurable request bytes.

Ordivon M1 has no MCP adapter, so M2 does not fabricate Ordivon HTTP metrics.
Legacy SSE response bytes are marked unmeasured rather than guessed.

## Median results

| Metric | Legacy | Ordivon M1 | Ordivon delta |
|---|---:|---:|---:|
| Successful | yes | yes | equivalent |
| Wall time | 973 ms | 723 ms | -250 ms |
| Logical calls | 7 | 10 | +3 |
| Context bytes | 6,441 | 9,355 | +2,914 |
| Consumed output bytes | 45 | 47 | +2 |
| Recover after caller disconnect | no | yes | improved |
| Fallbacks | 0 | 0 | equal |

Ordivon completed the journey in approximately 74.3% of the Legacy time, a
median wall-time reduction of approximately 25.7%.

That speed improvement is not sufficient for route eligibility. Ordivon used
42.9% more logical calls and returned 45.2% more context bytes. The M0 gates
require at least 30% fewer calls and 50% fewer context bytes.

All five measured pairs succeeded and produced matching pairwise semantic
digests. The result is stable across alternating backend order.

## Disconnect result

The Legacy recovery probe started a two-second process, terminated its MCP
Session, connected through a new Session, and attempted to read the original
PID output.

The new Session reported:

```text
No session found for PID <pid>
```

Therefore M2 records `recoveredAfterDisconnect=false` for Legacy. This claim is
about the model-visible process handle and output state. It does not claim that
the operating-system process must immediately stop.

Ordivon records recovery as true because `task-start` and `task-get` are
separate CLI processes and the latter recovered the durable Task result after
the initiating process had exited.

This is the main architectural benefit already established by M1 and preserved
under differential measurement.

## Agent-facing hotspots

The dominant Ordivon response costs are:

| Sequence | Operation | Median response bytes |
|---:|---|---:|
| 2 | `workspace-read` for README | 4,999 |
| 6 | terminal `task-get` | 1,292 |
| 10 | `workspace-diff` | 767 |
| 7–8 | two `artifact-read` calls | 661 combined |
| 1, 3–5, 9 | remaining lifecycle calls | 1,636 combined |

The initial full-file read is approximately comparable to Legacy. The material
regression is the separate Task and Artifact observation sequence plus extra
workspace lifecycle metadata.

M3 should not optimize systemd startup first. M2 shows that Runner latency is
already competitive. M3 must reduce the number and size of observations the
model consumes.

## Cutover decision

The evaluated journey is **not eligible** for default Ordivon routing.

Passed gates:

- task completion is not worse;
- wall time is within the allowed threshold;
- disconnect recovery is present;
- no Legacy fallback was used;
- pairwise semantic equivalence holds.

Failed gates:

- logical calls were not reduced by 30%;
- context bytes were not reduced by 50%.

The existing Legacy route remains available and unchanged. No traffic or tool
surface is switched by this result.

## M3 performance budget

M3 should target the same benchmark journey with these bounded changes:

1. Add one `task-await` observation that can return bounded stdout/stderr tails
   with the terminal result.
2. Keep full logs as Artifacts, but avoid two mandatory Artifact reads for small
   successful output.
3. Add compact response projections that omit repeated Task and Artifact fields
   unless diagnostic detail is requested.
4. Consider a batch workspace mutation request for related small writes, while
   preserving per-file digests and failure identity.
5. Do not weaken the M1 sandbox or durability boundary to gain benchmark speed.

The immediate target is at most six model-facing calls and at most 3,220
context bytes for this journey. Those values correspond to the M0 minimum gates
relative to the current Legacy median.

Wall time must remain below 1,070 ms, and disconnect recovery must remain true.

## Reproduction

Build M1 binaries first:

```bash
cargo build -p ordivon-exec --features universal-executor-m1 \
  --bin ordivon-m1-cli --bin ordivon-task-runner
```

Run the benchmark from the repository root:

```bash
node scripts/benchmarks/m2_differential.mjs \
  --iterations 5 \
  --warmups 1 \
  --output docs/audits/runtime/ordivon-m2-differential-evidence-2026-07-21.json
```

Validate the evidence independently:

```bash
python3 scripts/benchmarks/check_m2_evidence.py \
  docs/audits/runtime/ordivon-m2-differential-evidence-2026-07-21.json
```

## Limitations

- One local Python journey cannot represent all coding workloads.
- M1 is a local CLI, not an MCP adapter; only logical calls are transport-normalized.
- Legacy SSE response bytes are not measured, so no complete wire-byte comparison
  is claimed.
- Debug M1 binaries were used; release-mode performance remains unmeasured.
- The benchmark fixes source content to the exact M1 commit and does not measure
  dirty or moving source trees.
- It measures model-visible recovery, not every operating-system process outcome.
- The filesystem Task registry remains experimental and non-transactional.

The governed machine-readable evidence is:

`docs/audits/runtime/ordivon-m2-differential-evidence-2026-07-21.json`

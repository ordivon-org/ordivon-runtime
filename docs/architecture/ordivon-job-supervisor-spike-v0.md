# Ordivon Job Supervisor Spike v0

Status: locally verified technical spike
Authority: supporting architecture evidence
Production status: not implemented or deployed

## 1. Question

P1 froze the durable Job contract but did not prove that the operating system
could own a Job independently of an MCP session or the process that requested
it. P2 tests that missing substrate directly.

```text
Can systemd and cgroup v2 hold, identify, recover, and terminate one local Job
process tree after the launcher and remote MCP call have ended?
```

This spike does not implement `job_start`, persistence, policy enforcement,
MCP tools, or a production service.

## 2. Tested substrate

```text
systemd 261
PID 1 = systemd
cgroup v2 = cgroup2fs
user = root
```
The transient service uses:

```text
Type=exec
KillMode=control-group
TimeoutStopSec=2s
SendSIGKILL=yes
StandardOutput=journal
StandardError=journal
```

The dedicated Rust fixture is available only under the explicit
`supervisor-spike` Cargo feature. It is not part of the normal release target.

## 3. Adversarial fixture

The fixture creates:

```text
root
└── child       calls setsid()
    └── grandchild
```

All three processes ignore `SIGTERM`. Each writes PID, PPID, process group,
session ID, `/proc` starttime ticks, cgroup path, and signal behavior.
This is stronger than testing a cooperative `sleep` process.
## 4. Running identity

A recoverable running Job identity is frozen as:

```text
kernel boot ID
systemd unit name
systemd InvocationID
ControlGroup
MainPID
/proc/<pid>/stat starttime
```

A PID alone is insufficient. PID plus starttime is sufficient only within one
kernel boot. Boot ID prevents a WSL restart and later PID reuse from being
misclassified as the original Job.

The real cross-call test started a unit, allowed the launcher shell to exit,
and queried it from a later remote tool call. The later call observed the same
InvocationID, cgroup, MainPID, and process start identity, with all three
fixture processes still running.

## 5. Stop semantics

`systemctl stop` sent `SIGTERM`, waited approximately two seconds, and then
sent `SIGKILL` to all three resistant processes. The cgroup was removed and no
recorded PID survived.
A critical semantic result is:

```text
explicit stop request
+ TimeoutStopSec escalation
→ systemd Result=timeout, ExecMainCode=2, ExecMainStatus=9
```

That does not mean the Job deadline expired. Ordivon must persist the initiating
intent:

```text
STOP_REQUESTED       → cancelled
DEADLINE_EXCEEDED    → timed_out
natural nonzero exit → failed
natural zero exit    → succeeded
```

Systemd exit evidence alone cannot distinguish the first two cases.

## 6. Terminal recovery

The spike observed asymmetric transient-unit retention:

- a zero-exit unit was quickly garbage-collected and later returned
  `LoadState=not-found`;
- a unit exiting with status 7 remained `loaded/failed` with
  `Result=exit-code` and `ExecMainStatus=7` until reset.
Therefore systemd is the process-lifecycle owner, not the durable Job database.
A production `ordivon-job-runner` must atomically write a terminal `result.json`
before it exits. Recovery uses the runner result as primary terminal evidence;
systemd terminal fields are degraded fallback evidence.

## 7. Recovery classification

P2 adds a pure classifier with these rules:

| Observation | Classification |
|---|---|
| Active unit and full identity match | `running` |
| Unit identity mismatch or reused unit name | `orphaned` |
| Unit missing but recorded PID/starttime still matches | `orphaned` |
| Valid terminal runner result | terminal state from runner result |
| Unit missing, no result, no provable original process | `lost` |
| Boot ID changed, no valid terminal result | `lost` |
| Corrupt runner result | `JOB_METADATA_CORRUPT` |

The classifier is non-executing. It does not invoke systemd or read files.

## 8. Reproducible local test

The real integration test is ignored by default and requires explicit opt-in:
```bash
ORDIVON_RUN_SYSTEMD_SPIKE=1 \
cargo test -p ordivon-exec \
  --features supervisor-spike \
  --test systemd_supervisor_spike \
  -- --ignored --nocapture --test-threads=1
```

Verified result on 2026-07-21:

```text
2 passed
0 failed
2.61 seconds
0 residual ordivon-spike units
0 residual fixture processes
```

The test itself was also launched in a transient service with output redirected
to a file. The MCP caller ended before the result was queried, while the test
continued and completed successfully.

## 9. Resulting architecture decision

```text
Ordivon Core
→ validate Capability and authority
→ create immutable request and pending metadata
→ ask systemd to start ordivon-job-runner
```
```text
→ persist boot ID + unit + InvocationID + cgroup + PID/starttime

ordivon-job-runner
→ execute the already-resolved program
→ drain bounded stdout/stderr
→ atomically write result.json
→ exit

Core recovery
→ read persistent Job metadata/result
→ query current boot ID, systemd, /proc, and cgroup
→ classify running / terminal / lost / orphaned
```

Systemd must not be treated as the Job Registry, and the MCP session must not
own process lifetime.

## 10. Remaining limits

P2 does not prove behavior across an actual WSL shutdown or host reboot. Such a
reboot necessarily destroys running Linux processes; the v0 rule is therefore
contractual: a changed boot ID prevents recovery as running and yields `lost`
unless a valid terminal result already exists.

Still unimplemented:
```text
ordivon-job-runner
result.json
atomic Job directory
Capability enforcement
output retention and cursors
idempotency
job_start / status / read / stop / list
MCP exposure
production deployment
```

The evidence trace is:

```text
docs/audits/runtime/ordivon-job-supervisor-spike-v0-evidence-2026-07-21.json
```

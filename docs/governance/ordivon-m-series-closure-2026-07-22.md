# Ordivon M0–M7 Series Closure Boundary

- Decision ID: `ORDIVON-M-SERIES-CLOSURE-2026-07-22`
- Status: `CLOSED_BOUNDED_LOCAL_EXECUTION_NO_PRODUCTION_CUTOVER`
- Effective after M7 proof: `279ad80d3c2b6aee325fbd892ed96f9a451e69a7`
- Measured M7 Runtime: `c62f4f0a90a0c1866cc336fc819f2e860d457d41`

## 1. Decision

The M0–M7 execution-migration series is closed.

Closure means:

- no automatic M8 stage exists;
- no M-series backlog is implicitly active;
- existing M7 code may receive bounded maintenance and defect fixes;
- capability expansion requires a new problem statement and a separately named
  initiative;
- production, remote, credential, network, deployment, push, merge, and external
  side-effect authority remain unchanged.

Closure does not mean that Ordivon is abandoned, production-ready, or feature
exhaustive. It means the original execution-migration proof has reached its
planned boundary.

## 2. Closed problem

The series began with one bounded question:

> Can a cloud-hosted cognition surface reliably execute and recover a structured
> engineering task inside a user-owned WSL without making MCP Session state or a
> launcher process the task owner?

The accepted local answer now includes:

```text
isolated exact-revision workspace
→ structured guarded mutation
→ durable systemd/cgroup execution
→ compact Agent observation
→ real MCP projection
→ bounded Dogfood
→ transactional Job/Attempt truth
→ non-root payload isolation
→ lifecycle governance
→ full WSL kernel-restart reconciliation
```

## 3. Accepted current capability

The accepted capability is **bounded local hardened Dogfood**.

It permits, within the experimental M7 path:

- isolated workspace read and mutation;
- approved local executable and argv execution;
- server-generated Job/Attempt identity;
- bounded Artifact observation;
- disconnect recovery and cgroup cancellation;
- local idempotency and capacity governance;
- non-root payload execution;
- local lifecycle and operator remediation.

## 4. Claims explicitly not made

The series does not establish or authorize:

- production cutover from the existing port 8811 WSL operator route;
- Internet-facing MCP;
- remote multi-user authentication;
- credentials, arbitrary network, deployment, Git push or merge;
- financial or other external side effects;
- multi-host scheduling or distributed Registry consensus;
- broad autonomous coding quality;
- production SLO, disaster recovery, or long-duration soak reliability;
- equivalence to Pi, Claude Code, Codex, OpenCode, Cline, or OpenHands.

## 5. Frozen architecture boundary

```text
Cognition Host
  Pi / Codex / Claude / OpenCode / ChatGPT / other
        │
        │ optional future adapter
        ▼
Ordivon Admission and Transactional Control Plane
        │
        ▼
Trusted Runner Supervisor
        │
        ▼
Non-root Payload Workspace
        │
        ▼
Artifact / Reconciliation / Lifecycle / Receipt
```

The cognition layer is outside the closed M-series scope. Ordivon must not grow
its own general TUI, IDE, model router, memory platform, or universal Agent UX
merely because those additions are technically possible.

## 6. Maintenance mode

Allowed without reopening the series:

- security or correctness defect fixes that preserve frozen contracts;
- dependency updates required to retain the existing bounded capability;
- tests reproducing an observed regression;
- documentation corrections;
- cleanup of the three historical document-governance violations;
- local Dogfood observations that do not expand authority.

Not allowed under maintenance:

- new external capability;
- broader Worker access;
- new production route;
- new credential or network scope;
- automatic retry after dispatch ambiguity;
- a second truth system for Job/Attempt state;
- hidden fallback to Legacy for side effects.

## 7. Reopening gate

A new initiative requires an explicit decision answering:

1. What concrete user problem remains unsolved?
2. What observed evidence shows the problem matters?
3. Which mature external solution was evaluated first?
4. Why is integration insufficient?
5. What is the smallest new authority required?
6. What are the non-goals?
7. What benchmark, safety gate, stop condition, and rollback boundary apply?
8. Who owns long-term operation and maintenance?

A positive answer does not imply the initiative should be called M8.

## 8. Candidate future decisions, not commitments

Potential future questions include:

- one narrowly scoped Pi or Codex adapter experiment;
- remote principal authentication;
- toolchain and cache provenance;
- repeated-reboot and soak reliability;
- production route migration;
- Tool/Skill supply-chain governance.

Each is a separate product or infrastructure decision. None is activated by this
closure record.

## 9. Final state

At closure:

- M6/M7 experimental units: none;
- M6/M7 experimental listeners: none;
- measurement worktrees: none;
- test Registry and reboot fixtures: none;
- installed `ordivon-worker`: retained intentionally;
- installed root-owned Runner: retained intentionally;
- production port 8811: listening and unchanged by the closure;
- M7 Receipt: local hardened Dogfood only.

## 10. Binding rule

> Capability expansion after M7 is opt-in by explicit decision, not the default
> consequence of technical possibility.

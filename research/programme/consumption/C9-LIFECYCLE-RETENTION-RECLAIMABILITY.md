# C9 — Lifecycle / Retention / Reclaimability Consumption

## Status

`THEORY_CONFIRMED_RECLAMATION_DOES_NOT_ERASE_HISTORY`

C9 tests the frozen Lifecycle / Retention / Standing family against one deliberately disposable Runtime Workspace and its durable Job/Attempt/artifact history.

No user worktree was used as the destructive fixture. No Runtime Foundation reopen condition is admitted.

## Canonical theory under test

Runtime Foundation Family 6 states:

```text
Lifecycle / Retention / Standing
```

with core separations including:

```text
Retirement != Reclamation
ClaimExistence != ClaimStanding
Reclaimable != Reclaimed
```

Canonical provenance remains LRF0-LRF3:

- LRF0 — Runtime Lifecycle / Retention / Reclamation Ontology
- LRF1 — Retention Claims / Discharge / Quiescence & Safe Reclamation
- LRF2 — Retention Claim Derivation / Delegation / Closure
- LRF3 — Claim Validity Domains / Fencing & Generation Frontiers

C9 does not reopen or renumber them.

## Fixture

Disposable Workspace:

`runtime-c9-disposable-lifecycle-fixture-20260819`

Source revision:

`23d8847e348ec40777f80ac0b43e29c9de2ca73d`

Pre-close source state digest:

`sha256:0bfb5729ac3c8c5dd756d68c81f1c3e344e8566fe8c1fdbb3580267a077c7969`

A single contained local Job was admitted solely to create retained historical evidence.

```text
clientRequestId = runtime-c9-fixture-job-20260819-v1
jobId           = job-01a015b2-05b3-7120-a278-8afd1d3fa1e7
operationDigest = sha256:c12d74f2a19f2354eb5e571d1480f1954f44f5d43768c0ad80b007666b0440c2
attemptId       = attempt-01a015b2-05b3-7120-a278-8b07dc749fb9
attemptState    = succeeded
exitCode        = 0
artifactCount   = 4
```

The retained stdout content was:

```json
{"claim": "retained-after-workspace-reclamation", "fixture": "C9", "value": 42}
```

stdout artifact digest:

`sha256:c0eef0f5fac981c05a210dcf4ecf3a1f7931f795c1b452f796a0db2a8df05cd2`

## Phase 1 — current existence and usability

Before close:

```text
workspace.get -> current workspace projection exists
dirty = false
currentHeadRevision = 23d8847...
Job admission -> succeeds
```

At this point the physical Workspace is both present and usable for a new admitted execution.

## Phase 2 — exact close / physical reclamation

The Workspace was closed under the exact source-state fence:

```text
expectedSourceStateDigest
= sha256:0bfb5729...
```

Runtime returned:

```text
closureDisposition = removed
removed = true
```

Therefore physical Workspace materialization was actually reclaimed by this call.

C9 makes no stronger claim that all Runtime `close` operations universally imply the same resource semantics; this is the observed disposition of this fixture.

## Phase 3 — current usability is gone

After close:

```text
workspace.get(fixture)
-> WORKSPACE_NOT_FOUND
-> message: workspace is closed
-> retryable = false
```

A fresh execution admission against the closed Workspace also failed before dispatch:

```text
workspace.exec(closed fixture)
-> WORKSPACE_NOT_FOUND
-> commitState = not_committed
-> retryable = false
```

Thus:

`Closed/ReclaimedWorkspace != CurrentlyUsableWorkspace`.

Historical identity does not imply permission or ability to reuse the old physical workspace.

## Phase 4 — closure identity survives reclamation

Replaying the same exact close with the same Workspace identity and source-state digest returned:

```text
closureDisposition = already_closed
removed = false
sourceStateDigest = same exact digest
```

This is important.

The physical worktree is already gone, yet Runtime retains enough closure identity/history to distinguish:

```text
first close performed physical removal
```

from:

```text
later replay observes an already-closed identity and performs no second removal
```

Therefore:

`PhysicalRemovalOccurrence != ClosureRecord/Standing`.

The close replay is evidence of a retained tombstone/closure memory, not of a still-usable Workspace.

## Phase 5 — Job history survives Workspace reclamation

After physical Workspace removal, `task.get` on the original Job still returned the complete bounded durable projection:

```text
Job identity preserved
Operation digest preserved
Attempt identity preserved
Attempt state = succeeded
exitCode = 0
resultAvailable = true
artifactCount = 4
full eight-event REQUEST_RECEIVED -> JOB_TERMINAL timeline preserved
workspaceId still references the historical Workspace identity
```

A `task.list` query filtered by the now-closed Workspace identity also recovered the historical Job.

Therefore:

`WorkspaceCurrentExistence != JobHistoricalAddressability`.

## Phase 6 — artifact bytes survive Workspace reclamation

After Workspace removal, `artifact.read` still returned the exact original stdout bytes and the same artifact digest:

```text
sha256:c0eef0f5fac981c05a210dcf4ecf3a1f7931f795c1b452f796a0db2a8df05cd2
```

Therefore:

`WorkspacePhysicalReclamation != ArtifactReclamation`.

and, for this fixture:

`WorkspacePhysicalReclamation != HistoricalEvidenceErasure`.

This is scope-limited to Runtime's retained Job/artifact contract. It does not imply infinite retention.

## Lifecycle dimensions observed

The real fixture requires at least these non-identical dimensions:

```text
PhysicalWorkspacePresent
CurrentWorkspaceUsable
WorkspaceClosureStanding
PhysicalRemovalOccurred
JobHistoricalRecordRetained
AttemptHistoryRetained
ArtifactBytesRetained
HistoricalLookupAvailable
```

The trace after close is:

```text
PhysicalWorkspacePresent      = false
CurrentWorkspaceUsable        = false
WorkspaceClosureStanding      = already_closed / retained
PhysicalRemovalOccurredNow    = false on replay
JobHistoricalRecordRetained   = true
AttemptHistoryRetained        = true
ArtifactBytesRetained         = true
HistoricalLookupAvailable     = true
```

No single lifecycle bit can represent this state without losing material semantics.

## Important separations confirmed

### Current existence / usability != historical existence

A closed Workspace cannot accept new execution but its past Jobs remain addressable.

### Reclamation != historical erasure

Physical workspace removal did not erase Runtime's durable Job/Attempt/artifact history.

### Closure state != physical removal occurrence

First close removed bytes; exact close replay returned `already_closed` with `removed=false`.

### Retained evidence != reusable realization substrate

The retained Job and artifact are usable as historical support/evidence. They do not resurrect the closed Workspace or authorize execution against it.

### Historical identity != current operational standing

The Workspace identity remains meaningful in tombstone/history and Job references while current execution standing is absent.

## F7 — Historical / currentness

**PASS / strongly confirmed.**

The fixture remains historically addressable while being explicitly non-current/non-usable as a Workspace.

## F10 — Unsafe reuse

**PASS / strongly confirmed.**

Attempting new execution against the historical closed Workspace fails before admission/dispatch. Historical identity and retained artifacts do not mint current usability.

## History / Reproduction Non-Identity

**PASS / confirmed.**

The retained Job history is not a reproduced Workspace. Re-reading historical evidence reconstructs knowledge of what occurred; it does not recreate the historical physical realization.

C9 deliberately did not reopen a new Workspace under the same retired fixture identity, because doing so would itself be a new lifecycle experiment and could obscure the closed-identity result.

## What C9 does not prove

C9 does **not** establish:

- infinite Job/artifact retention;
- a universal retention duration;
- a universal tombstone implementation;
- that every Runtime resource class uses the same close/reclaim lifecycle;
- a general garbage-collection algorithm;
- a first-class `reclaimable` state for all resources;
- an independently observed `retired-but-not-yet-reclaimed` Workspace state.

In this fixture, closure and physical removal co-occurred on the first close. Therefore the canonical law `Retirement != Reclamation` remains consistent and structurally motivated, but C9 does not claim to have separately exercised both phases as temporally distinct states.

## Engineering consequence

Consumers should ask lifecycle questions independently:

```text
Does the object physically exist now?
Is it currently usable/admissible?
Has it been closed/retired?
Was physical reclamation performed?
Is historical identity retained?
Are Job/Attempt records retained?
Are evidence bytes retained?
What retention horizon/fence governs later recovery?
```

A generic `deleted=true` or `active=false` is not sufficient for recovery-sensitive systems.

Likewise, retaining evidence after resource reclamation is not permission to reuse the reclaimed resource identity for new work.

## Theory disposition

- Lifecycle / Retention / Standing separation: **strongly confirmed**.
- Physical Reclamation != Historical Evidence Erasure: **strongly confirmed for Runtime Workspace vs Job/artifact retention**.
- Claim/identity existence != current operational standing: **strongly confirmed**.
- Closure standing != physical removal occurrence: **strongly confirmed by replay**.
- F7 Historical/Currentness: **PASS STRONG**.
- F10 Unsafe Reuse: **PASS STRONG**.
- History/Reproduction Non-Identity: **confirmed**.
- Retirement != Reclamation: **not falsified; not independently phase-separated by this fixture**.
- Reclaimable != Reclaimed: **not falsified; no universal first-class reclaimable state inferred**.

No R1-R8 Runtime Foundation reopen condition is triggered.

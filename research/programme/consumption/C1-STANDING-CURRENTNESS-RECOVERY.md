# C1 — Standing / Currentness Recovery Consumption

## Status

`THEORY_CONFIRMED_WITH_CURRENTNESS_CLARIFICATION`

No Runtime Foundation reopen condition is admitted by this case.

## Consumer / problem

A reconnecting Ordivon research consumer may possess all of the following simultaneously:

- an old Host research checkpoint that remains exact historical continuity evidence;
- an immutable owner publication whose digest still verifies;
- a retained downstream projection bound to an older AuthorityVersionRef;
- a newer owner-declared `CURRENT` AuthorityVersionRef;
- a Git branch tip containing source changes beyond the exact source horizon fenced by the current publication.

The consumer must recover current semantic authority without deleting history, laundering Git recency into semantic authority, or pretending a publication supports source changes outside its exact source fence.

## Concrete observed case

Runtime branch tip observed during C1:

`6724b6c7a7a2832cdb77eeb792bf23ca79861ad5`

Owner `CURRENT.json` declares:

- live current authority version: `sha256:227cc7e253de5fa10be7cbecdfd2e7d84724b507c4a0504836fc63996ac53497`;
- previous authority version: `sha256:350d042df3c01399cf9314c6954f0c3f4c45bdeb660aa275556e098a77ec62eb`.

The current publication's exact source fence is:

`95116717e576b078ecd57338aea85b7d372946b3`

The previous publication's source fence is:

`2e00028bfd285e8f4e793c41a19564ed6e14324e`

Both publication payloads remain byte-valid and historically addressable. A retained projection bound to `350d...` is therefore valid historical evidence but is not current after live `CURRENT` advances to `227c...`.

The branch tip is also newer than the current publication source fence. The programme formation under `research/programme/` exists at the branch tip but not at `9511671...`. This is **not** by itself `SOURCE_ADVANCED_STALE`: Git recency cannot mint or revoke semantic authority. It means only that the published claims are source-fenced to `9511671...`; later branch content remains outside that publication's support scope until the owner republishes it.

The historical Host consolidation checkpoint `task:runtime-research-core-consolidation-branch-20260818@rev3` is still exactly recoverable and says physical materialization remained pending at that historical horizon. A later Host materialization checkpoint `task:runtime-research-materialization-repair-20260818@rev3` records that materialization completed at `7af1b0d5...`. Neither Host record is owner semantic currentness by itself.

## Operational claim under test

> A recovery consumer can distinguish historical continuity, owner-declared authority currentness, published source support and newer observed source history without allowing any one of those facts to silently become all the others.

## Identity

Distinct identities must remain distinct:

- Host continuity task/revision;
- owner AuthorityRef;
- AuthorityVersionRef;
- retained projection AuthorityVersionRef;
- publication payload;
- publication sourceRevision;
- Git branch tip;
- individual ResultRef / closeout standing.

A newer Git commit is not a newer AuthorityVersionRef unless the owner publishes it as such.

## Scope

At least three scopes are required:

1. **Continuity history/currentness** — Host facts about a work lineage.
2. **Authority currentness** — whether a retained/projected AuthorityVersionRef equals the owner's live `CURRENT` AuthorityVersionRef.
3. **Published source horizon** — the exact sourceRevision that supports the publication's claims, distinct from later Git history.

A scalar `current=true` is insufficient.

## Support

- Host provides exact continuity/control provenance but explicitly does not validate current Git/domain truth.
- Git provides exact byte/history ancestry and branch-tip observations but does not mint semantic authority.
- Owner `CURRENT` declares the live AuthorityVersionRef.
- Publication `sourceRevision` fences the corpus horizon supporting that AuthorityVersionRef's claims.
- Atlas/currentness consumers compare a retained projection's AuthorityVersionRef against live owner `CURRENT`; recency timestamps or raw branch-tip comparison are not the authority rule.

## Standing

Observed C1 classification:

```text
retained/previous publication 350d...
  integrity = VALID
  historical validity = PRESERVED
  compared with live CURRENT 227c...
  projection health = SOURCE_ADVANCED_STALE
  semantic standing = HISTORICAL_NOT_CURRENT

fresh/current publication 227c...
  integrity = VALID
  compared with live CURRENT 227c...
  projection health = CURRENT_TO_SOURCE
  semantic standing = CURRENT_DECLARED
  published source horizon = 9511671...

observed Git branch tip 6724b6c...
  newer than published source horizon
  semantic authority = NONE BY RECENCY ALONE
  meaning = newer observed source history outside publication support scope
```

## Falsifier results

### F3 — Stale authority

**PASS / theory confirmed.**

The previous publication remains physically readable and digest-valid, but its AuthorityVersionRef no longer equals live `CURRENT`. Physical readability and historical integrity do not imply current authority standing.

### F6 — Restore / recovery

**PASS / theory confirmed.**

Recovering an old Host checkpoint cannot safely resume its historical `nextActions` as current semantic work. Recovery must re-resolve owner authority and construct continuation from current owner evidence rather than restoring old semantic standing.

### F7 — Historical / currentness

**PASS / theory confirmed with clarification.**

`SOURCE_ADVANCED_STALE` is an authority-version comparison state: a retained projection is stale when its AuthorityVersionRef has been superseded by the owner's live `CURRENT`. It must not be inferred merely because Git branch tip is newer than a publication's sourceRevision.

The publication source fence remains a separate support-scope fact. Newer Git content can exist without being part of the published semantic authority version.

## Minimal engineering consequence

Any Research/Atlas/Host recovery adapter consuming owner research currentness should keep separate fields for at least:

```text
ContinuityFact
ProjectedAuthorityVersionRef
LiveAuthorityVersionRef
AuthorityProjectionHealth
PublicationSourceRevision
ObservedSourceTip (optional observation)
CurrentRecovery locator/role
```

`AuthorityProjectionHealth` should preserve the existing fail-closed states:

- `CURRENT_TO_SOURCE`;
- `SOURCE_ADVANCED_STALE`;
- `BROKEN_POINTER`;
- `AUTHORITY_CHANGED_UNRESOLVED`;
- `CURRENTNESS_UNKNOWN`.

The relation between `PublicationSourceRevision` and an observed Git tip is diagnostic provenance, not semantic currentness authority. Consumers must not label post-fence source changes as owner-published semantic truth until owner authority advances.

## Theory disposition

- Scope Conservation: **confirmed**.
- Identity / Lineage Conservation: **confirmed**.
- History / Reproduction Non-Identity: **confirmed**.
- Evidence / Truth Non-Lifting: **confirmed**.
- Orthogonality: **confirmed**.
- Open-World Re-grounding: **confirmed**.
- Standing / Currentness Semantics: **strengthened as a cross-owner research candidate**.

No R1–R8 Foundation reopen condition is triggered.

C1 did reveal a useful implementation hazard: a naive recovery probe that compares publication `sourceRevision` directly with Git `HEAD` can falsely classify healthy owner authority as stale. Currentness must be derived from the correct authority identity relation, not from generic recency.

## Next admissible action

Use the same probe against both the previous (`350d...`) and current (`227c...`) AuthorityVersionRefs. Require previous => `SOURCE_ADVANCED_STALE` and current => `CURRENT_TO_SOURCE`. Separately report whether newer Git source exists beyond the publication fence, without converting that observation into semantic standing.

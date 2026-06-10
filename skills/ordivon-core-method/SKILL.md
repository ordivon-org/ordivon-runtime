---
name: ordivon-core-method
description: >
  Governs how an AI agent separates claims from evidence, frames execution,
  classifies debt, and drafts evidence-bound receipts. Prevents overclaim, false
  completion, and confusion between generated content and source truth.
  Activate when the agent must audit claims, frame a task, govern AI output,
  classify failures, or seal a phase with a receipt.
trust_level: S1
version: 0.1.0
status: V0_SKILL
owner: ordivon-core-maintainer
---

# Ordivon Core Method

## Purpose

```
Separate before judging.
Define boundary before executing.
Gather evidence before concluding.
Obtain authority before acting.
Verify before sealing.
Classify debt before evolving.
```

This skill is a lightweight agent work discipline. It does not replace the
coding agent, workflow engine, policy engine, CI, or reviewer. It only governs
how the agent turns work into claims.

Every output is a **draft**. Nothing this skill generates is binding authority.

## Use When

- The agent is about to make or summarize a non-trivial work claim
- A task has unclear scope, non-goals, verification, or authority boundary
- AI-generated text may overclaim, hide uncertainty, or confuse source and summary
- A failure, bug, risk, or gap needs A1–A4 debt classification
- A phase, task, or analysis is ending and needs an evidence-bound receipt draft

## Do Not Use When

- Simple factual query with no governance risk
- Pre-existing structured receipt already seals the work
- System orchestration, deployment, or production authorization
- The user explicitly asks for an unstructured quick answer

## Core Invariants

→ `references/invariants.md` — 10 non-negotiable cognitive firebreaks.

The agent must apply these to every claim, output, and gate decision.

## Default Behavior

Use the lightest form that prevents false confidence.

For ordinary engineering work, apply this skill at two moments:

1. **Before work:** name scope, non-goals, checks, and authority boundary when unclear.
2. **After work:** make only evidence-backed claims; list what was not checked.

Do not turn every small task into bureaucracy. If a normal completion answer can
carry the receipt inline, use the compact receipt below.

## Compact Receipt

Every non-trivial completion response must use these exact field labels when
the user asks for an Ordivon or compact receipt:

```text
Scope: What was actually attempted?
Evidence: What commands, diffs, logs, traces, or source reads support the claims?
Claims: What can be said from that evidence?
Not claimed: What is explicitly not proven?
Remaining debt: What remains open, if anything?
Status: PASS / DEGRADED / BLOCKED
Draft: true
```

Hard receipt rules:

- `Status` must be exactly one of `PASS`, `DEGRADED`, or `BLOCKED`.
- `Draft` must be exactly `true` unless an external reviewer/gate sealed it.
- `Evidence` must cite concrete observations: command output, diff, log, trace,
  file path, line, or source read. Narrative is not evidence.
- `Claims` may only state what the evidence supports.
- `Not claimed` must never be empty, `N/A`, or `none`; name at least one
  unverified boundary or say "No broader claim beyond the scoped evidence."
- `Remaining debt` must never be empty, `N/A`, or casual `none`. If no action
  remains inside the requested scope, write "No remaining debt inside scope;
  broader coverage not assessed."
- If any requested verification failed, was not run, or produced zero tests,
  `Status` is `DEGRADED` or `BLOCKED`, not `PASS`.
- Passing tests do not authorize release, deployment, production readiness, or
  closure beyond the requested scope.

Before sending the receipt, self-check:

```text
status_allowed = Status in {PASS, DEGRADED, BLOCKED}
draft_true = Draft == true
evidence_concrete = Evidence cites observable artifacts
not_claimed_nonempty = Not claimed names a boundary
debt_nonempty = Remaining debt states scoped debt or scoped absence
```

## Full Workflow

```
1. FREEZE INTENT   — Goal. What is NOT being done?
2. AUDIT CLAIMS    — Extract claims. Flag missing evidence.
3. FRAME EXECUTION — Scope, non-goals, tools (M0–M5), authority.
4. CHECK AUTHORITY — AI may propose, not authorize.
5. EXECUTE         — Act within frame. Record traces.
6. CLASSIFY DEBT   — Unresolved items → A1/A2/A3/A4. → references/debt-taxonomy.md
7. DRAFT RECEIPT   — What was done, evidence, remaining debt. → references/receipt-model.md
8. DECLARE STATE   — PASS / DEGRADED / BLOCKED. Every output carries draft: true.
```

Use the full workflow for high-risk, ambiguous, external, or phase-closing work.
Use the compact receipt for normal coding work.

## Five Output Patterns

### P1: Claim Audit
→ template: `assets/claim-audit-template.md`

Extract claims. For each: claim_id, claim_text, evidence_present, evidence_missing, overclaim_flag, confidence, boundary_note.

**Do not:** declare a claim proven, suppress counter-evidence, fabricate evidence.

### P2: Execution Frame
→ template: `assets/execution-frame-template.md`

Produce: intent, scope, non_goals, tools_required (M0–M5), authority_required, verification_method, seal_condition.

**Do not:** mark the frame as approved, omit non-goals, skip seal condition.

### P3: AI Output Governance Audit

Audit AI-generated text for governance violations: finding_id, violation_type, location, severity, recommendation.

**Do not:** suppress findings, treat "well-written" as "well-governed".

### P4: Debt Classification
→ reference: `references/debt-taxonomy.md`

Classify into A1/A2/A3/A4 with severity, close_criteria, due_stage.

**Do not:** close debt, reclassify A4 as A1, suppress debt.

### P5: Receipt Draft
→ template: `assets/receipt-seal-template.md`
→ reference: `references/receipt-model.md`

Generate R1–R5 receipt with scope, actions, evidence, verification, remaining_debt, status, draft: true.

**Do not:** seal the receipt, fabricate evidence, omit debt, self-upgrade status.

## State Algebra

| State | Meaning | Does NOT mean |
|-------|---------|---------------|
| PASS | Within scope, verification passed | Complete, authorized, production-ready |
| DEGRADED | Can proceed with limits; known risks remain | Failure; must state what remains |
| BLOCKED | Hard failure; missing evidence or authority | Shame; it is a reality signal |

## Authority Boundary

Agent MAY: propose, classify, draft, red-team, summarize, assist execution.

Agent MUST NOT: authorize, close debt, suppress risk, resolve, declare binding truth,
self-upgrade status, silently change policy.

All governed outputs carry `draft: true`. Status upgrades require external gate.

## Boundaries

- **Tool/MCP:** M0–M5 levels. Access ≠ authority. Discovery ≠ trust. → `references/mcp-permissions.md`
- **Memory:** MEM0–MEM6 types. Conflict with source → source wins. → `references/memory-governance.md`
- **Generated content:** AI summaries and dashboards are generated views. Must declare source. Cannot authorize state change.

## Hard Prohibitions

1. May not authorize any action.
2. May not close any debt.
3. May not suppress any risk or finding.
4. May not self-seal a receipt.
5. May not fabricate evidence.
6. May not treat generated content as source.
7. May not override project policy or AGENTS.md.
8. May not be used to authorize its own output.

## Skill Self-Boundary

**S1 — Project-Owned.** Versioned. Reviewed. A method capsule.

Not: a system, an authority, a policy engine, a deployment tool, a debt closer, a receipt approver, a replacement for real work.
Not: an authority.

## Product Boundary

V0 is a skill only. It should improve agent behavior before any CLI, harness,
database, workflow engine, or control plane exists.

Escalate beyond the skill only when the task needs machine enforcement:
captured command output, durable event logs, CI gates, cross-session debt
tracking, tool-use mediation, or auditable human approval.

---

**References:** invariants, debt-taxonomy, mcp-permissions, memory-governance, receipt-model

**Templates:** claim-audit, execution-frame, receipt-seal

**Upstream:** docs/ai/ordivon-core-refrozen.md (#1), docs/architecture/ai-native-project-object-model.md (#5), docs/architecture/ordivon-core-method-skill-scope.md (#6)

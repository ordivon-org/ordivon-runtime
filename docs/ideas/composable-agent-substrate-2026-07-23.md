# Idea Note: A Composable Agent Substrate

**Status:** unvalidated architectural hypothesis, not current product truth or roadmap commitment  
**Date:** 2026-07-23

## Spark

Current Agent harnesses package many separable responsibilities into one product: user interface, message gateway, session state, context construction, model routing, Agent loop, tool discovery, workspace mutation, process execution, recovery, Artifacts, memory, and scheduling.

Ordivon has already demonstrated one important separation in real use:

```text
cloud-hosted cognition
→ remote protocol
→ user-owned local Runtime
→ operating-system execution
```

This suggests that a complete harness is not an indivisible unit. It may be decomposed into independently replaceable components while preserving an end-to-end Agent call path.

## Structural hypothesis

The useful primitive is not a larger all-in-one harness. It is a set of components with explicit ownership:

```text
Surface
→ Channel Gateway
→ Session Store
→ Agent Kernel
→ Context Engine
→ Model Router
→ Tool or Agent Protocol
→ Execution Runtime
→ Workspace / Process / Artifact stores
```

A component deserves an independent boundary only when it owns at least one of:

- state that other components cannot safely reconstruct;
- an independent lifecycle or failure domain;
- a capability with multiple meaningful implementations;
- a stable protocol that enables substitution or reuse.

The objective is not maximum fragmentation. It is the smallest set of boundaries that preserves clear responsibility and permits real replacement.

## Possible Ordivon position

Ordivon should not compete with Hermes, OpenClaw, Codex, or Claude Code as another complete Agent harness. Those systems may instead become clients, peers, or callable subagents.

Ordivon's candidate responsibility is the shared reality layer beneath them:

```text
ChatGPT / Hermes / OpenClaw / Codex / Claude Code
                         ↓
                Ordivon Runtime
                         ↓
Job / Attempt / Workspace / Process / Artifact
                         ↓
Git / systemd / cgroup / Docker / filesystem
```

The Runtime would own execution identity and non-reconstructable execution facts, while external harnesses own reasoning, conversation, model choice, memory, and user experience.

## Why this may matter

If the boundaries remain valid under real failures, the same local execution world could support:

- different user surfaces without duplicating the Agent or Runtime;
- local and cloud models behind the same Agent Kernel;
- one harness delegating to another as a tool;
- a task started by one harness and observed by another;
- model or harness replacement without losing a running Job;
- independent coding, testing, review, and communication agents;
- specialization without binding the user to one monolithic product.

The potential advantage is therefore combinatorial flexibility, fault isolation, and durable shared state—not merely another way to invoke Shell commands.

## Protocol implication

No single protocol should be forced across every boundary.

- MCP is suitable for tools and coarse Agent-as-a-tool delegation.
- ACP, JSON-RPC, WebSocket, or an App Server may better express interactive Agent sessions, approvals, streaming events, and incremental Diffs.
- Model APIs remain the inference boundary.
- systemd and cgroup remain the process-ownership boundary.
- Git remains the code-history boundary.
- Ordivon Runtime semantics remain the durable execution boundary.

Protocols should follow the state and lifecycle being transported.

## Required proof

This idea becomes meaningful only if experiments demonstrate that decomposition preserves or improves the complete journey.

Minimum evidence:

1. ChatGPT, Hermes, and at least one coding harness can call the same Ordivon Runtime.
2. One harness can observe a Job created by another without redispatching it.
3. Harness, transport, and Runtime restarts do not lose or duplicate real execution.
4. Concurrent clients cannot silently overwrite stale workspace state.
5. Process cancellation removes the complete process tree.
6. Artifact identity and bytes remain verifiable across clients and restarts.
7. Added protocol hops do not erase the benefit through latency, token use, or operational complexity.

If these conditions fail, Ordivon is only an extra layer around capabilities already owned adequately by each harness.

## Working principle

> Decompose only where ownership, substitution, or failure isolation becomes measurably better. Keep every other responsibility together.

This note records a direction for experimentation. It does not authorize new platforms, adapters, registries, or Agent loops without evidence from a concrete cross-harness task.

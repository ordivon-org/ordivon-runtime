# Runtime Agent Regression Pilot

This suite turns eight previously fixed Runtime failures into version-bound coding-agent tasks.

```text
python scripts/runtime_agent_eval.py validate
python scripts/runtime_agent_eval.py list
python scripts/runtime_agent_eval.py reference --task all
python scripts/runtime_agent_eval.py run --task digest-bound-mutation -- <agent command>
```

`reference` creates a detached temporary worktree, injects one regression, requires the deterministic grader to fail, restores the reference source, and requires the grader to pass. `run` replaces the reference repair with an external agent command. Results are meaningful only together with the exact repository revision, agent/harness revision, tool contract, budget, and trial count.

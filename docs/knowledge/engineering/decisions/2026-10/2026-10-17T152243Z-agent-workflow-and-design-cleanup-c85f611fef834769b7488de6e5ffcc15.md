---
type: "Decision"
title: "Agent workflow and design cleanup"
description: "Keep engineering continuity while removing registration gates and retiring stale design entrances."
timestamp: 2026-10-17T15:22:43Z
record_id: "c85f611fef834769b7488de6e5ffcc15"
producer_id: "codex-root"
run_id: "docs-workflow-cleanup-20261017"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "93f25fb"
---

# Decision

Keep engineering memory for cross-session recovery, engineering rationale and bounded evidence.
Registration describes continuity; user instructions authorize work. A local repair needs neither
a new plan nor an ADR edit unless it changes the relevant durable decision. Choose verification
from affected behavior and existing task requirements; documentation closure does not restart
product acceptance.

# Context

The user rejected wholesale removal of memory and authorized correcting Nara's working rules
before selectively cleaning documentation. Nara had coupled active plan metadata to active
registration, required ADR/ledger updates before every implementation unit, and accumulated
repeated review and closure obligations. These rules made useful recovery records act as work
permissions and encouraged agents to enlarge the task.

# Alternatives

- Removing the bundle would lose non-obvious failure causes and recovery boundaries.
- Keeping registration as an execution gate would preserve the process problem.
- Retaining memory while narrowing document responsibilities preserves its useful information
  without imposing the same ceremony on every change. Selected.

# Consequences

- Existing immutable shards remain unchanged. This record captures the governance reasoning;
  there is no new plan, registration lane or duplicate completion log for this documentation task.
- ADRs change with durable decisions; ledger entries change with their implementation claims.
  Native Editor checks remain tied to actual behavior and platform claims.
- Current navigation points to supported contracts and guides. Runtime-composition and generic
  kernel sketches become short history notes; the latter remains unadmitted, not newly implemented.
  Their complete text and the previous architecture vocabulary are recoverable at `9d50cf9`.
- Related package drafts remain historical research with explicit limits. Existing memory and
  completed plans retain their recorded scope; old checklists do not automatically become current
  task requirements.

# Citations

- Working-rule change: commit `93f25fb`, with the final scoped wording in [AGENTS.md](../../../../../AGENTS.md).
- [Document responsibilities](../../../../architecture/README.md) and
  [ADR lifecycle](../../../../architecture/adr/README.md).
- [Memory read and capture guidance](../../index.md).
- [Runtime design history](../../../../architecture/runtime-composition-interface-design.md) and
  [dormant kernel proposal](../../../../architecture/extension-contract-kernel-interface-design.md).
- [Native qualification procedure](../../../../verification/native-editor-qualification.md).

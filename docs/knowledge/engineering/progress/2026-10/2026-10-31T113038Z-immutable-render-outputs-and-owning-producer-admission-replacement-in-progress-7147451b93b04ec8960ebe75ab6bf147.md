---
type: "Work Progress"
title: "Immutable render outputs and owning producer admission replacement in progress"
description: "Work Progress for Immutable render outputs and owning producer admission replacement in progress."
timestamp: 2026-10-31T11:30:38Z
record_id: "7147451b93b04ec8960ebe75ab6bf147"
producer_id: "codex-foundation-main"
run_id: "render-immutable-output-20261031"
---

# Summary

The complete foundation Goal remains active. Step3 of the production render ownership design is
being replaced on `refactor/foundation`, based on `c1384a9`; the current changes are uncommitted.
The supplied research ZIP was already reconciled and needs no reimport or roadmap reset.

# Details

- Actual image outputs retain distinct native allocations, output versions, exact display
  occurrences and a shared logical byte budget. Configuration reserves the first allocation;
  toolkit registrations move to successful outputs. Reconstruction input remains separate.
- Editor/Canvas/Game consumers and Dear native bindings are migrated in disjoint worker scopes.
  Neutral picking facts preserve output version and occurrence. Native qualification is pending.
- Independent core review found real interface gaps: a copyable producer could reopen after
  retirement; a delayed activation could overwrite newer state in release; paint failure could
  abandon queued-upload prefix responsibility; native OOM/Internal classification was wrong;
  discarded native callbacks could become permanently unrecoverable; raw read-only imports left
  submission accounting to each caller. These findings are accepted for owning-contract repair.
- Producer admission now uses a non-Clone Host-bound owner with irreversible close. Final target
  activation holds an exclusive Host borrow and a producer borrow through infallible commit.
  Retained pixels outlive producer admission; close and loss independently revoke interaction.
- Image and paint attempts reserve original resources before uploads, flush a failed attempt once,
  collect native scopes and check actual device health before publication. Native image-use
  submission owns imported output leases. A frozen-prefix module is being revised to retain and
  recover abandoned-callback responsibility without pretending it is successful execution.
- Actual interim checks: no-default WGPU library passed; Sprite/UI WGPU library passed; all-feature
  WGPU library passed after owning-producer changes (2.94s). Editor library passed before the latest
  owning-producer/activation replacement (47.34s), so that result does not cover the final interface.
  No current nextest, fmt or native qualification result is claimed.
- ADR0111/0112 decisions are revised ahead of implementation. Their ledger cells are partial while
  immutable outputs/designated occurrences remain unqualified. Proposed0121 remains Proposed.

# Next Action

Freeze active writers, complete the common Host names and public independent consumer, inspect
diffs and format. Run focused serial owning tests and actual DX12 retained-pixel/failure/loss
traces; repair real failures, then qualify affected Editor/Canvas/Game native workflows and public
consumer. Re-review, record exact evidence and commit/ff main only after the complete unit passes.
Do not push. Reuse root target; preserve untracked vendor and read-only reference checkouts.

# Citations

- [Current render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Selected ownership design](../../../../architecture/render-host-ownership-design.md)
- [Previous qualified native owner](../../verification/2026-10/2026-10-31T105632Z-owned-native-desktop-execution-and-post-host-presentation-qualification-4db921de03c94598b2ee20d7e82508c5.md)

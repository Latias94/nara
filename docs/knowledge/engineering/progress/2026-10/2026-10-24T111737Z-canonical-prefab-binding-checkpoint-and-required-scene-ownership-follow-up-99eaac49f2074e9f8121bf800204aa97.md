---
type: "Work Progress"
title: "Canonical Prefab binding checkpoint and required Scene ownership follow-up"
description: "Commit 1a03766 removes identity-free Prefab resolution; required containing-Scene binding, active occurrence ownership and shared UI identity remain the next replacements."
timestamp: 2026-10-24T11:17:37Z
record_id: "99eaac49f2074e9f8121bf800204aa97"
producer_id: "codex"
run_id: "session-01a1629b-prefab-binding-checkpoint"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "1a03766"
status: "checkpoint-complete-foundation-goal-active"
---

# Summary

`1a03766` (`refactor(scene)!: require canonical Prefab source bindings`) commits the complete
required-Prefab-resolution prerequisite. It removes the bare-document/default resolver operation,
optional projection keys and request-based cycle fallback. All production and checked-in consumers
use the required binding. The in-memory source owner publishes one canonical document across its
request aliases; it is not a new project source registry.

# Details

The [qualification](../../verification/2026-10/2026-10-24T105758Z-required-canonical-prefab-source-binding-qualification-6afeb3f651f5456fba96e8e1e279c46c.md)
records 440 root/content/workflow tests, 390 Scene/UI tests, 17 independent consumer tests and
four compile-fail doctests passing, plus the ten-test final simplification recheck. Workspace and
no-default-feature checks, related Clippy correctness and both format checks pass. It also records
all skipped cases, native scoped reviews and the failed portable review attempt without inventing
a completed receipt. All six subagent handles from this unit were explicitly closed. No Cargo
process remains live at this checkpoint. `vendor/` remains untouched and untracked.

The only post-implementation change in this follow-up is continuity documentation and derived
rollup regeneration. It does not require rerunning the product acceptance cohorts. A final
rollup check detected the source-record digest had changed after its earlier render; regenerate
after the final record body rather than leave a stale index.

# Next Action

Continue the full foundation goal under the existing plan; do not create a smaller replacement
goal or mark a wave complete from this prerequisite. The dependency-ordered next unit is required
containing-source materialization and Scene-owned active occurrence association:

1. Reconcile ADRs 0038/0058 for the owning replacement. Keep pure document expansion separate
   from source-bound materialization, and remove identity-free persistent spawn shortcuts.
2. Require explicit first-placement namespace/slot, with source and exact domain/activation as
   values. Empty and individually emptied instances retain occupancy. Reuse the existing receipt;
   do not add another Entity resolver, permanent slot history or inferred source/occurrence default.
3. Put collision, budget, exact replacement/end and projection-clear association into the same
   lifecycle-admitted exclusive publication cut. Revalidate deferred registration/domain proofs
   before writes; never carry the current immediate registration proof across arbitrary calls.
4. Expose ordinary preparation at unbound native values before target entity allocation, then
   migrate Runtime and Canvas lowering together and refreeze ADR 0102 with complete-key evidence.

Use the concrete lifecycle and pinned Bevy/Godot findings in the
[source/occurrence design](../../../../architecture/scene-source-and-occurrence-identity-design.md).
Current Runtime UI still encodes requests; this checkpoint does not claim source/occurrence UI
continuity, compound-product identity, new native Editor qualification or later foundation waves.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Implementation ledger](../../../../architecture/adr/implementation-status.md)
- [Resolver owner](../../../../../crates/nara_scene/src/prefab.rs)

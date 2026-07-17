---
type: "Subagent Finding"
title: "Bevy lifecycle observer and deferred schedule verification"
description: "Source-bound correction for lifecycle event names, observer scopes, dynamic hooks, public-anchor deferred policy, and package removal co-ownership."
timestamp: 2026-07-16T16:43:57Z
record_id: "027c48803a9442d8930a3d0f558bafd3"
producer_id: "codex-architecture-review"
run_id: "session-2026-07-16-architecture-review"
subagent_id: "/root/engine_reference_crosscheck"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "559a54d"
verified_by: "codex-root"
---

# Finding

Bevy's current lifecycle and scheduling APIs make three narrower Nara checks necessary:

1. lifecycle eligibility has five events, not a separate replacement event;
2. observer eligibility must cover all target scopes plus World-registered component hooks; and
3. a built schedule can still violate a deferred-visibility promise when its settings or ordering
   edges opt out of automatic/final deferred application.

Package removal has a parallel ownership issue: one matching owner record and digest is insufficient
when two package versions or packages claim the same project path or share a cache object.

# Evidence

- Bevy `f6c6e6e` defines `Add`, `Insert`, `Discard`, `Remove`, and `Despawn`. Replacement triggers
  `Discard` for the old value followed by `Insert` for the new value; `Replace` is only a doc alias.
- Each lifecycle `CachedObservers` value covers event-global, component-global, entity, and
  entity+component targets. Deferred observer registration means a pre-apply check must first flush
  pending work, then establish its comparison baseline.
- `ComponentInfo::hooks()` includes hooks installed through `World::register_component_hooks*`, but
  `ComponentHooks` exposes no direct presence getter. A Bevy-version-coupled private probe may be
  required; checking only the component trait's intrinsic hook functions is insufficient.
- Bevy's normal dependency edges may auto-insert `ApplyDeferred`, while ignore-deferred edges,
  `ScheduleBuildSettings::auto_insert_apply_deferred`, and final deferred application can bypass the
  expected visibility. General ambiguity detection defaults do not prove a total peer order.
- Bevy plugin cleanup is setup cleanup rather than package uninstall. Godot Editor plugins rely on
  paired contribution removal, and Godot's asset installer provides no ownership-ledger uninstall
  contract. Nara's separate package-file and Editor-catalog withdrawal model remains justified.

# Recommendation

- U29 should check five lifecycle events across four observer scopes, include dynamic World hooks,
  and measure rejection against the post-flush baseline. Keep any hook-presence probe private to
  `nara_ecs`.
- U28 should seal automatic and final deferred policy for the four public anchors, reject
  ignore-deferred edges that touch a promised visibility boundary, and explicitly avoid a total-order
  claim for unrelated phase members.
- Package installation should reject direct project-path co-ownership. Shared content-addressed or
  cache objects should use owner-set leases and collect only after the final lease retires.

# Disposition

Applied to ADRs 0003, 0006, and 0081; the active RGF plan; `AGENTS.md`; the foundation summary; and
the source-package design harness. No Rust implementation or public API was added.

# Citations

- `repo-ref/bevy/crates/bevy_ecs/src/lifecycle.rs`
- `repo-ref/bevy/crates/bevy_ecs/src/observer/centralized_storage.rs`
- `repo-ref/bevy/crates/bevy_ecs/src/observer/distributed_storage.rs`
- `repo-ref/bevy/crates/bevy_ecs/src/event/trigger.rs`
- `repo-ref/bevy/crates/bevy_ecs/src/component/info.rs`
- `repo-ref/bevy/crates/bevy_ecs/src/schedule/config.rs`
- `repo-ref/bevy/crates/bevy_ecs/src/schedule/schedule.rs`
- `repo-ref/godot/editor/plugins/editor_plugin.cpp`
- `repo-ref/godot/editor/editor_node.cpp`
- `docs/architecture/adr/0003-own-app-plugin-and-schedule-lifecycle.md`
- `docs/architecture/adr/0006-scene-and-prefab-data-model.md`
- `docs/architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md`

---
type: "Verification Evidence"
title: "NUI-U8 immutable ordered runtime hierarchy and structural UI order"
description: "Verifies exclusive immutable hierarchy authority, immediate fail-closed completion, atomic ordered Scene projection and retirement, and entity-allocation-independent UI layout, hit testing, and rendering order."
timestamp: 2026-08-05T09:56:53Z
record_id: "6b8cb6a9690f464bb389c89167f2e0f0"
tags: ["nui-u8", "hierarchy", "ordered-projection", "ui-order", "verification"]
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "fc3a0b8ec4c6b348d67d54131be73dcfbc8becaf"
verified_by: "serial focused nextest, public compile-boundary tests, strict changed-crate Clippy, locked example/reference checks, formatting, and diff checks"
---

# Verification

NUI-U8 hierarchy and Scene projection landed at
`4b2146a90dd40146de828b870cf91a2880cec2e1`; the first real UI structural consumer landed at
`fc3a0b8ec4c6b348d67d54131be73dcfbc8becaf`.

- `Parent` and `Children` are immutable Nara-owned components with private construction. They no
  longer implement Bevy `Relationship` or `RelationshipTarget`, so public Bevy commands cannot
  create a second structural writer.
- `HierarchyWriter` owns ordered root/child placement, exact moves and removal, sequential batch
  resolution, complete candidate validation, and an infallible commit tail. Foreign component
  hooks or lifecycle observers reject during preflight before World mutation.
- Owner-controlled insert/discard hooks invalidate the topology generation immediately.
  `completed_topology_generation` also requires a matching completed `HierarchyOrder` and exact
  `HierarchyRoots` snapshot, so raw advanced removal or root divergence is fail-closed.
- Scene spawn, Prefab expansion, export, replacement, Retry, complete-instance retirement, and
  one-entity retirement lower through the same ordered owner. Surviving external descendant
  frontiers are promoted at the retired placement in original depth-first order.
- `HierarchyOrder` publishes a completed depth-first index once per dirty generation. UI layout,
  hit testing, and render queue source order consume that index; no `sibling_order`, query order,
  or `Entity::to_bits()` fallback remains in those paths.

# Result

Passed for the NUI-U8 closure criteria.

- `nara_hierarchy` passed 20 tests; `nara_scene` passed 109; transform/render/sprite consumers
  passed 60; `nara_identity` passed 45; and `nara_ecs` passed 21. All Cargo runs were serial.
- The independent `nara_ui_render` ordered-hierarchy integration test passed and proves layout,
  top-hit selection, and render batch color order remain structural when the requested hierarchy
  order is deliberately opposite the entities' bit ordering.
- The public product-capability compile boundary passed, including fixtures that prove callers
  cannot construct `Parent` or mutate `Children`.
- Strict `-D warnings` Clippy passed for the changed hierarchy/Scene/identity/ECS crates and for
  the UI libraries plus ordered integration test. The locked runtime UI example and all
  reference-game targets checked successfully. `cargo fmt --all -- --check` and diff checks passed.
- Broader all-target strict Clippy remains blocked only by unchanged Rust 1.97 baseline lints in
  `nara_app`, render, and sprite code. This unit did not suppress or expand those lints.

# Evidence

- Immediate invalidation, root divergence, cycle/duplicate/missing placement rejection, stable
  depth-first order, exact move/remove, stale source context, batch atomicity, and deterministic
  frontier promotion are covered by `crates/nara_hierarchy/src/tests.rs`.
- Scene spawn/export/replace/Retry, nested Prefab projection, retirement, identity preflight, and
  hostile raw relation removal are covered by `crates/nara_scene/src/tests.rs` and
  `crates/nara_scene/src/product_transaction_tests.rs`.
- Real cross-domain structural consumption is covered by
  `crates/nara_ui_render/tests/ordered_hierarchy.rs`; transform, render, sprite, examples, and the
  reference game use explicit ordered roots/children.
- Scene hierarchy orchestration and retirement were separated from document spawning into
  `crates/nara_scene/src/runtime_hierarchy.rs`, keeping the Scene module from accumulating another
  lifecycle implementation inside `spawn.rs`.
- Concurrent changes in `crates/nara_ui/src/tests.rs`, `crates/nara_ui_render/src/tests.rs`, ADR
  0101, and `docs/architecture/nara-foundation.md` remained unstaged and were not rewritten.

# Follow-up

Close the short post-U8 decision checkpoint before implementing NUI-U9/U10: admit the minimum
opaque Scene entity plus Prefab projection identity rule, settle input timeline/receipt semantics,
and record the reference UI's data-versus-Rust ownership and state matrix. This checkpoint must not
reopen the completed hierarchy authority or add a UI-local structural order.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u8-project-canonical-order-into-runtime-hierarchy`
- `docs/architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md`
- `docs/architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md`
- `docs/migrations/2026-07-engine-foundation.md#nui-u8-1-ordered-runtime-hierarchy-projection`
- `crates/nara_hierarchy/src/writer.rs`
- `crates/nara_hierarchy/src/relation.rs`
- `crates/nara_scene/src/runtime_hierarchy.rs`
- `crates/nara_ui_render/tests/ordered_hierarchy.rs`

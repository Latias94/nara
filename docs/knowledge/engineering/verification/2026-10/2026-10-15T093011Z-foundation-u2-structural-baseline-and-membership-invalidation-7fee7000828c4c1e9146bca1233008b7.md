---
type: "Verification Evidence"
title: "Foundation U2 structural baseline and membership invalidation"
description: "Two regressions observed before production changes; 149 hierarchy/scene/transform library tests and 23 root integration/privacy tests pass after the ownership correction."
timestamp: 2026-10-15T09:30:11Z
record_id: "7fee7000828c4c1e9146bca1233008b7"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Verification

Behavior changed: hierarchy preparation now uses a materialized outer baseline; every placed entity
has private lifetime invalidation. Existing hierarchy, Scene replacement/retirement, transform
completion, ScenePatch, and public privacy tests were inspected before changes.

# Result

The following serial commands passed with `CARGO_BUILD_JOBS=1`, offline, using the existing target:

- `cargo nextest run -p nara_hierarchy -p nara_scene -p nara_transform --lib --locked --offline --no-fail-fast --test-threads=1`: 149 passed.
- `cargo nextest run -p nara --test scene_patch_transactions --test scene_product_public_surface --locked --offline --test-threads=1`: 23 passed, including the slow trybuild privacy consumer.

The current toolchain reports existing deprecated atomic API/dead-code warnings outside this change.
No full-workspace, GPU, or native Linux/macOS behavior claim is made.

# Evidence

Before production changes, `placement_resolves_after_pending_entity_despawn` failed with success
instead of MissingEntity (Nextest run `30ca5e38-7f9b-45b7-93cc-c99630a53296`).
`despawning_childless_root_invalidates_completion_immediately` failed with Some(1) instead of None
(run `f51df90f-40f2-4e21-87f7-e9ccbb633222`). The final regression asserts immediate invalidation,
normal scheduled error handling, and no republished completion; App::update itself need not return
an error for the schedule's reported error.

Also added the reachable private-membership-removal regression. It verifies a removed membership
cannot regain completion and then miss a subsequent lifetime change. Unchanged-generation scan
coverage passes; no per-static-frame hierarchy scan was introduced.

Read-only review of the actual diff by `/root/mods_plugins` found no concrete remaining correctness
defect in baseline/commit ordering, hook/observer allowlists, Scene rollback, or the unchanged fast path.

# Follow-up

Continue U3 publication availability and U4 effect ownership; do not weaken the existing narrow
persistence lifecycle guards before U8 migrates their consumers. The final whole-plan review remains
required after later interfaces have settled.

# Citations

- `docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md` U2 / R5 / R11
- `docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md`
- `crates/nara_hierarchy/src/{writer,relation,validation,tests}.rs`
- `crates/nara_scene/src/{spawn,runtime_hierarchy}.rs`

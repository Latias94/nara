---
type: "Work Progress"
title: "Editor U9 dependency migration reaches compiled and semantic checkpoints"
description: "Goal active; U9 code and lock migrations are uncommitted, 134 substrate and 336 domain tests plus full workspace check pass; root runtime tests still compiling."
timestamp: 2026-10-16T05:12:04Z
record_id: "aed57c4312214365a9e9651468ee5724"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "b6d00f0"
---

# Summary

Goal-backed editor foundation work is active at U9. All changes below are still uncommitted against source `b6d00f0`; U1-U8 have not started.

# Details

- Active implementation registration: `81c99dbe5a5c4741b7021d932ab0c350`, lane `nara-editor-foundation-implementation-codex-root`.
- Bevy ECS/reflection/macros migrated to 0.20; public glam types aligned to 0.33.12; retained egui family moved to 0.36.2; WGPU public package moved to 30.0.1; Winit remains 0.30.13.
- Lifecycle observers now use typed event patterns, startup Scene SystemParam delegates SystemAccess, and the panic-severity test supplies Bevy's required payload.
- New `crates/nara_reflect/tests/math_reflection.rs` first failed on the old mismatched glam graph (GetTypeRegistration/PartialReflect missing), then passed with the coherent family.
- Original ECS/reflection baseline: 133 passes, nextest `ca9d7ccd-a2e8-436e-8f55-2af7029fc6e9`.
- Migrated ECS/reflection: 134 passes, nextest `aac171bd-083d-4efc-a6ec-2669ebce9718`.
- Migrated App/identity/Scene/transform/render/sprite-render/tooling library tests: 336 passes, nextest `06855c09-1f1e-426c-8584-042b88bf87c0`.
- Workspace all-feature/all-target check passes after updating BevyError::panic test syntax; existing deprecation/dead-code warnings remain.
- Independently locked consumers were updated. Presentation fixture updates required unlocking the coupled js-sys/wasm-bindgen/futures/web-sys family together; no unrelated bulk upgrade was used.
- Reviewer `u9_dependency_correctness_review` identified a confirmed incomplete platform repair: root/reference/presentation/public-prelude locks still retain wgpu-hal 30.0.0, while the desired Vulkan/Metal fixes live in hal 30.0.1. Upgrade the actual implementation package in every WGPU-bearing consumer before claiming U9 complete.
- Root owns Cargo serially. At this capture, exec session `80578` is still compiling the root all-feature library and runtime_instance/scene_component_composition/scene_sprite_serialization/startup_scene_public_surface tests. Do not start another Cargo until it finishes.

# Next Action

Collect the running root test result, update wgpu-hal to 30.0.1 in all affected locks and revalidate. Then finish external renamed-derive/schedule/module consumers, Rust 1.95 minimum check, ordinary Editor tests/native canary, formatting, review disposition and a precise U9 commit.
Only after U9 is committed may U1 (private Editor frontend split) and U2 (pure creation metadata) run as independent workers with disjoint files and root-only Cargo.
No production widget migration, creation-template implementation or Dear ImGui acceptance is yet claimed; do not complete the goal at this checkpoint.

# Citations

- `docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md`
- `docs/architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md`
- `docs/migrations/2026-10-editor-foundation.md`
- `docs/research/2026-10-16-editor-foundation/dependency-cohort.md`

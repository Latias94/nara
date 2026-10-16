---
type: "Verification Evidence"
title: "Editor U9 dependency cohort passes minimum compiler and native editor verification"
description: "Bevy 0.20, glam 0.33 and WGPU/HAL 30.0.1 validated with focused transactions, external consumers, Rust 1.95 and Windows native Editor."
timestamp: 2026-10-16T05:59:47Z
record_id: "447cd6ed651b44cc965962198ec34bce"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "b6d00f0"
---

# Verification

U9 / R11 of the Editor Foundation and Frontends plan. Candidate source is the reviewed dependency migration on top of `b6d00f0`; this record accompanies its local implementation commit.
U1-U8 and Dear ImGui adoption are not part of this claim.

# Result

The selected Bevy ECS/reflection/macro family is 0.20.0, public math is glam 0.33.12, retained egui is 0.36.2, WGPU and HAL are 30.0.1, and Winit remains 0.30.13.
The declared Rust 1.95 minimum passes the complete workspace feature/target graph.
The current Editor renders and completes its generic-open/recovery/browser/last-good/close canary on native Windows.
Linux and macOS native behavior remains unverified; upstream HAL fixes do not substitute for native evidence.

# Evidence

- Baseline ECS/reflection: 133 tests passed, nextest `ca9d7ccd-a2e8-436e-8f55-2af7029fc6e9`, before the migration.
- A new public Vec2/Vec3/Mat3 reflected-struct registration/dynamic round-trip first failed to compile against the old mismatched math family, then passed after alignment. It proves the actual enabled reflection boundary rather than merely counting lockfile packages.
- Migrated ECS/reflection: 134 tests passed, nextest `aac171bd-083d-4efc-a6ec-2669ebce9718`, Rust 1.99.
- App/identity/Scene/transform/render/sprite-render/tooling libraries: 336 tests passed, nextest `06855c09-1f1e-426c-8584-042b88bf87c0`, Rust 1.99.
- Root all-feature library/runtime/Scene selection: 285 behavior tests passed in `52442e46-c809-47e0-81b9-43ac2302cb81` on Rust 1.99. This run was not wholly successful: one diagnostic snapshot differed and the remaining slow privacy test was explicitly stopped before moving to the CI compiler. These are not counted as passes.
- `cargo +1.95.0 check --workspace --all-features --all-targets --locked --offline`: passed in 5m12s after the HAL repair. Existing dead-code warnings remain.
- Root privacy, composition, renamed runner and schedule tests: 16 passed, nextest `b325a690-dc67-4e16-8aa4-ceaf8d7ab0c1`, Rust 1.95. The renamed derive and module-boundary tests also passed in the earlier `e557743c-ec59-425e-90e8-5bf412eb8d30` attempt before its runner failure.
- Final combined root consumer selection after all fixes and formatting: 28 passed, nextest `d6ed0d9c-6f41-4a1d-90cb-996164ad1a1c`, Rust 1.95, 241.998s. This covers scene_component_composition, startup_scene_public_surface, derive_dependency_fixtures, module_consumer_boundary, schedule_extension_contract and runtime_runner_contract with runtime-core and one test thread.
- `cargo +1.95.0 nextest run -p nara-editor --lib --test application_contract --test generic_open_recovery --locked --offline --test-threads=1 --status-level fail --final-status-level fail`: 108 passed, nextest `ace54b7f-d559-420a-a969-c1f1ae6b0aff`, 9.612s execution.
- `NARA_EDITOR_GENERIC_OPEN_CANARY=1 cargo +1.95.0 run -p nara-editor --locked --offline -- tests/fixtures/generic-editor/known-unbound`: passed on native Windows. The guarded temporary project presented Project Manager and Recovery, browsed/opened Scene and Prefab, refreshed generation 1 to 2, retained both documents and retained last-good browser state after a directory-lineage failure. It emitted `nara_editor_generic_open_canary` v2, `result: ok`, then exited successfully.
- `CARGO_TARGET_DIR=target cargo +1.95.0 check --manifest-path reference-game/Cargo.toml --all-features --all-targets --locked --offline`: passed in 3m03s, including the excluded consumer's five migrated observer call sites.
- Root, reference-game and renamed runtime-runner formatting completed with Rust 1.95; `git diff --check` passed. Cargo was serial, with one build job and the shared target directory.

## Review findings and verification fixes

Independent correctness reviewer `u9_dependency_correctness_review` found the public WGPU update had left HAL 30.0.0 in some locks. Root updated the actual implementation package; the reviewer scanned all 12 project locks and confirmed all six WGPU-bearing locks use HAL 30.0.1, with no omissions.

The actual renamed runner diagnostic was `project.runtime-agreement.selected-owner-divergent` for `nara.sprite.components`.
Explorer `u9_bevy_migration_map` traced an existing fixture drift: the checked-in bundle predates `a3651c3`, which added y-sort metadata and renamed the Sprite/Tilemap sort alias.
The fixture bundle was regenerated through its exact recipe and canonical schema exporter, retaining its custom `renamed-root.schema` owner.
The diff contains only two aliases and two `y_sort_anchor` fields; strict agreement was not weakened.
The fixture now preserves diagnostics on failure; temporary regeneration code was removed.

The direct-module privacy probe now imports the private module itself and retains E0603, avoiding a secondary re-export diagnostic note.
The independent advanced-surface probe still compiles the admitted public entry point; all remaining private-construction/import probes remain in place.
The reviewer independently checked these dispositions and the excluded consumer observer migrations with no remaining findings.

# Follow-up

Commit U9 before implementing U1/U2. Root remains the only Cargo/index/format owner.
U1 characterizes and separates application/frontend ownership; U2 introduces complete pure creation templates. Both remain within ADR 0118.
Further external creation/edit/Play/Check/Export acceptance belongs to U3-U8, not this dependency checkpoint.

# Citations

- `docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md`
- `docs/architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md`
- `docs/migrations/2026-10-editor-foundation.md`
- `docs/research/2026-10-16-editor-foundation/dependency-cohort.md`
- `crates/nara_reflect/tests/math_reflection.rs`

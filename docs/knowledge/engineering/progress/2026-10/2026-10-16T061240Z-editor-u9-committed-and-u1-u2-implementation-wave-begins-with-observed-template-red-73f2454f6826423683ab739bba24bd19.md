---
type: "Work Progress"
title: "Editor U9 committed and U1 U2 implementation wave begins with observed template red"
description: "63ffe42 closes the dependency unit; frontend boundary and pure creation metadata workers are active, with root-only Cargo and observed missing-template API failure."
timestamp: 2026-10-16T06:12:40Z
record_id: "73f2454f6826423683ab739bba24bd19"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "63ffe42"
---

# Summary

U9 is committed as `63ffe42`; the worktree was clean immediately after that commit. The full Editor Foundation goal remains active.
U1 and U2 now have fresh independent workers in the shared checkout. No downstream unit is complete yet.

# Details

- Active root registration successor: `56672773517542bc933100efe7d81bc6`, superseding initial `81c99dbe5a5c4741b7021d932ab0c350` in lane `nara-editor-foundation-implementation-codex-root`.
- U9 evidence: `447cd6ed651b44cc965962198ec34bce`; final Rust 1.95 consumer selection passed 28 tests, nextest `d6ed0d9c-6f41-4a1d-90cb-996164ad1a1c`. Workspace and reference-game all-feature/all-target checks, 108 Editor tests, native Windows generic-open canary, formatting and independent correctness review also passed. Linux/macOS native claims remain open.
- `/root/editor_u1_frontend_boundary` owns Editor application/shell/lib/platform, new frontend modules and the directly affected Editor tests. No reference-game `EditorApplication::show` caller was found. The existing 108-test lifecycle baseline and native canary provide characterization before the move. The worker removes toolkit state/drawing and preserves one session/history/Play owner; project generations govern frontend-local drafts.
- `/root/editor_u2_creation_templates` owns reflect/derive and the specific built-in schema owners. It first wrote only `crates/nara_reflect/tests/creation_templates.rs`. Root executed `cargo +1.95.0 nextest run -p nara_reflect --features serde --test creation_templates --locked --offline --test-threads=1`; expected compile-red was observed: eight E0599 errors solely for missing `with_authoring_initial_value` / `authoring_initial_value`. Exec `97808` ended. Root then authorized production implementation.
- U2 will provide complete optional creation data and finite scalar derive literals. Actual built-in consumers are Transform2d, Sprite, Name and Visibility: three owners advance current catalog to generation 2 with generation-1 no-template predecessors, while component wire version stays 1. Camera, Tilemap and UI receive no speculative template expansion. Existing whole-file SerdeShapeLimits cover the new metadata grammar.
- Root owns all Cargo, formatter, index, integration, shared documents and commits. Workers must not run Cargo or modify manifests/locks/shared memory rollups; they must preserve other edits. All Cargo remains serial, one build job, shared target, Rust 1.95 for CI-sensitive diagnostics. No Cargo is running at this capture.
- Root owns subsequent checked-in bundle/fixture migrations. Re-export reference-game and collect-game with their actual recipes when U2 is ready; preserve intentionally degraded historical fixtures. The renamed runtime-runner bundle must retain its custom owner when regenerated.

# Next Action

Wait for U1 and U2 production readiness, run focused root-owned verification without racing the worker's writable files, and request independent review of the actual diffs.
Integrate and commit each completed unit with only its owned files. Do not dispatch U3 before U2 is committed, or U4 before U1 is committed.
Continue U3-U8 in dependency order, including the real Dear ImGui qualification gate before any full frontend promotion. Do not mark the overall goal complete at these partial checkpoints.

# Citations

- `docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md`
- `docs/architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md`
- `docs/knowledge/engineering/verification/2026-10/2026-10-16T055947Z-editor-u9-dependency-cohort-passes-minimum-compiler-and-native-editor-verification-447cd6ed651b44cc965962198ec34bce.md`

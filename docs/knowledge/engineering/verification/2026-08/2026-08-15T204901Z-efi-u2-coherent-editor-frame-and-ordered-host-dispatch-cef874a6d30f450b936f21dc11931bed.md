---
type: "Verification Evidence"
title: "EFI-U2 coherent Editor frame and ordered Host dispatch"
description: "Verifies one immutable post-drive Editor frame, exact ordered Host dispatch, durable stale-action diagnostics, and preserved Play, persistence, and independent consumer behavior."
timestamp: 2026-08-15T20:49:01Z
record_id: "cef874a6d30f450b936f21dc11931bed"
resource: "editor-authoring-loop-v1"
tags: ["editor", "authoring", "efi-u2", "frame", "actions"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "9fa6ea5"
verified_by: "focused-nextest-check-code-review"
---

# Verification

EFI-U2 is implemented by commit `9fa6ea5`. `nara_tooling` now publishes an immutable,
UI-neutral `EditorFrameModel` and exact ordered Editor actions. `EditorProjectSession` exposes only
the frame read, ordered dispatch, and drive/publication seam to external Adapters; former live
mutation and fragmented read paths are private owner helpers.

Each batch targets one exact frame, accepts at most one revision-advancing action per document,
and validates document revision plus runtime generation before mutation. A stale or otherwise
invalid action returns a typed inline rejection and is retained as a bounded Host diagnostic in
the next published frame. Play preparation no longer rebases from its originating revision to a
later document revision.

# Result

- Workspace, Play, persistence, runtime-edit, Apply Changes, dirty-close, and retirement facts are
  published together after Host drive instead of being joined through separate live reads.
- Navigation and one exact mutation can be ordered in one batch; a second mutation for the same
  document rejects without undoing or rebasing the first.
- Save completion and stale-action diagnostics become visible only in the next coherent frame.
- Host implementation responsibilities are private modules; no generic Editor Host trait or
  compatibility layer was added.
- The independent Reference Game Editor journey and host-parity probe consume the new public seam.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_tooling`
  - passed: 24 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run --features "tooling runtime-2d serde" --test workspace_play_runtime`
  - passed: 16 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run --features "tooling runtime-2d serde" --test editor_persistence`
  - passed: 12 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run --features "tooling runtime-2d serde" --test scene_play_mode`
  - passed: 2 tests in the focused integration run.
- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path reference-game/Cargo.toml --features editor --test editor_journey`
  - passed: 1 independent consumer journey.
- `CARGO_BUILD_JOBS=1 cargo check --manifest-path reference-game/Cargo.toml --features host-parity --bin host_parity_probe`
  - passed.
- `CARGO_BUILD_JOBS=1 cargo check --features "tooling runtime-2d serde"`
  - passed.
- `cargo fmt --all -- --check` and `git diff --check`
  - passed.
- Final API and correctness reviews found no remaining P0 or P1. The accepted review issue—stale
  action rejection missing from durable Diagnostics—was fixed and reverified with one focused
  regression.

# Follow-up

Activate EFI-U3 only. Migrate `nara_tooling_egui` to consume one `EditorFrameModel`, bind every
draft to exact document/revision facts, return one ordered action list, and render the fixed
toolbar, canonical Scene Tree, Inspector, and Diagnostics model. Keep the native application and
custom Nara UI parity inactive.

# Citations

- `crates/nara_tooling/src/frame.rs`
- `crates/nara_tooling/src/workspace.rs`
- `src/project_host/runtime/editor.rs`
- `src/project_host/runtime/editor/actions.rs`
- `src/project_host/runtime/editor/model.rs`
- `src/project_host/runtime/editor/persistence.rs`
- `src/project_host/runtime/editor/play.rs`
- `tests/workspace_play_runtime.rs`
- `tests/editor_persistence.rs`
- `tests/scene_play_mode.rs`
- `reference-game/tests/editor_journey.rs`
- Commit `9fa6ea5`

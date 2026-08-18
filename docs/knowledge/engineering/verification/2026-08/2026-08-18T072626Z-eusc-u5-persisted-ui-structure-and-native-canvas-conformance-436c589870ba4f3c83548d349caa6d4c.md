---
type: "Verification Evidence"
title: "EUSC-U5 persisted UI structure and native Canvas conformance"
description: "Verifies one real source-local UI structure trajectory through migration, Editor composition, Save/Reopen, content refresh, Runtime restart, and one exact native Canvas successor presentation."
timestamp: 2026-08-18T07:26:26Z
record_id: "436c589870ba4f3c83548d349caa6d4c"
tags: ["engineering-memory", "verification", "editor", "ui", "prefab"]
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "993a793"
verified_by: "codex-root"
---

# Verification

EUSC-U5 was verified at commit `993a793` from the repository root. Cargo commands ran serially
with `CARGO_BUILD_JOBS=1` and reused the shared target directory.

# Result

One guarded Reference Game project copy now proves the complete structural source trajectory. A
legacy v1 image brush is written through the canonical Prefab writer and migrated by the ordinary
project loader. The Editor opens the migrated source as dirty, creates one Image under the real UI
root, moves it to an exact sibling anchor, deletes the predecessor image node, edits an existing
action, commits one Canvas resize gesture, saves, refreshes immutable project content, reopens the
canonical source, and explicitly restarts Runtime.

The restarted ordinary ECS world contains the allocated source entity under the expected parent
and sibling order, the exact image visual, and the loaded image asset. This proves that the created
node is the image's surviving startup-closure dependency rather than an Editor-only cache.

The existing native Editor canary now appends one frame-lowered Text creation after its Canvas
gesture. The predecessor composite receipt is rejected, the exact successor receipt selects the
allocated node, the authoring revision and presentation generation advance, and history contains
exactly the gesture and structural transaction. No second window, GPU owner, presenter, or platform
matrix was added.

# Evidence

- `$env:CARGO_BUILD_JOBS='1'; cargo nextest run --manifest-path reference-game/Cargo.toml
  --locked --features editor-game-view-canary --test editor_ui_refresh --test-threads=1` - 1 passed
  in 0.548 seconds.
- `$env:CARGO_BUILD_JOBS='1'; cargo build --manifest-path reference-game/Cargo.toml --locked
  --features editor-game-view-canary --bin editor_game_view_canary` - passed.
- `& .\reference-game\target\debug\editor_game_view_canary.exe` - passed on native Windows using
  the existing single-window egui/WGPU Editor execution owner. The bounded command printed schema
  `nara-reference-game.editor-game-view-v2`, selected/preview/committed/structured Editor frames
  `22/24/25/26`, allocated entity `editor_prefab_entity_1`, final fact count `16`, Undo depth `2`,
  runtime generation `1`, resized extent `1120x869`, `129` sprite instances, Runtime UI present,
  followed by `editor_game_view_canary: ok`.
- The product test inspects structured `PrefabDocument`, `EditorUiAuthoringFrame`, persistence
  receipts, content generation, and ordinary Runtime ECS facts. It does not scan source text or use
  the component-only presenter.
- Focused development runs first exposed the missing sole-image closure, stale placement selection,
  and an incorrect pre-delete sibling expectation. Each remained a typed failing assertion and was
  corrected at its owning source/placement boundary before the final passing run.
- Independent read-only correctness review of `83d748f..993a793` returned no findings.
- `cargo fmt --all -- --check`, `git diff --check`, and staged-diff ownership checks passed.

# Follow-up

No successor implementation unit is active. Responsive UI authoring, a Nara UI Editor adapter,
docking, multi-window authority, and live Runtime mutation remain separate trigger-gated work.

# Citations

- [Editor UI Prefab Structural Composition v1 plan](../../../../plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)

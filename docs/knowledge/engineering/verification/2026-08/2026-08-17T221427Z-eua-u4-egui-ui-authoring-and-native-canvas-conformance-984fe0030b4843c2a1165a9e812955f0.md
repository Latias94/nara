---
type: "Verification Evidence"
title: "EUA-U4 egui UI authoring and native Canvas conformance"
description: "Verifies exact Prefab source navigation, typed egui authoring, atomic Canvas gestures, generic document history, and one bounded native Editor canary."
timestamp: 2026-08-17T22:14:27Z
record_id: "984fe0030b4843c2a1165a9e812955f0"
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "155d147"
verified_by: "codex-root"
---

# Verification

EUA-U4 was verified at commit `155d147`. Cargo commands ran serially from the repository root on
Windows, and the native canary was rebuilt and rerun from that exact commit.

# Result

Passed. The egui Adapter now exposes one generic Authoring surface over the existing Scene and UI
Prefab producers. It opens the exact Prefab source, reuses workspace selection and generic
document history, renders ordered UI hierarchy rows and typed descriptor controls, and returns one
revision-bound ordered action batch. Toolkit drafts and Canvas drag state are invalidated by exact
document/revision scope changes and never own source, history, persistence, GPU, or presentation
authority.

The fixed-fit Canvas distinguishes prepared pixels from exact presented input authority. It paints
selection and admitted move/resize handles from Nara UI facts, preserves a gesture while a successor
preview is awaiting its composite receipt, and commits a two-axis canonical patch as one history
record. The private native canary opened the Reference Game UI Prefab, presented 15 Canvas facts,
selected one visible HUD node through the exact receipt, previewed a `[12, 8]` authored move,
committed it with one Undo record, returned to the Scene, and completed the existing Game View
input/pause/step/resize trajectory in the same window and GPU generation.

# Evidence

- `cargo check -p nara_tooling_egui --tests` - passed.
- `cargo check -p nara-editor` - passed after the final Adapter review correction.
- `cargo nextest run -p nara_ui -E 'test(direct_move_lowers_and_revalidates_one_canonical_two_axis_patch) or test(flow_and_percent_geometry_never_expose_direct_handles)' --test-threads=1` - 2 passed.
- `cargo nextest run -p nara_tooling_egui -E 'test(prefab_source_navigation_is_explicit_in_both_directions) or test(ui_hierarchy_selection_uses_the_exact_prefab_document_identity) or test(typed_ui_boolean_control_emits_one_exact_prefab_edit)' --test-threads=1` - 3 passed.
- `cargo nextest run -p nara_tooling prefab_patch_history_is_exact_and_scene_commands_remain_kind_bound --test-threads=1` - 1 passed; an unrelated existing `nara_image` dead-code warning remained.
- `cargo build --manifest-path reference-game/Cargo.toml --features editor-game-view-canary --bin editor_game_view_canary`, followed by `reference-game/target/debug/editor_game_view_canary.exe` - passed at `155d147`; structured output reported UI frames `3 -> 5 -> 6`, 15 Canvas facts, move delta `[12, 8]`, Undo depth 1, and `editor_game_view_canary: ok`.
- `cargo fmt --all` and `git diff --check` - passed; Git reported only repository line-ending notices.

The verification intentionally uses one representative discrete field, one direct gesture, one
managed/percent rejection, and one native launch. It does not create a field, layout, DPI, GPU, or
platform matrix.

# Follow-up

EUA-U5 owns the remaining product-authority loop: Save the edited Prefab, publish a successor
immutable project-content generation, Reopen the same source, explicitly Restart Game View, and
prove that the Runtime UI consumes the same saved action, label, and geometry. It also closes the
plan, ADR/OQ links, implementation ledger, and completed registration.

# Citations

- [Editor UI Prefab Authoring and Canvas v1 plan](../../../../plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
- [EUA-U3 verification](2026-08-17T205912Z-eua-u3-exact-generation-ui-canvas-conformance-45077ece034c42569c6a460a562ab0b1.md)

---
type: "Verification Evidence"
title: "Native owned keyboard and font-policy qualification"
description: "Logical-key editing, shared fallible clipboard, exact keyboard focus and font-policy replacement preserve drafts and original document results."
timestamp: 2026-11-03T20:28:59Z
record_id: "8df745e066a042c79a397610b8615985"
producer_id: "codex-foundation-main"
related_plan: "docs/plans/2026-11-04-owned-inspector-and-editable-text-plan.md"
git_branch: "refactor/foundation"
git_commit: "6119c5c"
---

# Verification

This qualifies the keyboard/clipboard successor working tree based on `6119c5c`, following the
[scalar integration](2026-11-03T192310Z-native-owned-scalar-inspector-and-presented-input-qualification-fc9f002f34ed414e84470bf32a812b8e.md).
Windows 11, Rust 1.95.0, root `target`, `CARGO_BUILD_JOBS=1` and serial Cargo invocations.

| Gate | Result |
| --- | --- |
| Focused field and font-policy controls | 14/14, `33e38a5a-ebb9-4480-bffb-dbae0e90d0ea`; includes three new font-policy replacement controls |
| Ordinary Editor library regression | 302/302, `1f35b9e5-faea-41f2-9145-9b46c0a8bfd4`; 39 existing ignored cases excluded |
| Final selected native trajectories | 9/9, `b5d5a663-80f8-463e-98fb-b7bcd5bcd6b5`; includes both owned Inspector tests and seven unchanged predecessor/Host regressions |
| Correctness Clippy | Editor all targets/all features pass `-D clippy::correctness`; existing warnings and inherited large-error warnings remain |
| Editor build | `cargo build --locked -p nara-editor` passes in 2m16s, including the executable |
| Documentation | Changed Markdown local links and memory validation pass; pre-existing legacy memory warnings remain |
| Independent source review | Held-modifier handoff finding corrected; final limited review has no findings |

The ordinary Editor regression precedes the added held-Ctrl and repeated CursorLeft controls.
Final native qualification reruns both owned Inspector workflows and all seven selected native
regressions after their addition. The prior 730-test shared UI/text cohort retains its earlier
scope; this successor does not repeat the generated-starter build or claim a new shared-API gate.

Commands, executed serially:

```text
cargo +1.95.0 check --locked -p nara-editor --lib
cargo +1.95.0 nextest run --locked -p nara-editor --lib -E 'test(owned_inspector::) | test(field_editor::) | test(owned_panel::tests::)' --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --locked -p nara-editor --lib --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --locked -p nara-editor --all-features --lib --run-ignored only -E 'test(native_owned_inspector_) | test(native_field_draft_focus_and_ime_survive_unrelated_document_revision) | test(native_field_reset_and_remove_round_trip_authored_scene_and_prefab_overrides) | test(native_canvas_widgets_pick_move_resize_and_save_reopen_real_source) | test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive) | test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) | test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts) | test(native_project_fields_use_upstream_ime_composition_and_reject_stale_owner_commit)' --test-threads 1 --no-fail-fast
cargo +1.95.0 clippy --locked -p nara-editor --all-targets --all-features -- -D clippy::correctness
cargo +1.95.0 build --locked -p nara-editor
```

Native runs select `WGPU_BACKEND=dx12`, `NARA_WGPU_FORCE_FALLBACK=0`,
`DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1` and an installed standalone SimHei TrueType fallback through
`NARA_EDITOR_CJK_FONT`. Tests use hidden windows and do not move the user's pointer.

# Result and corrections

The Host shares one clipboard owner between Dear callbacks and the owned Inspector's fallible
port. Unicode Copy/Cut/Paste, failed-write preservation, multiline/oversized-paste refusal and
unchanged local history run through the actual retained drafts. Composition blocks clipboard and
navigation shortcuts. Explicit queued Apply blocks later typing and Paste before another paint.
Tab/Shift+Tab use enabled kernel controls; Enter/Space activate the focused button. Local Undo/Redo
remain distinct from toolbar document history. Save/Reopen preserves original entity records and
the complete unavailable-record manifest.

Text-token lifetime is distinct from keyboard-control ownership. Pointer exit cancels the active
gesture while preserving keyboard focus; repeating press/exit/release cannot Apply. Handoff clears
ordinary keys but preserves held modifiers. The independent review caught `clear_input_keys()`
clearing Ctrl: the receiving Dear text control could interpret Ctrl+A as ordinary input. Both
acquisition paths now use `clear_non_modifier_keys()`. The native counterexample keeps Ctrl held
across owned acquisition and transfer to the predecessor, then verifies the selected replacement
draft without changing source history.

Qualification setup initially expected a dirty draft after replacing text with its original value,
and later relied on the predecessor's Discard button during the held-Ctrl control. The final test
uses a distinct replacement, asserts its unchanged document basis and explicitly Discards through
the owned keyboard path before checking clean enabled-control traversal. These corrections do not
change production draft semantics or relax the traversal, text or history assertions.

The font-policy controls reuse the real frontend and Application harness. Equal ordered face IDs
retain controls, frame identity and dirty input. Reordering replaces controls and refuses old input
tokens while retaining queued drafts through admission refusal. Pending exact submission IDs survive
replacement; one accepted result clears only its field while a stale-document rejection preserves
the other field and unrelated dirty text. No additional draft/result store is introduced.

Enabling the existing `system-clipboard` feature adds `arboard`, `clipboard-win` and `error-code`.
Cargo resolves the root, Reference Game and both Editor-consuming fixture lockfiles without
upgrading existing package versions.

# Limits and continuation

The keyboard trajectory calls the same logical-key method used by production Winit ingestion;
Winit's private platform field prevents constructing a `KeyEvent` here. Its isolated memory
clipboard verifies both Dear callbacks and the owned fallible port without touching the user's
actual clipboard. This is not physical OS key or clipboard delivery qualification. Constructed
IME events do not establish OS event provenance or candidate-window placement.

Complete selected-panel replacement, boolean/structured/reference editing, keyboard-only shell
entry, the clipped live Scene/external-tool visual, virtualization, measured idle cost, native
text accessibility, detached-window/DPI transfer and Linux/macOS remain open under the
[owning unit](../../../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md).
Do not delete the predecessor or close R11/R12 or the foundation Goal at this checkpoint.

The [editing guide](../../../../guides/editable-text.md),
[native procedure](../../../../verification/native-editor-qualification.md) and
[ADR0041](../../../../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md)
own usage, qualification and handoff policy respectively. Raw diagnostics remain in TEMP.

---
type: "Verification Evidence"
title: "EUA-U5 saved Prefab content refresh and Runtime restart conformance"
description: "Verifies receipt-bound Prefab Save, immutable successor project-content publication, bounded generations, stale-source containment, explicit Restart, and one same-source Reference Game trajectory."
timestamp: 2026-08-17T23:30:48Z
record_id: "cf11b0c7d23a44e7b127b41199ee4ee7"
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ed9f1cd"
verified_by: "codex-root"
---

# Verification

EUA-U5 was verified against code commit `ed9f1cd`. Cargo commands ran serially with
`CARGO_BUILD_JOBS=1` on Windows. The verification stayed focused on the four owners changed by the
unit: project-content retention, Editor Host refresh, egui remediation, and the existing native
Editor canary.

# Result

Passed. An accepted Prefab Save receipt now schedules a complete project-content rebuild that
verifies the exact saved path and digest before publication. The current Editor snapshot, any
running Runtime snapshot, and a successor candidate share one budget host with a hard ceiling of
two distinct retained content revisions. Rejection leaves the last-good snapshot and previews
unchanged; the Editor reports `RefreshPending` or typed `Stale` state independently from file
persistence and refuses new Play/Restart until the source is clean and current.

Successful refresh atomically replaces the immutable project snapshot and Scene/UI preview owners
with a non-reused Editor content generation. A running Runtime keeps its old snapshot and becomes
out of date. Only an explicit Restart starts a Runtime from the successor generation. Reopen uses
the same exact Prefab source and schedules refresh without mutating an existing snapshot.

The serial Reference Game trajectory used a guarded project copy and ordinary Editor
Save/Reopen/Play paths. It changed Retry action identity to `reference-game.retry-saved`, resized
the authored control to 216 logical pixels through one continuous gesture, proved one Undo record,
saved and refreshed the project, then observed the same source document/entity, action, label, and
geometry from a freshly restarted real Runtime UI. The observation came from the product Runtime
plugin and did not install the component-only presenter fixture.

The existing native canary was retained as one launch. Its ordering was corrected to finish and
Stop the clean Game trajectory before committing the intentionally unsaved Canvas gesture, because
the completed U5 contract correctly rejects Play while a Prefab is dirty. The final run proved a
nonblank 15-fact Canvas, exact frames `25 -> 27 -> 28`, one Undo record, rejection of the old pick
receipt, and selection recovery from the current receipt.

# Evidence

- `cargo nextest run -p nara --features 'tooling,presentation-ui,presentation-2d,runtime-ui,serde,runtime-2d' editor_prefab_lane_saves_and_reopens_the_exact_source_document --test-threads=1` - 1 passed. The trajectory injected an external post-Save source change, retained the last-good content generation, rejected Play, then Reopened and published generation 2 with a generation high-water mark of 2.
- `cargo nextest run -p nara --features 'serde,runtime-2d' --test project_content_limits shared_budget_host_retains_at_most_two_distinct_content_generations --test-threads=1` - 1 passed. A third distinct generation rejected before publication; releasing the first lease admitted the successor and all charges returned to zero.
- `cargo nextest run -p nara_tooling_egui stale_project_content_emits_one_typed_retry_action --test-threads=1` - 1 passed.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --features editor-game-view-canary --test editor_ui_refresh --test-threads=1` - 1 passed. This is the serial real-project Save-to-Restart product trajectory.
- `cargo build --manifest-path reference-game/Cargo.toml --features editor-game-view-canary --bin editor_game_view_canary`, followed by `reference-game/target/debug/editor_game_view_canary.exe` - passed. Structured output reported Game frames `7 -> 10 -> 11 -> 14`, UI frames `25 -> 27 -> 28`, 15 UI facts, move delta `[12, 8]`, Undo depth 1, 129 sprite instances, Runtime UI present, and `editor_game_view_canary: ok`.
- `cargo check -p nara-editor` - passed after the canary-order correction and removal of the obsolete sub-state completion flag.
- `cargo +1.95.0 fmt --all -- --check`, targeted Rust 1.95 formatting checks for the three changed Reference Game files, and `git diff --cached --check` - passed. The separate Reference Game workspace still has unrelated pre-existing rustfmt drift in seven untouched files; it was not folded into this unit.

The first broad package-filtered `nextest` attempt did not reach a test because Windows could not
write an unrelated `measurement_policy.pdb` while the drive was full. The exact
`project_content_limits` test binary was then selected and passed. No broad release, field,
layout, GPU, DPI, or platform matrix was run.

# Follow-up

EUA-U1 through EUA-U5 are complete. This plan has no active successor. OQ-010 remains open and
continues to gate any Nara UI Editor-shell convergence; node composition, responsive authoring,
data binding, docking, native multi-window, and live Runtime mutation remain explicitly deferred.

# Citations

- [Editor UI Prefab Authoring and Canvas v1 plan](../../../../plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
- [EUA-U4 verification](2026-08-17T221427Z-eua-u4-egui-ui-authoring-and-native-canvas-conformance-984fe0030b4843c2a1165a9e812955f0.md)

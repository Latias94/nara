---
type: "Verification Evidence"
title: "Owned Editor Save custody native observation and measurements"
description: "Cross-frame Save ownership, retained filesystem cleanup, migrated consumers and decision-local owner-drive evidence."
timestamp: 2026-10-20T01:33:07Z
record_id: "9828019f2f934061a1773899ce356ffb"
producer_id: "codex-root"
run_id: "editor-owned-save-20261020"
related_plan: "docs/plans/2026-10-20-owned-editor-save-plan.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "1a90307f8607211f6bc0262f86ddea9a1a392222"
---

# Verification

Baseline `48f1d2b`; Rust 1.95.0, Windows 11, shared target and serialized Cargo with
`CARGO_BUILD_JOBS=1`. The root alone runs Cargo, formats, stages and commits. Parallel lanes
own lower admission, private operation custody, native lifecycle and independent consumers.
Final commits are `78c8ddd` (Scene admission) and `1a90307` (Save/runtime/native/consumer changes).
All final product tests execute the corresponding pre-commit tree; subsequent changes are
documentation/data closure only. No push, PR, new Goal or release qualification is performed.

| Check | Actual result | Nextest run |
| --- | --- | --- |
| `nextest run -p nara_scene --features serde --lib --locked` | Initial and final 138 passed | `cecf0250-9fba-4046-b5f9-0591006004a8` / `50cd1a80-f142-4efc-b3fc-ecdd3c5e6bbd` |
| Minimal-feature public `editor_persistence` + `workspace_play_runtime` | 35 passed, including unchanged opaque/linear trybuild authority test; ignored measurement excluded | `a5d8054b-0ee0-48bd-972c-739c4c6f5f77` |
| Minimal-feature root lib: persistence owner, runtime tests and private background tests, `--test-threads=2 --no-fail-fast` | Final 73 passed, 79 excluded | `aee62a8b-00e8-4666-bf8f-b696bfb96255` |
| Probe-free final root lib + both public binaries; same filters plus binaries, excluding the previously passing trybuild authority case | 107 passed, 81 excluded | `8d2592f9-c7a0-4bb1-b505-0a241d113d57` |
| Same final root/public cohort after truthful rejection/reconciliation routing | 107 passed, 81 excluded | `a8468c87-aaa2-4002-8a70-768fe0e5f8ea` |
| `nara-editor` lib + application_contract / authoring_loop / first_game_workflow / generic_component_composition / generic_open_recovery, `--test-threads=2 --no-fail-fast` | Final 185 passed, 20 ignored native tests excluded | `00626333-1b0e-490b-b43b-c242ad64d901` |
| Same Editor/application cohort after preserving native edit availability and reconciliation recovery | Final 186 passed, 20 ignored native tests excluded | `b6a570e0-e8e0-4da8-946d-438c7da5441f` |
| Windows/DX12 ignored lib: `native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source`, `native_canvas_widgets_pick_move_resize_and_save_reopen_real_source`, `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`, `--test-threads=1 --no-fail-fast` | 3 executed and passed, actual adapter and hidden native windows | `5f78bcf4-4b2d-4e66-b67c-957e1ae770cd` |
| Same final native selection after the final UI/recovery changes | 3 executed and passed, 177 excluded | `8eb65ec7-bb32-4116-bb45-da6fb65dfaf5` |
| Optimized attribution with private probes, later removed | 1 passed; 162 phase rows, warm-ups labelled; excluded from formal owner timings | `eba73d6b-acf3-41c4-a9e1-a69f5030c928` |
| Probe-free optimized measurement, process 1 / 2 / 3 | Each passed; 576 rows per process; exact bytes/revisions/digests and physical close checked | `b1ed0b92-d6aa-44a0-9434-e5118f70ee02` / `9622eeed-857c-43d2-97e1-470a29f0ab7c` / `07f552ae-c62d-4481-8cce-918767dfe2d2` |
| External module-consumer manifest, `-p nara-scene-module-consumer --test component_inventory --locked --test-threads=1` | 3 passed; independently compiled inventory, migration/Check and public Editor Save/Reopen | `7b75794d-748f-4243-9fcf-3a4163081781` |
| Reference-game manifest, `--features editor --test editor_journey --test editor_ui_refresh --locked --test-threads=1 --no-fail-fast` | 2 passed; later source Save/Reopen and compiled UI Prefab content refresh/restart | `1f06b0e7-59a0-4273-8fdc-fed1057e7e05` |
| `cargo +1.95.0 check --workspace --all-features --all-targets --locked` | Initial passed, 1m 03s; final all changes passed, 41.51s | Not a nextest run |
| `cargo +1.95.0 doc -p nara_scene --features serde --no-deps --locked`, `RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links'` | Passed | Not a nextest run |
| `cargo +1.95.0 fmt --all -- --check`, `git diff --check`, local Markdown target checks | Passed; 620 local targets in affected documents | Not a nextest run |

All root minimal-feature commands use `--no-default-features --features
project-workflow,tooling,runtime-ui,presentation-2d --locked`. Ordinary native warnings about
existing feature-dependent helpers are not promoted to broader cleanup in this slice.

# Result

Save admission captures its linear checkpoint and immutable authoring re-admission basis
together. The original Scene/Prefab Host remains in one retained slot while a task borrows it
for encode, preservation validation and publication. The owner polls without waiting for a file
call and recovers the same Host before consuming a receipt. A separate runtime close participant
keeps file cleanup reachable if the session is dropped; TaskPlugin proves worker/capture retirement.
No World, Workspace, full session, history or user callback moves to the task.

# Evidence

Deterministic before/after-publication barriers prove that later edits remain available and dirty,
pre-publication bytes remain old, and post-publication bytes do not constitute acknowledgement
before the worker returns. Drop after publication retains one current-thread quarantine owner
until release; final bytes and directory entries remain correct. This abandonment test uses a
successful publication; it does not inject a failed temporary cleanup and later unblock that
failure after Drop. Existing file-owner cleanup tests and normal-session close/retry cover the
failure path separately.

Task admission/queued cancellation before execution reports Busy without inventing a failed file
replacement. A worker unwind without a known outcome reports RequiresReconcile and marks the
Host uncertain; both Host retry admission and UI permit only that document's Reopen. A real
publication outcome remains authoritative even if logical cancellation wins the task terminal.

Private tests execute pool rejection, queued cancellation, running logical cancellation and actual
worker unwind/Mutex poison. They prove Host reuse before execution, retention through a logical
terminal, preservation of a real published receipt and RequiresReconcile after panic. The real
failed-save cleanup owner is also retained through normal close/retry.

Independent review found and corrected four cross-frame ownership hazards: preview temporarily
lost its Scene identity while the Host was borrowed; legacy Prefab navigation could bypass Busy;
content refresh could replace the authoring runtime and let its retiring participant take an
active operation; and an accepted retry retained an old Failed result, advancing a close prompt
too early. Focused tests cover each correction. Authoring basis capture was moved to request
admission so edits before the first drive cannot change its preservation expectation.

Initial test failures were fixture/API mistakes (private calls, missing selection, authoring-only
edits in a compiled session, an incorrect Ready expectation) and a real failure-atomic frame
regression from unconditionally driving the retained session during replacement preparation.
Tests now use ordinary actions and native patches where appropriate. Replacement queries the
existing immediate read-only persistence view and drives only pending work; the original
`Arc::ptr_eq` failure-atomicity oracle remains intact and passes. New tests are separated by
responsibility into `runtime/tests/owned_save.rs` and private background/native test modules.

Independent source review confirms the corrected custody, preview, navigation and deferred
runtime-replacement boundaries. Independent numerical review recomputes all 108 main groups
and nine worker-phase groups from raw rows and agrees with every summary and rounded report
cell. All retained maximum-drive samples are below the decision-local 8 ms trigger. V2 measures
per-operation maximum owner calls and completion observation separately; no total CPU or native
save latency claim follows. The two standalone HTML reports remain outside docs and pass static
nesting, unique-ID, anchor and local-target checks.

The final UI correction allows ordinary editing during Saving while retaining the independent
Idle gate for another Save/Reopen. Opening still disables editing. The shared field controller
uses this existing authoring policy; the new Workspace-to-frame policy case and the full final
Editor cohort pass. Selected native workflows rerun afterward. They do not inject a held Save
while clicking a native field; deterministic held-work/later-edit behavior is proved at the
session owner and the frontend policy is separately exercised.

Final error-routing and frontend-policy corrections follow the formal optimized runs. They
change only no-outcome rejection/reconciliation paths and native control admission; the measured
successful capture/worker/publication/acknowledgement path remains unchanged. The final root,
Editor, native and workspace checks execute those corrections. Cohorts overlap and their counts
must not be added as a unique combined total.

# Follow-up

Reopen is synchronous. Bound external Play source-receipt refresh and compiled Prefab content
refresh retain their own synchronous costs. No journal, generic operation service, Future
executor, async plugin/codec/panel trait or automatic code-currentness claim is added. The three
native paths are selected development qualification, not the complete release group or evidence
for monitor scanout, OS IME, accessibility, Linux or macOS.

Memory validation passes and derived rollups are regenerated/checked at closure. Existing legacy
record/path warnings remain historical; this slice does not rewrite those records. Unrelated
`vendor/` and `myeasylog.log` remain untracked and untouched.

# Citations

- [Implementation plan](../../../../plans/2026-10-20-owned-editor-save-plan.md)
- [Responsiveness comparison](../../../../benchmarks/2026-10-20-owned-editor-save.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)

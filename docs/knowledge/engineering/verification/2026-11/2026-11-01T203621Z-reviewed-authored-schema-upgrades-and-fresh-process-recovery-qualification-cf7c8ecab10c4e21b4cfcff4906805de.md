---
type: "Verification Evidence"
title: "Reviewed authored Schema upgrades and fresh-process recovery qualification"
description: "Complete CP1/S2 live and independent consumers, source authority, canonical UI construction and bounded Windows qualification."
timestamp: 2026-11-01T20:36:21Z
record_id: "cf7c8ecab10c4e21b4cfcff4906805de"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-final-qualification-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
git_branch: "refactor/foundation"
---

# Verification

Qualification of the complete [CP1/S2 authored-upgrade unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
on Windows 11, Rust 1.95.0, from the implementation based on `ed21b23051342587574be75da017da888270b597`.
The implementation and this record are committed together. Cargo ran serially with jobs1 and
the existing root target. No remote publication, alternate target directory or vendor changes
were part of this unit. The full foundation Goal remains active.

# Result

One public Session/Application operation captures the complete all-profile source inventory and
frozen working inputs, accepts a pure project-owned proposal, prepares and exposes the complete
Review, physically retires affected execution, publishes recovery/Pending/sources/bundle, and
binds all Workspace/history/saved and persistence-lane guards before final Ready. Installation
after the confirmed Ready receipt performs no fallible preparation. Proposal refusal returns
the same original input; safe publication retry retains its publisher and cumulative allowance.

An independent EnemyStats command converts version1 scalar health to version2 identified milli
values, including direct records, additions, complete Sets, Properties and dormant declarations.
It preserves unaffected armor, opaque records and topology. The engine independently validates
complete affected coverage, both schemas and the permitted difference. Successful adoption starts
an explicitly reviewed saved/history checkpoint. Ordinary editing, Undo, Save/Reopen and fixed-v2
Check/Runtime/Play follow; incompatible v1 execution bindings remain a refusal control.

A separately built CLI uses project-workflow-cli without Tooling or Editor. Real subprocesses
inspect and finish all-old, mixed and all-new Pending generations and refuse external, metadata
and recovery conflicts. Already-new leaves are observed without rewriting or fabricated replacement
receipts. Ready-before-ack is admitted through a fresh ordinary Open. Recovery executes the stored
reviewed outputs without rerunning conversion or reconstructing a vanished nominal basis/history.

Compiled Editor source commands and persistence now follow the actual retained Native/Data
authoring basis independently of Runtime execution. Preview uses only installed Native coverage
and preserves exact omissions. Source-safe Apply Changes retains opaque/read-only records and
uses real asset resolution. Root native-only persistence fallbacks and obsolete staged-retention
APIs are removed; legitimate independent Native Scene APIs remain.

The complete source/UI consumer also replaces unpublished UI migration history with canonical
baseline1 and real creation templates. Source recipes lower through declared properties and the
ordinary source construction/history owners. Construction uses the existing total256 operation
allowance; pure field editing remains8, including repeated writes before coalescing. Value/history
limits remain bounded, with mixed-history admission before copying.

# Evidence

These cohorts overlap and have different feature unions. Counts must not be added into a single
workspace pass or attributed to a later source state than the row describes.

| Cohort | Actual result | Run ID / scope |
| --- | --- | --- |
| Final affected Root source/persistence/public behavior | 392 passed, 1015 excluded | `de78b1d3-1a73-428a-93ca-df00b29d387b`; four binaries, no-UI feature union |
| Application/retirement behavior | 35 passed, 285 excluded | `f9ddcc94-31d9-4685-93f5-9a62e245d13b`; before canonical UI cleanup |
| Independent conversion/live/v2 and actual subprocess recovery | 22 passed, only internal child_entry excluded | `a2111ce4-52a2-44f8-8996-59077ad2f759`; includes six subprocess controls with the freshly built CLI |
| Root public consumers | 100 passed, three existing ignored nested-Cargo controls excluded | `f1216b03-2c9e-4c48-b86d-2cd379d6cf08`; eight binaries |
| Independent module consumers | 49 passed, one existing ignored control excluded | `43318acf-cc69-4ead-ba26-67353dbc48bc`; ten binaries |
| Project library and Reflect selected-value admission | 99 passed | `5f9a9b61-d83c-4a39-9abc-8e8fd851f02f` |
| Scene public consumers | 35 passed | `3ba5de4b-dc02-4cb0-9484-f4c4dc05784c`; five binaries |
| Scene documentation tests | 12 passed | Includes three new compile-fail controls |
| Scene/Tooling/UI all-feature libraries | 802 passed, zero skipped | `d703f405-6613-4dbd-b157-eb30db30fc04`; before the last two narrow construction/history budget corrections |
| Final construction/property/history controls | 12 passed, 541 excluded | `3a8d73f1-8fed-4f26-b155-533bd9cd91cd`; includes both final budget corrections |
| Final Reference UI and source-readiness workflows | 2 passed | `5841a0b6-3152-41f2-a44d-0bd99c07ab17`; actual Create/Move/Delete/history/Save/Reopen/content refresh/Restart |
| Selected native Windows/DX12 trajectories | 7 passed, 313 excluded | `f6f3acea-17fc-44b0-9378-e19310f274b6`; after source/UI production integration, before the two narrow budget corrections |
| Final Root/Editor all-feature libraries | 1741 passed, one failed, 41 skipped | `ffcd694c-3bf5-442d-a1f2-df42477356b9`; failure and repair below |
| Repaired cumulative-budget control, Root all-feature union | 1 passed, 1462 excluded | `57e1b3e4-1bd3-4fb1-8935-fa88a1fd98c7` |
| Repaired control, original Root/Editor all-feature union | 1 passed, 1782 excluded | `f79e79ca-7ac3-4f58-8571-ed680ef62173`; same package/feature union as the failed full run |

Correctness Clippy passed for nara, nara-editor, nara_scene, nara_tooling, nara_ui, nara_project
and nara_reflect with all features/all targets and `-D clippy::correctness` (session85367).
Ordinary style/large-result/shared-fixture warnings remain; this is not warning-free evidence.
Its final production behavior is unchanged by the subsequent unused-import removal and test
calibration. The real Reference authoring-bundle producer wrote and checked the canonical bundle;
the generic fixture receives its actual UI owner while preserving every other owner.

Final Root and Scene no-default-feature library checks passed with `--locked`. The final
repaired-control commands were:

```text
cargo +1.95.0 nextest run -p nara --all-features --lib --locked -E 'test(refused_complete_proposal_returns_the_original_capture_for_correction_and_retry)' --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 nextest run -p nara -p nara-editor --all-features --lib --locked -E 'test(refused_complete_proposal_returns_the_original_capture_for_correction_and_retry)' --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 check -p nara --no-default-features --lib --locked
cargo +1.95.0 check -p nara_scene --no-default-features --lib --locked
```

Formatting checks passed for the root and the behavior-languages, module-consumer,
authored-upgrade-recovery and Reference Game workspaces. Reference's first check identified
three wrapping-only differences in the already reviewed UI refresh test; its actual workspace
was formatted and the check passed. No behavior changed. Current architecture/plan/guide/research
and final evidence checks covered21 documents and1256 local file links. The deleted UI evolution
test's stale ledger link now names the current schema test. `git diff --check` passed. Engineering
memory validation passed with existing legacy warnings; immutable historical shards were retained.

## Failure And Repair Lineage

The earlier complete Reference run `7828fdff-22cb-48cf-8232-39afd7da6f16` passed157/159 across32
binaries, including its actual nested desktop build. One test assumed legacy UI migration would
make Open dirty; another omitted the newly retained readiness manifest from its exact file peak.
The replacement uses source-owned canonical UI, actual templates and the complete ordinary
workflow. The exact readiness file maximum is7 (`max(1+6, 1+1)`); other edge/member limits do not
change. Its final focused2 pass is recorded above. This is not a single full159/159 rerun.

Independent final construction review found declaration-free9 writes could coalesce under the
larger structural allowance and mixed >8-field history skipped its precopy gate. Pure-field
lowering now enforces8 before coalescing; valid mixed history crosses its actual bounded cost.
The final12 control includes positive construction/Undo and a valid60KiB template with nine
default writes refused by a50,000-byte history-entry ceiling before source copies.

The Root/Editor full run's only failure was the proposal retry test's hardcoded20,000,000 work
ceiling, which was selected for a smaller built-in Schema union. Full-feature UI templates made
the first semantic refusal reach that ceiling. Production default200,000,000 remains unchanged.
The test now first runs the same fixture with default limits, measures actual cumulative work at
semantic refusal, then repeats with that exact selected ceiling. Semantic refusal must still fit
exactly; the corrected retry must fail WorkLimit without recapture, lost input, disk/history
mutation or an unfenced state. The default-limit corrected proposal must reach Review. This
tests cumulative-budget semantics; it is not an independent absolute-cost accounting benchmark.
The focused rerun is distinct from the original full run, whose failure remains recorded.

Scope-specific independent reviews and fixes are preserved in the linked predecessor shards.
The final test calibration was independently reviewed by Lagrange
`01a198d0-177c-7b00-aaa8-4c0971683262`, with no assigned P1/P2. Planck
`01a198ce-d63c-7050-ae24-6d9d90815e71` checked current plan/ledger/research status and evidence
limits. Both were read-only and are closed. Their static reviews do not replace execution evidence.

## Native Scope And Limits

The seven actual DX12 controls use the required nonfallback adapter and hidden native windows:

- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`
- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`
- `native_ui_canvas_priority_survives_closing_retained_scene`
- `native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab`
- `native_scene_stock_composition_presents_external_effects_picks_edits_undoes_and_retires`

No user pointer was injected. This evidence does not qualify Linux/macOS, monitor scanout, a new
graphical upgrade dialog, the future editable Nara-drawn Inspector, complete rendering0121 or
power-loss durability. Process interruption is distinct from cross-file atomic replacement and
directory-sync guarantees. The complete live fixture explicitly selects finite128-handle upgrade
limits; the ordinary64-handle default refusal and independent Schema policy remain tested.

# Follow-up

The foundation Goal remains active. CP2 extraction must settle instance-root identity and
cross-boundary references before adopting a representation, then prove complete declaration and
occurrence correspondence. Joint motion/executor selection, material generations, full finite-graph
production rendering and editable custom Editor remain separate units. No compatibility reader,
general converter DSL or fabricated Schema succession was added to claim those outcomes.

# Citations

- [Final qualification checkpoint](../../progress/2026-11/2026-11-01T202529Z-native-dx12-and-final-construction-budget-controls-pass-before-full-root-editor-test-run-1e968c417b1940b4a9ac61ad5051d20d.md)
- [Independent process and public-consumer evidence](../../progress/2026-11/2026-11-01T192930Z-fresh-process-recovery-and-independent-source-consumers-pass-while-product-qualification-continues-dfbbb0a8998a433090d17aa6fad56b93.md)
- [Exact predecessor and Task-input corrections](../../progress/2026-11/2026-11-01T190332Z-exact-captured-predecessors-and-retained-task-inputs-pass-final-root-and-application-handoffs-95571bffbb114d509693a5c6cab03243.md)
- [Recovery CLI](../../../../../tests/fixtures/authored-upgrade-recovery/README.md)
- [Native procedure](../../../../verification/native-editor-qualification.md)
- [CP2 source study](../../../../research/2026-11-02-prefab-extraction-and-occurrence-continuity.md)
- TEMP logs: `nara-authored-upgrade-final-all-feature-libraries.log`,
  `nara-authored-upgrade-final-measured-proposal-control.log`,
  `nara-authored-upgrade-final-combined-proposal-control.log`,
  `nara-authored-upgrade-final-correctness-clippy.log`,
  `nara-authored-upgrade-final-native-dx12-tests.log` and the linked predecessor log names.

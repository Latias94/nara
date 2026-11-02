---
type: "Work Progress"
title: "Topology independent review corrections and Root Editor qualification triage"
description: "Work Progress for Topology independent review corrections and Root Editor qualification triage."
timestamp: 2026-11-02T10:15:34Z
record_id: "56612e614a7c4a768d3d322bd4fe6361"
producer_id: "codex-foundation-main"
run_id: "root-binding-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "dcbbb93"
---

# Summary

The active foundation Goal and complete CP2 scope are unchanged. Root binding/source placements
remain an uncommitted prerequisite on `refactor/foundation`, based on local main `dcbbb93`.
The infrastructure study is already reconciled. No complete extraction or publication/recovery
claim is made. All workers are source-ready and closed at this checkpoint.

# Details

An independent boundary review found two P2s after the preceding owner qualification:

- Root-binding lookup constructed `SourceEntity(root.clone())` before lookup admission. The
  private `ScopeNodes` now keeps separate source/addition BTreeMaps and borrows root IDs in both
  binding paths. It preserves namespace and traversal order without a duplicate index. The new
  public allocation executable observes synchronous-thread maximum allocation requests: a
  missing 16 MiB ID with a 1 MiB allowance cannot allocate that ID. Its valid same-spelling
  source/addition control remains separate. This is not an RSS or total-memory measurement.
- Exact source edits skipped all local topology checks when any placement used a projected
  address. The shared layout owner now retains local parent and relative-order constraints,
  using the existing `parent_cycle` and `order_sequences`. It distinguishes Root, direct Entity
  and projected Entity parent categories. Only identity comparisons requiring projected context
  are deferred. Positive unknown-parent editing/Undo/Redo/compile and eight invalid cases cover
  Scene/Prefab and native/Published authoring owners. The final bounded rereview found no
  remaining assigned P1/P2; it was source-only.

The six-binary public cohort passes **43/43**, run `434594f1-7aca-44a6-b944-f9a142aeb046`.
That result includes borrowed lookup and the first partial-layout correction. The final
Root-versus-projected-parent category correction was written afterward and still needs fresh
execution. Scene992/three skips and Tooling/UI274/one skip retain the earlier checkpoint's
coverage and similarly do not qualify this final increment.

Root/Editor all-feature, all-target compilation passed before the last private corrections.
The broad Root/Editor test command, run `9b867669-9e4f-4e6f-9385-5f92749e97f6`, then completed
**2,271 passed / nine failed / 50 skipped, 2,280 run**. Build took 14m33s and execution1032.518s.
Its successful cases include the real Transform2d nested/repeated binding consumer and ordinary
clean-room supervised Play. Four tests abort after their failed assertion while an Application
still retains physical owners; their Windows fail-fast exit is not separate evidence of a
reproduced stack-memory defect. Do not describe this broad run as green.

Disposition of its nine failures:

1. Independent capture Undo compared complete usage despite legitimate Vec capacity changes.
   A bounded worker traced input refund and one conservative closure-copy reservation. The test
   now independently accounts `placements.capacity * size_of::<SceneEntityPlacement>` plus
   entity capacity, compares every usage field exactly and adds repeated capture/source-fact
   controls. No production accounting bug was confirmed; fresh execution is pending.
2. Fake preview toolkit added an entity to an existing Scene using an empty expected placements
   vector. It now builds the transaction from the actual current document declarations.
3. The private-constructor trybuild snapshot still named `SceneDocument`; it now expects the
   actual private `SceneDraft` parameter, retaining the private-access compile refusal.
4-5. Two frontend tests compared a post-creation complete manifest to the old source manifest.
   They retain exact unavailable records/overlay checks, verify actual saved placements and
   compare complete accepted manifests across Save/Reopen.
6. The Application final-exit probe assumed exactly one worker. It now captures actual admitted
   workers, proves all but the gated worker physically retire, then requires all to exit after
   release. Fresh execution is pending; no lifetime check was removed.
7. Empty first repaint unexpectedly reports progress. Its assertion now includes before/after
   content and diagnostics; no behavior fix or changed expectation is claimed yet.
8. Authoring-loop first source edit does not become dirty. Its assertion now exposes actual
   diagnostics; root cause remains unconfirmed.
9. The generic composition consumer had the same full-manifest-after-creation assumption.
   It now checks complete protected entries/overlays and independently expected new placements,
   then retains complete manifest equality for component history and Save/Reopen.

Two new complete public consumers are source-ready:

- `tests/root_bound_scene.rs`: actual Transform codecs/runtime propagation, nested root binding,
  repeated occurrences, rotation/nonuniform scale, mixed sibling order and nine final entities.
  It passes in the broad Root run.
- `tests/fixtures/module-consumer/tests/root_binding.rs`: one independent sequence edits P's b
  between Q's d1/d2, preserves Q, exact Undo/Redo, physical JSON/RON Save/read/admission, repeated
  compile and ordinary Native spawn with eight entities and actual Parent/Children. It has not
  yet compiled or run.

All previously missed PrefabPlacement callers are migrated in the independent module and
Reference Game. The Starter and samples' six JSON sources are updated, plus Two Level's actual
local hierarchy observer. Nineteen persisted source JSON files pass syntax/required-shape
controls; a HEAD comparison checks all six sample/Starter component trees and root declaration
order unchanged. Eight hundred ninety-five affected-document local links exist. These checks
do not replace actual consumer runs. No migration reader or guide was added. Only local main
and the long-lived refactor branch remain in the merged-branch listing.

Current serial Cargo session is **36186**:
`cargo +1.95.0 nextest run -p nara-editor --all-features --locked --test application_contract
--test authoring_loop -E 'test(starter_opens_one_coherent_frame_and_empty_repaint_preserves_document_state)
| test(final_exit_maintenance_outlives_the_workspace_frame)
| test(starter_game_view_preserves_unsaved_source_and_explicit_play_lifecycle)' --test-threads=1
--no-fail-fast --status-level fail --final-status-level fail`.
Its log is `TEMP/nara-root-placement-application-failures.log`. Poll that session before starting
any other Cargo. Last known F: space exceeded110 GiB; no cleanup is needed.

# Next Action

Resolve the remaining Application failures from actual diagnostics, not by weakening assertions.
Execute the final shared-layout regression and all nine corrected failure paths, then final
Scene/Tooling/UI owner qualification as appropriate. Continue independent module, Reference Game,
sample/Starter consumers, real behavior-language/cooked workflows, affected Windows/DX12
structure/conversion/Canvas paths, minimal/no-serde, rustdoc negative and correctness Clippy.
Writers are frozen; global fmt was run immediately before session36186. Independent workspace
format checks still need execution. No unit commit or main merge has been made. Finish the
unit and its required checks, record exact results, commit precisely and fast-forward local main.
Then continue complete extraction correspondence and project publication/recovery. Keep the
foundation Goal active; no push/PR, vendor access or reference-checkout mutation.

# Citations

- [CP2 complete unit](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Previous owner checkpoint](2026-11-02T093049Z-root-binding-source-placement-passes-scene-and-tooling-ui-owners-ee8bc1687d164748b63e3ceb8d7200e2.md)
- [Infrastructure disposition](../../../../research/2026-11-02-infrastructure-comparison-reconciliation.md)

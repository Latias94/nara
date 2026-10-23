---
type: "Verification Evidence"
title: "Common source admission and retained Scene preview qualification"
description: "Three-domain source exclusion, explicit Scene preview selection, trusted source observation and bounded actual native evidence."
timestamp: 2026-10-23T18:04:56Z
record_id: "45ffac37614a4edaa792f08d650f292e"
producer_id: "codex-foundation-integration"
run_id: "foundation-r5c-source-preview-20261024"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "7e68ad2be5f75521631d008bd570b17499c6ebc3"
verified_by: "codex-root; source review 01a169d2-6066-7c52-83b6-46b6e0793b2b; native review 01a169d2-c9b1-7ca3-93f8-969f12a70d03"
---

# Verification

Qualified implementation: `7e68ad2be5f75521631d008bd570b17499c6ebc3`, on top of `bab0705`.
This is the common source-admission and retained-preview unit of R5c, not completion of R5c,
R2, delivery wave 1 or the global foundation rebaseline. The existing same-kind replacement
Open choreography remains in place. No public additive Open or multi-preview Runtime is claimed.
Only documentation changed after the final production-code qualification below.

# Result

- One private source-admission module borrows the existing concrete Scene, Prefab and extension
  maps. Exact project-relative Scene/Prefab sources activate their retained slot/history;
  extension duplicate Open keeps its explicit rejection. Other current-file aliases are rejected
  across all nine source-domain combinations, including later map entries and unavailable Hosts.
- File identity is reopened through each retained parent/target capability, observing atomic Save
  replacement and directory rename without ambient path reconstruction, native-ID comparison,
  a second source registry or weakened capability-session identity. Trusted anchored Prefab
  admission uses its actual retained Prefab-directory capability, even after directory rename.
- Browser preparation completion, predecessor retirement and final publication recheck alias
  exclusion. Failed publication removes only its candidate and restores the document active
  immediately before publication, without changing the retained Scene binding through rollback.
- An immutable frame publishes the last explicitly selected Scene separately from its active
  Inspector. Prefab/extension navigation does not choose another preview. Back to Scene and
  active-Scene re-selection use that binding. Closing its source clears it without map fallback.
- Ordinary Scene presentation validates the bound Scene/revision rather than an active Prefab.
  An inactive Scene may remain displayed, but geometry input still requires explicit activation
  and the original exact composite/image/mapping/device fences. Active UI Prefab Canvas retains
  priority; without either source the old Scene presentation retires.
- Trusted Running/Stop divergence observations resolve the retained runtime source document,
  not the active Inspector revision. Cancellation/physical-retirement source retention remains
  a separate replacement. Supervised Play still uses saved project startup, not preview selection.

# Evidence

## Final Executed Qualification

Windows, Rust 1.95.0, existing root `target`, `CARGO_BUILD_JOBS=1`, serial Cargo commands and one
nextest test thread. Native tests use an actual Windows/DX12 adapter with `WGPU_BACKEND=dx12`.
The independent consumer workspace reuses the root target. Both pre-existing untracked vendor
files remain untouched and uncommitted; no remote push or release publication was performed.

| Scope | Actual result | Nextest run |
|---|---|---|
| Root, Reflect, Scene, Tooling and Editor libraries | 845 passed, 25 ignored | `9871d400-5ed7-47e4-9bec-4337807a93be` |
| Seven selected actual Windows/DX12 trajectories | 7 passed, 199 other cases filtered/ignored | `97ddcae5-46e8-46dc-b845-ef1e46b57a16` |
| Independently compiled Editor panel/document consumers | 30 passed, 2 ignored | `596e8c11-8871-4dee-9122-da0821af847b` |

The final whole-library run includes the retained-directory regression and both final native
fixture corrections. All seven native trajectories executed explicitly:

- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`
- `native_ui_canvas_priority_survives_closing_retained_scene`
- `native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes`
- `native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source`
- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`

These are selected hidden-window native authoring/presentation/input/retirement evidence, not
monitor scanout, accessibility, Linux/macOS or the complete release matrix. Ordinary ignored
tests are not counted as passes. The 30-consumer cohort does not demonstrate additive same-kind
Open; its public prepared-edit workflow retains its previously recorded scope.

```powershell
cargo +1.95.0 nextest run -p nara -p nara_reflect -p nara_scene -p nara_tooling -p nara-editor --lib --locked --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-editor-panel-module-consumer --tests --locked --test-threads 1 --no-fail-fast
cargo +1.95.0 clippy -p nara -p nara_tooling -p nara-editor --lib --locked -- -D clippy::correctness
cargo +1.95.0 fmt --all -- --check
```

Clippy correctness and formatting pass. Clippy is not warning-free: root, Tooling and Editor
report 211, 18 and 68 non-correctness warnings respectively, with other dependency warnings.
No automatic Clippy fixes were applied. `git diff --check` and 130 local document link targets
pass before source commit. Raw final logs remain in the local temporary directory under
`nara-r5c-source-preview-{libraries,native,consumers,clippy}-final.log`.

## Red Evidence And Fixture Corrections

- `49c14ba2-2bec-4c0d-bc33-06060b1c287f` reproduces the Scene file-alias defect: Open returned
  `AwaitingClose` instead of rejecting an alias of the unsaved open source. Its initial anchored
  test lacked required entity selection and is not proof of a locator-scope defect; the fixture
  was corrected before the final passing Browser/anchor reuse test.
- `ca76f435-f3d3-4449-b3a9-b8476afcec68` reproduces explicit Scene selection retaining the old
  preview binding. `c7df359b-1446-4847-a232-cb5f5e4a3da7` reproduces trusted Play divergence being
  hidden by active Prefab navigation. Two alias-test aborts in that latter run were a fixture
  authority error: independently opening roots created foreign capability sessions. The fixture
  now retains one root and uses `.share()`; production identity equality was not weakened.
- Independent review found that an early trusted anchored check reconstructed the configured
  directory instead of using the retained Prefab root. `74ca4f70-0537-4bb3-9e0a-c53418eccec2`
  reproduces rejection after rename; `9cc9fb2b-7cc9-4928-ad48-5b7159afc943` passes after the fix,
  which is also included in the final 845-case run.
- The initial seven-native run passed five and failed the new two because their helper omitted
  anchor selection. After ordered SetActiveDocument/SelectEntity/OpenPrefabSource dispatch and
  exact result checks, the ordinary Scene case passed. The Canvas case then exposed another
  fixture protocol omission: a second Close was rejected while the first result was retained.
  The helper now verifies admitted Close and acknowledges its exact result. Both new cases pass
  in `72d1ef93-4ede-421b-b942-dbbf1224ad58` and the final seven-case run; no production input or
  persistence fence was relaxed to make them pass.

## Independent Review And Design Limits

Source correctness review `01a169d2-6066-7c52-83b6-46b6e0793b2b` found the trusted retained-directory
regression above. Native correctness review `01a169d2-c9b1-7ca3-93f8-969f12a70d03` identified the
unselected-anchor fixture and no other confirmed production issue in its scope. Five static
simplification lanes covered reuse, quality, efficiency, test value and comments:
`01a169de-6f0a-7072-a61a-d39b4955155d`, `01a169de-d7ff-7b81-9d15-6b8c01db58cc`,
`01a169df-42ef-78f3-8ed5-419d314e3c65`, `01a169df-d39b-7221-91a3-aa35b692f3ff` and
`01a169e0-43c6-70a0-84f5-5b828be0d85f`. Two private narrative comments were removed; proof paths
and trust-boundary checks remain. A possible future `token` naming improvement is not an adopted
authority change. No dedicated CE full, merge or release receipt is claimed. All handles closed.

Next-unit architecture review `01a169e8-bf07-7ae1-abc0-a7acc5595ff3` was read-only and closed.
It confirms that additive Open must delete unrelated predecessor admission, not just skip Close;
the anchored Prefab gate affects both execution lanes, and trusted multi-Prefab admission needs
multiple saved-content obligations first. It distinguishes conservative retirement waiting from
an unsafe proof that Play has no source. These are scoped design findings, not newly executed
multi-document public/native evidence.

That review also rules out a proposed false positive: ADR 0047 deliberately admits an immutable
observed source snapshot. Reopening a current target for alias exclusion does not certify latest
content or replace the original identity/digest baseline. Later Save remains DetectOnly. No
silent freshness guarantee or unmeasured stronger filesystem-CAS claim was introduced.

# Follow-up

The global foundation goal remains active, without a new token budget or scope reduction.

1. Normalize refresh source keys and retain all pending/failed Prefab digest obligations. A later
   save may supersede its own source, not erase another source's obligation. Qualify this before
   widening trusted anchored admission beyond one Prefab.
2. Delete the same-kind predecessor-close/admission choreography and displaced Tooling/UI states.
   Preserve explicit Close decisions, failed-Save retry, cancellation, index/alias admission and
   physical Host return. Editing unrelated A must not stale B's Open.
3. Retain exact trusted Play source through cancellation and retirement before claiming source-local
   unrelated Close isolation there. Keep global Scene Reopen-versus-Play exclusion meanwhile.
4. Extend the independent public consumer to two Scenes and two Prefabs, non-active query/prepared
   edits/history/Save/Reopen and exact Close; qualify the relevant native adapters. R5c projection,
   durable property identity/history policy and all remaining foundation waves remain incomplete.

# Citations

- [Owning design and dependency order](../../../../architecture/editor-document-persistence-custody-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [ADR 0111](../../../../architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md)
- [ADR 0034](../../../../architecture/adr/0034-editor-play-mode-world-boundary.md)
- [Native procedure](../../../../verification/native-editor-qualification.md)
- [Source admission](../../../../../src/project_host/runtime/editor/source_admission.rs)
- [Owner conformance](../../../../../src/project_host/runtime/editor/persistence/tests.rs)
- [Browser and anchored source tests](../../../../../src/project_host/runtime/tests/document_sources.rs)
- [Actual native trajectories](../../../../../editor/src/platform/imgui/scene.rs)

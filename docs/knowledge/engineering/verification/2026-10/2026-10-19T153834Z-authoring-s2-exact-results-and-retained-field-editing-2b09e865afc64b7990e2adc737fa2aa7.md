---
type: "Verification Evidence"
title: "Authoring S2 exact results and retained field editing"
description: "Exact frontend dispatch correlation, retained Inspector drafts and public panel/native qualification within S2."
timestamp: 2026-10-19T15:38:34Z
record_id: "2b09e865afc64b7990e2adc737fa2aa7"
producer_id: "codex-root"
run_id: "authoring-s2-20261019"
related_plan: "docs/architecture/authoring-interface-ergonomics-design.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "cd37a35"
---

# Verification

S2 / E3-E4 of the [authoring design](../../../../architecture/authoring-interface-ergonomics-design.md)
corrects N03 without reopening U1-U15. S1 remains complete; S3 inventory/constraints, tile tools
and G1-G9 are outside this delivery. No new Goal, push, PR or release was started.

Baseline is `56277a9` on `refactor/foundation-contract-hardening`. Root integrated three disjoint
implementation lanes and independent API, correctness and maintainability reviews. The tested
working-tree scope is Scene admission identity, tooling document projection, Editor/frontend/native
dispatch, field controls, affected tests and the independently compiled panel fixture. No lockfile
or dependency update is included. Unrelated `myeasylog.log` and `vendor/` remain untouched.

The final behavior source is `cd37a35`, after owner projection `b749bfb`. Subsequent report and
ledger updates are documentation-only and do not alter the tested behavior.

# Result

Windows, Rust 1.95.0, locked dependency graphs, shared `target`, `CARGO_BUILD_JOBS=1`. Cargo commands
were serialized. Native selections used `WGPU_BACKEND=dx12` and required the actual adapter with
`DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.

| Command / cohort | Actual result | Nextest run |
| --- | --- | --- |
| `cargo +1.95.0 nextest run -p nara_tooling --lib --locked --test-threads=1 --no-fail-fast` | 49 passed | `5de34e29-af28-466b-b40b-a46dfd829dbe` |
| `cargo +1.95.0 nextest run -p nara-editor --lib --test application_contract --locked --test-threads=1 --no-fail-fast` | 171 passed across two binaries; 18 ignored cases excluded | `a95a20f3-9624-4154-aefe-46972ac6f73d` |
| Editor lib, `-E 'test(frontend::imgui::interaction_tests::) \| test(frontend::imgui::field_editor::)'` | Final 21 passed after user-facing rejection message cleanup; overlaps the full cohort | `eda5c130-262f-48b8-80b2-5c63366a6984` |
| Independent panel workspace: `cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-editor-panel-module-consumer --locked --test imgui_contribution --test-threads=1 --no-fail-fast` | 2 passed | `083f568a-d5fc-4347-a442-31e266723b34` |
| Same external package, `--test native_runner --run-ignored only --test-threads=1` | Public native runner executed and passed; 1 test | `2110305b-a218-4a98-9dcc-32c8b9552136` |
| Editor lib, ignored selection: new field focus/IME, field Reset/Remove, recursive Inspector, view lifecycle and composed panel GPU reconstruction | All 5 executed and passed; 23.253s | `0a18ac8d-ea1f-43b9-bccd-fbf04e7d69e1` |
| Final Editor lib ignored selection: field focus/IME, recursive Inspector, resume cadence and Game input/resize/Step | All 4 executed and passed; 15.756s; first two requalify the final recursive-control simplification | `101701b6-44e7-42eb-801d-7b3986f49d6f` |
| `cargo +1.95.0 check -p nara-editor --all-targets --locked` | Passed; existing unrelated warnings remain | Not a nextest run |
| Root and independent panel package `cargo +1.95.0 fmt ... -- --check`; `git diff --check` | Passed after final Rust changes | Not a nextest run |

The Editor selections use `cargo +1.95.0 nextest run -p nara-editor --lib --locked
--test-threads=1 --no-fail-fast`, with `--run-ignored only` for native cases and the actual test
names below. The external workspace explicitly reuses the root target via `CARGO_TARGET_DIR`.

Native test selectors:

- `native_field_draft_focus_and_ime_survive_unrelated_document_revision`
- `native_field_reset_and_remove_round_trip_authored_scene_and_prefab_overrides`
- `native_recursive_inspector_drafts_round_trip_scene_and_prefab`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`
- `native_composed_panels_keep_independent_drafts_through_gpu_reconstruction`
- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`

# Evidence

- `AuthoringSchemaBasis` retains the owner's immutable admission through an opaque process-local
  Arc identity. Save/edit preserve it; independent same-path admissions do not alias. Reopen
  preserves schema admission but changes `revision.source_id()`. Scene and Prefab projections
  expose those facts without mutation authority; native sessions project no data-only admission.
- `submit` admits a finite set of owned actions and returns an opaque ID. A move-only frontend
  candidate carries its immutable private ledger. Foreign/superseded candidates cannot drive or
  settle the current candidate. Application filtering composes original action slots with closing
  slots; callbacks occur after drive and actual lifetime reconciliation. Helper results have one
  private recipient and never broadcast to the custom panel callback.
- Seven correlation tests cover two panels/multiple actions, same-frame stale/foreign candidates,
  unrelated direct batches, exact stale-frame rejection, closing holes, retargeted recipients,
  document removal during drive, and queue admission before ID/action publication. Retirement
  drops an old recipient; no promise is made to callback an already retired panel.
- Actual field controls retain u8 `256`, 8,256-character invalid input, unfinished recursive children,
  collapsed/unselected drafts and rejected submissions. Conflict requires deliberate review/reapply.
  Reset retry preserves Remove rather than becoming Set; a superseded boolean can retry its
  retained value without toggling back. Removed fields/components remain copyable/discardable.
- The draft budget counts dirty/pending entries, not clean visible controls. A real 65-field
  Inspector test scrolls to and edits field 65; a focused state test proves 64 retained edits block
  new input without evicting an old edit. Source/shape replacement keeps the original input and
  interpretation basis incompatible rather than rebasing it.
- The independent panel fixture calls `edit_field` without private engine hooks. Its custom field
  writer submits two writes from the same basis: the first succeeds, the second returns exact
  `Rejected(StaleDocumentFact)`; only one history entry and the first value remain. Another test
  distinguishes two custom submissions in one paint. The public native runner requires the exact
  selection callback before acknowledging the next projection, then exits normally and drops its
  panel once. Its artifact directory under the operating system temporary directory is
  `nara-external-native-panel-90304-1792424421133300100`.
- Native focus/IME drives actual field widgets in a hidden owned Winit window through the
  registered backend and GPU presentation. It records active InputText and native text-owner IDs,
  sends Chinese preedit, accepts an unrelated field mutation, confirms unchanged focus/preedit,
  commits composition and explicitly reapplies the conflicting draft through normal history.
- Private field controllers and submission routing are extracted from the old Inspector/module
  paths. Recursive row IDs remain local and stable. Obsolete test-only conversion code, unused
  recursive error state and the never-delivered public TargetRetired result were removed; tests
  now exercise the actual conversion path. Typed owner results remain unchanged.
- Independent reviews found and resolved missing-field recovery, Reset intent loss, clean-controller
  eviction, clean-field capacity starvation and boolean retry gaps. The final API review reports
  no findings. Maintainability review removed duplicate parsing/error ownership; it did not propose
  another framework or crate layer.
- Eight affected Markdown documents passed fence and 634 local-link checks. Both HTML reports
  outside the repository passed nesting, unique-ID and local/internal-link checks; no browser
  visual inspection is claimed. Engineering memory validates with pre-existing legacy/path
  advisories; its derived views are current.

Failed intermediate runs were diagnostic evidence, not passing qualification. Stable widget identity
exposed an existing Add Component popup that never closed explicitly: the old revision-bearing ID
had destroyed it as a side effect. Real clicks initially only dismissed that popup. Add Component
and Parent choices now close explicitly; ordinary input, recursive editing and Save/Reopen pass.
The stale-candidate and same-basis external tests initially expected the wrong result variant and
now assert exact `BatchRejected(StaleFrame)` and `Rejected(StaleDocumentFact)` respectively.
An initial Reset assertion confused effective default value with raw authored presence; the test
now checks Defaulted, the default value and the exact RemoveField operation, while existing
Save/Reopen regressions assert raw key omission. An early native startup probe needed stable
layout frames before accessing real controls. Temporary traces have been removed.

# Follow-up

S3 remains proposed: finite component constraints/safe error facts, then one component inventory
and an external version-evolution workflow. No generic async panel trait, operation bus, inline
Inspector plugin API, multi-document owner or automatic code-currentness guarantee is added.

Native evidence is Windows/DX12 only. Winit events are constructed for the owned Context; this
does not establish OS IME candidate-window placement or delivery, actual global typing/pointer
input, accessibility, monitor scanout or Linux/macOS qualification. The separate runner briefly
opens its ordinary visible window with isolated child preferences; it injects no OS input.
No full workspace suite, release/export cycle or performance percentile is claimed. Dirty structured
drafts still require per-paint traversal/cloning; measure a real large-container workflow before
adding caching or another controller abstraction. Existing unrelated warnings remain.

# Citations

- [Accepted field/result contract](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Owner identity projection](../../../../../crates/nara_scene/src/authoring_decode.rs)
- [Field controller and public helper](../../../../../editor/src/frontend/imgui/field_editor.rs)
- [Exact result regressions](../../../../../editor/src/frontend/imgui/submission_tests.rs)
- [Real field controls](../../../../../editor/src/frontend/imgui/interaction_tests/field_drafts.rs)
- [Native field focus/IME](../../../../../editor/src/platform/imgui/authoring_workflow_tests/field_drafts.rs)
- [External panel consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/imgui_contribution.rs)
- [Panel author guide](../../../../guides/editor-panels.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)

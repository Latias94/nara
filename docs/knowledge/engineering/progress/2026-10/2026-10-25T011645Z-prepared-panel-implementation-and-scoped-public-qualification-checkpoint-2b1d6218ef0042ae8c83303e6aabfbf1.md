---
type: "Work Progress"
title: "Prepared panel implementation and scoped public qualification checkpoint"
description: "Read queries are committed; real registered panels now prepare and consume source candidates with serial owner, public, native and static evidence; full review remains open."
timestamp: 2026-10-25T01:16:45Z
record_id: "2b1d6218ef0042ae8c83303e6aabfbf1"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "763840994a7114716b54e17bf307d50caad49dc3"
---

# Summary

The bounded read/query unit is locally committed as `7638409`
(`feat(tooling)!: expose bounded source authoring queries`). Its actual nine-lens CE review,
confirmed multi-row proof correction and targeted recheck are recorded in the preceding
[qualification](../../verification/2026-10/2026-10-25T002933Z-bounded-authoring-query-complete-review-and-aggregate-budget-qualification-893b5a3ffa104ec8bc7c7413c12e0c5d.md).
Only the 30 inspected owned paths entered that commit; `vendor/` remained untouched/untracked.

The next R2 unit is implemented but uncommitted: real registered panels can borrow finite
preparation access, retain actual source-owner candidates across paints, then consume them through
the existing ordered action/result route. Independently compiled Scene/Prefab workflows and scoped
serial tests pass. Complete simplification/review and preparation-cost attribution remain open;
this is not complete R2, a complete delivery wave or foundation Goal completion.

# Details

## Responsibility-Owned Changes

- ADR 0118 adopts finite, read-only paint preparation and consuming commands; the
  [design](../../../../architecture/editor-prepared-panel-design.md) compares mutable paint access,
  all-action candidate transport and the selected borrowed-preparation/ordered-commit split.
- Workspace reuses its exact document-query resolver through `validate_document_query_basis`.
  Session adds lifecycle-aware `validate_authoring_query_basis` and an exact `QueryBasis` error.
  The new owner test was observed failing with missing-method E0599 before implementation.
- Application splits read-only preparation from its mutable panel workspace. Context exposes
  existing patch/property/construction preparation, not a public Session or mutable document.
  No unsafe borrow split, candidate registry, alternate inverse or second history is introduced.
- `CommitAuthoringEdit` and `CommitAuthoringConstruction` consume existing candidates in Session.
  `EditorAction`/`EditorActionBatch` are move-only; current consumers use moved commands or clone
  only plain request data. Successful `AuthoringConstruction` outcomes carry the actual committed
  owner mapping. Only historical receipts/maps gain Clone/PartialEq, with exact scope affinity.
- Typed panel enqueue returns the original candidate on capacity refusal, without an entry or
  callback. Existing submission ID/origin/incarnation/filter/supersede paths remain in use.
- Actual independently compiled panels now discover component/root IDs, read an integer,
  compute a replacement, reject a stale Schema basis, prepare/cancel/retain, and submit two
  same-basis candidates: one succeeds and the other rejects stale. One Undo/Redo and Save/Reopen
  preserve the computed value on non-active Scene/Prefab sources. Host-side preparation is not
  substituted for the panel methods.
- A second real panel constructs two locally referenced entities, retains the fixed preview IDs
  across paints and consumes that candidate. Actual result mappings survive Undo/Redo and
  Save/Reopen with exact stored references. Neither mapping nor success is inferred from selection.

Read-only seam investigator `01a17075-6393-75d3-b334-7de287d28739`, transport worker
`01a1707f-1855-70b1-a398-9d6e901eda44`, and panel worker
`01a1707f-1e58-71c3-97dc-e514bf478443` completed and closed. The workers made disjoint changes
and ran no Cargo/formatter/index/commit operations. The parent inspected and integrated the
actual diffs, added exact basis/mixed-frame and independent consumer evidence, and owned all
serial qualification. Those implementation workers are not independent code-review receipts.

## Executed Evidence

Rust 1.95.0, root `target`, `CARGO_BUILD_JOBS=1`, no concurrent Cargo invocations:

| Scope | Actual result |
| --- | --- |
| Scene/Tooling all features, `dbda13c7-ced9-4cf1-9595-a428b5c2bd1c` | 404 passed, 4 skipped |
| Final root/Editor all-feature libraries, `1ff85acb-fafb-4470-827f-90dff0096864` | 588 passed, 28 skipped |
| Focused submission cohort, `82039b45-c01f-4876-931a-6f11ae4964ad` | 13 passed, 603 excluded |
| Final independent consumer, `571d5000-f492-4699-b0f2-901fedd39238` | 4 complete workflows passed |
| Actual selected Windows/DX12, `39903e99-3ad4-46ef-8e4d-3caaa9fc8231` | 3 passed, 613 excluded |
| Feature-off UI guard, `b66f9667-8ea3-4254-9f8d-9dc0ef44b5c1` | 1 passed |

The native selection executes ordinary structure widgets/history/Save/Reopen, Canvas
picking/move/resize/persistence and retained Scene/Prefab navigation/bound Close. It uses hidden
native windows, real DX12 presentation and one test thread. This is affected-existing-path
regression evidence, not a native prepared-panel widget or complete platform qualification.
The new panel workflow uses actual Dear ImGui context/registration and public Application routing
without a native window. No browser, Linux/macOS native, IME/accessibility or latency claim follows.

Workspace all-feature/all-target check passed after the move-only compile corrections. Final
correctness Clippy over Scene/Tooling/root/Editor, all features/all targets, passes. The portable
root compile passes for `wasm32-unknown-unknown` with
`runtime-2d,presentation-2d,presentation-ui,tooling,serde` and no default features. Both workspace
format checks pass. Existing unused/non-correctness warnings remain; no warning-clean claim.
Logs are retained as `nara-r2-panel-*.log` in the per-user temporary directory.

The final 17-Rust-path manifest is `nara-r2-panel-source.json` in that directory, SHA-256
`f1a97c89b1bb9c457752ca68e8cf2e8fa98fb4f279e0051cf36ad418a7420b8d`.
Its explicit recipe differs from the earlier read-unit recipe: hash the compact, sorted-key JSON
array of sorted path/per-file-SHA entries; each file normalizes CRLF to LF. No subsequent Rust
source changed. The HEAD is the baseline, not a commit of this prepared-panel implementation.

## Reproduced Fixture Failure And Correction

The first broad root/Editor run passed 505 cases before one new non-active preparation fixture
aborted. Exact single-test reproduction `fe7dbd2c-a48b-4dd2-a2ce-6cbc9e126b01` exposed the first
failure: the fixture assumed anchored Open had changed the active document. A diagnostic follow-up
`1ad8f491-8194-4c0d-9bd4-9c9077240e53` showed the real owner rejection
`project.editor.prefab-source-not-selected`. The existing contextual navigation requires a selected
anchor at `src/project_host/runtime/editor/persistence.rs:565`; it is not generic named-source Open.

The caller corrected test setup to select the anchor before that existing navigation action,
assert the actual report, and only then capture the non-active preparation baseline. An initial
asynchronous-Open hypothesis was rejected; its unnecessary wait was removed. No production
admission was weakened and no non-active/unchanged-state assertion was deleted. All 13 targeted
and all 588 final library cases pass. The secondary Windows `0xc0000409` panic-exit report was not
independently diagnosed and is not evidence of a discovered or fixed production buffer overflow.

Ergonomic follow-up: the contextual `OpenPrefabSource` name and explicit entity argument can
suggest selection-independent access even though it has a selected-anchor fence. Keep that
distinction visible when evaluating R1 navigation ergonomics; Browser named-source Open already
has a separate contract. Do not silently remove the contextual fence to make a test pass.

# Next Action

No agent or Cargo invocation from this checkpoint remains live; exec handles `45932`, `58756`,
`74991` and `76611` are terminal success. Earlier failed test/compile handles are terminal too.
Do not restart completed qualification merely because a later session resumes.

Run the actual simplification pipeline on this diff, protecting the owner/public proof tests,
then a complete CE code review based on `7638409`, including new owned files and excluding vendor.
The read-unit receipt does not review this successor. Address justified findings, reverify affected
scope, capture preparation-cost/ergonomic comparison and a coherent local Conventional Commit.
No push, PR or additional migration guide. Full R2 and dependency-ordered R1/R3-R12 work remain
active; do not shrink the Goal to this slice.

Retain the earlier Reflect `get_path` diagnostic-prefix allocation and populated/degraded
classification-index maintenance measurement follow-ups. Neither is resolved by panel transport.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Prepared-panel design](../../../../architecture/editor-prepared-panel-design.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Panel context](../../../../../editor/src/frontend/imgui/workspace.rs)
- [Submission regressions](../../../../../editor/src/frontend/imgui/submission_tests.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)

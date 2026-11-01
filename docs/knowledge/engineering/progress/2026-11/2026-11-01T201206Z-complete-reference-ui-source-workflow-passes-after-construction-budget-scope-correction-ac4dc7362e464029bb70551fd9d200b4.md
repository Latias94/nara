---
type: "Work Progress"
title: "Complete Reference UI source workflow passes after construction budget scope correction"
description: "Actual bundle production and exact UI owner sync pass; complete Reference UI and readiness2 pass after separating full construction from standalone eight-field batches."
timestamp: 2026-11-01T20:12:06Z
record_id: "ac4dc7362e464029bb70551fd9d200b4"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-construction-budgets-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
supersedes: "2b5140bf02e3460a9a8e072ae65d3514"
---

# Summary

The full foundation Goal remains active; authored-upgrade/source-authority WIP remains uncommitted
at ed21b230 on refactor/foundation. Actual canonical bundle production and the complete Reference
UI source workflow now pass. Current native verification is running. Construction-budget tests,
independent review, final owning/lint/minimal/consumer checks and complete-unit evidence/commit
remain; no push/PR or ff main has occurred.

# Details

Real Reference authoring_bundle --write (session66050) succeeds after3m17 build. The resulting
UI owner has generation1, component versions1, no predecessor and five actual initial templates.
Parent copied only that produced owner into generic-editor/image-import/schema/authoring.bundle.json,
then ran root target/debug/authoring_bundle.exe --check successfully. Structural checks confirm
every other owner in both bundles is identical to HEAD. No manual template JSON was invented.

The first targeted rerun failed compilation only because expanded test diagnostics borrowed a
partially moved report. Parent fixes the error arm to ref other. Subsequent run
`47d8be5a-1bcd-4b7c-8adb-42d6367ffaf9` passes readiness but still refuses Create; the improved static
diagnostic run `eb8b921d-b083-458d-acfa-f542f49ab75a` identifies exactly the eight-field write limit,
not missing templates, owner or payload mismatch. Windows aborts follow the failed assertion and
fixture teardown; they do not establish a separate stack-overflow diagnosis.

The actual complete UI recipe requires more initialization properties than a standalone small
field-edit batch. Tuning default values merely to fit8 would conceal that scope error. Parent
therefore makes existing256 MAX_AUTHORING_EDIT_OPERATIONS the shared count for construction entity
declarations, component additions and property writes. ConstructionDraft::admit_declaration sums
all three; push_write uses it. Private property::lower_construction uses the same property kernel
with the structural count, while ordinary lower still8. Mixed structure's field parse and inverse
validation use256 under the existing complete-patch operation bound; pure field patches retain8.
Both paths keep cumulative256 value nodes/64KiB logical bytes and original inverse/replay/history
limits. Tooling recipe precopy count follows the structural scope, with actual draft admission as
the total gate. No value budget or default cap was widened. ADR0113 records the scope distinction.

Changed production paths since owning251: Scene authoring/construction.rs, authoring/property.rs,
authoring_decode.rs, authoring_decode/structure.rs, and Tooling workspace/construction.rs. Tooling
recipe refusals now add bounded static reasons rather than only a generic unsupported message;
no user payload enters the safe summary. Scene/Tooling all-feature lib check passes2.61s.

Reference final focused run `5841a0b6-3152-41f2-a44d-0bd99c07ab17` passes2/2,0 skipped,0.743s after
1m21 rebuild. It executes the full canonical source UI Open/Create/Move/Delete/history/Save/Reopen/
content-refresh/explicit Runtime Restart and the exact Files7/edges8/members6 readiness fixture.
The previous full Reference run remains157/159 at its original source. Do not claim159 in a
single final passing run. Current owning251 predates only this budget-scope correction and must
be refreshed with the new owning regressions.

All earlier writers and integration reviewers are closed. Current two agents:

- Aristotle `01a198bb-23da-7530-8a7e-56ace48a378e` writes ONLY NEW
  crates/nara_scene/src/authoring/construction/limit_tests.rs. Parent must add cfg(test) include.
  It proves actual Data-source construction of more than8 fields with one history/Undo/Redo,
  standalone >8 field refusal and exact256-vs257 combined declaration counts. No Cargo/fmt/Git.
- Mill `01a198bb-bfe4-7752-8d61-e5a2a7adf447` is read-only independent review of budget-scope
  production changes, including inverse/history/value cost and mixed vs pure field admission.
  No Cargo or writes. Receive/close when complete.

Einstein final template/source integration review found no assigned P1/P2 and is closed. It
predates this budget-count change, and cannot replace Mill's review or dynamic limits tests.

# Next Action

Poll current serial Cargo session76732. Log TEMP/nara-authored-upgrade-final-native-dx12-tests.log.
It runs nara-editor all-feature lib ignored-only with actual DX12 and explicit nonfallback adapter:
three critical native trajectories, retained Scene navigation, UI Canvas priority/close, complete
Prefab conversion Save/Reopen and external stock Scene effects. Seven tests must actually run.
No other Nara Cargo is authorized while this run is active. The new test writer's file is not yet
included, so it cannot change the already compiling production source.

After native terminal: freeze test writer, add include, inspect diff→fmt, run complete Scene/Tooling/
UI owning libs with new tests. Resolve actual review/test findings, not caps/default tuning. Finish
Root+Editor all-feature related UI/source consumers and correctness Clippy all-targets, minimal
compilation and independent fmt checks. Reuse root target/jobs1. Run fresh relevant checks only;
do not repeat unrelated nested desktop builds for document closure. CLI process22/no-UI392 remain
earlier feature-scoped evidence; new source/public boundaries need their relevant controls.

Then final complete-unit verification/ledger/plan/guides, exact intended staging, Conventional
Commit and local ff main; no remote publication. Preserve vendor/concurrent work. Full Goal remains
active with CP2/motion/render/material/Nara-drawn editable Editor open. No permission is pending.

# Citations

- [Previous owning/templates checkpoint](2026-11-01T200006Z-source-owned-ui-construction-and-actual-creation-templates-pass-all-owning-controls-2b5140bf02e3460a9a8e072ae65d3514.md)
- [Current complete unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- TEMP logs: nara-canonical-ui-authoring-bundle-write.log,
  nara-source-safe-ui-reference-workflow-diagnostic.log,
  nara-construction-operation-budget-check.log,
  nara-ui-complete-construction-reference-tests.log.

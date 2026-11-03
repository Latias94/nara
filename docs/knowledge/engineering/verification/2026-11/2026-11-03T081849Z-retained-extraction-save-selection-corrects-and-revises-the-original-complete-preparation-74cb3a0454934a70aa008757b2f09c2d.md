---
type: "Verification Evidence"
title: "Retained extraction save selection corrects and revises the original complete preparation"
description: "Same-corpus selection correction, actual returned failure custody, public App lifecycle controls and reconciled preconfirmation semantics."
timestamp: 2026-11-03T08:18:49Z
record_id: "74cb3a0454934a70aa008757b2f09c2d"
producer_id: "codex-foundation-main"
run_id: "extraction-retained-selection-20261103"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "887803e4cf6d45a30952f93fef0604548a89af6a"
---

# Verification

Successor to the [live confirmation evidence](2026-11-03T074415Z-live-session-extraction-confirmation-and-original-publication-custody-pass-real-root-and-application-consumers-c3a619b0f0d34cd9bbee59adf90aa70f.md).
The full foundation Goal remains active. Current source remains authorized uncommitted WIP on
refactor/foundation; main and branch head are still `887803e`. This record qualifies retained
preconfirmation save-selection correction/revision, not complete CP2 or fresh-process/native
extraction. No push, artifact deletion or vendor/repo-ref mutation occurred.

# Result

The Session bridge no longer destroys a selectable preparation on failure. A concrete failure
retains the original Review/checkpoint-return/output/recovery owner and borrows its diagnostics.
Terminal capture/corpus failures remain distinct; `into_error` refuses to discard a retained
preparation. Corrected selection and valid-Review revision consume those exact owners through
their existing return protocols and continue the same corpus, nominal basis and Host/Source Work.

The Session exposes AwaitingSelection, borrowed original document/key/path options, owner-defined
`can_revise`, and asynchronous revision. Unstarted revised Tasks retain their exact input for
explicit Retry, a different selection or cancellation. Callback/slot storage refunds require
physical return, including retained failures. Dropped selection Vecs refund their own live storage;
an invalid persistence selection that still owns its Vec retains that reservation. Cumulative work
and observed peaks are never renewed. Retry respects the actual owner capability rather than
repeatedly spending against a known exhausted allowance.

The Application uses the same Session operations. Replacement Open refuses without losing the
correction facade. Close/Exit cancels retained unpublished preparation, waits actual worker return
and then resumes ordinary dirty Save/Discard. A revised Review alone installs or publishes nothing.

Reconcile ADR0047 and the owning plan's earlier blanket refused-revision wording: selection changes
are permitted only before confirmation, when no publisher/accepted file evidence exists. An
invalid new proposal keeps its original preparation but supersedes the old encoded Review; it
does not require two candidate copies. Outstanding checkpoint-return failure retains its exact
owner/proof. After confirmation selection cannot change and every refusal retains the entire
publisher, Review and actual accepted evidence. No compatibility promise is involved.

# Evidence

- First Root/Editor all-feature library check passed in14.77s. Final minimal Root check passed
  in0.47s; correctness Clippy for Root/Editor libraries passed in15.25s with warnings remaining.
  Final `cargo fmt --all -- --check` passed. Cargo used Rust1.95, jobs1 and root target serially.
- First related Root run `2c21a1e2-6ece-4807-96dd-a5ad8d1718f7`:204/207 passed,1451 outside.
  All four private retained-owner controls passed. Three new Runtime assertions incorrectly
  required a document allocation address to survive ordinary Undo/Redo. That editing path may
  publish another immutable allocation; compare source incarnation, content, history and saved
  state after those edits. Keep pointer equality before history edits and the private corpus
  allocation checks. No production source/history behavior was weakened.
- Focused Runtime revision rerun `93cb43b6-cd72-4b90-94d2-119dfab39ed0` passed4/4,
  build1m06s/run3.154s. Then tighten the unstarted Retry capability guard.
- Final related Root run `ba7727b6-cea6-46e5-ba77-59838173dbd1` passed207/207,1451 outside;
  build27.18s/run75.540s. Covers initial invalid selection correction and actual Ready, SaveCurrent
  on/off/back, invalid revised selection followed by corrected publication, physical revision
  Cancel/Close, same-corpus saved-Guard refusal, exhausted checkpoint-return preservation,
  actual task/selection storage lifetime, older publication/upgrade/Schema/Play-binding controls.
- Editor run `27ad60d3-37f5-4cac-be0c-d05ec4eafe4a` passed15/15,312 outside;
  build2m32s/run5.755s. Covers real Application correction through rejected replacement into
  actual publication and Close/Exit x Save/Discard from AwaitingSelection, plus predecessor
  operation/maintenance/custodian controls. Use the public Frame Model, not Root-only test getters.
- Initial fmt found the nested Editor test module's inherited `#[path]` base; explicitly route
  it to prefab_extraction/selection.rs. This was a test module-resolution correction, not a product
  capability or compatibility path. Source was frozen for actual compile/test runs.
- Independent Runtime correctness and Core adversarial review reported no assigned P0-P2.
  All implementation/review agents are closed. The separate recovery-design explorer is closed.

TEMP logs: `nara-extraction-selection-libs-check.log`, `nara-extraction-selection-related-tests.log`,
`nara-extraction-selection-runtime-tests.log`, `nara-extraction-selection-final-related-tests.log`,
`nara-extraction-selection-editor-tests.log`, `nara-extraction-selection-minimal-check.log`,
`nara-extraction-selection-clippy.log`. All corresponding Cargo sessions are terminal.

# Follow-up

Implement the public process-recovery boundary now specified in the owning plan. Independent
recovery must work with project-workflow-cli without Tooling or a fake Workspace. Reuse recovered
generation, file/future-Project and Ready publishers directly; split live-only imports/Origin arms
as needed rather than narrowing the consumer to require the Editor.

Ordinary admission already rejects Pending before bundle/source load, but its Editor diagnostic
still incorrectly calls every operation an authored upgrade. Use neutral pending-source-authoring
wording. The real recovery-kind discriminator is the authenticated persistent header, not the
manifest recovery path or diagnostic. A finite read-only inspector should transfer the already
loaded transport and budget into the selected concrete recovery owner; no fresh allowance/read.
Recovered publication has no old Workspace/history/Host: use Project publish guard, preserve
actual outcome snapshot/custody and do not copy CP1's receipt-only disposal of prepared Session.

Prove real process boundaries across Q/sidecar/partial-original stages, already-new identity,
strict ordinary Pending refusal, unchanged profiles/Schema and late64 refusal followed by a
new process selecting a larger finite ceiling. Normal Open/Check/Play and native Editor remain
required before complete CP2 commit/local fast-forward. Later foundation waves stay open.

# Citations

- [Complete plan and selected process boundary](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [ADR0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [Retained preparation](../../../../../src/project_host/authoring/project/prefab_extraction/session_review.rs)
- [Exact retained failure](../../../../../src/project_host/authoring/project/prefab_extraction/session_review/failure.rs)
- [Revision return protocol](../../../../../src/project_host/authoring/project/prefab_extraction/session_review/revision.rs)
- [Session selection](../../../../../src/project_host/runtime/editor/prefab_extraction/selection.rs)
- [Actual Runtime consumers](../../../../../src/project_host/runtime/editor/prefab_extraction/tests/selection.rs)
- [Actual Application consumers](../../../../../editor/src/app/persistence_tests/prefab_extraction/selection.rs)

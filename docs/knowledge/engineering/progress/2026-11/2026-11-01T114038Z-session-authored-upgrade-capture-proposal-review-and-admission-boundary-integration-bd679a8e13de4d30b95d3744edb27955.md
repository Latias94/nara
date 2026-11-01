---
type: "Work Progress"
title: "Session authored upgrade capture proposal review and admission boundary integration"
description: "Actual Session capture and complete proposal review, repaired Application lifecycle fences, and ongoing separation of operation work from published Schema capture policy."
timestamp: 2026-11-01T11:40:38Z
record_id: "bd679a8e13de4d30b95d3744edb27955"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-20261101"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
git_branch: "refactor/foundation"
git_commit: "ed21b23"
supersedes: "d0cbdbaf09964670b866a9ab64359372"
---

# Summary

The full foundation Goal remains active. `main` and `refactor/foundation` remain at `ed21b23`;
the authored-upgrade work is uncommitted. The actual Session now owns complete capture, pure-data
proposal admission, finite background preparation and a borrowed complete review. Application
Close/replacement and Browser/reconciliation admission corrections are source-integrated. Final
confirmation, coordinated publication/adoption and actual process recovery remain unfinished.

# Details

Acceptance fences writes immediately, drains only the Save accepted before that operation and
observes its actual installed outcome before capturing. The worker consumes the complete owned
Workspace input, all-profile inventory, original source observations and cumulative allowances.
Its borrowed command inputs contain actual captured bytes and working documents. A queued Task
cannot retain an unadmitted proposal: counts, complete paths/keys, bytes, selectors and request
storage are admitted before queueing. Preparation reuses the existing recovery, Schema and
source/Workspace owners. A refused proposal returns the same original baseline for correction,
settles only disposed attempt storage/handles, and keeps performed work and observed peaks.

Review borrows the actual complete prepared recovery members, including exact old disk,
reviewed-working and successor bytes and their open-document associations. Its sequential merge
iterator preserves associations through `nth` and `last`. The explicit history policy is to save
reviewed working copies and establish new history only on successful adoption. Preparation and
cancellation preserve original sources, both history branches, checkpoints and disk. Cancellation
of started work waits for actual task return before dropping inputs and releasing the write fence.
Task failure does not fabricate a successful capture or install a candidate.

Independent lifecycle review found four concrete P2s: Application Close/Exit could wait forever
behind the fence; replacement Open could strand cancellation; queued Browser refresh caused idle
polling; and additive reconciliation bypassed the common fence. Close now consults the Session's
`can_cancel()` and then uses normal dirty arbitration after actual return. Replacement refuses
without moving Application state. A queued refresh stays owned but does not request maintenance
while suspended; cancellation resumes it. Additive reconciliation observes the common fence.
Application tests cover AwaitingProposal and undriven Draining. The deterministic running-capture
physical-return gate remains a Root Session test; no cross-crate fake completion hook was added.

The metadata inventory now consumes `AssetMetaIdentityCandidate::into_parts()` and retains the
first bounded read's actual bytes. Recovery uses that immutable observation rather than rereading
or reconstructing settings. The former consuming identity API remains for its other real consumers;
there is no old-format reader or compatibility alias.

Actual Session retry first exposed the ordinary capture's 20M work ceiling being reused for the
whole upgrade. The complete operation now selects its Host work ceiling at acceptance, with a
finite 200M default and explicit `begin_authored_schema_upgrade_with_host_work_limit`. The initial
operation starts a new work meter while retaining existing content/storage/native admission.
All its capture, proposal attempts and later phases continue the same meter; exhaustion reports
`WorkLimit`. A default invalid-to-corrected retry succeeds; an explicit 20M control exhausts
without discarding its baseline or histories.

A second review found operation policy/history leaking through final Ready into ordinary Schema
capture. The selected successor prepares its ordinary capture baseline under incumbent policy
before publication, using actual admitted catalog/Adapter/basis costs, encoded inputs and a
conservative bounded settings projection. Private Pending and final Ready use that baseline.
The operation's cumulative usage stays in its published outcome/Session evidence. Installation
does not overwrite the Schema baseline with the whole operation budget. New captures, escaped
validation, opened-source binding and reclamation scopes admit continuing inventory custody;
same-operation `validate_with_budget` transfers the already charged scope without charging again.
Tests check normal policy after different work ceilings, a new operation's meter, continuing
storage/handles, and actual saved-startup validation after the private project installation.

Serial verification so far, Rust1.95.0/jobs1/root target:

- Initial Session/baseline/schema-owner capture cohort **54/54**, run
  `d068ba04-5a4d-466f-95ae-2b2df1decb17`.
- First expanded proposal cohort **137/139**, run `cfe04d5d-6e62-458b-8771-76bf65ee1b87`.
  One test compared canonical paths with authored binding locators; that oracle was corrected.
  Focused retry exposed actual `DecodeWorkExceeded`, not lost input ownership. After whole-operation
  admission was selected, the pre-handoff-separation cohort passes **139/139**,
  `6c61af1f-65b7-4902-915e-4793dbb7f00b`.
- After budget/custody separation, first clean cohort **203/204**,
  `373044d3-10af-4c0a-a98b-c8b9cfb509fd`. Its only failure was the padded-bundle test's obsolete
  equality between ordinary Schema usage and cumulative operation usage. Corrected assertions
  preserve the original capacity controls. Final cohort **204/204**, 1097 excluded,
  `696d6058-5266-40a2-b4e1-9ad0406c77b9`, build1m12s/run20.959s.
- Root and independent fixture formatting completed. A subsequently added actual Session
  one-unit capture-work refusal control has not yet run at this checkpoint.
- Earlier Editor attempt96572 ended with `os error112` while building the Root rlib, not a Rust
  error or test result. The user interruption/resume left no Cargo or compact process live and
  about719GiB free. Current Editor focused attempt31733 is a real clean serial build, not yet a
  terminal test result. Its log is in TEMP, `nara-authored-upgrade-editor-tests.log`.

Read-only review of the concrete replacement found no remaining assigned production P1/P2.
It independently found the obsolete padded-bundle assertion above. No new native qualification,
actual fresh-process recovery or complete v2 edit/Save/Reopen/Check/Play result is claimed here.
All review/implementation agents used in this checkpoint have returned and were closed.

# Next Action

Finish the running Editor focused test and compile/run the independent public Session consumer
in `tests/fixtures/behavior-languages/tests/support/authored_upgrade_session.rs`. Its six-file
fixture includes active/inactive dirty sources, nested/repeated/dormant overlays and an inactive
profile. It tests incomplete/malformed/corrected proposals under the same capture, literal v2
review outputs and retained Undo/Redo. That new fixture is source-ready, not yet executed.
Run the added Root capture-work refusal control and appropriate feature/native checks serially.

Then implement confirmation and publication, rather than another general audit. The selected
next boundary is one attached nominal Application Play-owner binding: only that owner can take
external launch requests or attest the exact operation/recovery/incumbent/generation retirement.
An unattached Session cannot export launches. Concrete physical close/cleanup precedes the ack;
Root verifies the attached attestation, not another crate's process itself. Prepare an empty Play
replacement before acknowledgment. Once retirement is issued, cancellation waits for physical
return and replacement installation before resumption. These are selected contracts, not APIs
already implemented by this checkpoint.

Use consuming private review/source parts for final Workspace binding; retain original proofs
through publication. Keep Host guards in local storage, then borrow persistence commits from
them, alongside Workspace, lane-fact and project destination guards. Hold all across the exclusive
Ready publisher. Success consumes already-bound commits and moves prepared project/Play custody
before releasing admission. Refusal/uncertainty retains the fence and actual file outcomes.
Qualify actual new-process interruption/conflict/idempotence and v2 follow-up workflows before
committing the whole unit or ff-merging main. Do not push or create a PR.

# Citations

- [Current complete unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Previous checkpoint](2026-11-01T103545Z-owned-complete-workspace-upgrade-inputs-and-real-worker-preparation-d0cbdbaf09964670b866a9ab64359372.md)
- [Session operation](../../../../../src/project_host/runtime/editor/authored_upgrade.rs)
- [Proposal owner](../../../../../src/project_host/authoring/project/source_upgrade/proposal.rs)
- [Ready owner](../../../../../src/project_host/authoring/source_upgrade_ready.rs)
- [Application facade](../../../../../editor/src/app/authored_upgrade.rs)
- [Independent consumer](../../../../../tests/fixtures/behavior-languages/tests/support/authored_upgrade_session.rs)

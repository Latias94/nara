---
type: "Verification Evidence"
title: "Independent task activity custody and shared Image runtime qualification"
description: "R4b first custody unit, isolated reaper recovery correction and actual two-Runtime Image evidence; deployment and lazy comparison remain open."
timestamp: 2026-10-25T15:57:28Z
record_id: "13caef5f8c39471aa58529789ae9c530"
producer_id: "codex-root"
run_id: "foundation-task-scopes-20261025"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/task-activity-scopes"
---

# Verification

The first optional Tasks activity-custody unit is verified from the branch worktree based on
`bd75495`. Rust `+1.95.0`, Windows, `CARGO_BUILD_JOBS=1`, serial Cargo and the ordinary checkout
`F:/SourceCodes/Rust/nara/target` were used. No separate C:/D: target, push or PR was created.

# Result

`TaskFacilities` owns physical execution; `TaskScope` owns independently closed admission and
executor-owned physical return. The existing `TaskPools` submission interface is scope-bound when
obtained through a scope. Runtime registration survives facade replacement and rejection.
Coalescing/cutoffs are activity-local, physical statistics and worker joining are facility-local.
The [design](../../../../architecture/task-activity-scope-design.md) owns the exact contract.

Two actual Image consumers prove independent same-AssetId/coalesce-key PNG reload and result
application. A held uncooperative task keeps the first Runtime `CloseIncomplete` while its sibling
reloads; after RetryClose, the sibling reloads again. Four actual configured workers remain live
after both scopes retire and become zero only after explicit physical shutdown. Two I/O workers
leave spare capacity; this does not prove fairness under total saturation or improve latency.

# Evidence

- Final `cargo nextest run -p nara_tasks -p nara_image --all-features --locked --test-threads 1`:
  **189 passed**, no skipped tests, run `23a53269-aed2-4b6d-b564-3c8662b1055c`. This includes
  seventeen scope tests and both actual Image consumers. Log: ignored
  `target/r4b-scope-final-tasks-image.log`.
- Renamed-root/direct-module consumer: **1 passed**, run
  `133d9389-42bd-42e8-b005-10fc26762bbd`, using the excluded module-consumer workspace with the
  same checkout target. Log: ignored `target/r4b-scopes-independent-consumer-final.log`.
- Initial nine scope tests passed at `d0bb7c71-8f7a-4b3f-9296-29075bb1406f`; the then-current
  Tasks/Image regression passed 157 at `c6863f04-dbd8-40f4-a017-601164d2b28c` and 162 at
  `110e74e3-4899-4a73-b875-01c903b21fdc`. Final evidence supersedes these for the later changes.
- Focused all-feature/all-target Clippy with correctness/suspicious denied passed. Existing
  large-error, unused-code and other non-denied warnings remain; no Clippy fix was applied.
  Log: ignored `target/r4b-scopes-clippy.log`.
- The initial workspace all-feature locked check passed; final all-feature/all-target coverage is
  recorded by `target/r4b-scope-final-workspace-check.log`. Changed Markdown links and whitespace
  checks pass. Root formatting and the independent fixture file are checked separately.

Initial fixture compile errors were test construction mistakes: using nonexistent
`OrderedTaskResults::insert/capture_ready`, formatting a non-Debug spawn outcome, then looking for
an advanced type in the ordinary root prelude. They were corrected to existing public interfaces;
the final independent test uses `advanced_prelude`, not a new ordinary-prelude export.

## Independent Review And Correction

- Consumer/source research and design review: agent `01a173a2-f7f8-7d10-af07-6a38a6679ebd`.
  It identified a P1 recovery gap: a failed disposal-reaper start retained pending custody but
  scope poll did not retry the supervisor. The fix retries while own work is pending; independent
  source recheck closed P1. Isolated thread-local failure injection proves first retain fails and
  repeated scope poll alone returns the capture exactly once. No global test reaper is mutated.
- Correctness review: agent `01a173b7-c2db-7ee1-835b-a40b88e4d34d` found no other confirmed bug.
  Its requested detached-completed-output and reaper recovery tests were added, then rechecked.
  A final Image-consumer and shared-Arc lifetime review also found no confirmed defect. Both
  reviews were read-only; Cargo execution belongs to the integration owner, not the reviewers.
- Image consumer implementation was delegated only in the new Image test file to agent
  `01a173b9-6132-70e1-8e37-cdcbe5a44fbc`, then read and executed by the integration owner.
  Agents were closed after their work completed.

The plugin retains an explicitly supplied strong facilities Arc. If the Host releases its own
reference early, the final plugin/Runtime may trigger physical Drop fallback. Scope/facade values
themselves do not own facilities shutdown. Existing arbitrary completed-output destructor latency
is not made bounded by this unit; the design and plugin documentation retain that distinction.

# Follow-up

R4b remains open: compare lazy per-Runtime and shared facilities on the same real consumer,
measure accepted-admission-to-start and interactive wall time, then select concrete Host policy
and qualify trusted compiled Editor authoring/Play integration. The new timing sample and worker
counter are instrumentation, not performance evidence. No default product allocation was changed.
R4c, R3, the broader foundation plan and its full Goal remain open. Native graphics was not changed
or requalified here; the prior R4a evidence retains its own source/platform scope.

# Citations

- [Scope implementation](../../../../../crates/nara_tasks/src/runtime/scope.rs)
- [Scope tests](../../../../../crates/nara_tasks/src/tests/scopes.rs)
- [Actual Image consumers](../../../../../crates/nara_image/src/tests/activity_scopes.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/tests/task_activity_scopes.rs)
- [Master plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)

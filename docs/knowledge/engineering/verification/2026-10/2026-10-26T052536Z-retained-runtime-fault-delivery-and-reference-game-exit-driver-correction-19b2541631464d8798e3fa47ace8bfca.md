---
type: "Verification Evidence"
title: "Retained Runtime fault delivery and Reference Game Exit driver correction"
description: "Separate already delivered execution faults from retirement failures while preserving typed fault data and physical Exit progress after Workspace retirement."
timestamp: 2026-10-26T05:25:36Z
record_id: "19b2541631464d8798e3fa47ace8bfca"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/prefab-addition-provenance"
base_commit: "59f911d"
git_commit: "6b3d4588498a34c152e5f526d79716a57ff2adf7"
---

# Verification

Owner correction after `59f911d`, discovered by the broad Reference Game run during the
[typed Prefab-origin replacement](2026-10-26T041116Z-typed-prefab-instance-addition-provenance-and-complete-consumer-propagation-331ec66de92c4315971800928d8f2dc7.md).
Do not treat the initial targeted GREEN as final closure: subsequent independent review found
and reproduced typed-field loss in the new delivery paths. The corrected owner, post-fix review,
complete Reference Game rerun and selected actual native regressions now pass within the scopes
recorded below.

# Result

- The Reference Game Exit helper required a Workspace frame after semantic retirement. It now
  drives authoring through the frame when present, otherwise retained Open Host maintenance,
  until the actual `EditorApplicationUpdate::exit_ready()` physical-completion result. `None`
  alone is never ready; original timeout and final assertions remain.
- Desktop correctly delivered the intended managed execution fault, then retained the Runtime
  through physical cleanup. `CleanupOwner::completion_failure_report` treated that same sticky
  fault as a newly discovered retirement failure. This duplicated the public error envelope.
- The concrete Host now associates fault delivery with the exact non-reused Runtime generation.
  Runtime's first-fault `OnceLock` cannot be replaced. A successor with identical metadata is
  still a new fact; no message/code matching, global history or universal error bus is involved.
  Unreported faults, late close-time faults and real plugin/participant/task failures remain.
- Independent review exposed that acknowledgement must follow a complete typed lowering, not
  merely a fieldless generic error. Outcome capture, bounded Stop and Desktop's stopped/faulted
  path now pass the actual fault into one typed diagnostic lowering. Desktop also matches the
  actual fault's kind/source/detail before acknowledging a structured Runner failure.
- ADR 0084 distinguishes execution-fault delivery, cleanup error and physical completion. A
  failed execution never becomes successful because its later cleanup succeeds. Retirement
  does not produce another copy of a fault already delivered by that activation.

# Evidence

Serial Rust 1.95.0, `CARGO_BUILD_JOBS=1`, root `target` including the Reference Game workspace:

- Full Reference Game all-feature RED `72258b04-72f8-433a-b78d-45be3fabd185`: 168 passed,
  2 failed of 170. Failures: `external_supervised_stop_then_editor_exit_retires_the_owner` and
  `real_product_hosts_publish_the_same_bounded_semantics_and_fault`.
- Isolated parity RED `66779cfb-8181-415a-8007-b64e64b035f4`: the intended primary was
  `project.desktop.runner-failed` with `managed-runtime/system/nara.ecs.fallible-execution`;
  the extra error was fieldless `project.run.runtime-faulted`. The bounded temporary
  `[DEBUG-origin-parity]` probe was removed after collecting these facts.
- Initial Root regression GREEN `a64505fd-14d8-407d-9c54-d2b3af60bf54`: 11 passed, 448 filtered.
  Real owner tests cover three drive entries, same-metadata successor faults, independently
  failing close participants/plugins, late close-time faults and task cleanup failures.
- Original two Reference Game failures initially GREEN as
  `f39b383e-9dcd-4c0b-bf85-4179ce7f0d3d`: 2 passed, 3 filtered. The parity probe's exclusive
  error assertion was not relaxed, and the Exit helper kept its physical-ready requirement.
- Reviewer-field-loss RED `ae4f66b2-f731-4f22-a874-db951e5cfadd`: three assertions reached the
  expected absent-field failures; two test processes then aborted while unwinding live owners.
  The harness now completes retirement before asserting captured observations, without changing
  the failing metadata expectation. Clean RED `a5b72019-554f-43d7-a6fd-81a288ff9c4b`: all three
  cases fail normally for the intended missing fields in 0.5 seconds.
- Those cases cover outcome capture after a retained reporter's new fault (including classified
  code/origin), bounded immediate and Pending Stop with actual plugin close-time faults, and a
  real Runtime whose successful shutdown hook reports a fault before Desktop terminal lowering.
  None is a fabricated `DiagnosticReport` standing in for Runtime behavior.
- Corrected typed-field regressions `752ec52f-6490-4392-964c-833b04deef9c`: 3 passed. Full
  all-feature Root library `7cc5fd43-9fcd-4622-bd56-904b21f14935`: 458 passed, 3 skipped.
  The subsequent simplification lends the actual fault instead of cloning its opaque App error;
  it does not change lowering or acknowledgement semantics.
- Final `cargo +1.95.0 fmt --all` and workspace all-feature/all-target Clippy with
  `-D clippy::correctness` passed. Existing lint warnings remain; this is not a zero-warning
  claim. The final Root source is the same frozen state inspected by the post-fix reviewer.
- Final complete Reference Game all-feature run `17c65e73-f002-4349-a7b7-50087e91db76`:
  170 passed, 0 skipped in 88.124 seconds. This includes both original failing trajectories and
  the real source-addition write-back refusal. No strict error-count or physical-ready assertion
  was relaxed. Earlier RED and targeted GREEN runs retain their recorded scope.
- Selected actual Windows/DX12 Editor run `f5a69fd9-5ee4-4c95-a923-5721817ddc37`: 3 passed,
  242 excluded/skipped by selection. The exact trajectories are fixed-World Canvas/input/step,
  retained Canvas priority through source Close, and unrelated-Prefab Close preserving the
  presented Play source until its associated Scene closes. The
  [origin evidence](2026-10-26T041116Z-typed-prefab-instance-addition-provenance-and-complete-consumer-propagation-331ec66de92c4315971800928d8f2dc7.md)
  lists their full test names. These are selected regression results, not the full native matrix
  or new native teardown/failure-injection qualification.

## Independent Challenge

Worker `01a17685-e36b-7203-ac08-acdac88c34a4` changed only the Reference Game Exit driver.
Worker `01a1768b-7243-7b02-bdbf-54f1aaacff5a` implemented the Host-local delivery correction and
real owner regressions. Both are closed; neither ran Cargo. Parent owns integration and execution.

Reviewer `01a17698-104b-7660-8112-7da7526b1171` initially stopped when parent formatting moved
lines. The parent identified its authorized `cargo fmt --all`, froze the four reviewed files by
SHA256 and requested an actual re-review. That completed review found one P2: generic diagnostics
were marked delivered and prevented subsequent structured metadata from being reported. The
parent reproduced it before correcting lowering. The initial empty finding list from the stopped
review is not a passing review receipt. The completed post-fix follow-up confirms actual typed
lowering in outcome capture, immediate/Pending Stop and Desktop, exact Runner fault association,
generation isolation and independently retained cleanup failures. It reports no remaining
confirmed findings in those four files. Their SHA256 values matched at the review's start and
end and at parent integration; the reviewer ran no Cargo and is closed. The parent owns the
test results, not the reviewer.

# Follow-up

- Keep origin replacement and this diagnostic/caller correction as precise local commit scopes.
  Do not hide a failing broad run behind the earlier targeted passing cohorts.
- The ordinary Application progress API still needs an ergonomics investigation: consumers must
  currently branch on frame presence and use hidden `advance_project_open` after frame retirement.
  Correct physical driving is not proof that the public no-action driver is ergonomic.
- This does not finish the full foundation rebaseline, native platform matrix, arbitrary failure
  injection or complete declarative Prefab replacement. The overall Goal remains active.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0084](../../../../architecture/adr/0084-executable-runtime-ownership-and-isolation.md)
- `src/project_host/runtime.rs`, `runtime/action.rs`, `runtime/tests.rs`
- `reference-game/tests/editor_supervised_play.rs`, `reference-game/tests/host_parity.rs`
- `reference-game/src/bin/host_parity_probe.rs`, `editor/src/app.rs`

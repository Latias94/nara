---
type: "Verification Evidence"
title: "Retained authoring replacement and physical retirement qualification"
description: "Bounded predecessor and rejected candidate custody, exact file Host transfer, sticky disjoint completion and affected Editor maintenance evidence."
timestamp: 2026-10-25T17:26:48Z
record_id: "f93efd28e9324ff4accf45fc3d38311c"
producer_id: "codex-root"
run_id: "foundation-authoring-retirement-20261026"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/editor-authoring-retirement"
git_commit: "5c02f6b"
---

# Verification

This is the bounded retained-authoring-replacement unit of R4b, not completion of R4b or the
foundation Goal. Source is `5c02f6b` plus the intended Tasks, Tooling, root Editor owner and native
Editor changes on `refactor/editor-authoring-retirement`. Rust 1.95.0, serial Cargo,
`CARGO_BUILD_JOBS=1` and the existing checkout `target` are used. No remote publication or separate
C:/D: build output is created; pre-existing `vendor/` is excluded.

# Owner And Consumer Scope

- One retained predecessor or rejected post-start candidate remains a complete Runtime until
  managed close and task/disposal return are physically complete. Another refresh cannot construct
  an owner while the slot is occupied; later saved-source obligations remain queued.
- Successful publication validates the image successor before moving exact file-Host custody.
  It then publishes the content and replaces the live Runtime. The successor continues real
  Save/Reopen while the predecessor is held; rejection keeps incumbent publication/custody intact.
- Completed disjoint worker reports collapse into bounded totals once. Cleanup failure remains
  sticky across slot reuse and final Session close, separately from physical completion.
- The public frame carries actual retirement Pending. Native maintenance observes it and content
  refresh Pending without adding a presentation cadence; Application notices actual content and
  retirement-state changes.

The four new Prefab tests use actual Session Save/refresh/Reopen, not replacement mocks. Their
held Compute task separates logical cancellation, task-body return and late-result destruction.
Session close stays Incomplete while destruction is held. The final report counts two or three
disjoint generations exactly once and remains stable on repeated close polls. Another test covers
both predecessor and rejected-candidate cleanup panic, slot reuse and one retained diagnostic.
The final-custody-refusal test keeps generation capacity valid, closes only candidate custody,
and proves unchanged incumbent content/image catalog plus actual Save/Reopen while candidate
late-result destruction is held. This reaches the final transfer refusal rather than an earlier
capacity refusal.

# Executed Checks

| Check | Result | Evidence |
| --- | --- | --- |
| Tasks all-features unit suite with report aggregation | 85 passed | nextest `93c11152-e922-421a-8be0-f32bb20c0165` |
| Root all-feature content-refresh cohort, including all three held-owner tests | 5 passed | nextest `98333d57-98be-4ceb-98ae-85686b2880ed` |
| Final root all-feature library regression after simplification | 419 passed, 3 skipped | nextest `69ad459f-2024-45c8-b7f8-791f7c1cf7a8` |
| Final ordinary Editor library suite, including corrected maintenance deadline proof | 198 passed, 30 native cases skipped | nextest `a2a57bd3-f999-4d6d-ba65-14f854d6f78d` |
| Selected actual Windows/DX12 native trajectories | 11 passed, 217 unselected cases skipped | nextest `a5e11285-6217-4bb3-9e0a-0384c74825fa` |
| Final all-feature Tasks/Tooling library suites | 147 passed | nextest `89647abe-3b02-47b3-a588-b296833412cc` |
| Locked workspace all-features/all-targets check | Passed | `target/r4b-authoring-retirement-workspace-check.log` |
| Affected root/Tasks/Tooling/Editor all-feature/all-target Clippy | Passed with existing warnings; correctness/suspicious denied | `target/r4b-authoring-retirement-clippy.log` |
| Rust formatting | Passed | `cargo +1.95.0 fmt --all -- --check` |
| Supplemental final-custody-refusal proof after review | 1 passed | nextest `b6054151-f23e-47ce-971d-736ad8d73de3` |
| Final post-review root all-feature library regression | 420 passed, 3 skipped | nextest `10a67b93-2de4-47a3-8a49-c7136096dd7b` |

The first root cohort precedes only the behavior-preserving internal poll rename/comment cleanup;
final regressions and native/check/lint gates are recorded with their actual scope. Existing warnings
are not promoted to a claim of warning-free compilation. Raw logs remain ignored under `target`.

Native selection is the procedure's three critical trajectories, enhanced Canvas widget
Save/refresh/Reopen, all five `platform::imgui::retirement_tests`, unrelated Prefab Close preserving
the Play source, and actual external-running quiet maintenance/child exit. All execute serially
with `WGPU_BACKEND=dx12`. Canvas observes a newer Current content generation and completed
authoring retirement before Reopen. The external trajectory's real cold supervised Cargo/READY
path takes 443.335 seconds; it passes rather than being treated as a skipped slow case. That
trajectory's three quiet cycles concern external process maintenance, not a held authoring owner.

After review added only the final-custody test and corrected one internal contract comment, the
420-case root suite, locked workspace all-target/all-feature check and affected Clippy gate pass
again. Their final logs are `target/r4b-authoring-retirement-post-review-{root,check,clippy}.log`.
Formatting also passes. Production executable code did not change after the native and ordinary
Editor runs, so their scope remains applicable without another native acceptance cycle.
Changed Markdown links and engineering-memory validation pass, with existing legacy-memory
warnings preserved. Generated rollups are rendered from the new evidence shard.

# Failed Check And Review Corrections

The initial focused ordinary Editor maintenance test failed at nextest
`c149c58c-eb9a-4508-8900-428e90f302c8`. The new assertion incorrectly assumed its fixture had no
unrelated image-hydration presentation deadline. The isolated frame now explicitly clears
image/source import Pending before testing retirement-only deadlines; observations are asserted
after Session cleanup. Production scheduling was not weakened. The subsequent Windows process
exit code `0xc0000409` does not independently establish stack or memory corruption.

Independent adversarial review found old tests sampling generation budgets before physical
retirement and a later Stale assertion racing the new gate. Both now wait with finite deadlines
for actual retirement. The public consumer re-acquires its final frame so later frame-bound actions
use current authority. No production owner is released early to satisfy old generation counts.

The simplification passes retained all proof tests and seams. Reuse/quality both identified the
same duplicated import-delay calculation; production now reuses `image_import_poll_deadline`.
The internal poll name states its newly observed failure return, and redundant production
comments were removed. No efficiency or test-value defect was confirmed. Suggestions to edit
proof-test comments were not applied because the evidence scope protects those tests unchanged.

These are post-implementation owner/consumer checks; no red-before-implementation observation is
claimed for the four new tests. The initial failing maintenance fixture is retained as failed
verification, not as evidence that the production retirement algorithm failed.

### Completed Independent Review And Follow-up

CE review `20261026-014014-27c03c96` completed with seven separately dispatched source reviewers:
correctness, project standards, testing, maintainability, API contract, reliability and adversarial.
It returned `status: complete`, `Ready with fixes`, one P2 test-coverage finding and no confirmed
production defect. A fresh validator confirmed that the generation-exhaustion tests could not
discriminate the final custody/publication rejection boundary. Main added the fourth test above
using the existing private hook; no new production injection interface or executable change was
needed. The `transfer_to` documentation now describes an attempt and success postconditions,
not an infallible call. The supplemental test passes.

Fresh scoped reviewer `01a1743b-d729-7612-a928-f38ef80075d5` independently closed finding #1 in
source, including valid-capacity reachability, catalog generation discrimination, real incumbent
Save/Reopen during held destruction and exact final reports. It found no new defect. Its read-only
result is separate from Main's executed test evidence. The recorded actually-held native quiet
maintenance limitation remains; it was not relabeled as a reproduced scheduling fault or a passing
integration test. No justified review finding remains unapplied.

The earlier delegated whole-review attempt was degraded because nested subagent dispatch was
unavailable. It is not the completion receipt. Main recovered with the seven direct leaf dispatches,
one fresh validator and separate merge/report leaves; all results were collected and agents closed.
The seventh reviewer waited for a released slot after the six-agent concurrency cap. External
programming/model CLIs were not used; local independent reviews do not establish different-model
corroboration. Raw review artifacts remain outside the repository, identified by the completed run
ID; the concise findings and disposition are retained here.

# Follow-up

- Initial App/authoring Open construction failure ownership is not repaired by this unit.
- Product `TaskPlugin` allocation remains eager. Measured lazy-private facilities are a deployment
  candidate, not the current default. The prior matched release comparison is not rerun or claimed
  as a new performance result here.
- Trusted Play's complete Session-close path and full R4c/remaining waves remain open.
- Root held-owner, deadline helper and native Canvas evidence have different scopes. The Canvas
  workflow actively paints while waiting; it cannot prove three quiet native maintenance cycles
  under an actually held authoring owner. No production injection API is added for that claim.

Read-only construction sidecar `01a17413-8426-7411-a9ca-8c036cd45baf` traced all three initial
failure stages and the public source-drift rejection. Its concrete owner gaps, existing retained
App mechanisms, pinned Bevy/Godot comparison and bounded next acceptance are captured in the
activity design's Initial Construction And Open Failure Follow-up. This was source research, not
an executed initial-Open held-work reproduction. The sidecar completed and was closed.

# Citations

- [Activity and retained replacement design](../../../../architecture/task-activity-scope-design.md)
- [Master plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)

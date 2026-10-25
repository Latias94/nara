---
type: "Verification Evidence"
title: "Startup preparation owner retry and final Application shutdown"
description: "Bounded R4b standalone startup facility ownership, exact failure evidence, independent review and serial qualification."
timestamp: 2026-10-25T23:27:08Z
record_id: "4281deb3281042fc9095f7cf2e247c4b"
producer_id: "codex-root"
run_id: "foundation-startup-custody-20261026"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/startup-preparation-custody"
git_commit: "f44e88a0eb5efd0f5311e0735765da36ca79853c"
---

# Scope And Ownership

This is the bounded R4b successor to the private-lazy Editor/whole-Session unit at `f44e88a`.
The persistent Open Host owns a separate standalone facility outside every Session. It now uses
one reusable lazy pool, retains a permanently failed pool until physical return, and creates a
replacement only on a later explicit Open. A private shared concrete retirement object lets
either the Host or failed Open keep the same physical owner alive. Maintenance never creates
replacement facilities; ordinary pressure, source failure, cancellation and admitted task panic
do not replace a healthy pool. Linear attempts remain transportable and acquire no Runtime owner.

Exact `TaskRejection` and unadmitted capture cleanup failure remain distinct from admitted Tasks
close and Runtime evidence. Application maintains failed custody independently of Recovery,
gates Retry, preserves Close Project/dirty Cancel reuse, and irreversibly waits for healthy Host
shutdown only once final process Exit is committed. Native final errors consume the cumulative
Host report independently of Session and graphics failures. Abnormal event-loop return makes a
finite local cleanup attempt and cannot turn an incomplete report into success.

`TaskPools::poll_shutdown` performs one existing close-owner poll. Independent review found that
the older blocking convenience driver held that mutex across sleeps; it now releases it after
each poll. This changes no close state, deadline or once-only accounting. No recovery registry,
facility-generation list, automatic retry, generic executor trait or compatibility layer was added.

Pinned references were rechecked read-only: Bevy `85503941e488152711a4695bbe248e9ed9f453fc`
joins TaskPool workers in `bevy_tasks/src/task_pool.rs`; Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` joins explicit worker-pool ownership in
`core/object/worker_thread_pool.cpp::finish`. They support physical ownership, not a claim to
provide Nara's cross-frame retained Open/Retry protocol.

# Review And Failed Approaches

Original completed full seven-lens review:
`C:/Users/Frankorz/AppData/Local/Temp/compound-engineering-nara/ce-code-review/20261025-225404-76a27001`.
Correctness, standards, testing, maintainability, reliability, adversarial and API-contract
reviewers returned artifacts, followed by independent merge, validation and report leaves.
The original receipt remains **Ready with fixes**, with four confirmed findings. Subsequent
closure must not rewrite that historical verdict or imply it reviewed later edits.

- #1: three existing persistence tests assumed Session disappearance meant final Exit, and one
  forbade genuine retirement-change observations. The first integrated run actually failed all
  three. Revised tests drive final Host return and check publication after each individual advance
  and drive. A first repair used a nonexistent field, then the editing-eligible `workspace()` view;
  compilation and the next run exposed those mistakes. The canonical `workspace_generation()`
  reads the retained published incumbent as well as ordinary publication. This is a test-oracle
  correction, not evidence that an unauthorized candidate was published by production code.
- #2: a real concurrent blocking/single-poll test first failed on receive timeout; after releasing
  the owner mutex between blocking polls, the complete Tasks cohort passes. Both helper threads
  and the held task are released/joined before the regression assertion, including on the red path.
- #3: the implementation ledger needs a bounded successor with actual evidence, leaving the prior
  entry's recorded scope and remaining foundation work intact.
- #4: the first native test stayed inside one `resumed` callback, so its no-redraw assertion was
  tautological. The replacement is a staged real event handler. A scoped reviewer additionally
  found a last-cycle hole: releasing in `new_events` preceded queued window-event delivery.
  Release now occurs at the following real `about_to_wait` entry after its quiet check, before
  another production poll, so the preceding poll's delivered redraw still fails the oracle.
  Actual execution then exposed a fixture precondition: Project Manager's focused path field
  legitimately repaints its caret. Detailed watchdog output showed `Closing`, manager state,
  `want_text_input=true` and periodic presentations, not a blocked maintenance loop. Focus-loss
  alone did not dismiss that active text widget. Ordinary Escape press/release through the owned
  Context input establishes the intended no-presentation-demand state without altering counters
  or production cadence. The native positive then passes. A temporary test-only mutation requesting
  redraw on every quiet poll fails with the exact unchanged-retirement redraw assertion; the
  mutation was removed by its precise patch before final qualification, without reverting files.

A separate five-lens simplification pass preserved every proof test and its seams, consolidated
startup diagnostics, removed a redundant private comment, and avoided cloning report maps merely
to read completion. The private enum rewrite suggestion was not applied during simplification:
the existing proof fixtures directly exercise the two private slots, whose production transitions
remain bounded and covered. No public abstraction or compatibility mechanism was added for it.

# Executed Evidence

All orchestrated Cargo commands run serially in this checkout, Rust `+1.95.0`, jobs=1, using
`F:/SourceCodes/Rust/nara/target`. No C/D build target was created and no new clean was run.
Raw logs stay in ignored `target/r4b-startup-*`; they are not tracked evidence artifacts.

- Lazy deployment red: strengthened existing held-worker test observed unused Compute worker
  count `1` instead of `0` before the root implementation change.
- Initial focused Tasks/startup-owner cohort: 102 passed,
  `e5c17730-61d9-407a-bdcf-a58e761b3b35`.
- Initial root/Tasks/App/Image libraries: 712 passed, 4 skipped,
  `e8febd29-0d84-409d-ba66-7a2e9af52360`.
- External default-off `test-support` consumer: 1 passed,
  `6d6de9ff-4476-4789-8e5a-27e5d23a9ee1`.
- Initial integrated run: 483 passed, 3 failed, 269 not run, 34 skipped,
  `c1dfb50a-b47c-467d-9054-3ba65045a1ea`. This is failed evidence, not superseded by the earlier
  passing owner cohorts. The next run failed compilation on the test field typo; the following
  run executed 756 tests with 755 passed and one failed oracle, `02eb238a-942d-4131-9b39-e0a4b1d578f7`.
- Concurrent close red: receive timeout, `f15d2784-a920-4998-80a0-d921ddebd6e8`.
  Corrected complete Tasks library: 93 passed, `d9284f27-c81f-4ae9-9af6-6038d94fe5c3`.
- Five public integration binaries: 64 passed, 3 skipped,
  `8b915a2f-516a-49e8-9f9e-501fc0aef5e0` (`editor_persistence`, `project_host_boundary`,
  `workspace_play_runtime`, `scene_file_workspace`, `project_runtime_boot`).
- Tasks with default features off and headless root `project-workflow,tooling` library check pass.
  Focused Tasks/root/Editor all-feature library Clippy passes correctness/suspicious deny gates;
  existing warnings remain. Final integrated/native verification is recorded at closure below.
- Native quiet positive: 1 passed, `8f3edb7e-19b4-4a38-9390-6746498b7763`, with three actual
  maintenance wakes at an unchanged settled presentation baseline (eight total setup presentations).
  The same inactive-window visible fallback was actually used. Earlier timeout runs
  `bcebf98b-c24e-4063-82ae-c97d68995843`, `dc1f3d21-327f-4805-8d85-030d52e6ae01`, and
  `5f9209a2-64de-4a18-996b-2a9329b7c898` remain failed fixture evidence, not qualifications.
- Native quiet sensitivity red: `294ae74f-caf6-4f27-b0bd-96da52097514`, expected failure
  `unchanged retirement delivered a redraw, frame or presentation after settling`.

## Independent Closure

Two fresh scoped reviewers checked the caller-owned fixes rather than rewriting the original
receipt. #2's lock/accounting and discriminating cleanup test are Closed; #1 is Closed after the
published-incumbent identity correction; #4's real event delivery and release ordering are Closed
statically, with actual native and sensitivity execution recorded separately. The dirty Cancel
gap now performs CloseProject/Discard followed by successful explicit Open while the same original
worker remains started=1/exited=0 and joined=0, then final Exit joins exactly that worker. #3's
ledger successor links this evidence without editing the prior entry's historical scope.

## Final Product Qualification

The source under test is the owned, initially uncommitted branch diff against the frontmatter
baseline `f44e88a`; that metadata names the predecessor, not a claim that the predecessor contains
these changes. The temporary sensitivity mutation is absent from the final source; the selected
default-off real worker failure test support remains.

- Final root/Tasks/Editor all-feature libraries: **756 passed, 34 skipped**,
  `ca38e349-4312-4632-a990-309b61d0fdf2`. This executes the repaired persistence oracles,
  real dirty-Cancel reuse and native final-result/error tests; ignored trajectories are separate.
- Windows/DX12 native critical three plus retirement six: **9 passed**,
  `f72c3eea-be6b-4933-9312-088f050747a8`. The quiet-exit test again observes three actual
  maintenance wakes with no baseline redraw/presentation change, then real worker join and
  graphics destruction before process Exit. The inactive visible fallback is recorded as used.
- Normal and native tests run serially by Cargo invocation. A failing native prerequisite would
  not be a passing skip. No actual OS pointer/keyboard events were injected; Escape uses the
  owned Context's input queue, as in other native owner-input proofs.
- Final App/Tasks/Image libraries: **262 passed, 1 skipped**,
  `0a53b873-554b-4b34-b0b0-4ec3875c283e`. Final external test-support consumer: **1 passed**,
  `b0c60e8b-1dee-4867-bfef-0dfca4a28d40`. These overlap the Tasks cohort; do not add them as
  distinct total coverage. The final Tasks default-feature-off check also passes.
- `cargo check --workspace --all-features --all-targets --locked` passes. Focused Tasks/root/Editor
  all-feature/all-target Clippy passes `-D clippy::correctness -D clippy::suspicious`; existing
  style/dead-code/large-error warnings remain. No warning-clean or whole-workspace test claim is made.
- The separately reviewable Tasks prerequisite is committed as `e09ff8b`
  (`feat(tasks): expose single-poll physical shutdown`). The dependent Editor/source changes and
  this record form the second local commit; integration is local-only, with no push or PR.

# Limits And Next Work

This is not full R4b, R4c or foundation completion. Startup first-use failure is injected around
the real first submission, not a lazy constructor, closed inline pool or admitted body panic.
Io2/fail-index1 creates one actual worker, admits no task, remains unjoinable while held and joins
exactly one after release. Healthy facilities are reused after pressure and dirty Cancel.
Worker counts are custody evidence, not latency/CPU/memory benchmarks. Native results qualify
only the executed Windows/DX12 trajectories, not Linux/macOS, monitor scanout or all trusted
authoring/Play first-use failure paths. Continue the dependency-ordered master plan after the
bounded owner and its review findings close; do not reduce the active Goal to this unit.

# Citations

- [Master rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Selected task ownership/deployment design](../../../../architecture/task-activity-scope-design.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)

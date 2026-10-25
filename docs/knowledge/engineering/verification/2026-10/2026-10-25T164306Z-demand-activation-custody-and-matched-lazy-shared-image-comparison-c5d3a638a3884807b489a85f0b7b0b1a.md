---
type: "Verification Evidence"
title: "Demand activation custody and matched lazy shared Image comparison"
description: "Qualifies lazy per-kind workers, concurrent physical retirement, actual joined retention and a matched release Image comparison that challenges unconditional physical sharing."
timestamp: 2026-10-25T16:43:06Z
record_id: "c5d3a638a3884807b489a85f0b7b0b1a"
producer_id: "codex-root"
run_id: "foundation-task-demand-20261026"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/task-demand-activation"
---

# Verification

R4b demand activation and the bounded Image comparison are verified on the worktree based on
`c62ff32`, following the merged scope-custody source `375c80b`. Windows, Rust `+1.95.0`, serial
Cargo, `CARGO_BUILD_JOBS=1`, and the ordinary checkout `target` were used throughout. No C:/D:
build target, remote publication, migration guide or default product allocation change.

# Result

`TaskPools::try_new_lazy` and `TaskFacilities::try_new_lazy` activate only a used kind before
admission. First-use OS startup can delay the submitter and fail with `WorkerStartupFailed`;
partial worker handles remain in physical custody. Eager construction reuses the same machinery
without losing its earlier startup-failure retirement path. The private workers module owns
activation, retained handles, nonblocking join polling and failure state.

Physical close does not wait for an activation mutex or take an incomplete handle snapshot.
Abnormal Drop transfers the same worker-custody Arc to the existing reaper, including in-flight
OS spawn reservations. Deterministic tests verify post-reservation success/failure, partial
activation, scope close independent of unadmitted startup and actual reaper `join`, not only
thread exit. Configured worker limits and observed started/reserved workers remain distinct.

The [release report](../../../../benchmarks/2026-10-26-task-facility-locality.md) compares genuine
lazy per-Runtime and lazy shared facilities at matched total two-worker I/O/eight-slot capacity
through actual PNG application and exact independent activity ownership. Both candidates have
zero I/O workers before first use. Their active count is `[1, 1]` versus `[2]`, not a worker-count
saving for shared facilities. All unused worker kinds remain dormant.

All sixteen interference PNG jobs per candidate are observed admitted before held-work release;
zero samples are excluded. Private sibling PNGs finish before release in 16/16 trials, shared in
0/16. Under the same two held closures, median separate probe queue wait is 5us private versus
30,238us shared. Scope ownership cannot establish execution-progress isolation. The measured
direction now favors qualifying lazy private isolation for the interactive Editor instead of
unconditionally sharing facilities based on object count. This is not default deployment or a
product performance qualification.

# Evidence

- `cargo nextest run -p nara_tasks -p nara_image --all-features --locked --test-threads 1`:
  **198 passed**, one ignored measurement excluded, nextest
  `6a629678-ca78-4157-9a36-e189a502b9dc`. Includes all original eager/scope regressions and nine
  demand-activation tests. Log: ignored `target/r4b-lazy-final-regression.log`.
- Standalone Tasks84: all passed at `9928cce2-e1e1-40d0-a891-f118d18a8783`; earlier Tasks82 and
  Tasks83 cohorts also passed before the final deterministic gap tests were added.
- Excluded workspace renamed-root/direct consumer: **2 passed**, nextest
  `ab5297c0-3130-4c6b-b221-7985c0e8ff85`, including a public lazy facility/Runtime consumer. Later
  changes are test-only probes, formatting and documentation; production activation semantics
  are unchanged. Log: ignored `target/r4b-lazy-independent-consumer.log`.
- Release measurement: **1 passed**, 32 complete trial records, nextest
  `acf39ada-7c6d-4f5b-9b50-08a087c1c7e0`. Actual release/source-application checks use the final
  conservative `image_admission_observed_offset_us` field. Six ordinary reloads and the pressure
  reload plus separate probes give sixteen actual submissions per trial. The report records all
  sample counts, nearest-rank P50/P95, attribution and limits. Raw/summary are in ignored target.
- Final locked workspace all-features/all-targets check, focused all-feature/all-target
  correctness/suspicious Clippy, root and independent-fixture format, changed document links and
  whitespace checks pass. Existing warnings remain. Logs:
  `target/r4b-lazy-final-workspace-check.log`, `target/r4b-lazy-final-clippy.log`.

## Independent Review

- `01a173d0-2b09-73d0-acc8-8ae05abe0575` traced the concrete product topology against Git source
  `c62ff32`, not concurrently edited files. It found the need to retain canonical PluginPlan
  identity, separate one-shot admission from replayable definitions, retain physical ownership
  across failures, and distinguish scope/physical reports. Its authoring startup/refresh/close
  obligations are recorded in the design, not claimed fixed by demand activation.
- `01a173d3-8ccd-7c32-8bd5-fd6caa21c905` implemented only the new bounded Image comparison file.
  Integration reviewed it and ran all Cargo/measurement commands; the worker did not run Cargo.
- `01a173d7-a1eb-78f0-84f4-7d1f5950eecb` adversarially reviewed activation/close/drop and found
  no confirmed bug. Its two test gaps were addressed with an actual join counter and deterministic
  post-reservation pause, then independently rechecked. The measurement review also required
  confirming PNG admission before release rather than interpreting every requested reload as
  under pressure. Integration added the conservative owner-observed timestamp, recheck accepted
  that rule, and raw output proves 16/16 valid per candidate. No exclusions were hidden.
- All agents were read-only except the one assigned new test file; they completed and were closed.

# Follow-up

The full foundation Goal remains active. R4b now has activity custody, demand-activation and one
bounded matched real-domain comparison, not complete product deployment. Do not build a universal
fairness scheduler or assume a shared queue preserves interactive progress. Next implement and
qualify the actual Editor isolation policy through trusted authoring/Play. Preserve its exact
configuration, failed-start and content-refresh retirement owners, Scope versus physical close
evidence, and final Session join. Existing Drop/quarantine safety is not a normal-close proof.
If a shared deployment remains justified, use explicit typed candidate admission rather than
capturing a runtime facilities Arc in canonical replayable plugin definitions. No new generic
service registry is selected. R4c and all remaining plan waves remain open.

# Citations

- [Workers](../../../../../crates/nara_tasks/src/runtime/workers.rs)
- [Lazy tests](../../../../../crates/nara_tasks/src/tests/lazy.rs)
- [Actual Image comparison](../../../../../crates/nara_image/src/tests/activity_scopes/comparison.rs)
- [Activity and product design](../../../../architecture/task-activity-scope-design.md)
- [Release report](../../../../benchmarks/2026-10-26-task-facility-locality.md)
- [Master plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)

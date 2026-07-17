---
type: "Subagent Finding"
title: "RGF-U5 runtime ownership closure review"
description: "Independent closure review of the corrected managed-runtime ownership, fault, driver, and finite-close contracts."
timestamp: 2026-07-17T01:41:35Z
record_id: "a5bcca0c785c4057ad9fc9ef765b4445"
tags: ["rgf-u5", "review", "runtime", "ownership", "faults", "shutdown"]
status: "closed"
producer_id: "codex-u5-closure-review"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
subagent_id: "/root/u5_closure_review"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ff6b111b24fe692c23e1b42ef0a2e3cd407647fc"
verified_by: "independent-review"
source_head: "2e6b92f0773904b3ff431e52906fa6e69bae88bf"
reviewed_implementation_commit: "ff6b111b24fe692c23e1b42ef0a2e3cd407647fc"
reviewed_code_diff_fingerprint: "cb23536128b2cd3f16685b04f9772f1274d622dc"
reviewed_requirements_fingerprint: "0ea3a250561f102cd763432c315f8a336fcb160d"
reviewed_untracked_code_paths: []
---

# Finding

The P1 findings in the source-bound RGF-U5 review were valid at its original snapshot. The
corrected staged snapshot and implementation commit close every P1 without accepting ADR 0084 or
adding a second App, World, schedule, or time authority. No P0 or P1 remains open.

The task/service finding is specifically about production-shaped domain integration evidence.
`nara_tasks::TaskPlugin` cannot infer whether arbitrary work is required; the domain integration
system classifies the typed terminal or service outcome and reports a required failure through the
runtime's sticky fault channel. Optional and last-good outcomes remain domain results.

# Evidence

## Snapshot identity

| Field | Value |
|---|---|
| Source head | `2e6b92f0773904b3ff431e52906fa6e69bae88bf` |
| Reviewed implementation commit | `ff6b111b24fe692c23e1b42ef0a2e3cd407647fc` |
| Staged code diff fingerprint | `cb23536128b2cd3f16685b04f9772f1274d622dc` |
| Staged requirements fingerprint | `0ea3a250561f102cd763432c315f8a336fcb160d` |
| Untracked implementation paths | none |
| Decision status | ADR 0084 remains Proposed |

The code fingerprint covers `Cargo.toml`, `nara_app`, `nara_gameplay`, `nara_tasks`, `nara_winit`,
examples, the independent reference game, and the two root runtime integration targets. The
requirements fingerprint covers repo rules, the active U5 contract, relevant Accepted ADRs, the
Proposed ADR 0084 evidence input, and the runtime Interface harness. The independent final pass
reviewed `git diff --cached HEAD`, so concurrent unstaged architecture edits are outside this
closure.

## Finding disposition

| ID | Disposition | Named implementation evidence |
|---|---|---|
| U5-RV-01 | Fixed | `admission_failure_retains_and_retires_transferred_owners`, `dropping_an_admission_failure_transfers_its_owner_to_quarantine`, and `dropping_a_startup_failure_transfers_its_owner_to_quarantine` prove ordinary failure propagation retains a driveable owner. |
| U5-RV-02 | Fixed | `dropping_an_incomplete_runtime_does_not_destroy_its_close_owner`, `close_participant_panic_during_drop_retains_the_owner_for_retry`, `quarantine_is_process_observable_and_driven_on_the_owner_thread`, and `dropping_multiple_runtimes_with_blocked_task_owners_is_bounded_and_reapable` prove bounded, observable, owner-thread-affine quarantine without `mem::forget`. |
| U5-RV-03 | Fixed | Candidate/runtime reporter and handler replacement tests plus `candidate_world_scope_captures_fallible_observer_errors`, `driver_world_scope_captures_fallible_observer_errors`, and `close_participant_world_errors_use_the_runtime_fault_bridge` prove canonical fallback capture across every allowed World scope. |
| U5-RV-04 | Fixed | `a_preexisting_candidate_fault_prevents_startup_side_effects` and `a_fault_after_readiness_is_not_published_as_running` prove report/publish linearization and no false Running window. |
| U5-RV-05 | Fixed | `stop_is_observable_as_stopping_before_close_work_runs` and `pending_stop_preserves_surface_retirement_authority_until_the_runner_barrier` prove Winit retires its surface/provider before runtime close removes mutation authority. |
| U5-RV-06 | Fixed | `initial_retirement_incomplete_is_retried_within_the_remaining_budget` proves the helper retries an initially incomplete retirement and never reinvokes completed work. |
| U5-RV-07 | Fixed | `panic_severity_fallible_systems_unwind_out_of_runtime_drive`, `rust_system_panics_unwind_from_variable_and_every_fixed_set`, and `close_participant_panics_unwind_out_of_runtime_drive` prove U5 contains no runtime panic-containment policy. |
| U5-RV-08 | Fixed | `real_required_task_integration_faults_the_runtime_in_the_same_drive`, `real_required_service_integration_faults_the_runtime_in_the_same_drive`, and their optional counterparts prove real domain integration rather than direct reporter injection. |
| U5-RV-09 | Deferred | RGF-U24 owns the concrete product start action; RGF-U25 measures author-facing concepts and caller glue before that action becomes permanent. |
| U5-RV-10 | Fixed | `close_time_fault_remains_visible_after_runtime_stops` and `plugin_shutdown_failure_is_terminal_and_visible_without_claiming_close_success` preserve sticky fault, close evidence, `CloseFailed`, and Host teardown failure without making `Stopped` dishonest about ownership. |
| U5-RV-11 | Deferred | U5 proves the public runner/state boundary; RGF-U13 owns the supported native event-loop smoke and desktop parity evidence. |
| U5-RV-12 | Accepted residual | Owner: `nara_tasks`. Trigger: any hosted Windows/Linux timing flake or threshold approach replaces the affected wall-clock assertion with a barrier/channel or injected work budget while retaining only a generous deadlock ceiling. |

## Automated verification

- `cargo nextest run --locked -p nara --test runtime_instance --test runtime_driver_boundary
  --test-threads=1`: 56/56 passed.
- `cargo nextest run --locked -p nara_app -p nara_gameplay -p nara_tasks -p nara_winit
  --test-threads=1`: 144/144 passed.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test runtime_core
  --test-threads=1`: 1/1 passed.
- `cargo nextest run --locked --workspace --test-threads=1`: 778/778 passed with three declared
  conditional skips.
- `cargo nextest run --locked -p nara --test architecture_docs --test-threads=1`: 5/5 passed.
- `cargo check --locked --workspace`, `cargo check --locked -p nara_app`, and all three combined
  runtime-2d/runtime-ui/desktop-winit/render-wgpu example checks passed.
- `cargo fmt --all -- --check`, `cargo doc --locked -p nara_app --no-deps`, staged/working
  `git diff --check`, and the final independent staged-snapshot review passed.
- Static searches found no production runtime `mem::forget`, panic containment, Winit
  `App::run_once`, or project/Host dependency in `nara_app::runtime`.

# Recommendation

Close RGF-U5 at implementation commit `ff6b111`. Continue with the dependency-ready plan units;
do not promote ADR 0084 until RGF-U23 evaluates the later U26/U24/U25/U17/U13 evidence as required
by the active plan.

An explicit per-system or per-observer error handler is caller-owned handling and is not a fallback
authority bypass. Exact `StepFixedTick` intentionally runs the complete FixedUpdate transaction,
not `TaskUpdate`; a domain that needs step-time terminal visibility must integrate in fixed Prepare
or report directly.

# Disposition

Closed. U5-RV-01 through U5-RV-08 and U5-RV-10 are fixed, U5-RV-09 and U5-RV-11 are assigned to
named active units, and U5-RV-12 has an explicit residual-risk owner and trigger. The two observed
efficiency opportunities, terminal native-wait 10 ms wakeups and a second close-ledger scan per
poll, are non-blocking P2 follow-up and do not weaken the ownership contract.

# Citations

- `docs/knowledge/engineering/subagents/2026-07/2026-07-16-rgf-u5-runtime-code-review.md`
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`
- `docs/architecture/adr/0039-main-loop-time-pause-and-runtime-state.md`
- `docs/architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md`
- `docs/architecture/adr/0076-play-runtime-debug-control-and-observation.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/migrations/2026-07-engine-foundation.md`
- `crates/nara_app/src/runtime.rs`
- `tests/runtime_instance.rs`
- `crates/nara_winit/src/tests.rs`
- `examples/support/runtime_retirement.rs`

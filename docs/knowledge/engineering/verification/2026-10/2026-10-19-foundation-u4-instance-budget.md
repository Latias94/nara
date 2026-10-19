---
type: Verification Evidence
title: Foundation U4 cumulative render scope instance budget
description: Runtime views and Editor targets share demand reservations with separate completion accounting.
timestamp: 2026-10-19T08:21:22Z
record_id: 91f633d4b6ba4a0aa137a959d501a654
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Scope And Contract

Initial implementation: `0b078a0` (`fix(render): enforce cumulative instance admission`).
The follow-up adds transaction statistics and native integration evidence to that implementation.

## Contract

One private `InstanceBudget` is created for each Runtime frame transaction or Editor Host render
transaction. Every prepared Sprite/UI submitter draw reserves its admitted instance bytes before
GPU buffer allocation. A failed preparation drops the reservation. A successful queue submission
commits the reservation for that scope. Replacing the scope does not let an older reservation
mutate the new scope.

The existing per-draw 64 MiB admission remains the first check. The new owner-level check prevents
two individually legal views or targets from holding more than 64 MiB in one render scope. This
does not add cross-target atomic rollback after a previous target has already been submitted.

# Verification

All commands were run serially on Windows with `CARGO_BUILD_JOBS=1` and Rust 1.95.
The initial owner-level implementation passed:

- `cargo +1.95.0 nextest run -p nara_render_wgpu --all-features -E 'test(/backend::tests::/)' --test-threads=1`: 23 passed, 99 skipped.
- `cargo +1.95.0 nextest run -p nara_render_wgpu --all-features -E 'test(/budget::tests::/)' --test-threads=1`: 3 passed, 119 skipped.
- `cargo +1.95.0 nextest run -p nara_render_wgpu --all-features -E 'test(/quad::tests::/)' --test-threads=1`: 4 passed, 118 skipped.
- `cargo +1.95.0 nextest run -p nara_render_wgpu --all-features -E 'test(/editor_execution::tests::/)' --test-threads=1`: 13 passed, 109 skipped.
- `cargo +1.95.0 fmt --all` and `git diff --check`: passed.

The owner tests cover cumulative two-view rejection, scope replacement while an old committed
reservation is alive, checked-add overflow, RAII drop refund, commit retention and next-scope
recovery. These initial suites overlap; their counts must not be summed as a unique total.

The follow-up integration run `53e182db-ee98-46f6-b0ff-1ee4792b5bda` passed both new cases with
`--all-features --run-ignored all`, a serial test thread, `WGPU_BACKEND=dx12` and
`DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`:

- `view_probe::mixed_sprite_ui_scope_rejection_refunds_prepared_views_and_recovers`: real Runtime
  Sprite views and a UI overlay share admission. An over-budget overlay rejects and refunds all
  prepared views; dropping a legal candidate also refunds; a later legal preparation renders the
  expected red/green Sprite and blended UI pixels.
- `editor_execution::imgui::tests::bare_primary::editor_targets_share_submitted_scope_budget_and_recover_next_frame`:
  real Windows window/device initialization, two independent Editor image targets, successful first
  submission and typed second-target rejection, completion without refunding submitted demand,
  and both targets succeeding in the next Host frame. No surface presentation is claimed.

The integration tests use a private reduced test ceiling so the real production path can prove
aggregation without half a million quads. Production keeps the 64 MiB ceiling and the separate
two-by-40 MiB owner regression. There is no public test-only batch construction API.

Independent static review found no correctness blocker in the original reservation implementation
or the follow-up counters/integration cases. The counters report live candidate demand and
committed transaction demand from the same budget owner; they are not physical residency metrics.

# Completion Accounting And Cohort

`backend::tests::new_frame_demand_is_independent_of_older_physical_completion` now joins scope
reset with existing GPU-completion tokens. The old frame's 64-byte physical charge remains after
the new frame begins with zero demand. Completing that old token retires only its physical charge;
completing the new frame's token cannot refund its already submitted transaction demand.

Run `cdc554bd-49eb-41b2-aeb9-07f3724990d4` passed the complete WGPU all-features cohort: 122 passed
and three native-desktop cases ignored. This includes the completion-token regression and mixed
Sprite/UI readback. The new ignored two-target case was run successfully in the native integration
run above; the two older native cases are not claimed by this cohort. U4's focused acceptance is
complete. The final workspace gate remains a separate plan-wide requirement.
Editor physical completion tracking was not expanded into a separate framework.

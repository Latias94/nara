---
type: "Verification Evidence"
title: "Editor authoring task budget and Bevy feature qualification"
description: "Scoped authoring pool correction, before/after workload evidence, custody tests and Bevy ECS feature trial."
timestamp: 2026-10-21T07:16:13Z
record_id: "b9b5f36f37fd4db8858dfec9a08a3e33"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-21-task-execution-budget-plan.md"
git_branch: "main"
git_commit: "bd07f2877f3b4398007225d84d5de96d6ff04f92"
verified_by: "Rust 1.95.0, serial nextest, focused native DX12 supervised Play, portable wasm check, binary inspection, Windows process sampling"
---

# Verification

T1–T3 passed for the fixed Editor authoring allocation and the inspected Bevy ECS feature trial.
The authoring App now installs a private 2/1/1 worker configuration (I/O/Compute/AsyncCompute),
retaining default queue limits and shutdown policy. The ECS default feature set is unchanged because
the `async_executor` trial produced no size benefit and did not remove the package from the Editor
dependency graph.

# Result

There is no material owner-workload latency regression in the small measured sample, and the named
worker reduction is directly attributable to the fixed authoring configuration. Existing bounded
admission, cancellation and physical close contracts remain covered by focused tests and the task
system suite.

# Evidence

All Cargo commands ran serially in the shared `target` directory with Rust 1.95.0. The repository
was on `main` at `bd07f2877f3b4398007225d84d5de96d6ff04f92`; the verified dirty source scope was:

- `src/project_host/runtime/editor/authoring_runtime.rs`
- `src/project_host/runtime/editor.rs`
- `src/project_host/runtime/tests.rs`
- `tests/support/editor_responsiveness_measurement.rs`
- `docs/research/2026-10-21-task-execution-baseline.md`
- `docs/plans/2026-10-21-task-execution-budget-plan.md`

The unrelated untracked `vendor/` directory was preserved and excluded.

- `cargo +1.95.0 fmt --all -- --check`: pass.
- `git diff --check`: pass.
- Authoring pool and close/import counter tests, after restoring default ECS feature selection:
  `cargo +1.95.0 nextest run -p nara --lib --all-features --locked --offline -E 'test(generic_editor_image_import_publishes_metadata_and_preserves_last_good_on_failure) | test(editor_project_close_retries_timed_out_image_task_owner_before_releasing_session) | test(project_host::runtime::editor::authoring_runtime::task_config_tests::)' --test-threads=1`:
  **4/4 pass**.
- Task, reflection and derive contracts during the feature-off trial:
  `cargo +1.95.0 nextest run -p nara_ecs -p nara_reflect -p nara_tasks --locked --offline --test-threads=1`:
  **236/236 pass**.
- Ignored owner-workload probe before and after the worker change:
  `cargo +1.95.0 nextest run -p nara --test editor_persistence --features 'tooling runtime-2d serde' --locked --offline --run-ignored only --success-output immediate -E 'test(measure_editor_owner_thread_work)' --test-threads=1`:
  **1/1 pass each run**, 16 samples at each of 64, 1,024 and 4,096 source entries. At 4,096
  entries, Open/Save/Reopen completion P50/P95 were 66.308/71.729, 66.327/69.678 and
  49.722/63.309 ms before; 66.625/75.725, 67.835/74.440 and 49.026/51.491 ms after. Results are
  small and mixed. Sixteen samples are too few for stable tail estimates; P95 and nearest-rank P99
  are the maximum observation. Local logs are under `target/` and are not committed.
- Focused Windows/DX12 supervised Play test:
  `cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_imgui_supervised_play_preserves_last_good_and_successor_generations)' --test-threads=1`:
  pass. This covers the actual managed child build, last-good preservation, successor generation
  and normal child shutdown; it is not the full native Editor qualification group.
- During the ECS `async_executor` feature-off trial, the root, Editor, headless example and
  `wasm32-unknown-unknown` portable library checks passed. The 236-test ECS/reflection/task cohort
  also passed. The trial configuration was reverted; `Cargo.toml` and `Cargo.lock` have no diff.
- Feature-off versus default-feature binary sizes: Editor debug 70,740,480 vs 70,698,496 bytes
  (feature-off was 41,984 bytes larger); headless release `hello_world` 6,784,000 vs 6,778,880
  bytes (feature-off was 5,120 bytes, or 0.076%, larger). The `async-executor` package remained
  in the broader Editor graph through an unrelated dependency path.
- Windows idle process sample: 86 to 48 total OS threads and 53 to 7 named Nara task workers
  after the 2/1/1 authoring allocation. Private bytes changed from 459.6 to 431.7 MiB; working set
  from 260 to 221.2 MiB. These are short samples from one 32-logical-processor machine, not
  controlled repeated benchmarks. Normal close and process exit were observed in both runs.

The image-import test snapshots actual authoring `TaskPools` counters: I/O admission, completion
and result take advance. The held-import close test verifies shutdown requests cancellation.
`TaskStats::oldest_queued_age` is not an admission-to-start distribution, so queue-wait P99 was
not measured. Owner-drive time excludes GPU completion, so GPU frame P99 was not measured. These
observations support only the scoped fixed-pool change.

# Follow-up

No task-runtime module extraction was needed because queue, terminal-state and retirement contracts
were not changed. The measured change does not justify a general worker cap for Game runtimes or
arbitrary plugins. Broader simultaneous-owner/repeated-session matrices, true queue-wait tails and
end-to-end GPU frame latency remain separate qualification work.

See the [execution baseline](../../../../research/2026-10-21-task-execution-baseline.md) and
[budget plan](../../../../plans/2026-10-21-task-execution-budget-plan.md) for evidence and limits.

# Citations

- [Authoring runtime composition](../../../../../src/project_host/runtime/editor/authoring_runtime.rs)
- [Authoring pool counter and shutdown tests](../../../../../src/project_host/runtime/tests.rs)
- [Owner workload measurement harness](../../../../../tests/support/editor_responsiveness_measurement.rs)
- [Native Editor qualification procedure](../../../../verification/native-editor-qualification.md)

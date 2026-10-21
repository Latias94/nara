# Task Execution Baseline And Feature Boundaries

Date: 2026-10-21. Initial consumer baseline: `bf8711f`, using its qualified Windows release export.
This follows the [initial study](2026-10-21-task-dependency-and-deprecation-preliminary.md).
The initial snapshot measured thread attribution only. Follow-up Editor pool and owner-workload
measurements are recorded below; they do not establish GPU frame latency or task queue-wait tails.

## Findings

The immediate execution concern is Nara's eager default worker allocation, not a demonstrated
duplicate Bevy worker pool. Preserve the existing task custody contract while measuring changes
to worker creation and allocation. Do not replace the ECS or add a generic executor interface
merely because both task packages appear in Cargo.lock.

### Enabled dependency profiles

Locked offline `cargo +1.95.0 tree -e features -i bevy_tasks --all-features` was captured separately
for root `nara`, `nara-editor`, the Reference Game and the two-level sample. Every inspected graph
selects **one `bevy_tasks 0.20.0`** through `bevy_ecs`; `async_executor` and `futures-lite` are
enabled, and `multi_threaded` is absent. Local logs: `target/tasks-*-feature-graph.log`.
This extends the preliminary single-package observation to those four actual feature unions.

The corresponding Bevy single-threaded pool implementation does not create worker threads.
Its global task handles are initialized lazily. Full Bevy's App TaskPoolPlugin is a separate
composition; Nara's dependency on ECS does not install that plugin.

### Actual Windows idle sample

The exported desktop starts an ordinary interactive Meadow session. A local Windows probe waits
for its visible window, then takes three process-scoped thread snapshots at one-second intervals.
It reads thread descriptions, posts WM_CLOSE only to that child's visible window and waits for
normal exit. Startup output confirms Meadow; exit is **0**. No gameplay job was submitted by the
probe. These are observations of this one run, not an idle CPU or tail-latency benchmark.

| Snapshot | Nara I/O workers | Nara Compute workers | Nara AsyncCompute workers | Other threads | Total |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | 32 | 16 | 17 | 67 |
| 2 | 2 | 32 | 16 | 38 | 88 |
| 3 | 2 | 32 | 16 | 38 | 88 |

The final snapshot's Other group includes three named Nara custody threads:
`nara-task-owner-reaper`, `nara-task-owner-drop-0` and `nara-task-owner-drop-1`.
It also includes main, WGPU WGL, four named D3D background threads and 29 unnamed threads.
Do not attribute unnamed or graphics threads to Nara task workers. The measured Nara task total
is **50 worker threads plus 3 custody threads**. There is no observed second threaded Bevy pool.

The first probe treated only HRESULT zero as success and could not name the threads. Windows
HRESULT also permits positive success values; the corrected probe uses a nonnegative success
test. Only its subsequent complete named snapshots support the attribution above.
Local evidence: `target/task-thread-observation.json`, `target/task-thread-desktop.*.log` and the
single-purpose `target/task_thread_observation.py`. The probe is not a maintained product tool.

`TaskPoolConfig::default` derives Nara's two I/O workers, one full available-parallelism Compute
allocation and another half-parallelism AsyncCompute allocation. `spawn_task_worker` creates
named native workers during pool construction. The observed 2/32/16 matches that policy.
Default TaskPlugin installation also appears in both the product and authoring runtime paths;
this is a reason to measure concurrent owners, not evidence that a particular Editor session
already has multiple copies of all 50 workers.

## Comparison With Pinned Bevy

Read-only `repo-ref/bevy` is `85503941e488152711a4695bbe248e9ed9f453fc`.
Its full App defaults allocate I/O and async-compute fractions with per-kind caps, then assign
remaining capacity to Compute. With 32 available threads, that policy calculates 4/4/24, rather
than Nara's 2/16/32. This is a source-derived comparison; no equivalent Bevy application was timed.

Bevy's process-global pools and Nara's runtime-owned physical close are different contracts.
A runtime cannot truthfully claim that it joined a shared global pool. Reuse would need exact
per-runtime outstanding-work and physical-return evidence, while retaining cancellation,
admission bounds, result ownership, panic isolation and retryable shutdown. Lower worker count
alone cannot establish that an adapter preserves those guarantees.

## Feature Trimming Candidates

| Feature | Actual boundary | Treatment |
| --- | --- | --- |
| `std` | Current Nara is a std engine; ECS std also enables utility parallel support | Retain; no no_std refactor is justified. |
| ECS `bevy_reflect` | No direct first-party ReflectComponent consumer was found, but `nara_ecs` reexports Bevy's prelude, including feature-gated reflective types | Trial only with public/derive/independent consumer checks; removal changes the exposed substrate surface. |
| `bevy_reflect` package itself | `nara_reflect` directly owns a Bevy TypeRegistry and GetTypeRegistration integration and publicly reexports reflection | Required today; disabling the ECS bridge does not remove this package or prove a size saving. |
| `async_executor` | All inspected ECS profiles remain single-threaded; no first-party use of Bevy's Future executor was found | A bounded feature experiment is plausible; task dependency presence may remain and Nara's 50 workers are unaffected. |
| `backtrace` | BevyError captures backtrace information when enabled; managed runtime failures consume that substrate | A debugging behavior tradeoff, not a free cleanup. Retain unless qualification and measured benefit justify changing it. |

This inspection did not modify features or choose an executor replacement. Prelude reachability
is a public interface fact even when first-party source does not directly name a type.

## Editor Authoring Pool Trial

The authoring App previously installed `TaskPlugin::default()`, which on this 32-logical-processor
machine created 2 I/O, 32 Compute and 16 AsyncCompute workers for one opened project. A source
trace found that the fixed authoring composition submits source-index, document-open, persistence,
image-import and image-reload work only to I/O. It found no production submission to either
authoring Compute queue. Startup preparation owns a separate 1/1/1 pool; external supervised Play
runs in another process. This attribution does not describe arbitrary project plugins.

The trial changes only the fixed Editor authoring pool to 2/1/1. It retains each queue's default
pending limit and the default shutdown policy. The startup and Game runtime compositions are
unchanged. Tests pin the configuration and its installation; adding a non-I/O authoring consumer
requires reassessing the allocation.

| Open-project idle observation | Before | After |
| --- | ---: | ---: |
| OS process threads | 86 | 48 |
| Named Nara task workers | 53 (authoring 2/32/16 plus startup 1/1/1) | 7 (authoring 2/1/1 plus startup 1/1/1) |
| Private bytes | 459.6 MiB | 431.7 MiB |
| Working set | 260 MiB | 221.2 MiB |
| Idle CPU mean / P95 over the recorded interval | 0.028% / 0.048% | 0.014% / 0.048% |
| Window close and process exit | normal close observed | normal close observed |

These are short process samples on one 32-logical-processor Windows host, not controlled repeated
benchmarks. Other OS, graphics and window threads account for the remainder and are not attributed
to Nara. The named worker count fell by 46; no claim is made that the whole OS process fell by the
same amount or that the CPU difference is statistically significant.

The ignored owner-workload test now waits for the source index to settle against its existing
deadline instead of driving a fixed 32 zero-duration frames. Matched 16-sample DEBUG runs covered
64, 1,024 and 4,096 source populations (the largest fixture was 483,534 bytes), reporting Open,
Save, Reopen, idle-drive and per-operation maximum owner-drive timings. At 4,096 entries:

| Measure | Before: P50 / P95 | After: P50 / P95 |
| --- | ---: | ---: |
| Open completion | 66.308 / 71.729 ms | 66.625 / 75.725 ms |
| Save completion | 66.327 / 69.678 ms | 67.835 / 74.440 ms |
| Reopen completion | 49.722 / 63.309 ms | 49.026 / 51.491 ms |
| Idle owner drive | 2.404 / 2.577 ms | 2.385 / 2.578 ms |
| Save operation's maximum owner-drive duration | 2.811 / 3.725 ms | 2.878 / 3.805 ms |
| Reopen operation's maximum owner-drive duration | 2.825 / 3.195 ms | 2.937 / 3.229 ms |

The measured owner-workload changes are small and mixed; this run shows no material latency shift
at its resolution. Each P95 is the maximum of only 16 samples, so neither P95 nor the matching
nearest-rank P99 is a stable tail-latency estimate. Local before/after logs are
`target/editor-owner-workload-nextest.log` and
`target/editor-owner-workload-nextest-after.log`.

Tests also sample the real authoring pool around image import: I/O admission, completion and result
take all advance; the held-import close test reports an I/O cancellation request. These assertions
verify which queue the workflow uses and that close enters cancellation, not task queue latency.

## Bevy ECS Feature Trial

The `bevy_ecs/async_executor` feature was disabled for a bounded trial while preserving `std`,
`bevy_reflect` and backtrace support. The ECS package, root crate, Editor and a headless example
compiled under the trial; the task/reflection/derive cohort passed 236/236 tests. The portable wasm
check also passed for the trial graph. No first-party consumer of Bevy's Future executor was found.
However, the `async-executor` package remained in the broader Editor dependency graph through an
unrelated dependency path, so this ECS feature toggle does not remove that package from the lockfile.

The matched feature-off builds did not produce a binary-size benefit:

| Artifact | Feature disabled | Default feature enabled | Difference |
| --- | ---: | ---: | ---: |
| Editor debug executable | 70,740,480 bytes | 70,698,496 bytes | disabled build +41,984 bytes |
| Headless `hello_world` release executable | 6,784,000 bytes | 6,778,880 bytes | disabled build +5,120 bytes (0.076%) |

Retain the default feature. This experiment changes no manifest or lockfile feature selection. The
test cohort and portable target compilation establish the trial API surface only; they do not prove
browser runtime support or a whole-workspace release build.

The current task statistics expose current queue age, not admission-to-start distributions, so
they cannot provide actual queue-wait P99. The owner-drive measurements also exclude native
WGPU completion and do not constitute an end-to-end GPU frame P99. Keep those boundaries explicit;
do not substitute completion latency or owner-thread time for either metric.

## Next Qualification

The [execution-budget plan](../plans/2026-10-21-task-execution-budget-plan.md) now records the
scoped result: use 2/1/1 only for the fixed Editor authoring pool, retain default Bevy ECS features,
and leave startup/game pools and public task interfaces unchanged. Queue-wait P99 and GPU-completion
frame P99 remain unmeasured. They require dedicated scheduler-boundary and native GPU-completion
instrumentation and are deferred unless a future change affects a used queue's scheduling or render
cadence. The 16-sample owner test and process samples are local qualification evidence, not a
portable benchmark or general worker cap.

## Source Anchors

- [Nara worker allocation and construction](../../crates/nara_tasks/src/runtime.rs)
- [Nara product task composition](../../src/product.rs)
- [Authoring runtime composition](../../src/project_host/runtime/editor/authoring_runtime.rs)
- [Editor authoring task and shutdown assertions](../../src/project_host/runtime/tests.rs)
- [Owner-workload measurement harness](../../tests/support/editor_responsiveness_measurement.rs)
- [Nara ECS reexports](../../crates/nara_ecs/src/lib.rs)
- [Native reflection registry](../../crates/nara_reflect/src/registry.rs)
- [Bevy pool policy](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/src/task_pool_plugin.rs)
- [Bevy ECS features](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/Cargo.toml)
- [Bevy task implementation selection](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/lib.rs)

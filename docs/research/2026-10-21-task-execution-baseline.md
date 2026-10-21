# Task Execution Baseline And Feature Boundaries

Date: 2026-10-21. Consumer baseline: `bf8711f`, using its qualified Windows release export.
This follows the [initial study](2026-10-21-task-dependency-and-deprecation-preliminary.md).
It measures one product's threads, not CPU cost, memory cost, frame latency or build attribution.

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

## Next Qualification

The [execution-budget plan](../plans/2026-10-21-task-execution-budget-plan.md) orders actual Editor
owner multiplicity and workload attribution before policy changes, then controlled feature trials.
It keeps private module extraction with the touched owner and avoids a new task kernel or blanket
async traits. Build size/time, idle CPU/memory, task latency and frame P99 remain unmeasured here.

## Source Anchors

- [Nara worker allocation and construction](../../crates/nara_tasks/src/runtime.rs)
- [Nara product task composition](../../src/product.rs)
- [Authoring runtime composition](../../src/project_host/runtime/editor/authoring_runtime.rs)
- [Nara ECS reexports](../../crates/nara_ecs/src/lib.rs)
- [Native reflection registry](../../crates/nara_reflect/src/registry.rs)
- [Bevy pool policy](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/src/task_pool_plugin.rs)
- [Bevy ECS features](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/Cargo.toml)
- [Bevy task implementation selection](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/lib.rs)

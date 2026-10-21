# Task Dependencies And Atomic Deprecation: Preliminary Findings

Date: 2026-10-21. Implementation basis: `104f14d`, with P3 consumer work still in progress.
This is a partial source investigation, not completion of P1-P3 or the subsequent cost study.
No runtime, build-time, binary-size or thread-count measurements were performed for this note.
The user-authorized follow-up is tracked in the [persistent two-level plan](../plans/2026-10-20-persistent-two-level-game-plan.md#follow-up-triggers).

## Atomic Rename

The coordinating agent checked installed Rust sources for `rustc 1.95.0`
(`59807616e`, 2026-04-14) and stable `1.99.0` (`b940084d7`, 2026-09-28).
`library/core/src/sync/atomic.rs` marks `try_update` stable since 1.95.0 and
`fetch_update` deprecated since 1.99.0. The latter forwards to the former with the same arguments.
Current [official Atomic documentation](https://doc.rust-lang.org/std/sync/atomic/struct.Atomic.html)
and the [AtomicU64 alias](https://doc.rust-lang.org/std/sync/atomic/type.AtomicU64.html),
checked on 2026-10-21, describe Rust 1.99.1.

Nara declares Rust 1.95 in [Cargo.toml](../../Cargo.toml). A rename therefore requires no MSRV
increase. Preserve both ordering arguments, closure behavior and `Result` handling; the closure
may run repeatedly during compare-exchange retries. This is API maintenance, not a demonstrated
performance optimization. Audit all first-party occurrences and toolchain warnings after P1-P3;
do not edit vendored dependencies or infer that every compiler warning has the same cause.
Current examples include `nara_tasks::TaskPools::allocate_task_id`, App ownership counters and
render identity counters. This note changes no Rust code.

## Actual Dependency Graph

The coordinating agent ran, against the locked offline graph:

```text
cargo +1.95.0 tree -p nara_tasks -e features -i bevy_tasks --locked --offline
```

- One `bevy_tasks 0.20.0` is selected through `bevy_ecs 0.20.0`; `nara_tasks` does not add
  another version. See [nara_tasks manifest](../../crates/nara_tasks/Cargo.toml),
  [nara_ecs manifest](../../crates/nara_ecs/Cargo.toml) and [Cargo.lock](../../Cargo.lock).
- ECS defaults are `std`, `bevy_reflect`, `async_executor`, `backtrace`.
  `async_executor` enables `bevy_tasks/async_executor`; the task dependency also includes
  `futures-lite`. The inspected package graph does **not** enable `multi_threaded`.
- The duplicate-package listing contains `hashbrown 0.16.1` and `0.17.1`.
  Repeated host/target entries of the same package are not evidence of duplicate linked code.
  A package graph alone does not establish retained code size or feature-specific build cost.

These observations apply to that command's selected graph. Recheck workspace, Editor,
reference-game, sample and independent consumer feature unions before generalizing them.

## Pool Construction And Responsibilities

Registry `bevy_tasks`/`bevy_ecs` 0.20.0 source was inspected alongside pinned Bevy
`85503941e488152711a4695bbe248e9ed9f453fc` under read-only `repo-ref/bevy`.
The following anchors identify the corresponding pinned implementations.

| Boundary | Source-grounded behavior |
| --- | --- |
| Bevy global pools | [`usages.rs:7-16`](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/usages.rs#L7-L16) uses `OnceLock` and `get_or_init`; importing the dependency does not initialize these pools. |
| Bevy pool implementation | [`lib.rs`](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/lib.rs) selects the single-threaded implementation without `multi_threaded`. Its `new_internal` creates no worker threads; `thread_num() == 1` denotes its execution model, not a spawned worker. |
| Bevy threaded construction | [`task_pool.rs:160`](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/task_pool.rs#L160) creates the requested workers when the threaded pool is constructed. Global initialization is lazy; worker creation inside that constructor is not. |
| ECS execution | [`executor/mod.rs:48`](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/schedule/executor/mod.rs#L48) chooses a single-threaded default without `multi_threaded`. The multi-threaded executor uses `ComputeTaskPool::get_or_init(TaskPool::default)` in its run path (`multi_threaded.rs:274`); parallel queries also use the compute pool when enabled. |
| Full Bevy App | [`task_pool_plugin.rs:32`](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/src/task_pool_plugin.rs#L32) explicitly initializes its default pools. This is a different composition from merely depending on `bevy_ecs`. |
| Nara product work | [`runtime.rs`](../../crates/nara_tasks/src/runtime.rs), `TaskPoolExecutor::threaded` and `TaskPlugin::build`, create configured workers during plugin construction. Three queue kinds do not mean three threads. |

Nara's default allocation is two IO workers, a compute allocation derived from available
parallelism, and an async-compute allocation derived from half that parallelism, subject to
per-kind and aggregate caps (`default_worker_allocation`). `AsyncCompute` executes owned
potentially blocking closures; it is explicitly **not** a Future executor. Production uses
threaded pools; the inline driver is a test control.

[ADR 0008](../architecture/adr/0008-runtime-concurrency-and-task-pools.md) separates World
scheduling from background integration. [ADR 0052](../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md)
adds bounded admission, domain-scoped pending coalescing, first-terminal results, cooperative
cancellation, diagnostics and physical shutdown custody. These duties are implemented in
`TaskPools`, `TaskState`, `TaskPoolsCloseOwner` and `TaskPoolsCloseParticipant`.

In particular, terminal cancellation does not prove that a closure, captures or worker returned.
Plugin-created close ownership remains in the runtime ledger even if its World-facing facade
is removed; unfinished Drop transfers custody to a process reaper. Existing
[execution tests](../../crates/nara_tasks/src/tests/execution.rs) and
[close tests](../../crates/nara_tasks/src/tests/close.rs) cover these distinctions, queue pressure,
panic isolation, partial construction failure and retryable uncooperative-worker shutdown.
This note did not rerun those tests.

Bevy's scoped ECS jobs and Future execution are relevant reuse candidates, but their existence
does not by itself replace Nara's observable admission and close protocol. Conversely, Nara's
custody contract does not prove that its current execution mechanism or worker allocation is optimal.

## Bounded Follow-Up After P1-P3

1. Inventory first-party deprecations on MSRV and current stable. Apply the equivalent atomic
   rename separately, retaining ordering/exhaustion tests; distinguish dependency warnings.
2. Record exact enabled graphs per supported product/feature profile. Trial disabling individual
   unneeded ECS defaults only after tracing reflection, error/backtrace and executor consumers.
   Do not assume removing `async_executor` removes `bevy_tasks`: ECS still depends on it.
3. Measure native Editor idle, Play, Stop, repeated sessions and headless execution: actual thread
   counts/creation timing, idle CPU, committed memory, task latency and frame P99. Separate compiler,
   GPU/window and process-reaper threads from task workers. Include concurrent live runtime owners.
4. Attribute serial clean/incremental build time and optimized executable size under matched
   toolchains, features, profiles and cache conditions. Dependency names and lockfile duplication
   are inputs to attribution, not cost results.
5. Compare the alternatives below on the same real Save/Open/content-load consumer and held-work
   close/cancel tests before selecting an implementation or changing an Accepted contract.

| Alternative | Required proof before adoption |
| --- | --- |
| Trim unnecessary ECS features; retain Nara execution | All public/derive/independent consumers still compile; real size/build benefit and runtime behavior are measured. |
| Keep Nara custody and execute through a concrete Bevy pool adapter | Blocking work cannot starve frame jobs; bounded admission, affinity, physical-return evidence, panic behavior and retryable close remain truthful. A global pool cannot be falsely reported joined by one runtime. |
| Retain distinct ECS and Nara execution boundaries | Measured worker allocation, resource use and tail latency justify the separation; unnecessary idle or duplicate workers are removed where proven. |

No replacement scheduler, generic executor trait, larger thread budget or new public async API is
selected here. The open question is measured execution reuse **under the existing product custody
contract**, not whether two package names look redundant.

The subsequent [execution baseline](2026-10-21-task-execution-baseline.md) extends feature tracing to
four product graphs and records actual Windows thread attribution. Its execution-budget plan owns
the remaining owner/workload measurements and controlled corrections.

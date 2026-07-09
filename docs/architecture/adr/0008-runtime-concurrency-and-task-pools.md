# ADR 0008: Runtime Concurrency and Task Pools

**Status**: Accepted
**Date**: 2026-07-08

## Context

A mature game engine needs asynchronous IO, background asset loading/importing, compute jobs, hot reload, audio, optional render threading, and deterministic main-world updates.

Rust offers many async runtimes, but a game engine should not force gameplay systems to become general async application code. The main ECS `World` must remain deterministic and safely scheduled. Async work should enter the world through explicit channels, commands, resources, or asset state transitions.

Reference engines split responsibilities:

- Bevy uses task pools such as IO, async compute, and compute.
- Godot has a worker thread pool, threaded resource loading, and configurable render threading.

## Decision

nara will use an **engine-owned task pool model** with explicit main-thread integration.

Initial task classes:

```text
Main Thread
  App lifecycle, world mutation, window event loop, plugin lifecycle

Compute Pool
  CPU-heavy parallel jobs with bounded frame integration

Io Pool
  Asset file IO, watchers, non-blocking resource reads

Async Compute Pool
  Long-running background preparation, imports, decompression, procedural generation

Render Backend Thread
  Deferred. wgpu backend starts single-threaded/main-thread integrated, but the render seam must allow a future render thread.
```

```mermaid
flowchart TD
    Main[Main Thread / App Update] --> Schedule[ECS Schedules]
    Io[IO Task Pool] --> AssetEvents[Asset Load Events]
    Async[Async Compute Pool] --> ImportEvents[Import / Decode Results]
    Compute[Compute Pool] --> JobResults[Frame Job Results]
    AssetEvents --> Main
    ImportEvents --> Main
    JobResults --> Main
    Main --> Render[Render Backend]
    Render -. future .-> RenderThread[Optional Render Thread]
```

Core rules:

- ECS world mutation happens on the main app schedule unless a system is explicitly scheduled by `bevy_ecs` for safe parallel execution.
- Background tasks do not hold direct mutable references into the main `World`.
- Asset loading and import work report results through asset state transitions/events.
- The engine owns task pools; nara does not expose Tokio as the user-facing runtime contract.
- Headless deterministic mode must be possible.
- Render threading is a future optimization, not a Phase 1 requirement.

## Alternatives Considered

### Option A: Use Tokio as the public engine async model

**Pros**: Mature async ecosystem, easy networking/server patterns.

**Cons**: Poor fit as the main gameplay scheduling model; can leak async lifetimes and runtime assumptions into user code.

**Decision**: Rejected as the public engine model. Tokio or async runtimes may still appear behind adapters if needed.

### Option B: Single-threaded runtime only

**Pros**: Simple deterministic behavior.

**Cons**: Asset IO, import, hot reload, procedural generation, and editor tooling will block or become ad hoc.

**Decision**: Rejected for a mature engine target.

### Option C: Engine-owned task pools with explicit integration (Chosen)

**Pros**: Mature engine shape, clear main-world ownership, easy to reason about, compatible with asset pipeline and future render threading.

**Cons**: More infrastructure than a minimal runtime; requires task lifecycle and diagnostics.

**Decision**: Chosen.

## Consequences

- `nara_tasks` or equivalent task infrastructure should exist before serious asset loading/importing.
- `nara_app` should define where task results are ticked/applied, likely in fixed stages.
- Asset and scene loading APIs should model pending/loading/ready/failed states.
- Render backend seams should avoid assuming all GPU work always happens inside gameplay systems.
- Tests should support a deterministic single-threaded configuration.

## Implementation Notes

- `nara_tasks` is the engine-owned task crate. It exposes `TaskPoolKind`, `TaskExecutionMode`,
  `TaskPoolConfig`, `TaskPools`, `TaskHandle<T>`, `TaskCancellationToken`, `TaskResult<T>`, and
  `TaskStats`.
- `TaskExecutionMode::Deterministic` executes submitted work inline and still requires explicit
  result polling. This keeps tests predictable without changing the main-thread integration model.
- `TaskExecutionMode::Threaded` uses nara-owned std worker pools for IO, compute, and async-compute
  classes. Tokio and async-std remain implementation options for future adapters, not public engine
  contracts.
- `nara_app::CoreStage::TaskUpdate` is the first scheduled integration point for background work.
  `TaskUpdateSet::{Poll, CoalesceAssetChanges, SpawnAssetJobs, ApplyAssetResults}` defines the
  current frame-ordering contract.
- Background tasks own their inputs and return data through `TaskHandle<T>`. They do not borrow or
  mutate `World`.
- Cancellation is cooperative through `TaskCancellationToken`. Asset reload uses generations to
  ignore stale results before mutating asset state.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Main world safety | Background tasks cannot mutate `World` directly | API review and task handle API |
| Asset async path | Asset load results integrate through scheduled events/states | Image reload tests |
| Deterministic tests | Single-threaded task mode exists | `nara_tasks` deterministic tests |
| Runtime independence | User code is not forced to use Tokio | Dependency/API review |
| Render extensibility | Render backend can later move to separate thread without changing gameplay components | Design review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Task pools overcomplicate Phase 1 | Medium | Medium | Define interfaces now; implement minimal single-thread executor first if needed |
| Async tasks outlive assets/world state | High | Medium | Use handles, generations, cancellation tokens, and scheduled apply points |
| Render thread design conflicts with wgpu constraints | High | Medium | Start single-threaded; validate wgpu ownership before enabling separate render thread |
| Tests become nondeterministic | Medium | Medium | Provide deterministic task ticking and single-thread pool configuration |

## Follow-Up Questions

- Should task pool worker sizing become app-configurable from `nara.toml` or stay explicit code-first
  setup only?
- Should networking/scripting use a separate runtime model later?
- What diagnostics should long-running or repeatedly failing tasks emit?

## Citations

- Bevy reference: `repo-ref/bevy/crates/bevy_app/src/task_pool_plugin.rs`
- Godot reference: `repo-ref/godot/main/main.cpp`, `repo-ref/godot/core/io/resource_loader.h`
- Asset identity decision: [0007-asset-identity-and-import-pipeline.md](0007-asset-identity-and-import-pipeline.md)

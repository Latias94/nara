# Owned Editor Reopen: Preparation And Owner Publication

This follows the frozen [v1 synchronous investigation](2026-10-20-editor-responsiveness.md),
[v2 owned Save](2026-10-20-owned-editor-save.md) and the
[Reopen plan](../plans/2026-10-20-owned-editor-reopen-plan.md). Earlier raw data stays unchanged.
Final product and measurement consumer source: `8f5003b` (baseline `4356001`). Verification
executes its pre-commit tree; subsequent closure changes are documentation/data only.

## Ownership And Affinity

Reopen admission captures exact document/kind, complete base revision, source binding, original
lane path and authoring schema admission. The worker owns a capability-bound read source and
bounded decoder. Pure authoring classification and unavailable-record preservation run there;
native registry codecs and migrations execute only on owner admission. The original publication
Host stays in the session, retaining its conflict baseline and any temporary cleanup.

Before native callbacks or Workspace replacement, the owner checks every captured fact and the
existing trusted Scene Runtime exclusion. A later edit rejects the result, preserving document,
history, source incarnation, saved checkpoint and Host baseline. Only successful publication
commits the read result's own identity and byte digest to the Host. Opening blocks new Play /
Restart / RunLastGood requests through ordinary, frame-bound and external routes. Existing
Prefab-running and already-running external-process semantics remain unchanged. Switching the
active tab cannot retarget a prepared result.

Reopen cancellation differs from Save: it has no irreversible file write to preserve. After
physical preparation returns, only `Completed(())` permits owner admission. Cancelled, failed
or rejected tasks discard their data and leave the owner unchanged. A mutex held through the
read/preparation prevents logical terminality from certifying physical return. Existing
TaskPlugin/runtime quarantine retains abandoned running closures; no second executor is added.

## Protocol

Use the same local Windows 11 / NTFS / Rust 1.95.0 release environment and public ignored consumer
as v2. The envelope is now `editor-owner-thread-v3`, with Reopen request, first drive, maximum
drive across that operation and request-to-observed-completion reported separately. Save's v2
phase names remain for comparison; the envelope labels the complete v3 experiment.

Each process covers 64 / 1,024 / 4,096 shallow independent empty entities, two warm-up cycles
and sixteen retained samples per population. A new session/pool is opened and physically closed
each cycle. Schema/fixture creation, outputs, byte/revision/digest oracles and the restore of
baseline bytes remain outside timing. Source indexing drains before persistence. Baseline source
sizes remain 7,758 / 121,038 / 483,534 bytes. Open and idle phases remain separately visible.

Completion includes request admission, scheduling, 1 ms measurement polling, owner admission and
frame projection. It does not measure native UI latency under 16 ms maintenance, pure worker
execution, total owner CPU or storage speed. Per-operation maximum drive differs from v1's one
complete synchronous Reopen drive. Each per-process nearest-rank P95 equals max at sixteen
samples and is not a stable tail estimate. Keep the 8 ms threshold as a local investigation
trigger, not an SLA, timing assertion or supported entity ceiling.

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.95.0 nextest run --cargo-profile release --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --test editor_persistence --locked --run-ignored only -E 'test(measure_editor_owner_thread_work)' --no-capture
```

## Results

Three optimized processes pass byte/revision/digest, actual Reopen replacement and physical-close
oracles. Each retains 672 numeric rows across fourteen phases, three populations and sixteen
samples per group. Process values remain separate; no independent percentiles are subtracted.

| 4,096 entities / Reopen phase | Run 1 P50 / P95 ms | Run 2 P50 / P95 ms | Run 3 P50 / P95 ms |
| --- | --- | --- | --- |
| Request | 0.005 / 0.005 | 0.004 / 0.005 | 0.004 / 0.005 |
| First owner drive | 0.314 / 0.403 | 0.315 / 0.402 | 0.312 / 0.455 |
| Maximum owner drive per Reopen | 0.495 / 0.965 | 0.513 / 0.780 | 0.571 / 0.881 |
| Completion observation | 10.986 / 11.976 | 11.433 / 13.020 | 11.294 / 11.765 |

The frozen v1 full synchronous Reopen drive has P50 9.706 / 9.729 / 9.648 ms and P95
10.394 / 10.104 / 10.309 ms. The new maximum owner call is below the local 8 ms investigation
trigger in every retained sample; completion now includes scheduling and polling. This supports
reduced per-call owner blocking in this fixture, not faster storage, less total work, native UI
latency or a general performance guarantee. Native codec costs are absent from these data-only
empty-entity probes.

The [summary](data/2026-10-20-owned-editor-reopen/summary.json) retains all 126 per-process groups;
raw rows are [run 1](data/2026-10-20-owned-editor-reopen/run-1.jsonl.gz),
[run 2](data/2026-10-20-owned-editor-reopen/run-2.jsonl.gz) and
[run 3](data/2026-10-20-owned-editor-reopen/run-3.jsonl.gz).
Run IDs: `4e792315-9926-4c17-8b61-2f0371481bca`, `dfa596a3-7e4f-458e-b2ae-049f28b1ce48`
and `0b57d544-4a26-4ef2-b11f-4af5b3ca6deb`. Final source binding is recorded with the verification.

## Concurrency Limits And Next Work

This is DetectOnly, not a consistent-file snapshot or strong CAS. An external write after read
may follow the prepared bytes before publication. The owner publishes the captured bytes and
their matching baseline; it never overwrites the later external file. A subsequent Save detects
the successor as a conflict. Another late read would still leave a check/use window and is not
added as a false transaction guarantee. Same-length concurrent mutation during read remains
outside `read_to_end_bounded`'s snapshot guarantee. ADR 0091 remains Proposed.

The empty-entity experiment does not characterize complex native codecs, large history disposal,
Prefab content refresh, bound external-Play source receipts, Browser Open, startup closure,
cold/slow storage or native redraw interruption. Native codecs and owner projection can remain
material synchronous work; measure a real consumer before changing their affinity or contract.

Independent review also found a **pre-existing post-terminal cleanup gap**: after Save returns its
Host to the session, operation-scoped Save custody no longer retains it. Dropping that session
can abandon retained `TemporaryFileCleanup`, whose failed-cleanup form does not retry on Drop.
Reopen's read-task quarantine does not repair this gap. Normal explicit close/retry remains
covered, and the new Drop test covers a successful read without pre-existing failed cleanup.
The next ownership repair should retain each Scene/Prefab Host for its entire lane lifetime,
including after runtime stop and lane replacement. It should precede additional startup/source
latency work and must not fabricate an idle `SaveWork` to hide a separate retirement obligation.

The [verification record](../knowledge/engineering/verification/2026-10/2026-10-20T022302Z-owned-editor-reopen-preparation-and-publication-qualification-557ea95db99b4b8a87829870db1d6b13.md)
records actual tests, source review and selected Windows/DX12 qualification. The three native
trajectories do not establish other platforms, monitor scanout, accessibility or a full release.

## Mature Engine Source Comparison

Local Bevy `85503941` integrates owned load results through
[`handle_internal_asset_events`](../../repo-ref/bevy/crates/bevy_asset/src/server/mod.rs)
with `&mut World`, and its asset info owner discards results whose corresponding handle no
longer exists. The applicable lesson is that prepared data and permission to publish it have
different owners, rather than exposing arbitrary owner state to a worker.

Local Godot `81ab961f` exposes threaded request/status/result in
[`resource_loader.cpp`](../../repo-ref/godot/core/io/resource_loader.cpp), but retrieving an
unfinished result can wait and drive service progress; it also handles main-thread connection
transfer explicitly. Nara's every-frame poll uses nonblocking observation and retains native
callback affinity rather than copying a blocking get operation. Neither comparison establishes
Nara's file transaction guarantees or justifies an async owner trait.

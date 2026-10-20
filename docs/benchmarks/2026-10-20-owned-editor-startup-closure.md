# Owned Editor Startup-Closure Preparation

Status: implementation `eb1ee98` and qualification pass within the
[verification scope](../knowledge/engineering/verification/2026-10/2026-10-20T032446Z-owned-startup-closure-publication-and-responsiveness-qualification-f187b61104d4443187cea8b9bc392460.md). See the
[bounded plan](../plans/2026-10-20-owned-editor-startup-closure-plan.md).

The [v1 ordinary Editor experiment](2026-10-20-editor-responsiveness.md) measured startup-closure
owner calls above the private 8 ms investigation trigger at 4,096 empty entities. This follow-up
moves that concrete preparation to the existing task executor. It follows the
[Host lifetime cleanup correction](../plans/2026-10-20-editor-host-lifetime-custody-plan.md).
Earlier v1/v2/v3 raw data remains frozen.

## Ownership And Observation

The linear Open attempt owns the exact prepared project/schema capabilities and budgets. Startup
file reads, decode, dependency traversal and pure schema classification may execute on a worker.
Only a completed result associated with that live attempt may become an authoring receipt.
Compiled Adapter admission, native Plugin/registry construction, session/runtime creation and
Workspace publication stay on the owner thread. There is no async Plugin or codec trait.

Dropping the attempt revokes publication interest and requests cooperative cancellation. It does
not preempt OS reads. The existing standalone task owner retains running captures through its
reaper. Failed/cancelled/rejected preparation cannot publish a partial or late session. An Open
Host reuses explicitly small worker/queue limits rather than creating a runtime or pool per attempt.

The application compares phase changes before requesting presentation. Repeated pending polls
within StartupClosure do not require repaint; existing native Opening maintenance still observes
completion. Retained-project replacement, cancellation and Save-before-refresh keep their existing
owners and ordering.

## Measurement Protocol

Use the same Windows 11/Rust 1.95 optimized ordinary data-only fixture as v1/v2/v3: 64 / 1,024 /
4,096 shallow empty entities, two warmups and sixteen retained samples per population, three
separate serial processes, shared target, `CARGO_BUILD_JOBS=1`, local NTFS and ordinary background
work. These are scale probes, not rendered-game or cold-storage benchmarks.

The `editor-owner-thread-v4` envelope records startup first/max owner advance and observed startup
completion separately. Completion includes scheduling, 1 ms measurement polling and phase-output
instrumentation. First advance includes lazy task-owner creation. `open-v4-completion` spans the
first advance through session observation; capability acquisition, Host construction and `begin`
are outside that interval. It includes the other synchronous Open stages. SessionPublication still
measures actual session/runtime construction and initial projection.

A separate ordinary linear attempt reaches SessionPublication, then is dropped before creating
a session. `startup-v4-prepared-discard` measures actual owner disposal of that prepared receipt.
It is not an untaken TaskTerminal destructor measurement, total worker CPU, physical cancellation
latency or a generic user-destructor guarantee. Save/Reopen retain their earlier phase names in
the new envelope so incidental measurements remain visible without rewriting historical data.

Each process should retain 864 rows (18 phases × 3 populations × 16 samples). With sixteen samples,
nearest-rank P95 equals max; neither the private trigger nor these samples establish a native SLA
or a stable tail estimate. Record process identities and retain raw numeric rows after verification.

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.95.0 nextest run --cargo-profile release --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --test editor_persistence --locked --run-ignored only -E 'test(measure_editor_owner_thread_work)' --no-capture
```

## Results And Limits

All three optimized processes pass the ordinary source/Save/Reopen and physical session-close
oracles. Each retains 864 rows; the [summary](data/2026-10-20-owned-editor-startup-closure/summary.json)
contains 162 groups. Raw numeric data: [run 1](data/2026-10-20-owned-editor-startup-closure/run-1.jsonl.gz),
[run 2](data/2026-10-20-owned-editor-startup-closure/run-2.jsonl.gz),
[run 3](data/2026-10-20-owned-editor-startup-closure/run-3.jsonl.gz). Nextest identities are
`e0dc126a-d8e1-426f-8ab7-c230f089b621`, `d0874c4c-a85d-44c1-955b-9478c76832f7`,
`7579024b-f48a-436f-8bf5-5c18c8c65848`.

Figures are P50 / P95 milliseconds, separately by process, at 4,096 empty entities. With sixteen
samples per row P95 equals max; individual percentiles are not subtracted to invent pure I/O cost.

| Operation | Run 1 P50 / P95 | Run 2 P50 / P95 | Run 3 P50 / P95 |
| --- | --- | --- | --- |
| v1 synchronous startup owner call | 9.920 / 11.194 | 10.048 / 10.312 | 9.911 / 12.028 |
| v4 startup first owner advance | 0.059 / 0.066 | 0.059 / 0.081 | 0.065 / 0.077 |
| v4 startup max owner advance | 0.059 / 0.066 | 0.059 / 0.081 | 0.065 / 0.077 |
| v4 startup completion observation | 10.733 / 11.611 | 10.638 / 11.629 | 10.721 / 11.649 |
| v4 full Open completion observation | 20.254 / 21.863 | 20.231 / 21.604 | 20.473 / 21.628 |
| v4 prepared receipt disposal | 0.222 / 0.382 | 0.244 / 0.347 | 0.261 / 0.533 |
| v4 session publication owner call | 5.923 / 6.388 | 6.008 / 6.682 | 6.055 / 6.550 |

The startup owner calls no longer cross the private 8 ms investigation trigger for this fixture.
This improvement concerns caller occupancy: the closure still takes roughly 11 ms to be observed,
and full Open about 20-22 ms. It does not establish faster storage or lower total CPU. Owner-only
session creation remains a material synchronous call, below the trigger in these retained rows.
The first and max startup rows happen to coincide for this workload; no general first-call bound
follows. The additional disposal attempt runs outside the full Open span and before incidental
Save/Reopen measurements; do not treat those later phases as an isolated causal comparison with
earlier envelopes.

Native callback affinity, source consistency, unknown-record preservation, budget diagnostics and
physical task retirement remain independent acceptance requirements. Selected native Manager,
Scene/Prefab, Canvas and paused-resume trajectories pass, but these timings do not measure the
native 16 ms polling delay, input-to-display latency or monitor scanout.

Browser document admission, bound Play source receipts, compiled content refresh, slow storage,
complex native codecs and native input-to-display latency remain separate measurements. This slice
does not add multi-document ownership, a Future executor, stronger cross-file snapshots or CAS.

The final owner sweep found a separate Image metadata cleanup abandonment gap. Its
[publication custody repair](../plans/2026-10-20-editor-image-publication-custody-plan.md) is the next
correctness priority; the startup measurements and passing selected consumers do not qualify it.

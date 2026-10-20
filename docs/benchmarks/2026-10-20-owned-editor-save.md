# Owned Editor Save: Owner Responsiveness

This follows the frozen [synchronous investigation](2026-10-20-editor-responsiveness.md)
and the [Save implementation plan](../plans/2026-10-20-owned-editor-save-plan.md). The workload
and local Windows/NTFS/Rust 1.95.0 release environment remain the same. The original v1 raw
samples remain unchanged. This experiment examines owner-call responsiveness after moving
encoding, authoring re-admission and file publication to the existing task pool.
Final lower-owner source is `78c8ddd`; Editor/consumer source is `1a90307`. The formal
measurements precede final error-routing and native editing-policy corrections; the measured
successful request, worker, publication and acknowledgement paths are unchanged by those fixes.
The subsequent [owned Reopen qualification](2026-10-20-owned-editor-reopen.md) implements read
preparation separately; synchronous-Reopen statements here describe the frozen v2 slice.

## Interpretation And Protocol

The ignored public consumer remains
[editor_responsiveness_measurement.rs](../../tests/support/editor_responsiveness_measurement.rs).
The v2 numeric envelope is `editor-owner-thread-v2`. Each process creates fresh sessions for
64 / 1,024 / 4,096 shallow empty entities, with two warm-up cycles and sixteen retained samples
per population. Each session physically closes before the next cycle. Warm-ups warm code,
storage caches and system state; they do not reuse one persistent worker pool.

| Phase | Measured interval |
| --- | --- |
| `save-v2-request` | Ordinary Save action admission, checkpoint and matching authoring-basis capture |
| `save-v2-first-drive` | First owner drive, including task admission and frame publication |
| `save-v2-max-drive` | Maximum complete owner drive among all calls for that Save |
| `save-v2-completion` | Request start through owner observation of the terminal Saved result |

Completion includes queueing, worker scheduling, the measurement driver's 1 ms polling and
owner acknowledgement. It is neither pure worker execution nor storage duration. The native
Host uses 16 ms maintenance, so these figures do not measure native UI save latency. Maximum
drive is a maximum within each operation; its per-process P50/P95 distribution differs from
v1's one synchronous full Save call. Comparing them identifies reduced blocking per owner call,
without claiming reduced total computation, total owner CPU work or faster storage.

Open/Reopen and idle phases remain separately visible. Result identity, captured revision,
persisted digest/bytes, dirty state, Reopen contents and physical session close are asserted
outside timing. The fixture contains no components, images or Prefabs and no bound external
Play receipt. It does not qualify slow/cold storage, complex game content, Browser Open,
code-currentness or native presentation responsiveness.

Run one process at a time, using the shared target and `CARGO_BUILD_JOBS=1`:

```powershell
cargo +1.95.0 nextest run --cargo-profile release --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --test editor_persistence --locked --run-ignored only -E 'test(measure_editor_owner_thread_work)' --no-capture
```

## Worker Phase Attribution

A separate exploratory process uses temporary private probes around Scene encoding,
authoring re-admission/preservation validation and file publication. These probes describe the
refactored worker path. They cannot retroactively allocate the frozen v1 synchronous 15 ms call
to those phases. Probe output and its surrounding scheduling can affect the experiment; this
process is excluded from the three formal v2 owner-call runs. The probes are removed before
building the final measurement consumer, leaving no production environment variable, logging
or timing API.

Run `eba73d6b-acf3-41c4-a9e1-a69f5030c928` passed. Its 162 rows include two labelled warm-ups
and sixteen timed samples for each population and phase. The phase values are independently
measured; adding their independent medians or percentiles would not reconstruct a complete call.

| 4,096 entities / worker phase | P50 / P95 ms |
| --- | --- |
| Scene encoding | 1.754 / 2.137 |
| Authoring decode, classification and preservation validation | 9.419 / 11.959 |
| File publication, including conflict observation, sync and replacement | 4.838 / 6.354 |

Validation alone is material on this fixture, supporting moving the full owned path rather
than only the final write. This does not justify removing validation or changing its safety
contract. [Phase rows](data/2026-10-20-owned-editor-save/worker-phases.jsonl.gz) and
[summary](data/2026-10-20-owned-editor-save/worker-phases-summary.json) retain all populations.

## Results

All three probe-free optimized processes pass the public byte/revision/digest and physical-close
oracles. Each retains 576 numeric rows: sixteen samples across three populations and twelve
phases. P95 is nearest-rank and equals max with sixteen samples; these are separate processes,
not pooled observations or stable tail estimates. The source sizes remain 7,758 / 121,038 /
483,534 bytes. Every successful Save contains the extra captured edit.

Figures below are per-process P50 / P95 milliseconds.

| Population / phase | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| 4,096 / request | 0.220 / 0.409 | 0.230 / 0.298 | 0.257 / 0.323 |
| 4,096 / first drive | 0.407 / 0.676 | 0.402 / 0.520 | 0.404 / 0.548 |
| 4,096 / maximum drive per Save | 0.716 / 1.359 | 0.693 / 1.645 | 0.763 / 1.072 |
| 4,096 / completion observation | 17.379 / 19.478 | 17.907 / 18.935 | 17.803 / 22.120 |

For context, the frozen v1 full synchronous Save drive has P50 14.973 / 14.926 / 15.117 ms and
P95 16.357 / 15.559 / 15.792 ms. The v2 owner call now returns between worker steps and observes
completion later. Its measured per-operation maximum drive is below the local 8 ms investigation
trigger in every retained sample, while completion-observation P50 spans 17.379-17.907 ms and
P95 spans 18.935-22.120 ms.
This is a responsiveness result within the fixture, not a faster-file-write or native frame SLA.
The source safety work remains present on the worker.

The [complete summary](data/2026-10-20-owned-editor-save/summary.json) retains all 108 groups;
raw samples are [run 1](data/2026-10-20-owned-editor-save/run-1.jsonl.gz),
[run 2](data/2026-10-20-owned-editor-save/run-2.jsonl.gz) and
[run 3](data/2026-10-20-owned-editor-save/run-3.jsonl.gz).
Run identities: `b1ed0b92-d6aa-44a0-9434-e5118f70ee02`,
`9622eeed-857c-43d2-97e1-470a29f0ab7c` and `07f552ae-c62d-4481-8cce-918767dfe2d2`.
The [verification record](../knowledge/engineering/verification/2026-10/2026-10-20T013307Z-owned-editor-save-custody-native-observation-and-measurements-9828019f2f934061a1773899ce356ffb.md)
binds final source, checks and independent review.

## Correctness And Remaining Work

Owned Save retains the original file Host independently of task terminality. The session owner
recovers that Host before acknowledging a receipt, while an explicit runtime close participant
preserves cleanup custody if the session is abandoned. Deterministic tests cover publication
held before/after replacement, later edits, logical cancellation, queue rejection, panic poison,
preview continuity, navigation Busy, content-runtime replacement and retry of failed close Save.
They establish the tested ownership paths rather than a general bounded-latency destructor claim.

Reopen remains synchronous. The measured startup closure also remains a candidate for focused
owned preparation. Source-receipt refresh after bound external Play and compiled Prefab content
refresh retain separate synchronous costs; this empty-entity cohort does not characterize them.
No Future executor, async owner trait, journal or project-wide transaction is introduced.

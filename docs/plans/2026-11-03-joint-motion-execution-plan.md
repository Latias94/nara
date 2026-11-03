# Joint motion and execution comparison

Owner: R4c/R9/R10 in the [foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md).
This is a bounded execution unit, not a new roadmap or a completed qualification.

## Question and controls

Run separately authored animation and physics systems against the same translation-only parent,
root-motion kinematic child and contacting dynamic body. Compare public sets, a private finite
phase-plan prototype, and `bevy_app::App::empty()` before choosing a production phase interface.
Reuse Bevy's executor and graph admission. Do not implement another scheduler or task pool.

The fixture owns only the comparison. Its raw-schedule publication oracle does not replace
Transform's production snapshots or qualify a replacement for Nara's managed lifecycle.
Run the selected composition additionally through the real managed owner and Transform projection.

1. Establish deterministic ordering, deferred visibility and rejection before execution. Record
   missing-anchor/cycle behavior and actual integration glue for each candidate.
2. Replace the stand-in with pinned Rapier 2D 0.33.0. Compare direct serial step with a scoped,
   non-`'static` borrowed step and explicit join. Hold the worker to prove publication cannot pass
   it. This is not solver-internal parallelism or a performance result.
3. Exercise zero/multiple ticks, Pause and exact Step, parent edits, fixed/presented projection,
   NonSend placement, deferred pose commands and faults. Separate consumption Ack, completed
   simulation and retained effects. Retain ADR 0057's narrow Ack unless evidence justifies an
   explicit owning-contract change.
4. Use the results to select the smallest production execution policy. Explicit serial execution
   must survive dependency feature unification. Any admitted parallel profile needs its own
   executed evidence; unsupported profiles refuse before Startup. Automatically driven schedules
   share the composition policy; explicitly driven custom schedules retain their owner policy.
5. Before phase-interface freeze, select the captured-history/discontinuity contract and bounded
   prediction/effect admission. Reuse the existing history owner. Complete rollback, collision
   authoring and general affine authoring remain their separate R10 qualification.

## Verification and delivery

Use Rust 1.95.0, one Cargo invocation at a time, `CARGO_BUILD_JOBS=1`, the root `target`, and focused
nextest. Compare actual solver results and contacts, not a trace alone. Review independently and
commit local, intentional scopes. Update owning ADRs and the implementation ledger only for
implemented behavior; keep prototype and production evidence distinct.

## Initial comparison result

The independent [workspace](../../tests/fixtures/motion-execution/README.md) runs the same
eight-input sequence through all three candidates. Direct and borrowed Rapier results match in
this same-build translation-only scene, including actual dynamic-floor contacts. The managed
consumer separately exercises the real Transform snapshots, Pause/exact Step, a held worker,
deferred pose visibility, command consumption followed by late failure and pre-Startup refusal.
It does not prove that the raw publication oracle equals every production spatial sample.

| Alternative | Observed author responsibility and limit |
| --- | --- |
| Public sets | Animation exports one intent set; Physics exports its target/solve/commit/callback sets and local chain. One product edge orders intent before target. Existing plugin prerequisites reject a missing domain, and Bevy rejects cycles. A reference to an absent set alone still does not certify a provider. |
| Finite sealed plan | Three schedules require an extra closed inventory, a sealing pass and a segment driver. End-of-segment flushes also satisfy the oracle. Inventory checks reject absent/duplicate segments, not an empty or semantically incomplete segment. It provides no new lifecycle or success semantics by itself. |
| `App::empty()` | One explicitly selected update schedule satisfies the same oracle without another runner. Matching the fixture's graph is easy; replacing Nara's managed admission, fault/control and physical retirement owner is not demonstrated by this experiment. |

No tested workflow requires a new automatic phase-plan interface. Retain public sets and the
existing managed owner for this unit rather than ship the private prototype. This is not evidence
against future automatic domain segments or a completed R4c freeze; the joint temporal/effect
gates and broader domain consumers remain open.

The feature-unified RED run (`1c67bd2f-2334-424e-9710-764c5bda972f`) observed Startup and every Core
schedule on Bevy workers despite no Nara execution-policy request. The owning correction explicitly
constructs all automatic schedules with Bevy's serial executor, including private cleanup. There
is no new executor/profile API. Raw explicit parallel comparison remains fixture-only; custom
schedules retain their author's explicit executor. The production policy trades implicit ECS
parallelism for predictable driver placement; this is not a throughput or latency measurement.

Retain ADR 0057's narrow consumption Ack. The complete managed failure trace consumes a real
command and captures/acknowledges it, publishes the solved spatial cut, then faults later in the
same schedule. The drive fails and remains sticky; the Ack and deferred effects remain. Therefore
Ack does not certify whole-tick success or authoritative confirmation. The stronger D07 rule is
not selected. New borrowed solver work must finish before its pose commit, independently of Ack.

## Shared temporal selection

Retain the existing `WgpuHistoryLineage` as the CPU temporal owner. Ordinary Host slot reset
invalidates an already prepared basis but intentionally does not fence every unprepared capture.
For an effect-incompatible semantic cut, `discontinue` advances that lineage's revision while its
previous clones remain frozen. New admission, first preparation, target adaptation and candidate
submission cannot refresh an old sample. Reset only the affected slots through the existing Host;
the lineage identity, successful watermark, other slots and retained output/submission owners stay.
ADR0094 records alternatives and the requirement to serialize complete Host operations when a
domain needs strict ordering against cross-thread cuts; revision checks do not undo native work.

The existing independent two-View history consumer now uses the real borrowed motion authors.
Original Presented stamp, poses, virtual shader time, real time, interpolation and pause state
travel in one immutable typed capture. The shader receives quantized original virtual time.
Its native test interleaves zero/multiple ticks, delayed execution, Pause, solver correction plus
KeepLocal reparent, exact Step, stale unprepared/prepared captures, selective first-View reset,
unaffected second-View history and another independent lineage. Numeric float histories and final
pixels are read through the actual Host and compared with the existing CPU accumulation oracle.
This does not classify all camera/shader/material edits or qualify native window presentation.

The bounded prediction/effect successor below selects its own state/refusal/confirmation model.
No generic rollback or mandatory effect API is introduced by the temporal fix.

## Bounded prediction and PCM effect unit

Use the same independent motion authors in a closed consumer. Keep its logical simulation tick
separate from the managed driver's monotonic clock: an explicitly driven custom schedule executes
one finite attempt without rewinding App clocks or introducing another automatic driver. The
earlier fixed/Transform qualification remains a separate requirement, not evidence inherited from
this custom schedule. Do not expose a generic World snapshot or arbitrary system replay API.

Compare three restoration choices: pose-only copying loses solver contacts and warm-start state;
reconstructing a fresh managed Runtime conflates prediction with generation replacement; a concrete
owner checkpoint can restore this consumer's exact state in its existing managed World. Select the
last only after same-build checksum and continued-simulation controls prove it. Rapier 0.33.0
documents `PhysicsPipeline` as rebuildable scratch; retain all persistent solver structures,
integration parameters, handles and sampled/target values. Reinitialize scratch and invalidate
attempt-local join permission on restore. Refuse opaque configured worker gates.

The finite recoverable model comprises motion inputs/root-motion phase, selected transforms,
logical entity membership and values, explicit seeded RNG, queued deterministic commands, and
admitted typed asynchronous outcomes. Include complete solver checkpoint bytes in the authoritative
checksum. Raw ECS handles are not durable logical identity. No system-local state is admitted;
a contribution requiring unreconstructible Local, an unmodeled native call or external I/O refuses
before execution. This is a closed authored inventory, not a static purity analyzer or enforcement
against arbitrary trusted Rust code.

Attempts carry the existing Runtime generation and a non-rewound ordinal. Abandoning and rerunning
the same logical tick rejects the old attempt's late result before state/effect application; a
different Session's matching tick/ordinal token also refuses admission, simulation, abandonment
and confirmation. Sort admitted outcome keys independently of physical worker completion order
and refuse unclassified/missing outcomes. Checkpoint at a complete quiescent boundary, after
deferred commands and borrowed solver join.

A concrete PCM owner consumes semantic cue intents only after confirmation of the exact current
attempt. Sound identity uses input event/occurrence/cue/sequence, without an attempt identifier.
Its duplicate ledger and confirmed frontier are not speculative state. Keep at most 33 ancestor
semantic state digests for genesis and 32 logical ticks. Restore replaces the speculative branch
history only after its confirmed ancestor matches; a later checkpoint on a discarded branch must
not rewrite that prefix. Advance confirmation only after successful audio publication. Refuse
correction of an already confirmed tick rather than pretend to undo audible effects. A
deterministic sample clock schedules a finite tone into offline PCM; test nonzero sample output,
silence before confirmation, repeat suppression, late-sample refusal and actual owner shutdown
through managed retirement.
This does not select a production audio backend or qualify a physical speaker/device.

Acceptance compares a corrected rerun with a clean corrected trajectory, including full state
checksums and continuation. Exercise entity spawn/despawn, queued commands, RNG and outcome order;
hold an old result until its replacement attempt exists. Preserve the ordinary sticky-fault and
partial-effect regression. Confirmed audio, stale-attempt refusal and simulation success remain
separate observations. Run serial before any separately admitted parallel comparison. Full R10
collision authoring and arbitrary Rust/native rollback remain outside this finite qualification.

## Bounded qualification result

The closed same-World restoration and offline PCM model is implemented in the independent fixture.
Corrected complete-state checksums and continuation through all 32 admitted logical ticks pass in
direct and borrowed-solver modes. Final default and feature-unified cohorts pass 22/22 and 23/23;
the existing actual DX12 history cohort remains 7/7. The
[qualification](../knowledge/engineering/verification/2026-11/2026-11-03T162556Z-bounded-same-world-prediction-and-confirmed-pcm-effects-preserve-exact-attempt-and-ancestor-ownership-71be3014fb29466a8995909c5e20b8c6.md)
retains the RED counterexamples, exact commands, independent review and limits. ADR0024 records
the selected state/refusal/confirmation boundary. R4c's remaining consumers and the separate R10
collision/affine-authoring requirements stay open; no production phase or generic prediction API
is frozen by this result.

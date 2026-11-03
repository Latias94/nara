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

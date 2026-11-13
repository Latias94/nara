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

## Managed startup pause correction (November 13)

Runtime work resumes while Editor UI and physical mixed-DPI qualification are deferred. The
public `RuntimeInstance::start` path previously reset the pause setting after Startup and always
promoted a healthy owner as `Running`. This contradicted ADR0039's committed Startup time policy;
the existing internal clock test did not exercise managed promotion.

Preparation now validates time resources without changing them. Promotion reads the final
`RuntimeTimeSettings` pause value, with faults retaining priority. Configuration and Startup may
each request an initially paused Runtime; Startup may also resume an initially paused
configuration. Subsequent control still belongs to the existing Runtime queue.

The new public regressions exercise paused real-time task/render/cleanup progress with no
simulation advance, exact `StepFixedTick`, applied control tickets, `Resume`, and ordinary
retirement. The pre-fix run `61b39082-df59-4033-979d-129c8c16ce68` passed one control and failed
both paused-start cases at the initial state assertion. The focused corrected run
`2d05b0b1-5979-43db-8cb7-8b304ef91ef1` passes 5/5. The release-mode App library/public cohort
`0ce5ad05-d9f9-4c7b-97cb-1dd8cf4fcfc5` passes 164/164 with one explicit skip on Windows,
using Rust 1.95.0, offline locked dependencies, one build job and serial nextest execution.
Independent review found no confirmed P1/P2 issue in the changed startup/control path.
All-target/all-feature correctness Clippy and package formatting checks pass. Existing Clippy
warnings remain; this is not a warning-free workspace claim.

The [managed Runtime guide](../guides/managed-runtime-runner.md) describes the initial pause
precedence. This correction adds no gameplay-state machine, background-window policy, new
scheduler or native presentation qualification. Full R4c/R10 and the foundation rebaseline
remain open.

## Headless exact-run entry maintenance (November 13)

Follow the public startup-pause correction through the original product Host. An exact-tick
`HeadlessRunIntent` promises one paused maintenance frame before its fixed transactions; the
current private driver infers first entry solely from `RuntimeState::Running`. Starting paused
can skip task integration and reject a product whose simulation readiness is restored there.
The same driver discards an App exit returned by its entry maintenance frame before stepping.

Reproduce both behaviors through the public HeadlessRun and compiled product recipe. Separate
the Host's explicit once-per-run exact-mode preparation from its one-exact-tick operation.
Preparation must use the same published Runtime, apply Pause only when needed, drive one paused
maintenance frame, retain original registry/fault admission, and propagate exit before a fixed
transaction. Frame-mode runs retain their existing path and time policy. Do not add another
Runtime, clock, scheduler, retry loop, preparation flag or generalized driver abstraction.

Pinned Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`,
`crates/bevy_app/src/main_schedule.rs::Main::run_main`, keeps one-time startup orchestration
separate from ordinary schedule dispatch. This supports an explicit existing-Host phase here;
it is not an exact-step API to copy or evidence of Nara behavior.

Acceptance covers initially Running/Paused entry, maintenance-restored readiness, two exact
ticks with a single maintenance frame, exit before the first fixed tick, unchanged ordinary
frame/zero-scale behavior, original cleanup/fault delivery, source and processed product entry.
Independently review the changed Host boundary and run focused serial release nextest followed
by the affected public/core consumer controls. This is a runtime successor; Editor UI and mixed
DPI remain deferred, and the full foundation rebaseline remains active.

The public pre-fix run `93f1bd03-dc23-42bc-ac74-9dd3d2767e6b` passes the existing exact/frame
control and fails both initially-paused cases: one observes only two fixed stages, while the
readiness case fails with `project.run.exact-step-rejected`. Run
`c7e081ec-0bb0-434d-a3f3-4f4c1d7ba0a0` passes the initially-paused ordinary-frame control but
fails entry Exit, observing `task, last, fixed, fixed` instead of `task, last`. These are executed
release-mode public product regressions, not inferred failures or performance measurements.

The corrected public startup cohort `f55989c8-662e-4f2c-88d9-9a73544e45f6` passes 21/21 with no
skips. Besides all four new tests it retains stable Image/TileSet publication, source-closure
continuity, tick/frame limits, fixed-tick exit, terminal snapshots, exact command admission and
same-owner incomplete cleanup. A separate source/test correctness review finds no confirmed
P1/P2. The extended four-mode fault/retirement control exercises an already reported fault;
it does not independently inject a new fault during entry maintenance.

The independently locked processed Host cohort `5b296e64-7fa6-4c7d-b2ca-5f4aeae7d9e3` passes
3/3 with no skips, including both new exact-entry cases and the original compound-asset travel
test. Actual `RuntimeContentBase` input is captured before removing its input directory. The
new cases preserve the original Scene receipt, processed document and asset allocation through
entry and ticks, observe weak source retirement and one shutdown, and retain a stable repeated
terminal result without a source Check receipt. These cases decode through the same domain
codec without recording in the existing travel test's shared counters; its test body and
assertions remain unchanged.

Both executed cohorts use Rust 1.95.0, offline locked dependencies, release mode, one build job,
the original root target and serial nextest on Windows. Root qualification selects
`runtime-2d,serde,project-tooling` without default features; the independent consumer retains its
own declared feature closure. Reproduction targets are:

```text
cargo +1.95.0 nextest run --locked --offline --release -p nara --no-default-features --features runtime-2d,serde,project-tooling --test project_runtime_boot --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --locked --offline --release --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-asset-domain-consumer --test processed_host --test-threads=1 --no-fail-fast --no-tests fail
```

Final affected Host/public run `925bfe82-6940-48d3-8e5f-8331404a7528` passes 71/71: 50 Host
tests plus the 21 public startup tests. The other 781 library tests are outside this filter,
not newly qualified. This includes exact stepping, four-mode reported-fault/retirement handling,
real task shutdown, incomplete same-owner cleanup, original startup/publication failures,
processed Input custody and Scene request cancellation/travel. The command is:

```text
cargo +1.95.0 nextest run --locked --offline --release -p nara --no-default-features --features runtime-2d,serde,project-tooling --lib --test project_runtime_boot -E 'test(project_host::runtime::tests::) | binary(project_runtime_boot)' --test-threads=1 --no-fail-fast --no-tests fail
```

Correctness Clippy passes for the root library/public startup target with that feature selection
and for the independently locked `processed_host` target. Root-package and asset-domain-consumer
format checks, added document links/fragments and staged whitespace checks pass. Existing
non-correctness warnings remain. This qualifies the bounded Headless entry replacement on
Windows CPU execution; it supplies no native presentation, other-platform, seat/gamepad,
asynchronous-initialization completion or whole-R10 qualification.

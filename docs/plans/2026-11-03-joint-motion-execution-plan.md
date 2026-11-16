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

## Explicit Production Executor Successor

Status: implemented and qualified for the selected Windows code-first workflow. Implementation
is `40e8e81`, independent consumers/CI are `6d45b7b`, and the discovered game Material eligibility
correction is `81916c9`. Full R4c/R10 and the foundation Goal remain active. Runtime work continues
while Editor/Dialogue product work remains paused. The existing serial and raw-parallel comparison
is the baseline; this successor admits an explicitly selected production policy through the
original managed owner.

Keep `App::new()` serial. Add a configuration-time `ExecutionProfile` selection for all engine
automatic Startup, Core and private frame-cleanup schedules. Parallel uses Bevy's existing
executor and requires an explicit Nara build feature plus an already initialized, nonempty
Host-initialized process-global Bevy `ComputeTaskPool`. Refuse unavailable policy before Startup. Nara neither creates
another scheduler/pool nor treats dependency feature unification as a policy request. A selected
policy cannot change during Finish or after seal; custom schedules keep their explicit owner
policy. Pool workers, borrowed solver jobs and cross-frame Tasks remain separate resource facts.

Separate App configuration identity from plugin-plan identity. The former includes the selected
execution policy with the exact plugin plan, while the latter remains the original plugin
declaration/definition composition. This closes the game reconstruction compatibility gap
without treating worker count, arbitrary Rust systems or binary identity as plugin-plan data.

Qualify the actual public managed Runtime with default, unified and explicit parallel consumers.
Reuse the independently authored Animation/Rapier motion, actual borrowed job join, deferred
pose/Transform projection, NonSend owner placement, zero/multiple ticks, Pause/exact Step and
command consumption followed by a later fault. Prove original error routing, two independent
Runtime owners and physical cleanup. Exercise missing-feature/missing-pool refusal before any
Startup effect, real independent peer overlap and custom schedule policy preservation. Keep
all existing default serial, raw comparison, collision and prediction/audio assertions. Profile
selection alone does not qualify bit-exact results, solver-internal parallelism, arbitrary game
repeatability or a throughput improvement. Run Cargo serially with the current locked toolchain
and original target, review independently and update ADR0003 and current implementation evidence.

Also run the original real-GPU motion/history trajectory through an explicitly selected parallel
fixture composition. Preserve its original numeric accumulation, captured clocks, zero/multiple
ticks, Pause/exact Step, solver correction/KeepLocal reparent, stale-capture/prepared refusal,
selective View reset and retained-output/submission assertions. The fixture build feature selects
the consumer's profile; the production default stays serial. Native execution remains outside
the World. A successful parallel CPU trace alone is not the captured-history qualification.

### Production Executor Implementation And Qualification

The original App installs the selected executor after plugin Finish and before graph admission
and Startup. Runtime observation reads the selection from its existing retained owner. The
implementation uses Bevy ECS/tasks 0.20.0; the pinned Bevy source
`03ed5a27c92715c8fa2188f31f04e454ec39942f` supplies the corresponding executor, schedule and
task-facility reference. It is a development checkout, not an assertion of exact tag equivalence.
The render consumer's independent lock replaces its previous Bevy tasks 0.20.1 with the selected
exact 0.20.0 dependency so its Host and the App use the same facility. The required async-channel
and event-listener closure is added. Unrelated Windows dependency-edge rewrites from resolution
are removed, and locked metadata accepts the retained choices. This is not a general dependency
upgrade or a claim that every independent patch version matches the root workspace.

Replacing a Bevy executor resets its final-deferred setting. The App therefore retains requests
made through its controlled configuration API, restores them when installing either executor,
then reasserts the original public-anchor policy before graph initialization. A public regression
observes Update commands in the first and next frame under both settings and profiles. The
private frame-cleanup schedule also has actual overlapping peers. Custom schedules keep their
original executor and explicit driving rule.

Production review caught an invalid ordered-map choice for `InternedScheduleLabel`; the retained
configuration uses a HashMap solely for insertion and lookup, without introducing iteration
order. The first motion test build caught a stale World API spelling in a new shutdown probe;
it was corrected to the installed `non_send` accessor without removing assertions. Independent
production and consumer/test reviews find no remaining confirmed P1/P2. Those reviews are
read-only and do not substitute for execution.

Initial release-mode execution on Windows with Rust 1.95.0 passes the explicit parallel motion
consumer 36/36 (`430f4da4-c81d-4c9b-aa46-c83a7f5a11b5`), the complete App default cohort
182/182 (`abbd5eb3-19cf-4e44-ad07-0b42cf703973`) and feature-enabled cohort 183/183
(`a0f0a43b-2c08-44b2-ba83-b1aa8c7ad194`). Each App run leaves the existing disposable-child
close-custody test unselected. Independent default motion passes 30/30
(`2ec3261e-96d0-4c47-92ff-8d1d525f7b67`), and dependency-unified motion passes 31/31
(`1f99c588-1544-4ca1-9d04-7e6fc6009425`), retaining the original collision and closed
prediction/offline-PCM assertions. Captured history, ordinary game compatibility and final
build/lint checks are qualified below.

The original `history` target retains its strict native hardware gate. The CI software control
instead selects the existing complete `joint_history` Material/motion trajectory with
`production-parallel`; no hardware assertion is weakened to admit a software result.

The first native consumer build exposed two missing exhaustive matches for the new PluginError
in the root product diagnostic adapter. Product preparation now reports a framework refusal as
`project.run.execution-profile-unavailable`, preserving the selected profile and static refusal
reason. A real plugin invocation returning that same typed error retains its actual plugin/hook
and lifecycle context. Actual source-Host refusal/cleanup and managed build/shutdown attribution
regressions cover that distinction. Root run `936518a3-97a6-4497-8398-8dd744c983ff` passes
93/93: 54 selected Runtime Host controls plus all 39 public plugin-composition tests. The other
880 library tests are outside this filter. Both new diagnostic regressions pass, including
static reason fields, absence of fake plugin/hook attribution, retained actual invocation
identity, original no-publication/start-claim release and exactly-once acquired-owner cleanup.
It uses the root's `runtime-2d,serde,project-tooling` feature selection without defaults.

The final native trajectories pass on Windows/DX12 with explicit adapter selection:

| Consumer | Selection | Actual result | Nextest run |
| --- | --- | --- | --- |
| Original motion/history | Parallel, RTX 4090 hardware | 1/1; six unrelated tests outside the filter | `f1d0f695-f4b0-4c51-bb3f-05445637deac` |
| Original motion/history | Serial, RTX 4090 hardware | 1/1; six unrelated tests outside the filter | `43dd7d22-d6b0-4587-ba24-c682819c7eb3` |
| Joint Material/motion/history | Parallel, Microsoft Basic Render Driver software | 1/1; no skips | `cb26cec1-3eba-407c-86a5-5248bba8bd76` |

The joint software run observes four cuts, 14 temporal publications, four late original stock
outputs and the expected native pipeline change from seven to eight. The original World is
dropped, retained producer outputs remain exact and explicit cleanup reports no failures. These
are executed correctness and ownership checks, not a timing or performance qualification.

The actual independently locked Two-Level Game default run
`d48e08c4-dc4f-4a4a-b738-f9b074e8427c` executes 110 tests: 99 pass and 11 fail, with two
native/build tests unselected. Every failure reaches the same stale game-owned eligibility
inventory: the default Runtime2d recipe now installs `nara.material2d`, but the game has not
classified that service. Ten resume tests reject healthy capture; the continuity-bin control
panics on the same refusal and its test process exits with `0xc0000409`. That exit does not
independently establish a general panic-cleanup diagnosis. This is an executed product gate,
not a green compile-only compatibility claim. Correct the sample's explicit inventory after
inspecting the Material plugin; schema plus empty storage does not authorize omission of
actual material assets/components from a claimed complete state. Preserve unknown-service
refusal and rerun the original full default tests.

Source inspection confirms that `Material2dPlugin` installs only the native Schema/codec and
empty `Assets<Material2dAsset>`. The game now classifies that installation while explicitly
refusing any populated Material asset storage or any actual Material2d component. Both Headless
and Desktop capture use the same game-owned eligibility check. A public negative control uses
a real Material domain/Shader to exercise asset-only and component-only state independently,
checks repeated refusal without mutating tick/queue/player state, then continues gameplay and
physically closes the original Session. Existing unknown-service refusal and all full-state
comparison assertions remain. Independent review finds no confirmed P1/P2; final execution
of the corrected ordinary game cohort `69da58bc-b05f-4664-81b7-f681450c4758` passes 111/111
across 16 binaries, with two original native/build tests unselected. All 11 earlier failures
pass, alongside the new Material negative control, full three-instance state reconstruction,
continued semantic steps, nonzero phase/debt, content/configuration refusal, unknown services,
SaveSlot refusal and original command/Close custody. The standalone lock is unchanged. This is
ordinary source/headless execution; the ignored actual-build/native trajectories are not rerun
by this cohort.

Final verification uses Rust 1.95.0, locked offline dependencies, one build job, the original
root target and serial nextest execution. The Two-Level Game `desktop` feature passes an
all-target compile check. Correctness Clippy passes for App/ECS all targets with the feature,
the root library/public plugin-composition target with the explicit feature, independent motion
all targets with `production-parallel`, both selected native history targets, and the game's
desktop library/resume target. Existing non-correctness warnings remain; no warning-free claim
or unrelated cleanup is implied. Root, motion, render-policy and game formatting checks pass.
Changed manifest/lock TOML, CI YAML, added local document links/fragments and whitespace checks
pass. CI selections are configured locally; no remote execution or publication occurred.

Stable `wasm32-unknown-unknown` App production-library checks pass with the feature disabled and
enabled. The enabled build retains explicit `UnsupportedTarget` admission; no browser execution
or Wasm Parallel support follows from compiling that refusal path. Actual native execution in
this unit is Windows/DX12 hardware/software only. Other platforms, worker/latency budgets,
solver-internal parallelism, arbitrary parallel plugin effects, closed-model prediction under
Parallel and a persisted project execution setting remain unqualified. Runtime close retains
its original schedule/work/NonSend obligations without claiming process-global pool destruction.

The next runtime work follows the remaining R4c/R10 consumer and domain gates in the foundation
plan. This unit adds neither a new automatic phase-plan abstraction nor a scheduler, task pool,
render-thread owner or whole-Foundation completion claim.

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

## Concrete Game Audio And Native Retirement Successor

Status: implemented at `edc9ea2` and qualified on Windows/Rust 1.95, starting from `ce363a0`.
Full R10/R12/Foundation remains active. Continue the runtime-first priority. The selected
consumer is the actual Two-Level Game with an explicit optional audible desktop composition;
the existing independent offline prediction/PCM qualification keeps its own semantics. Do not
promote a new universal AudioBackend, select an official engine default, or make shipping and
headless recipes acquire an audio device merely because its implementation was compiled.

Own the initial CPAL integration in the game, before extracting a shared crate. Two fixed,
game-owned short clips represent committed pickup and travel-entry events. Decode/prepare clips
outside the device callback. Produce effects at the actual gameplay/Scene commit sites, with
Runtime generation, active Scene instance and a monotonic game-event identity; do not infer
events by comparing presentation snapshots. Repeated pickup observation, failed travel, Retry
and Save/Restore reconstruction do not invent new effects. Successful Scene replacement retires
old-instance pending/active sounds even if later reconstruction fails. An audio refusal reports
its own failure and never rolls back already committed gameplay.

Select short one-shot effects that continue on the independent device clock while game time is
paused. Resume never replays them; exact Step and a successful asynchronous travel commit during
paused maintenance can each publish their actual new event once. Output callback frame positions
are distinct from fixed ticks, estimated playback position and physical audibility. Commands,
voices, duplicate/control history and observation capacity are finite. The real callback performs
bounded work without locks, allocation or game/World access. Queue refusal preserves the original
request/result semantics; no unbounded deferred retry or second gameplay-command queue is added.

Use the original managed close participant for the concrete device aggregate. Stop admission
before teardown and retain the original stream, callbacks and prepared data until physical
retirement is established. CPAL 0.17.3's cached WASAPI source conditionally skips its original
worker join when Terminate cannot be sent; callback-capture destruction alone does not prove
that worker has ended. Resolve this owning dependency contract against current official source
before claiming native cleanup. Do not hide it behind a detached closer, a fabricated receipt or
a new generic native-lifecycle service. Native library shutdown remains cooperative unless an
actual backend contract supplies a harder guarantee.

The selected official CPAL **0.18.2**, revision
[`e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7`](https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/wasapi/stream.rs#L479-L494), resolves that gap: its WASAPI stream Drop attempts
Terminate, then waits for the original worker even when command submission failed. Only self-join
is excluded, so the original Nara driver must own destruction. Join panic results remain ignored
by CPAL; termination evidence is distinct from successful playback. The Apache-2.0 publication
requires Rust 1.85 and is compatible with this unit's retained Rust 1.95 baseline. Use its exact
unified Error API and by-value StreamConfig, with the ordinary WASAPI feature closure; no custom
WASAPI implementation, library fork or toolchain upgrade is selected. CPAL is target-Windows and
optional in the game. Hound prepares fixed mono PCM16 clips and rtrb supplies finite SPSC storage;
no game-owned decoder, resampler or general queue framework is introduced.

Acceptance includes pure cue/mixing/clip oracles, actual game pickup and successful/failed Scene
transitions, paused/exact-step behavior, bounded refusal, old-instance cancellation and generation
isolation. Run the same audible game composition through ordinary DesktopRun with real device
callbacks. Compare bounded captured output against independently decoded original clips, not just
a nonzero counter. Hold an actual callback during managed close, prove no premature Complete and
release it before the original stream/worker retirement finishes. Exercise partial startup failure
and a fresh Runtime after Close. Keep startup, error observation, accepted/started/completed/cancelled
effects, callback retirement and native worker join as separate facts.

Pinned references: Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`,
`crates/bevy_audio/src/audio_output.rs` and its Rodio dependency; Godot
`7a37606a433c0e7735525037cdf774f043146a21`, `servers/audio/audio_server.cpp` and
`drivers/wasapi/audio_driver_wasapi.cpp`. CPAL/Rodio public play/pause or queue-empty results
must not be interpreted as physical completion. Actual human audibility, system-mixer output,
device hot-switch/recovery, native prediction playback, arbitrary audio restore, foreign native
execution and numeric latency budgets remain outside the selected qualification. The current
game reconstruction owner must keep refusing unrepresented native audio state. Full R10/R12
and Foundation remain active; UI/Dialogue product work stays paused.

### Concrete Audio Implementation And Initial Qualification

The optional game-owned `audio-cpal` module at `edc9ea2` now prepares two original PCM16 WAV assets before
stream creation, mixes finite stereo `f32` output at 48 kHz, and owns CPAL's original WASAPI
stream in the Runtime's driver-thread close participant. Compilation alone installs no audio
plugin. The explicit `audio_desktop` recipe and ordinary entry share actual game commits,
command/input/Scene ownership and DesktopRun with the reference game. Default/cooked recipes and
the exact development reconstruction inventory do not admit this new native service implicitly.

The normal output callback owns its preallocated mixer and SPSC consumers/producers, with no
World access, ordinary lock or allocation. There are sixteen voices, 4096 non-evicting semantic
effects and a 64-command queue; new work refuses explicitly at capacity. Callback cue records
reserve accepted/start/terminal space before admission. Latest-instance cancellation uses an
independent atomic barrier, so a full command queue cannot prevent old-instance retirement.
Game observation separately reserves cue and scope capacity; a scope-log refusal cannot discard
an already accepted cue's terminal fact. Optional callback capture is finite, and overflow is a
fault rather than silently incomplete evidence. These finite allowances are selected consumer
limits, not an unlimited music/effect system or a measured real-time cost guarantee.

The initial CPU selection passes42/42, run `acf2afe4-ad34-4abf-b1a0-1a956998621a`. The first
real-device lifecycle selection passes2/2, run `a683061b-0b8f-4d8f-93f9-c2662eba0663`: a held
actual callback prevents managed completion until released; later Startup failure retires the
opened device through its original owner. Those are checkpoints preceding independent review.
Review finds a game-journal partition error and an asynchronous worker-retirement/last-admission
race; both are corrected in their owners. Callback final destruction now closes admission and
holds the original producer mutex during final drain; ordinary callback work remains lock-free.
An exact checked-admission/worker-exit control proves the last queued request gets its refusal
and returns unused credits. The same review catches unguarded test unwind; the fresh Runtime is
immediately guarded and a real-device injected assertion proves retirement before propagation.
One stale CPU fixture assertion after the retirement correction fails44/45, then is corrected
while preserving its explicit `stream_created=false` and `stream_drop_returned=false` facts.

The corrected focused CPU selection passes45/45, run `fe904024-7d8a-4bd2-af9f-f2c91c67b1a2`.
The full DX12/native game plus three physical lifecycle controls passes4/4, run
`44bd0768-65be-4cd4-9b52-5ac6e7512f42`; the same ordinary game passes1/1 on Vulkan, run
`20f83429-93e5-4169-92f3-2bb0a333160d`. The game uses a private project copy, actual exact
commands, Pause/Step/Resume and deliberate missing-Cavern refusal/repair. Its first trajectory
requires exactly one pickup and one successful travel cue, old-instance cancellation on Retry,
and no new sound on Save/Restore. A fresh Runtime on the recovered original event loop executes
the full ordinary Travel script with three pickups/four entries, preserving original game facts.

A separate directly retained DX12/RTX4090 report compares160512 and174912 stereo samples across
80256 and87456 output frames against independently decoded original WAVs; both maximum errors
are zero. It observes one/four old-scope cancellations, complete original native retirement and
unchanged old-generation witnesses. Those output facts precede the final oracle strengthening:
consumer review finds that a completed voice could cross its first scope cut, and Retry/Restore
scopes were not required to name actual observed game instances. The oracle now checks the first
replacement boundary and every actual instance, and the workflow joins each Retry/Restore's
applied scope to its exact GameSnapshot. Four new pure counterexamples include matching but
incorrect post-cut PCM and unfamiliar no-cue scopes. Their actual final target passes4/4, run
`438d9536-e050-4dac-a650-f7d6f07a40e6`. The strengthened complete DX12/native selection passes4/4,
run `6dc69e76-2abd-4989-9bde-a357154c1f87`; Vulkan game acceptance passes1/1, run
`93849daa-9a21-45fc-8420-8915d8eab84c`. Both retain all original assertions and zero audio faults.

The complete all-feature ordinary sample selection passes233/233 with20 separately selected
ignored cases, run `bc86afa5-2bf0-4883-9062-a9c39cef42d0`; its selection predates addition of the
four independent oracle tests above. The final minimal cooked headless selection passes119/119
with zero skips, run `bfbe1e37-d5ae-4881-bc98-8a3b809a4ff0`. Selected counts overlap and are not
pooled. All-target/all-feature correctness Clippy passes, as do root/sample formatting and
manifest/lock/workflow/document checks. The existing independently locked panel library and its
diagnostic example that actually consumes the game both compile unchanged; this is source/API
integration evidence, not resumed native Editor development. Cargo's exact shipping normal/build
graphs keep CPAL/Hound/rtrb, watcher and UI packages out of both shipping selections. The sample
lock adds the selected optional audio closure without changing existing package versions or
sources; only necessary Apple dependency edges join those new lock members. Three scoped review
roles close without remaining P1/P2, and all eight research/implementation/review agents close.
CPU and oracle gates are configured for ordinary Windows CI; no remote CI/native-device run is
claimed. Actual foreign devices, audible/system-mixer output, latency budgets, hot-switch,
streaming/cooked audio and native reconstruction remain unqualified. Full R10/R12/Foundation
and the paused UI/Dialogue scope are unchanged.

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

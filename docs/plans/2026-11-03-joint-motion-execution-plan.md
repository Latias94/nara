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
game reconstruction owner must keep refusing unrepresented native audio state. The explicit
[audible development successor](#audible-development-with-explicit-effect-reset) selects reset
of game effects rather than restoration of that native state; ordinary audio recipes retain
their refusal. Full R10/R12
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

## Source-Owned Sound Bank And Source-Free Audible Player

Status: implemented and qualified for the selected Windows/Rust1.95 workflow, starting from
`efe3f21`. The preceding native consumer embeds both WAVs in the executable. This successor
replaces that production path with a game-owned CPU Sound Bank domain and complete source/processed
delivery through the original asset, Scene, cook, package and Runtime owners. Existing offline
prediction remains separate; Editor/Dialogue product work stays paused.

Use one primary bank Product for the game's finite pickup/travel inventory. Its source JSON names
two Assets-relative raw PCM16 WAV dependencies; only the bank has a typed identity sidecar. The
source importer reads all bytes through its admitted reader, validates both complete clips before
publication and records actual raw dependencies. SourceAssetBuild supplies the existing tracked
group and Runtime codec, without another cache or format resolver. The processed bank contains
only complete Product identity, fixed format descriptors and canonical PCM samples, without raw
source paths, importer recipes or trusted content hashes. Bounds, work/retention cost and finite
float/length checks belong to that concrete codec.

Declare the bank in one game-owned persistent Scene component under the original Schema provider.
Both levels use the same game bank identity; every recipe selects its source/processed CPU facets
independently of the optional Windows CPAL device. Headless play may decode CPU content but never
opens a device. Current content and escaped clip samples retain their actual original collection
and budget owner. A Runtime's one-shot bank is fixed at audio startup: later travel may admit only
the same exact bank meaning. A changed bank requires a fresh audio Runtime; do not silently play
old bytes under changed Scene content or invent callback-side allocation/reclamation to support
unqualified live bank reload. Old captured banks remain immutable after source edits or retirement.

The existing cooker builds the bank through build_source and includes its verified group in the
same closed catalog as the actual Scenes/Images. Add a separately selected audible desktop Player
placement with its own generated content expectation and exact Cargo features. Its native device
opens only at actual Runtime startup, never during Check/cook/composition. Existing headless and
silent desktop placements remain device-free, with their actual feature graphs checked. Generated
Player startup and package publication retain exact code/content binding and original process
custody. All required source files, including bank JSON/metadata and both WAVs, join the existing
freshness fence. No build-time include_bytes fallback or source-directory lookup is allowed in
the shipping audio path.

Acceptance must prove source import/tracked auxiliary edits, malformed/oversized/foreign Product
and processed-format refusals, source/Runtime sample equality and complete original retention.
Run the ordinary source audible game against acquired content, then actual production cooking,
relocation, source/cache removal and a fresh sealed audible Player with original native device,
gameplay, sample/cause and physical cleanup evidence. Corrupt required bank content and mismatch
generated code/catalog to refuse before gameplay/native publication, preserving earlier accepted
packages. Retain existing silent/headless delivery and development reconstruction controls with
the new declared Scene content. Independently review domain/retention and delivery adapters,
run focused serial nextest/native checks and create precise local commits. No root source/schema
stripping, generic audio module, streaming, hot switching, arbitrary native audio restoration,
foreign execution or numeric latency/memory claim is implied. Full Foundation remains active.

### Sound Bank Implementation And Initial Qualification

The implementation uses the game's primary Product
`6c7d4d42-cb57-4a47-9f16-741293f5aab2` and one `SoundPalette` on each existing level camera.
`SoundBankDomain` uses the original SourceAssetImporter/SourceAssetBuild owners; the source-free
64-byte header plus canonical PCM16 format admits only this complete primary identity. Source
JSON is bounded to 1,024 bytes, each WAV to 100,140 bytes and one cue to 48,000 mono 48 kHz frames.
Runtime encoded input is bounded to 192,064 bytes, import work to 1,304,576 bytes and Runtime work
to 1,156,224 bytes. Retention includes both Arc allocations, counters and alignment. These are
admission limits and modeled reservations, not observed peak memory or latency measurements.

The original native device now opens at Tooling after successful Scene/game initialization.
Its existing Mixer owns the acquired sample allocations and original ProjectDomainAssets lease;
ordinary voices retain only their slot and cursor. No production clip embeds WAV source.
Prepared travel compares the complete original bank with actual target content before commit.
Changed PCM under the same Product refuses without replacing the incumbent Scene or publishing
a cue. Restoring the file permits ordinary travel again.

Pinned Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f` AudioSource retains encoded Arc bytes in
its decoder; pinned Godot `7a37606a433c0e7735525037cdf774f043146a21` playback retains its WAV
resource Ref. These are lifetime mechanisms, not immutable version guarantees. Their source
files are respectively `crates/bevy_audio/src/audio_source.rs`, `audio_output.rs`, and
`scene/resources/audio/audio_stream_wav.{h,cpp}`. Nara keeps the original immutable value and
budget owner rather than inferring content from an ID or generated path.

Independent review exposes two concrete corrections. First, making a plain CPU collection retain
the startup Scene generation blocks later edited Scene revisions under the original two-generation limit. The
source collection now retains its original ProjectContentLease without a Scene-generation lease;
actual Scene sources, escaped Images and embedded Material views preserve their existing generation
owners. Pure CPU bytes remain charged through the last collection drop. Second, fixed WAV names
in the cooker fence disagree with the descriptor's legal relative paths. The original verified
Image and Sound Bank groups now supply their actual raw observations; the fence checks those
original digests and captured identities through build/publication without rebaselining a change.

Initial all-feature library compilation passes. The actual compiled headless project tool regenerates
the authoritative Schema bundle. An initial focused CPU run passes 157/175: the new authored
reference used an invalid ComponentValue tag and independent fixtures reopened a different root
capability. The Scene now uses the original structural asset-reference encoding, and each fixture
shares its original admitted root. The corrected full selected CPU run passes 175/175 with three native
cases selected separately, run `1c557a5d-fef4-4e39-8780-f6ac7ecb2103`. This includes 22 domain controls,
five source/processed/retention/travel consumers and five acquired-PCM oracle controls.

Actual Windows hardware/native acceptance passes 4/4 on DX12, run
`fbb33035-8862-4e0d-9d92-4236123c5e81`, and the ordinary game passes 1/1 on Vulkan, run
`ec2fef4b-ef83-4a8e-ad7b-e14013531803`. Both preserve original Pause/Step/Resume, failed source
travel, Retry, Save/Restore, fresh Runtime and complete original stream/window/pump retirement.
The first game trajectory adds changed-bank refusal and three newly authored Scene revisions
while the original bank remains live; it now has one pickup/four travel cues. The fresh ordinary
script retains its three pickups/four travel cues. Every callback sample is compared with
independently decoded original WAVs. Affected shared-owner controls pass 119/119,
run `1b6d5ecd-bf7b-434d-a34b-3e718b8bcf01`, including the original escaped-Image third-generation
refusal/recovery. The seven original/actual-import cooker fence controls pass 7/7, run
`5317445e-7076-4732-b035-74cb519bd214`.

The first complete generated audible package builds, relocates and removes its copied source/cache,
then fails honestly before Resume: the new validation adapter selected initial input isolation but
did not perform the original input activation. The original gate rejects Resume and the failed
Runtime, stream, window, worker and physical pump retire. The Player adapter now freezes its exact
neutral movement inventory, completes paused maintenance, requests original focus/physical-key
reconciliation and waits for Active without simulating a tick. First-tick launch and Resume follow
that real activation; no native or Runtime gate is relaxed. A second run completes every sealed
worker assertion but the outer test cannot parse a marker sharing libtest's test-name line. The
worker now starts its structured report on a new line; product assertions remain unchanged.

Final complete audible publication passes 1/1, run
`77bc692f-76c8-4870-aeef-1f0fe5c1207c`. It performs a real release cook with exact
`cooked-player,desktop,audio-cpal`, defaults disabled, and physically renames a valid raw WAV while
updating its descriptor before cooking. The exact five-member pack and generated build inputs are
checked, the whole installation moves, and the copied source/cache are physically removed before a
fresh allowlisted-desktop worker runs the executable from an unrelated empty directory with no Cargo
or rustc path. The verified Player executes the original complete Travel trajectory and compares
208,512 stereo samples over 104,256 frames with zero maximum error. Its independent parent uses
the original source WAV PCM and actual cue/scope records to reconstruct the complete output hash;
the Player's separate oracle uses its immutable acquired bank. Both agree exactly.

A second actual Player admits physical D/E input, collects/travels and closes through WM_CLOSE.
Both original processes exit successfully without forced termination, with empty containment,
stopped readers, drained output and destroyed original windows. Their original Runtime, stream,
callbacks, surface/provider and physical pump retire. Corrupt bank bytes and a changed compiled
catalog expectation each refuse before gameplay/device publication; accepted installation digests
remain unchanged. Actual Windows DX12/RTX4090 and WASAPI are the scope; callback data does not
establish human audibility, system-mixer/DAC delivery, scanout or numeric latency. The audible
placement always drains bounded callback capture and retains PCM only for verification, so its
observation cost is explicit.

Final ordinary all-feature sample regression passes 269/269 with 22 explicitly selected native/build
cases outside this aggregate, run `96a18c7d-449c-4fa6-be96-403c9e325170`. Minimal cooked-headless
with defaults disabled passes 141/141 without skips, run `31b74756-4e6f-41e6-84d2-41d1ca387b20`.
The existing actual headless release delivery
passes 1/1, run `30fc87a8-1ab2-4b28-881d-b3f9f6d10b8d`. The first silent desktop delivery fails
at its old four-member catalog assertion after a successful cook. Its adapter now checks the exact
five-member Scene/Image/bank set, decodes the bank and preserves all original physical play,
Save/fresh-Restore and native cleanup requirements. The complete corrected silent desktop delivery
passes 1/1, run `5db7901b-ec84-431c-a15d-9cc95b95aff8`. The complete independently locked asset-domain
consumer passes 95/95 without skips, run `a2294a1c-d6e2-4634-b7d7-a4ac1b2beaa0`; its CPU collection
escape control preserves charged bytes/reservations without pinning a Scene generation. The
existing independent Editor diagnostic consumer's library/example/test compile passes with its
declared optional game dependency. That consumer's lock adds only the existing Hound3.5.1 package
and the game's new normal dependency edge; no version/source changes are selected. The shared
CPU-custody correction is committed at `bd5941e`; complete game/delivery implementation is
`b292ed9`. Actual-rebuild and static checks follow separately; these overlapping cohorts are not
one aggregate test count.

The existing production game-owned rebuild/adoption regression passes 1/1, run
`4b44fbe4-2054-4b3a-969d-eb17f9dbc91a`, after actual movement-body and private-representation
edits with the new Schema/content inventory. Complete canonical restoration, compile/probe failure
preservation, foreign/stale/consumed ticket refusal, fresh-adopter state and independent predecessor
retirement remain qualified. This control exercises the existing headless development placement;
native audio reconstruction remains explicitly unsupported.

All-target/all-feature correctness Clippy passes for the sample, and correctness Clippy passes for
the affected root Runtime library with `runtime-2d,serde,project-tooling`, defaults disabled.
Root/sample format checks pass. The first independent format check flags only the newly edited
budget assertion; its rustfmt layout is corrected without behavior changes. Existing non-correctness
Clippy warnings remain; this is not a warning-free workspace claim. TOML/lock/JSON and CI YAML
parse, added local links/fragments and whitespace checks pass. The CI selection adds CPU bank and
acquired-PCM controls only; no remote CI or physical audio device runner is claimed. Eight scoped
implementation/research/review agents are closed. Full R10/R12/Foundation remains active.

The corrected independently locked workspace format check passes. Fresh exact normal/build
dependency trees confirm that headless and silent desktop exclude CPAL/rtrb; audible desktop selects
CPAL0.18.2/rtrb0.3.5. All three select CPU Hound3.5.1 and exclude development notify, Editor/UI
toolkits and Cargo metadata. The cooker separately binds each actual executable's explicit feature
selection; an all-feature supervising test is not the shipping dependency graph. All Cargo
qualification is serial, jobs1, locked/offline, release, Rust1.95 on Windows with the original root
target. The game Schema/catalog change requires regeneration and recooking of earlier installations.

## Audible Development With Explicit Effect Reset

Status: implemented at `6da2745` and qualified on Windows/Rust1.95 below, following the November 17
Runtime delivery priority. The existing audible game and source-desktop development Host now form
one usable workflow. This
is game-owned composition through the original Runtime/CPAL/native-process owners, not a new
audio backend, arbitrary native-state snapshot or a performance experiment.

Add an explicitly selected audible desktop placement. Every process in that placement starts
with gameplay sound publication disabled. Probe commands change normal canonical game state
without queuing sound or reserving future effect identities. A fresh adopter restores the same
game state but starts with empty voices and no replay of historical pickup/travel effects. The
composition fingerprint records this reset policy. Ordinary audible recipes keep refusing exact
reconstruction; unknown services remain unsupported.

Keep adoption committed independently of predecessor cleanup. For the audible placement only,
activation/output enablement waits for confirmed original audio/process physical retirement;
an incomplete retirement cannot be treated as permission to play. Completed errors remain
observable without blocking independently safe new ownership. A prepared/cancelled
candidate never gains publication authority. Explicit enablement applies to the sole adopted
process; native focus and Resume retain their original independent checks. Ordinary Pause still
allows already started one-shots to finish on the device clock. Samples already delivered to a
device are not reversible.

Use the actual develop/continuity/public Host path for body and private-structure edits. Preserve
complete state/probe/presentation comparison, fresh-adopter construction, stale-ticket refusal
and original game/stream/window/process cleanup. Exercise suppressed probe effects, no historical
replay, new post-adoption sounds, compile/probe refusal and postcommit Pending behavior. Extend
the original native audio controls for the publication gate and callback samples; do not rerun
unrelated rendering/Editor matrices or add a new benchmark. Record actual Windows/device scope
and distinguish callback output from human audibility. Full R10/R12/Foundation remains active.

### Audible Development Qualification

The game protocol is version six. The original process owner admits its exact placement and
Runtime-bound audio diagnostics, including a final `AudioClosed` status alongside normal native
cleanup. These flags cannot replace the actual exit/containment/readers/output receipt. The Host
keeps actual incomplete audio retirement as a refusal to enable output. Completed game errors
remain separate and do not replace the original stream/callback and process completion facts. Ordinary
headless/silent packets and their existing controls retain their own variants.

The first production check exposes an unconstrained startup closure error type after adding the
audio status exchange. Its explicit `Result<(), String>` fixes inference without changing failure
custody. The affected release selection then passes 222/222 in
`b3897a97-09c6-4df0-976e-26b24c69ceb7`, with eleven native/actual-build cases skipped as declared.
It includes audio mixing/admission, gate capacity/identity/PCM, recipe fingerprints, protocol and
client caching/close, CLI parsing, complete resume/Scene semantics and ordinary watcher controls.
The initial dependency/test build takes 12m04s after the local build cleanup; test execution takes
18.614 seconds. This is qualification cost, not a runtime-performance observation.

Independent production reviews find no P1/P2 in the reset policy, conditional service eligibility,
original audio close, exact process diagnostics or adopted-route gate. Test review identifies one
missing branch: physically complete but failed predecessor retirement. The added actual source
consumer makes the original game's shutdown hook fail, retaining that actual process result and
healthy audio retirement. Final owning-contract review corrects the initial gate's overcoupling:
the existing close design separates failure from physical completion. The final successor must
enable new healthy output after confirmed old stream/callback/process retirement while retaining
the unrelated game failure. Pending or unconfirmed audio still refuses. The corrected case and
complete trajectory are qualified again below; initial results keep their earlier source scope.

The actual WASAPI selection passes 7/7 in `67c5261a-f1cf-4318-9fc0-6a7ef456d4dc` (3.452 seconds
after a 54.06-second affected build; 171 outside the selection). It includes the four new reset
controls and all three original native close/startup/unwind controls. The real game first collects
and travels while callback output is entirely zero, enables without replay, then compares complete
new pickup/travel PCM against the original independent WAV oracle with zero maximum error. A fresh
Runtime restores canonical state with a new disabled mixer and zero accepted effects. Ordinary
audio and unknown-service capture still refuse. Original held-callback, partial-startup and panic
cleanup keep stream/callback release independent from playback counters. These tests use a public
HeadlessSession with actual CPAL; the separate desktop consumer owns native presentation/adoption.
Neither scope establishes human audibility or hardware DAC latency.

The initial public Host target passes 2/2 without skips in
`655d4dff-725c-4deb-adf1-9ef5339af213` (528.556 seconds, after a 1m33s outer release build).
The failed-predecessor case takes 393.771 seconds including the first native-target development
dependency build (6m08s). Its original game shutdown returns failure after actual audio retirement;
the contained process exits unsuccessfully with empty containment and drained readers/output.
The initial gate continues refusing after physical completion; that overbroad policy is superseded
by the error/completion correction above. This initial green run is not final qualification of
the corrected failed-predecessor behavior.

The normal trajectory takes 134.783 seconds and performs six real rebuild attempts: intentional
compiler failure, rejected complete-state probe, cancellation, stale-basis cancellation, movement
body A160/B80 and private-structure C adoption. It retains meaningful cross-level progress, exact
canonical state and the original sealed/presented witness. Initial gameplay suppresses three
effects; the rejected probe and adopted Pending B each suppress a real pickup. No suppressed cue
is accepted, and A/B/C each accept one new enabled event. Each original probe is physically
discarded before a fresh adopter exists. B/C permit paused Step during Pending cleanup but refuse
enablement/activation/Resume; after normal predecessor retirement, old cues never replay and new
events complete. Actual native D Down/Up and continuous play then exercise the original input/
Runtime owners. Final audio/stream/callback, game, window, containment/readers/output and source
fixture cleanup pass.

The normal trajectory reports hardware Vulkan. Its parent requested DX12, but the existing sealed
desktop-session projection does not forward that hint; the result receives no DX12 credit. The
presentation witness is distinct from the separate PCM oracle and from scanout. These are
Windows/Rust1.95 correctness workflows, not latency distributions or native audio-state restoration.

The ordinary `develop --placement audio-desktop` CLI also passes a real bidirectional JSON run
with 35 validated records and actual nested dev builds. It starts disabled, discards a real travel
cue, enables without replay, completes a new effect, prepares/probes/adopts through the production
Host, refuses enablement during Pending predecessor cleanup, permits paused Step, then retires the
original predecessor and enables a new event. Explicit Close exits zero. Its three final reports
are the silent probe (zero accepted effects), predecessor (one) and active adopter (one), all with
normal game close, actual successful contained-process receipts and original audio physical flags.
Only the temporary diagnostic JSON/stderr files remain in its work parent. The driver stays outside
the repository; it adds no new protocol or verification framework.

Final corrected qualification passes 2/2 without skips in
`ef68a3ed-3930-454e-bc43-e156691efb06` (184.801 seconds after a 1m25s affected release build).
The failed-close case now passes in 50.319 seconds: actual A shutdown fails, original audio/
callbacks/process finish, and B enables without historical replay and completes a new cue while
the original failed result remains observable. This supersedes the initial permanent-refusal
expectation. The complete normal A/B/C trajectory passes again in 134.482 seconds with the same
six actual rebuild attempts and all prior state, presentation, Pending, native-input and physical
cleanup assertions. It again reports hardware Vulkan. Audio-device/PCM implementation is unchanged
by this Host-only permission correction, so its already passing seven native cases are not repeated.

The original silent continuous-development trajectory passes 1/1 without skips in
`3d700ea5-3353-4c93-aadd-9b3f35c6e97f` (155.611 seconds after a 7.20-second outer build). It
still uses exactly the silent child features while the outer test compiles audio support; actual
body/private-structure changes, native input, Pending-time play and physical cleanup remain intact.
The four selected controls with only `development`, no desktop/audio features, pass 4/4 in
`4689ffda-3be7-45d8-8496-fdd8eccdbfc1` (0.169 seconds, 23 outside the filter, after a first
9m15s minimal-feature build). They retain explicit unsupported-placement refusal, sound-command
input admission, exact feature selection and a healthy usable headless game after refusal.
These build/execution observations establish no speedup or numerical performance target.

Final all-feature/all-target release Clippy passes with `-D clippy::correctness`, followed by root
and independently locked game workspace formatting checks. Existing non-correctness warnings
remain. Added local document links/fragments and ordinary staged/working whitespace checks pass.
The existing manifests and locks are unchanged by this unit, and all fifteen paused UI/Dialogue
files retain their original bytes. Independent implementation, consumer and final close-contract
reviews close their actionable findings. No owned Cargo, child or review agent remains active at
delivery. Documentation closure does not require repeating the passing native workflows.

All source implementation, review corrections and affected checks remain local. General audio
playback-state migration, hot-switch/streaming, human audibility, hardware output latency, foreign
native execution, continuous Editor Play and the full R10/R12/Foundation outcome remain open.
Editor/UI/Dialogue work remains paused; compact Tilemap representation and further small performance
experiments remain deferred. The next unit follows missing Runtime capability, not another timing loop.

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,audio-cpal --lib --run-ignored only -E 'test(audio::native_tests::)' --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,audio-cpal --test audio_development --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail
```

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

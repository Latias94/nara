# Runtime, Execution, and Spatial Architecture Direction

Date: 2026-10-30. Research baseline: Nara `main`,
`75e1284ed28bc5a96686609f7dae8805d1924788`.

## Scope and evidence

This is a source-based study of App, managed Runtime, ECS execution, Tasks, Transform, and
Hierarchy. It makes no Rust changes and does not select an Accepted architecture replacement.
No Cargo commands, product tests, new performance runs, or subagents were used. Existing test
sources and dated measurement records are counterevidence, not newly executed verification.
General profiling/diagnostic feedback and platform qualification are covered by the coordinating
research branch and are deliberately outside this report.

Evidence labels are deliberately separate from issue classification. **source-confirmed** means
the cited implementation or Accepted contract establishes the property without executing it.
**unmeasured** marks integration behavior, performance impact, or replacement benefit not qualified
in this study. Earlier benchmark results are identified as historical measurements. A confirmed
explicit limitation does not, by itself, constitute a confirmed defect.

Local reference checkouts were clean when inspected. All reference paths below are relative to
the named checkout and refer to these exact commits, not to their current upstream branches:

| Reference | Checkout | Fixed commit |
| --- | --- | --- |
| Bevy | `repo-ref/bevy` | `ae6152152d6fb8308ad6038314b48ef295e80fbc` |
| Godot | `repo-ref/godot` | `7be59b463a4798834d72d6d0a2942ddd01d920ee` |
| Fyrox | `repo-ref/fyrox` | `5495d012b32145674501af2e67d2fd06a06f04dd` |

These pins differ from the Bevy/Godot pins in the October 23 reassessment. Mechanisms were
checked in the current local checkouts rather than carried forward by citation alone. Nara's
actual locked `bevy_ecs` 0.20.0 registry source was also read for executor feature selection;
the reference checkout is not substituted for that dependency. No web retrieval was needed;
the inspection date is 2026-10-30 and there are no external retrieval claims.

The governing context is [STRATEGY](../../../STRATEGY.md), the
[architecture map](../../architecture/README.md), and Accepted ADRs
[0002](../../architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md),
[0003](../../architecture/adr/0003-own-app-plugin-and-schedule-lifecycle.md),
[0005](../../architecture/adr/0005-dimension-aware-runtime-with-2d-first-authoring.md),
[0008](../../architecture/adr/0008-runtime-concurrency-and-task-pools.md),
[0024](../../architecture/adr/0024-determinism-fixed-update-and-replay-policy.md),
[0039](../../architecture/adr/0039-main-loop-time-pause-and-runtime-state.md),
[0052](../../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md),
[0057](../../architecture/adr/0057-authoritative-fixed-tick-and-command-ingress.md),
[0084](../../architecture/adr/0084-executable-runtime-ownership-and-isolation.md),
[0097](../../architecture/adr/0097-future-capable-2d-3d-spatial-transform-model.md),
[0100](../../architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md),
[0101](../../architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md), and
[0117](../../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md).
The [October 23 reassessment](../../architecture/foundation-architecture-reassessment.md),
especially D07-D09, and its
[rebaseline plan](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md), especially
R4 and R10, already identify much of this pressure. Their proposed directions are distinguished
from the implementation and Accepted contracts below.

## Verdict

The evidence supports retaining the Bevy ECS substrate, one managed execution owner per
simulation World, separate time domains, non-linked structural hierarchy, and immutable spatial
publications. These choices do not inherently require a future engine-wide rewrite for physics,
animation, multiple Worlds, or a custom-drawn Editor.

The expensive uncertainty is the combined execution protocol: who produces motion intent, who
commits solver poses, when an authoritative tick has succeeded, and which work must have returned
before an owner releases its state. The first independently authored Physics-like and
Animation-like consumer should be brought into **R4c's schedule comparison**, together with a
small R10 motion slice, before phase topology or animation/physics public APIs harden. Waiting
for complete physics, a complete animation system, or the custom-drawn UI migration would leave
the most consequential design assumptions untested.

No confirmed violation of a current supported contract was established in this bounded review.
That is not a claim that the runtime is defect-free. The six findings distinguish demonstrated
implementation properties from predicted integration risks:

| ID | Classification | Core finding | Decision trigger |
| --- | --- | --- | --- |
| RE1 | architectural risk | Closed automatic execution has not proved independent multi-phase domains | First combined physics/animation schedule, before R4c selection |
| RE2 | architectural risk | ECS execution mode is implicit in dependency features rather than an admitted execution policy | First parallel simulation consumer |
| RE3 | architectural risk | Finite task custody and native close are unsuitable as a universal frame-job protocol | First solver-owned work and Stop/restart integration |
| RE4 | architectural risk | Complete spatial publication currently imposes whole-population work and exclusive World barriers | Combined motion consumer, then representative sparse-scene scale |
| RE5 | intentional capability gap | Pose write authority and discontinuity semantics are not yet a joint domain contract | The same early R4c/R10 consumer |
| RE6 | intentional capability gap | Local TRS is not closed under general affine import or world-preserving reparenting | Before persistent animation targets and affine authoring are committed |

## Foundation checks that constrain the findings

**Multiple Worlds are supported through multiple owners.** `crates/nara_app/src/runtime/owner.rs:139`
stores one World, schedule collection, generation, and fault owner, with explicit thread affinity.
`crates/nara_app/tests/runtime_ownership.rs:67` exercises independent threads constructing,
driving, and retiring their own Worlds with non-Send data. This is materially different from a
process-wide singleton World. There is a bounded process fault-route table, currently 131 routes
(`crates/nara_app/src/runtime/fault_route.rs:7` and `:128`), with explicit admission failure;
it is not evidence that only one runtime can execute. A large server-sharding workload would need
to revisit that concrete admission capacity, not replace ECS or merge all Worlds.

Bevy's `crates/bevy_app/src/sub_app.rs:574` performs extraction and update across separate
SubApps at the Bevy pin above. Its ordinary update loop is sequential; merely introducing a
SubApp does not prove render/simulation overlap. Nara can keep explicit capture/transfer between
simulation and a future UI/render owner without making a live mutable World jointly owned.
The present borrowed spatial snapshot is not a cross-thread retained packet; any such packet
needs an explicit owned capture boundary.

**The fixed clock is a useful foundation.** `crates/nara_app/src/lib.rs:1977` plans a frame before
committing time; `:1998` advances each due fixed tick before running its schedule; `:2038` gives
exact stepping a separate transaction. Existing tests at `:2721`, `:2737`, `:2812`, `:3101`,
`:3126`, and `:3160` cover clock progression, deferred boundaries, tracker rotation, bounded
catch-up, and overload refusal. PreserveDebt and DiscardExcess are deliberate profiles, not
accidental elapsed-time loss. Fixed steps alone do not establish replay determinism: D09/R10's
explicit RNG and authoritative external-outcome policy remain necessary. Neither a missing
network stack nor a missing audio implementation is counted as a defect here.

**Recent repairs change the baseline.** Installed schedules now receive fallible pre-start graph
admission, and explicit custom schedules run through the managed owner
(`crates/nara_app/src/lib.rs:1642`; `crates/nara_app/src/runtime.rs:2279`). GlobalTransform2d is
no longer a transferable ECS component authorizing freshness
(`crates/nara_transform/src/lib.rs:57`). Spatial readers bind World, generation, phase, and time
(`crates/nara_transform/src/snapshot.rs:34`). Task scopes, lazy facilities, and owner-local close
participants exist. The old findings that all work owns eager per-runtime pools, every local
participant must be Send, or copied global components can authorize foreign completion must not
be repeated as current defects.

## RE1 — Automatic phases need a real cross-domain composition proof

**Classification: architectural risk.** The current driver is coherent for its supported game;
its extensibility has not yet proved the protocol required by independently authored motion
domains.

**Evidence status:** source-confirmed automatic order, pause policy, and publication/Ack placement;
unmeasured independent physics/animation composition and candidate replacement benefit.

**Source evidence and trigger.** `crates/nara_app/src/lib.rs:120` fixes `CoreStage::ALL`;
`:1998` repeats only `FixedUpdate`, and `:2098` hardcodes the stages that run while paused.
`crates/nara_app/src/runtime/owner.rs:239` installs the fixed Prepare/Simulate/Finalize chain.
Only Simulate is the public joinable fixed phase (`crates/nara_app/src/lib.rs:148`). A custom
schedule is explicitly driven between frames, requires Running state, and advances no clocks
(`crates/nara_app/src/runtime.rs:2279`). It therefore cannot, by itself, supply an automatic
per-tick physics pre-sync/solve/post-sync protocol or a paused Editor animation-preview phase.

The concrete trigger is combining root-motion evaluation, kinematic intent, solver advancement,
solver pose writeback, and final spatial observation in every due tick, including exact step.
A Host manually running another schedule once per displayed frame is not equivalent when the
frame contains zero or several ticks. Routing paused preview through an incidental render or
maintenance stage would also make its temporal policy accidental.

The current spatial barrier makes this more concrete. Hierarchy completion is after all Simulate
members (`crates/nara_hierarchy/src/lib.rs:269`), and fixed Transform publication follows it
(`crates/nara_transform/src/propagation.rs:73`). A solver needing fresh world-space targets from
an animation system earlier in that same Simulate interval cannot obtain them merely by joining
the existing completed-fixed reader helper: that helper is after the whole interval. It must
either own a precisely scoped target conversion or participate in a newly defined intermediate
publication. The joint consumer should determine which is necessary before adding another global
propagation barrier.

There is a contract reconciliation to do before expanding finalization. Current command
retirement is `CompleteConsumption -> Capture -> Acknowledge` inside Finalize
(`crates/nara_gameplay/src/lib.rs:135`, `:190`, `:305`). Accepted ADR 0057 at lines 148-173
explicitly permits an unrelated fault after acknowledgement without reversing that completed
batch. D07's proposed whole-successful-tick finalization is stronger. A later physics/animation
phase cannot silently treat today's Ack as proof that all of its work succeeded. This is a
future composition issue, not a newly discovered contradiction in today's command contract.

**Existing safeguards and counterevidence.** Public systems can already compose sets within
Simulate, with normal Bevy access/deferred ordering. `add_fixed_transform_projection_systems`
provides a semantic completed-spatial read point (`crates/nara_transform/src/lib.rs:89`).
`crates/nara_app/tests/schedule_registry.rs:72`, `:221`, and `:255` cover graph rejection,
inert custom schedules, and managed explicit execution. This is not a request to invent a
scheduler because custom labels are missing.

**Reference mechanisms.** At the Bevy pin, `crates/bevy_app/src/main_schedule.rs:350` has an
ordered FixedMain schedule list and insertion before/after anchors; `:393` drives the complete
list each fixed iteration. `crates/bevy_app/src/app.rs:146` offers `App::empty()` as an actual
control candidate. At the Godot pin, `scene/main/scene_tree.cpp:1303` orders process groups and
joins worker groups before continuing. These are concrete protocol mechanisms; they do not
supply Nara's fault, Ack, or close guarantees automatically.

**Recommended boundary and deferral cost.** Exercise public sets first and compare the same
consumer with a configuration-time sealed phase plan and `bevy_app::App::empty()` plus Nara
lifecycle ownership. If sets express the entire workflow without private anchors, retain that
smaller contract. Otherwise admit a finite ordered phase plan, retaining Bevy's scheduler and
access analysis. Specify clock, pause/exact-step behavior, deferred visibility, and the success
boundary together. Reconcile ADRs 0003 and 0057 if adopting the stronger completion meaning.
Do not build a general looping workflow language. Deferral beyond independently shipped domain
plugins spreads private phase ordering into gameplay, adapters, replay capture, and tools, which
then have to migrate together.

**Existing coverage:** D07 and R4c already prescribe this comparison; R4c remains open in the
plan. The recommendation is to advance its consumer together with R10, not create a competing
execution roadmap.

## RE2 — Parallel execution needs an explicit policy before integrations rely on it

**Classification: architectural risk.** Serial default execution is not itself a bug or proof
that Bevy ECS is the wrong substrate. The risk is allowing dependency feature unification to
silently determine the execution model against which domain integrations are qualified.

**Evidence status:** source-confirmed manifest/default-executor coupling; unmeasured current
consumer feature unions, parallel integration behavior, and any speedup or oversubscription cost.

**Source evidence and trigger.** `Cargo.toml:99` uses `bevy_ecs = "=0.20.0"` without enabling
`multi_threaded`; `crates/nara_ecs/Cargo.toml:9` forwards that dependency. The inspected locked
crate's `Cargo.toml:64` omits that feature from defaults, while its
`src/schedule/executor/mod.rs:44` selects the default executor from feature/platform cfgs.
Nara constructs schedules with `Schedule::new` (`crates/nara_app/src/runtime/owner.rs:239`) and
does not expose raw built-in executor replacement (`crates/nara_app/src/lib.rs:1368`). Manifest
inspection and the existing October 21 four-profile study support the serial default; no new
Cargo feature graph was generated in this research.

Thus Nara Tasks workers are not evidence of concurrent ECS systems. Conversely, an independent
consumer enabling the same Bevy dependency's multi-threaded feature can change default schedule
execution through feature unification. This conditional follows from the source; it was not
executed here. A physics/animation integration that depends on incidental serial ordering among
unordered systems is therefore an unsuitable long-term contract.

**Existing safeguards and counterevidence.** The facade exports the real Bevy types rather than
an alternative borrowing model (`crates/nara_ecs/src/lib.rs:14`, `:45`). Bevy access analysis
continues to enforce resource/component conflict safety. The single owner thread is compatible
with workers running eligible systems; it does not require every system to be serial. The
independent-World test above already protects owner-thread construction and destruction.
Semantic ordering still needs explicit sets even under serial execution.

**Reference mechanisms.** At the Bevy pin, `crates/bevy_ecs/src/schedule/executor/mod.rs:44`
contains the same explicit feature-dependent executor selection, and
`crates/bevy_app/src/task_pool_plugin.rs:160` apportions the physical pool budget across kinds.
At the Godot pin, `scene/main/scene_tree.cpp:1303-1330` admits ordered threaded process groups
and waits at their join. Neither engine equates background thread count with valid simulation
parallelism.

**Recommended boundary and deferral cost.** Before qualifying parallel gameplay, make the
supported execution profile deliberate at composition/seal and exercise the same joint consumer
under serial and parallel scheduling. Preserve non-Send placement and explicit dependency edges.
Account for ECS, solver, and background facilities at the Host's concrete composition boundary;
do not simply enable another all-core pool. This does not require a new executor implementation
or make unconditional worker sharing desirable. Delaying until several domain plugins rely on
serial accidents turns a feature toggle into a broad ordering and lifecycle repair.

**Existing coverage:** the
[execution-budget plan](../../plans/2026-10-21-task-execution-budget-plan.md) explicitly declines
to enable multi-threading as dependency cleanup. R4b/R4c provide the appropriate consumer and
facility boundary. This finding adds no general tracing or platform-performance work.

## RE3 — Keep finite task custody out of the synchronous solver hot path

**Classification: architectural risk at a future integration boundary.** The current Tasks
contract deliberately pays for work that can outlive its submitter. Generalizing it into every
physics/animation job would import lifetime costs and timing semantics those jobs may not need.

**Evidence status:** source-confirmed owned job shape, disposal accounting, and independent close
semantics; unmeasured solver-job overhead and concrete physics/animation Stop/restart behavior.

**Source evidence and trigger.** `crates/nara_tasks/src/runtime.rs:1724` accepts only owned
`FnOnce(TaskCancellationToken) -> T + Send + 'static`; `:1759` creates shared task state and
`:1782` boxes the job. IO, Compute, and AsyncCompute are queues for this same execution family,
not distinct borrowing/Future models. `:3252` and `:3279` deliberately separate terminal
publication from capture/result disposal. `crates/nara_tasks/src/runtime/scope.rs:63` states that
scope completion does not prove worker join or native resource retirement.

The trigger is a solver or animation evaluation that needs borrowed frame data and must finish
inside the current fixed tick. Turning each small job into an owned cross-frame task requires
copying/retaining inputs, handling rejection/cancellation, and later integration. TaskUpdate runs
before the fixed loop (`crates/nara_app/src/lib.rs:120`), so using its ordinary next-frame result
cut for current-tick solver authority would change the simulation protocol. The presence of a
Compute queue does not make that substitution correct. These are source-level obligations, not
a measured per-job latency claim.

The related close constraint remains explicit: `crates/nara_app/src/runtime/close.rs:176` visits
independent entries in reverse order but continues after Pending. The public contract at
`crates/nara_app/src/runtime.rs:953` requires a fixed dependent aggregate to gate its own provider
release. A solver controller, borrowed/native state, and outstanding solver workers must be such
an aggregate or retain a concrete provider lease. Registering them as unrelated entries would
not create completion dependencies.

**Existing safeguards and counterevidence.** This is not the old missing-scope or Send-only
problem. TaskFacilities and TaskScope already separate worker lifetime from activity custody
(`crates/nara_tasks/src/runtime/scope.rs:9`, `:69`). Tests at
`crates/nara_tasks/src/tests/scopes.rs:145`, `:243`, and `:477` cover late-result disposal, child
admission, and independent Runtime retirement. Owner-local typed close is tested in
`crates/nara_app/tests/native_close_owners.rs:434` and `:598`. Editor authoring already selects
lazy private execution with a 2/1/1 configuration
(`src/project_host/runtime/editor/authoring_runtime.rs:851`, `:893`). The measured
[lazy/shared comparison](../../benchmarks/2026-10-26-task-facility-locality.md) demonstrates that
activity isolation does not reserve progress against arbitrary blocking closures. It does not
justify merging all facilities into one global pool.

**Reference mechanisms.** At the Bevy pin, `crates/bevy_tasks/src/task_pool.rs:317` provides a
scoped join for borrowed work, distinct from the owned Future spawn at `:566`. At the Fyrox pin,
`fyrox-core/src/task.rs:50`, `:127`, and `:135` accept Futures and route completed values through
a result channel; `fyrox-impl/src/engine/task.rs:60` separates background execution from
main-loop application. At the Godot pin, `core/object/worker_thread_pool.cpp:655` and `:725`
provide group jobs and explicit completion waits. These mechanisms distinguish work families;
their APIs do not automatically satisfy Nara's stronger physical-return contract.

**Recommended boundary and deferral cost.** Keep Nara Tasks for finite owned background work;
use Bevy/solver-owned scoped execution for same-tick borrowed jobs, with the solver owner joining
before publishing authoritative output. Add a private Future/reactor adapter only when an actual
waiting-I/O consumer requires it. Keep typed domain result application and one domain-owned close
aggregate. Do not add a universal ServiceHub, capture introspection, writer ACL, or public close
graph to make all work look identical. The joint consumer should exercise Stop while work is
held, failed-but-returned work, and a retained dependent owner. Deferral until domain workers and
native handles are distributed across plugins makes safe Stop/restart a cross-module rewrite.

**Existing coverage:** D08 and R4a/R4b, including the adopted
[owner-local close design](../../architecture/native-close-ownership-design.md) and
[activity-scope design](../../architecture/task-activity-scope-design.md), already cover the
chosen lifecycle mechanisms. The remaining question here is their use by a concrete motion domain,
not reopening the completed custody corrections.

## RE4 — Spatial publication is correct but currently pays global work for local motion

**Classification: architectural risk.** This is an execution/data-locality limitation with
existing measured evidence, not a claim of a present frame-budget violation.

**Evidence status:** source-confirmed scan/rebuild and exclusive-system shape; historically
measured in the October 23 harness; current product impact and incremental alternatives remain
unmeasured in this study.

**Source evidence and trigger.** `crates/nara_transform/src/propagation.rs:59` installs exclusive
publication work at Startup, each fixed tick, PostUpdate, and Extract. `:121` takes `&mut World`;
`:149` compares every local/entity/parent row, and `:182` rebuilds the entire candidate if any
row changed. The rebuild at `:211` reconstructs source/global maps and structural indices and
walks the complete captured hierarchy. Optional interpolation adds source capture and another
complete projection (`crates/nara_transform/src/presentation.rs:122`, `:165`, `:253`).

The trigger is a large mostly-static scene with a small animated/physical island, multiple due
fixed steps, or several independent preview Worlds. Adding a 3D domain by copying this algorithm
would repeat the same population scans and exclusive scheduling barriers. Enabling Bevy's
parallel executor alone cannot parallelize an exclusive `&mut World` system or make a whole-cut
rebuild proportional to changed descendants.

**Existing safeguards and counterevidence.** Private immutable cuts fixed real temporal/provenance
problems; removing them would be a regression. Unchanged matrices are reused, observation vectors
and candidate buffers are retained, and the existing hierarchy order is reused rather than a
second adjacency graph. Hierarchy has its own validated baseline fast path
(`crates/nara_hierarchy/src/writer.rs:137`) and unchanged-generation coverage
(`crates/nara_hierarchy/src/tests.rs:648`). Read-only snapshot SystemParams can still run together;
the publication barrier is the concern, not a blanket statement that all spatial readers serialize.

The [October 23 cost record](../../benchmarks/2026-10-23-low-level-spatial-cuts.md), lines 61-108,
reports final-source 4,096-member P50 values of 38.3 us static, 174.3 us for one changed leaf,
and 173.1 us for a changed root; warmed static/leaf windows allocate zero. Its 65,536-member
figures show the same whole-population scaling. Those are historical harness observations, not
new results at `75e1284e`, a current product requirement, or a P99 product qualification.

**Reference mechanisms.** At the Bevy pin, `crates/bevy_transform/src/systems.rs:111` marks
dirty trees, and `:505` propagates through parallel work with static-subtree pruning. The source
also acknowledges change-filter scans at `:272`; this is not a zero-scan oracle. At the Fyrox pin,
`fyrox-impl/src/scene/graph/mod.rs:1280` consumes node-change messages, coalesces affected roots,
and updates selected subtrees at `:1357`. It still allocates a visited array sized by pool capacity
at `:1291`, so its comments are not evidence of universally O(changes) total cost. The useful
comparison is how mutations identify affected work, not an assertion that either engine is
uniformly faster.

**Recommended boundary and deferral cost.** First use the motion tracer to identify the required
publication cuts; then compare incremental private payload maintenance and changed-root
propagation on representative sparse and dense scenes. Retain an atomic publish boundary and
all current source identity checks. Ordinary safe mutation can occur multiple times within one
Bevy change tick (`crates/nara_transform/src/snapshot_tests.rs:77`), so replacing the source
comparison with `Changed<Transform2d>` alone is insufficient. Any producer revision or scoped
mutation mechanism must account for that behavior or explicitly change the supported contract.
Do not create a generic TransformDomain abstraction merely to share 2D/3D code. Delaying until
rendering, physics, animation, and 3D each add private caches makes later invalidation repair much
larger; changing only the private storage behind today's readers remains comparatively contained.

**Existing coverage:** ADR 0100 explicitly accepts the comparison scan; the October 23 spatial
correction and benchmark already record its tradeoff. R10 provides the combined consumer, while
the benchmark's follow-up trigger owns further locality work. No new profiling pipeline is
proposed here.

## RE5 — Authored, simulated, and presented pose need one joint ownership decision

**Classification: intentional capability gap.** The absence of a physics or animation module is
not the issue. Their shared write/read protocol has deliberately not been implemented or qualified.

**Evidence status:** source-confirmed local write access and interpolation/reset rules;
unmeasured joint motion behavior. The teleport example below is a source-derived prediction,
not an executed regression or a claimed failure of a supported teleport API.

**Source evidence and trigger.** `crates/nara_transform/src/lib.rs:28` exposes an ordinary mutable
TRS component used as authored/runtime-local authority. The completed snapshots make reads
truthful, but they do not decide which eligible system should produce the local pose. The fixed
starting sample is captured after Prepare and before Simulate
(`crates/nara_transform/src/presentation.rs:98`); `:169` pairs it with the resulting local pose
when the parent frame matches. Presentation resets pairs for pause, later source edits, or parent
changes (`:215`), but the public surface has no per-entity discontinuity input distinguishing a
teleport/correction during Simulate from normal motion.

For example, an unchanged-parent entity moved from x=0 to x=100 during Simulate enters the normal
interpolation pair. The source path predicts an intermediate presentation position when alpha is
between zero and one. That is correct for continuous movement and may be wrong for a future
teleport command; no currently promised teleport operation is claimed broken. Similarly, animation
and solver systems both taking mutable Transform access are memory-safe but do not thereby have
a semantic rule for who wins. This becomes costly when root motion, kinematic targets, dynamic
bodies, and correction all share that component.

**Existing safeguards and counterevidence.** Historical, fixed, and presented reads are distinct;
presentation never writes simulation locals. Pre-simulation edits are already treated as the
starting sample, and parent changes reset the affected pairing. Tests at
`crates/nara_transform/src/snapshot_tests.rs:409`, `:640`, `:668`, and `:715` cover these boundaries
and reject foreign history. ADR 0097 at lines 152-158 already requires body-type-specific scale
and parenting rules and plugin-owned animation evaluation. A completed affine snapshot does not
authorize a rigid solver to accept arbitrary shear.

**Reference mechanisms.** At the Fyrox pin, `fyrox-impl/src/scene/graph/mod.rs:1436` processes
hierarchy changes, syncs native state, advances physics, and then updates nodes. Its rigid-body
owner explicitly distinguishes sync-to-physics and solver readback with a parent transform
(`fyrox-impl/src/scene/rigidbody.rs:629`, `:645`). At the Godot pin,
`scene/2d/physics/rigid_body_2d.cpp:152` installs solver state while suppressing transform
notification feedback, and `scene/main/node.cpp:976` provides an explicit interpolation-reset
operation. These are concrete authority and discontinuity mechanisms, not arguments for copying
a scene-tree object model.

**Recommended boundary and deferral cost.** The early joint consumer should give animation/gameplay
typed motion intent, the solver authoritative dynamic-body pose output, and the Transform owner
a single commit/publish cut. Define teleport/correction interpolation reset, parent-frame policy,
and the allowed rigid-body transform subset in that same consumer. Ordinary non-physical gameplay
can keep direct local editing; a World-wide ACL would neither sandbox trusted Rust nor solve the
domain protocol. A minimal 2D fixture can expose these issues before full 3D or animation content.
Deferral until save/replay state, animation tracks, gizmos, and runtime bodies independently assume
they own Transform forces a semantic migration across all of them.

**Existing coverage:** D09 and R10 explicitly identify pose roles, root motion, parenting, teleport,
and interpolation. R10 is recorded as not started. Move this bounded motion slice to the start of
R4c rather than treating it as a final integration check after schedule design.

## RE6 — A future affine workflow can require a local schema change, not a new hierarchy

**Classification: intentional capability gap.** Current KeepLocal placement and affine global
composition are valid. They do not promise a future exact KeepWorld or arbitrary imported local
matrix can be stored in current TRS.

**Evidence status:** source-confirmed stored representation and supported operation boundary;
the representational counterexample is analytical. Authoring ergonomics, numerical tolerances,
and migration costs of the candidate replacements remain unmeasured.

**Source evidence and trigger.** Local values contain translation, one angle, and axis scale
(`crates/nara_transform/src/lib.rs:28`); the codec persists those exact fields (`:198`) and exposes
their field identities (`:277`). Propagation multiplies affine matrices directly
(`crates/nara_transform/src/propagation.rs:260`) and never decomposes the global result. Thus
rotated non-uniform ancestors already produce correct global shear, but that does not provide a
local shear representation for a future imported node or reparent result.

A simple analytical trigger is an old world rotation of 45 degrees and a new parent scale of
(2,1). Keeping the world pose needs local linear part `diag(1/2,1) * R(45 degrees)`. Its two
columns have dot product 3/8, whereas a 2D rotation times diagonal scale has orthogonal columns.
It cannot be expressed exactly as current local TRS. A short local Python arithmetic check
confirmed this matrix example; it did not execute or test Nara. Singular parents additionally
require an explicit inverse-refusal policy.

**Existing safeguards and counterevidence.** ADR 0097 at lines 129-160 accepts KeepLocal and
explicit refusal of unsupported affine import; residuals, KeepWorld variants, and 3D fields remain
Proposed. Structural ancestry is dimension-neutral and does not imply lifetime or physics
ownership (`crates/nara_hierarchy/src/lib.rs:1`). Direct non-spatial parents intentionally stop
inheritance (`crates/nara_transform/src/propagation.rs:243`;
`crates/nara_transform/src/snapshot_tests.rs:308`). An identity spatial grouping node is an
explicit way to preserve a chain. These boundaries support a separate 3D domain and independent
custom UI layout without redefining every entity as a universal 3D object.

**Reference mechanisms.** At the Godot pin, `scene/2d/node_2d.cpp:132` derives rotation/scale/skew
from the affine value, `:139` constructs the local matrix including skew, and `:148`/`:388`
implement world-preserving reparent through affine parent conversion. At the Fyrox pin,
`fyrox-impl/src/scene/transform.rs:79` stores pre/post rotation, offsets, and pivots as well as TRS,
reflecting an import-driven authoring model. A richer model alone is not proof of exact general
reparenting: its `fyrox-impl/src/scene/graph/mod.rs:686` reconstructs local position/rotation/scale.
At the Bevy pin, `crates/bevy_transform/src/components/global_transform.rs:182` explicitly warns
that TRS reparent decomposition requires nondegenerate, non-sheared input. Mature engines expose
real representational tradeoffs; their existence does not make lossy decomposition correct.

**Recommended boundary and deferral cost.** Before persistent animation targets or affine authoring
tools harden, compare one import/edit/animate/Undo/round-trip workflow using the existing proposed
candidates: TRS plus residual, typed direct affine, and explicit authored skew/pivot fields.
Keep rejection as the honest current behavior. Choose canonicalization and whether authored base
fields survive world-preserving edits together with interpolation, not as separate schema patches.
Do not preemptively implement all 3D or FBX fields. The unavoidable possible break is localized to
spatial source representation, codecs, property targets, and their consumers; hierarchy, ECS,
managed Runtime ownership, and completed-read semantics can remain. Deferral until large content
libraries and animation curves depend on the old field meaning makes that localized break much
more expensive.

**Existing coverage:** the
[spatial affine candidate design](../../architecture/spatial-affine-representation-design.md)
already retains the alternatives, while ADRs 0005/0097 explicitly decline a future schema
guarantee. It is a Proposed comparison, not an implemented migration plan. R10's parenting and
motion consumer should expose the needed subset; the first actual affine-authoring workflow
should decide the larger persistent representation.

## Where to advance the first joint consumer

The next runtime architecture unit should start with the following bounded workflow, rather than
finishing an abstract phase API first. This is a recommendation for the existing R4c/R10 owners;
it does not edit their plan or require a complete physics/animation product release.

1. **Compare execution using two independently authored domains.** Use an Animation-like producer
   of root-motion intent and a Physics-like consumer with dynamic and kinematic roles. Establish
   input/intent, solve, pose commit, spatial publication, and command-success observation. Run the
   same workflow through public sets, the finite sealed-plan candidate, and the Bevy App control.
   Resolve missing anchors and the Ack scope before selecting a replacement. A deterministic
   stand-in can compare ordering first; a thin real-solver integration must check the chosen body
   and worker assumptions before declaring the boundary suitable for physics.
2. **Carry motion and lifetime through the same consumer.** Exercise zero/multiple fixed ticks,
   pause, exact step, deferred spawning, reparenting, a non-spatial grouping boundary, teleport,
   correction, and held worker retirement. Verify a fixed snapshot describes committed solver
   state and a presented snapshot describes the selected interpolation policy. Failures must keep
   their actual effects and ownership evidence; a broader tick-success contract must not silently
   inherit today's narrower command Ack semantics. Do not add audio or networking merely to
   increase coverage count.
3. **Resolve representation and scaling only where this workflow creates pressure.** Keep ordinary
   2D TRS and KeepLocal while bringing forward one affine authoring/animation round trip before
   its fields become persistent ecosystem contracts. Compare sparse and dense spatial updates and
   explicit serial/parallel scheduling after the semantic oracle is stable. Reuse the existing
   focused harnesses; no source-analysis script or compiler-like proof layer is warranted.

For the custom-drawn Editor, preserve a Host/UI update domain that can remain responsive while
game simulation is paused, and consume explicit spatial/render captures. Sharing a UI document
hierarchy or renderer infrastructure does not require sharing mutable gameplay pose authority or
the game's virtual clock. RE1 and RE5 are the relevant runtime prerequisites; the separate UI and
render research owns the frontend and graphics design.

The largest avoidable refactor is delayed agreement about execution and pose ownership. The
available evidence does not justify replacing Bevy ECS, linking structural parents to destruction,
building a universal task kernel, or choosing a universal transform representation in advance.

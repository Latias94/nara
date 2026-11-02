# Runtime, Execution, Identity, Hierarchy, and Transform

**Source assessment, 2026-11-02.** Nara: `a2640a10973b6d1724176a506237bc458aa5f3bc`.
Read-only references: Bevy `ae6152152d6fb8308ad6038314b48ef295e80fbc`,
Fyrox `b5b151b603bba0549ac03db676568726f17e6807`, and Godot
`95c0fa5fe8a019adc29d074c16b64469b04571f7`. All reference paths below belong to
these pins. Nara locks `bevy_ecs` **0.20.0** (`Cargo.toml:100`, `Cargo.lock:425`);
the Bevy checkout identifies itself as **0.20.0-dev** (`repo-ref/bevy/Cargo.toml:3`).
Reference-main behavior is not evidence of Nara's resolved dependency behavior.

The recommendation is to **retain Nara's execution and semantic ownership, converge
ordinary composition mechanics, and defer new execution/storage machinery until a
consumer demonstrates its value**. This follows STRATEGY's independent-module and
complete-workflow requirements, Accepted ADRs 0002/0003/0008/0039/0052/0057/0058/
0084/0097/0100/0101/0117, and the implementation ledger. R4c remains open; R10
motion/repeatability is explicitly not started. Those are planned capabilities,
not newly discovered defects.[^authority]

## Invariants before mechanisms

A host must distinguish elapsed wall time, paused/scaled game time, authoritative
ticks, and presented interpolation. Borrow compatibility does not establish causal
order: animation intent, solver input, borrowed-job completion, pose commit, and
spatial publication need explicit edges. A cancelled result is not proof that a
closure or native provider can be destroyed. Likewise, document identity, logical
residency, live activation, and allocator slots answer different questions.
Structural parenthood need not confer lifetime ownership.

Data locality must be assessed after those meanings are fixed. Contiguous ECS
components, graph nodes, lookup indexes, and immutable snapshots optimize different
access patterns. Counting files or types cannot decide whether their costs are
necessary. The mechanisms below establish work and ownership, not measured speed.

## Comparison matrix

| Requirement | Nara | Bevy reference | Fyrox reference | Godot reference |
|---|---|---|---|---|
| Clocks and causal order[^clock] | Host supplies delta; App plans bounded fixed catch-up, discard/preserve debt and exact paused steps. Closed automatic stage order; domains order Bevy sets. More explicit policy to maintain. | Main dispatches schedule labels; virtual delta accumulates into repeated FixedMain runs. Facilitator schedules use a serial executor; inner schedules remain separate. Flexible ordering requires author-declared edges. | Executor accumulates lag, normally uses fixed dt, but periodically substitutes accumulated lag above its throttle threshold. Engine orders plugins/scripts before scene updates. Convenient loop with a different overload policy. | Main timer chooses process delta and physics steps, caps catch-up, then orders physics sync, query flush, callbacks and step. SceneTree orders node processing. Timing semantics are distributed across tree and servers. |
| Borrowed versus owned work[^tasks] | Bounded `Send + 'static` closures, typed terminals, cooperative cancellation and separate disposal return. Activity scopes can share facilities; they are not lexical borrowing scopes or fairness reservations. | Future-based spawn plus lexical `TaskPool::scope` that joins borrowed work. Standard pool accessors are global; explicit pools also exist. Scoped execution and background tasks are distinct choices. | Futures run on a thread pool; UUID-tagged, type-erased results return through a channel to plugin/script handlers. Convenient callbacks retain explicit scene/node/script routing. | Worker tasks/groups have explicit completion waits. SceneTree joins threaded process groups before advancing group order. Native userdata lifetime and safe object access remain caller/engine obligations. |
| Close and owner lifetime[^close] | Thread-bound owner, finite polling, sticky errors, retry and retained incomplete custody. Activity return differs from worker join. Typed composites enforce dependent release. | Exit messages/sets provide an exit hook; plugin cleanup runs during initialization. An owned TaskPool joins threads in Drop. These alone do not express Nara's resumable close evidence. | Engine Drop removes scenes and clears dynamic-code-bearing objects before plugins. A concrete destruction sequence keeps code alive through dependent destruction. | Main cleanup orders loader, tree, language and server teardown; WorkerThreadPool finish joins threads. Concrete process teardown differs from independently retiring one embedded runtime. |
| Semantic identity[^identity] | World-bound indexes distinguish source IDs, persistent keys, runtime incarnations and locators. Extra maps/history buy stale-activation and foreign-World rejection. | Entity is an opaque generational handle with no durable wire-format promise. The inspected scene API resolves/spawns into entities; that does not supply Nara's persistent/unloaded/deleted contract. | Generational pool handles coexist with UUID `SceneNodeId`, an ID-to-handle map, and resource/original-node associations. It also needs identity beyond storage slots. | ObjectDB combines slot and validator for live ObjectID; PackedScene resolves serialized node paths during instantiation. Live object validity and scene addressing remain different mechanisms. |
| Structure versus ownership[^hierarchy] | Immutable private-field relations, explicit ordered roots, one atomic batch writer; ancestry does not cascade destruction. Sparse touched-parent overlays still copy affected sibling lists. | ChildOf maintains Children through relationship hooks; built-in Children enables linked destruction/cloning. Custom relationship targets can omit it, but hook mutation is not Nara's batch transaction. | Graph owns parent/child structure; remove_node traverses and frees descendants. Convenient subtree ownership is part of the model. | Node deletion deletes children; deferred queue deletion flushes at tree boundaries. Tree membership carries more lifecycle meaning than Nara's structural projection. |
| Spatial freshness and locality[^spatial] | Complete Historical/Fixed/Presented views validate World, generation, role/time and membership. Every completion scans local values; changed input rebuilds a full candidate, unchanged input reuses matrix payload. | GlobalTransform components, change detection and optional dirty-tree optimization; parallel traversal uses scoped workers. Requires maintaining change/relationship invariants and traversal synchronization. | Node messages select changed chains for propagation, with capacity-sized visited flags; graph update also scans pool capacity unless overridden. Boxed node behavior adds indirection. | CanvasItem invalidates descendant caches and lazily composes globals through direct CanvasItem parents. Work moves toward mutation/read boundaries; this is not an immutable tick snapshot. |

## Traced frame, fixed tick, and close

The native entry is `nara_winit::run_frame_transition` -> `RuntimeInstance::drive`
-> `drive_app_frame` -> `RuntimeOwnerState::run_frame_transaction`. The owner contains
one World and one Schedules collection; App configuration and Runtime execution do
not maintain duplicate worlds or clocks. Managed schedule execution validates its
authority and calls Bevy `Schedule::run` directly.[^frame]

Frame planning precedes `First -> TaskUpdate -> PreUpdate`. Each due fixed iteration
advances FixedTime, then runs `Prepare -> Simulate`. Hierarchy completion follows
Simulate; Transform explicitly flushes deferred commands and publishes the fixed
spatial cut before Finalize. Gameplay chains completion -> Capture -> Ack inside
Finalize. PostUpdate and Extract can publish later spatial cuts; presentation is
derived after Extract propagation. Successful frame cleanup clears ECS trackers
once. An exact paused step runs the fixed transaction directly, preserving existing
debt/remainder; it does not first integrate TaskUpdate. Therefore background-result
polling cannot be the same-tick solver join.[^frame]

The close continuation is `Stop -> begin_close -> drive_close`: plugin shutdown,
then reverse-registration participant polling, then physical-completion evaluation.
The ledger continues past a pending independent participant; reverse order is **not**
a dependency-completion barrier. A TaskScope reports Pending until accepted work and
executor-owned disposal return, while shared workers may remain alive. CompleteWithError
permits physical retirement but preserves failed Stop. This justifies a composite
provider/user owner where release depends on another participant.[^close][^tasks]

The reference chains likewise cross owners: Bevy App/SubApps -> Main -> Time's
FixedMain loop; Fyrox executor -> Engine pre_update -> Scene/Graph update; Godot
Main iteration -> SceneTree physics processing -> WorkerThreadPool group join,
then server step. None makes borrowing safety alone a simulation-order policy.[^clock][^tasks]

## Five concrete decisions

For ordinary gameplay, a mover can use `Query<&mut Transform2d>` in
`FixedUpdateSet::Simulate`. Completed-pose readers use
`add_fixed_transform_projection_systems`; presented readers use
`add_presented_transform_projection_systems` and `PresentedSpatialSnapshot2d`.
Finalize/Capture reads the fixed view directly. The everyday cost is understanding
these few freshness choices and asynchronous outcomes. Internal publication stamps,
candidate ownership and close bookkeeping should remain behind those existing
entry points.[^frame][^spatial]

1. **Keep one managed runtime; trial convergence of App plumbing, not removal of its semantics.**
   The ordinary author should register systems/sets and use one host drive path.
   Nara already delegates query borrowing, graph building and execution to Bevy;
   its own ownership boundary supplies unpublished startup, generation isolation,
   paused stepping and retained close. The counterproposal is `bevy_app::App::empty()`:
   it can supply a minimal container without imposing default Main scheduling.
   Compare that in R4c against the current App and sealed phase plan. Migration must
   carry fallible seal, deferred boundaries and faults; otherwise saved plumbing is
   exchanged for host-specific wrappers. Fyrox/Godot show why concrete engine-owned
   orchestration can remain useful. Adopt only demonstrated simplification; no
   present evidence warrants an App replacement or another scheduler.[^frame][^clock][^close]

2. **Freeze semantic joins and executor policy before widening public phases.**
   Physics-like and Animation-like authors need a documented pose/input freshness
   contract, not private-system ordering knowledge. Current Simulate membership
   deliberately does not order peers. Schedule construction uses Bevy's default
   executor; the locally inspected locked 0.20.0 source selects serial/parallel by
   platform/features. Thus explicit seal-time profiles matter under feature
   unification; this is an already-planned R4c structural risk, not a new regression.
   Counterproposal: copy Bevy's open schedule-order lists or keep only public sets.
   That is viable if composition validates all required joins and finalization.
   Cost: conformance tests and an intentional compatibility surface. Borrowed solver
   jobs must join before pose publication; a new general task executor is unnecessary.[^execution][^clock][^tasks]

3. **Retain owned task custody; keep borrowed jobs and physical facility policy separate.**
   Ordinary background producers should submit owned inputs, handle typed rejection,
   and apply results at their domain boundary. Their callers should not implement
   disposal flags or infer successful close from cancellation. Counterproposal:
   replace Tasks with Bevy futures or Fyrox callback tasks. That can reduce executor
   maintenance, but still requires Nara's admission, first-terminal, scope and late
   disposal semantics; replacing syntax does not remove these obligations. Godot's
   explicit waits demonstrate a join, not bounded responsive close. Shared facilities
   save duplicated physical owners but provide no progress isolation. Preserve the
   existing lazy-private/shared choices; qualify latency policy with actual consumers.
   Retain typed close composites rather than adding a universal dependency graph.[^tasks][^close]

4. **Keep identity and non-owning hierarchy; converge access through their existing owners.**
   Do not expose incarnation bookkeeping as ordinary gameplay ceremony or resolve
   durable keys repeatedly in hot loops: resolve at admission, then use live ECS
   access within the admitted lifetime. An existing test reactivates the same Entity
   while rejecting its old runtime reference, proving that Entity generation alone
   is insufficient. Fyrox's UUID/handle split and Godot's scene-path/ObjectID split
   reinforce the distinction. Counterproposal: one UUID per object plus built-in
   Bevy hierarchy. That loses activation and lifetime distinctions. A custom unlinked
   Bevy relationship is a serious narrower alternative, but must preserve ordered
   batch failure atomicity and prevent alternate hook-driven writers. Keep the current
   writer until it does. Cost retained: indexes, bounded claims, validation and touched
   sibling copies; do not generalize these into a universal object registry.[^identity][^hierarchy]

5. **Preserve immutable spatial reads; optimize their storage only against measured consumers.**
   Ordinary readers already receive one SystemParam view rather than manually joining
   validity tokens. This is useful ergonomics and prevents live-local fallback from
   mixing phases. Current retained observation rows and hierarchy order already avoid
   rebuilding a second adjacency graph, but any changed source still rebuilds a full
   payload. Counterproposals are Bevy-style component globals, Fyrox message-driven
   subtrees, or Godot lazy caches. They change freshness and mutation assumptions;
   a private dense/indexed snapshot or incremental candidate could instead preserve
   the API. Cost: dirty invalidation, candidate atomicity and memory duplication.
   Defer that storage change until scan/allocation attribution justifies it; do not
   claim any engine is faster from these mechanisms.[^spatial]

## Contracts to settle before they force a larger rewrite

The R4/R10 freeze must specify **which event commits an authoritative tick**. Current
ADR 0057 permits an unrelated later failure after Ack and promises no rollback.
Choose either that narrow consumption acknowledgement or a stronger tick-finalization
boundary encompassing solver/spatial/deferred effects; record command acknowledgement,
tick success and retained effects separately. Do not silently turn Ack into replay
or transaction durability.[^execution]

Also freeze simulation-pose ownership, query/callback freshness, correction/teleport
history reset, and the distinction between tick snapshots and presented values.
Interoperating independently authored motion systems is the trigger, already present
in R4c/R10, rather than future engine size. Before streaming/save/restart crosses
runtime generations, preserve durable-reference versus activation semantics and state
the history-budget/epoch policy. Before code unloading, prove every dependent job,
callback and provider has returned. Existing projection-epoch retirement requires
whole-domain exclusivity; it is not permission to clear arbitrary identity history.[^authority][^identity][^close]

Keep non-TRS import and KeepWorld authoring separate: R10 already requires a complete
edit/animate/persist consumer before choosing an affine authoring representation.
Current local TRS plus global affine composition does not settle that choice.[^authority][^spatial]

## Three executable acceptance consumers

1. **Independent motion fixture:** separately authored animation and solver modules
   compose through public anchors. Run zero/two fixed steps, Pause, exact Step,
   reparent and teleport. Record intent -> borrowed join -> pose commit -> fixed
   read -> Capture/Ack -> presented read. Inject deferred failure and failure after
   Capture; assert the selected acknowledgement/success contract and visible retained
   effects. Repeat for each admitted executor profile and a dependency enabling
   Bevy multi_threaded, including NonSend work. Existing spatial phase tests provide
   the starting oracle; add a thin real solver before interface freeze.[^execution][^tests]
2. **Two embedded runtimes:** share facilities, block A's closure and late-result
   destructor, cancel A, then request Stop. Assert terminal cancellation while return
   remains Pending, B remains independently usable, RetryClose retains A's original
   owner, and facility join occurs only at host close. Include completed-error and
   provider-dependent release cases; compare lazy-private versus shared occupancy
   under matched capacity. Extend the existing scope tests through an independent
   public consumer, rather than inventing another close protocol.[^tests][^close]
3. **Identity/spatial churn fixture:** two Worlds repeatedly unload/reactivate the
   same logical entity, reorder/reparent and mutate locals twice within one change
   tick. Reject foreign/stale tokens; verify historical, fixed and presented reads
   remain coherent and declared budget exhaustion refuses without partial mutation.
   Collect rows scanned, payload rebuilds, bytes and elapsed distributions for static,
   sparse-change and full-motion scenes. Use the current complete candidate as the
   correctness oracle for any locality alternative.[^tests][^spatial][^identity]

**Evidence limits:** source and existing test assertions were inspected; no Cargo,
tests, benchmarks or native execution ran. No new empirical Nara bug is claimed.
Phase/executor/motion gaps are declared missing capabilities; composition-sensitive
ordering and growing snapshot work are structural concerns. Their product impact,
performance ranking and proposed substitution benefits remain unmeasured. Build,
iteration, observability, content, rendering and Editor strategy belong to the other
audit contributions.

## Source anchors

[^authority]: `STRATEGY.md:33`; `docs/architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md:19`; `docs/architecture/adr/0003-own-app-plugin-and-schedule-lifecycle.md:22`; `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md:97`; `docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md:31`; `docs/architecture/adr/implementation-status.md:533`, `:569`, `:1385`, `:1401`; `docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md:393`, `:973`, `:995`.

[^clock]: Nara `crates/nara_app/src/lib.rs:76`, `:861`, `:1977`, `:2038`; Bevy `repo-ref/bevy/crates/bevy_app/src/app.rs:146`, `repo-ref/bevy/crates/bevy_app/src/main_schedule.rs:290`, `:311`, `repo-ref/bevy/crates/bevy_time/src/lib.rs:92`, `repo-ref/bevy/crates/bevy_time/src/fixed.rs:243`; Fyrox `repo-ref/fyrox/fyrox-impl/src/engine/executor.rs:474`, `repo-ref/fyrox/fyrox-impl/src/engine/mod.rs:1639`, `:1691`, `repo-ref/fyrox/fyrox-impl/src/scene/mod.rs:528`, `repo-ref/fyrox/fyrox-impl/src/scene/graph/mod.rs:1436`; Godot `repo-ref/godot/main/main.cpp:4973`, `:5010`, `repo-ref/godot/scene/main/scene_tree.cpp:640`.

[^tasks]: Nara `crates/nara_tasks/src/runtime.rs:1066`, `:1718`, `:3252`, `crates/nara_tasks/src/runtime/scope.rs:83`, `:136`, `:246`; Bevy `repo-ref/bevy/crates/bevy_tasks/src/task_pool.rs:317`, `:566`, `repo-ref/bevy/crates/bevy_tasks/src/usages.rs:14`; Fyrox `repo-ref/fyrox/fyrox-core/src/task.rs:91`, `repo-ref/fyrox/fyrox-impl/src/engine/task.rs:213`, `repo-ref/fyrox/fyrox-impl/src/engine/mod.rs:1816`; Godot `repo-ref/godot/scene/main/scene_tree.cpp:1313`, `repo-ref/godot/core/object/worker_thread_pool.cpp:700`, `:725`.

[^close]: Nara `crates/nara_app/src/runtime.rs:2579`, `:2707`, `crates/nara_app/src/runtime/owner.rs:139`, `:335`, `:387`, `:531`, `crates/nara_app/src/runtime/close.rs:112`, `:170`, `crates/nara_app/tests/close_dependency_characterization.rs:176`; Bevy `repo-ref/bevy/crates/bevy_app/src/app.rs:279`, `:1516`, `:1529`, `repo-ref/bevy/crates/bevy_tasks/src/task_pool.rs:616`; Fyrox `repo-ref/fyrox/fyrox-impl/src/engine/mod.rs:2842`; Godot `repo-ref/godot/main/main.cpp:5291`, `repo-ref/godot/core/object/worker_thread_pool.cpp:857`.

[^identity]: Nara `crates/nara_identity/src/domain.rs:678`, `:1185`, `:2085`, `crates/nara_identity/src/domain/scene_projection_epoch.rs:12`, `crates/nara_identity/tests/reactivation_identity.rs:39`, `docs/architecture/adr/0058-stable-runtime-identity-and-entity-references.md:64`; Bevy `repo-ref/bevy/crates/bevy_ecs/src/entity/mod.rs:365`, `repo-ref/bevy/crates/bevy_scene/src/spawn.rs:182`, `:439`; Fyrox `repo-ref/fyrox/fyrox-core/src/pool/handle.rs:38`, `repo-ref/fyrox/fyrox-impl/src/scene/base.rs:231`, `:456`, `repo-ref/fyrox/fyrox-impl/src/scene/graph/mod.rs:1932`; Godot `repo-ref/godot/core/object/object.cpp:2524`, `repo-ref/godot/scene/resources/packed_scene.cpp:226`.

[^hierarchy]: Nara `crates/nara_hierarchy/src/relation.rs:183`, `crates/nara_hierarchy/src/writer.rs:355`, `:389`, `:660`, `docs/architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md:57`; Bevy `repo-ref/bevy/crates/bevy_ecs/src/hierarchy.rs:105`, `:148`, `repo-ref/bevy/crates/bevy_ecs/src/relationship/mod.rs:270`; Fyrox `repo-ref/fyrox/fyrox-impl/src/scene/graph/mod.rs:2068`; Godot `repo-ref/godot/scene/main/node.cpp:282`, `repo-ref/godot/scene/main/scene_tree.cpp:1646`.

[^spatial]: Nara `crates/nara_transform/src/lib.rs:85`, `crates/nara_transform/src/propagation.rs:121`, `:211`, `crates/nara_transform/src/snapshot.rs:74`, `:166`, `docs/architecture/adr/0097-future-capable-2d-3d-spatial-transform-model.md:49`; Bevy `repo-ref/bevy/crates/bevy_transform/src/systems.rs:42`, `:111`, `:505`; Fyrox `repo-ref/fyrox/fyrox-impl/src/scene/graph/mod.rs:1176`, `:1280`, `:1436`, `repo-ref/fyrox/fyrox-impl/src/scene/node/mod.rs:361`; Godot `repo-ref/godot/scene/main/canvas_item.cpp:201`, `:634`, `:1159`.

[^frame]: `crates/nara_winit/src/lib.rs:982`, `:1707`; `crates/nara_app/src/runtime.rs:2521`, `:2669`; `crates/nara_app/src/runtime/owner.rs:139`; `crates/nara_app/src/lib.rs:1584`, `:1934`, `:1977`, `:2038`; `crates/nara_hierarchy/src/lib.rs:269`; `crates/nara_transform/src/propagation.rs:59`; `crates/nara_transform/src/presentation.rs:88`; `crates/nara_gameplay/src/lib.rs:135`; Bevy outer update `repo-ref/bevy/crates/bevy_app/src/sub_app.rs:575`.

[^execution]: `crates/nara_app/src/lib.rs:148`, `:1622`; `crates/nara_app/src/runtime/owner.rs:239`; `docs/architecture/adr/0057-authoritative-fixed-tick-and-command-ingress.md:148`, `:171`; `docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md:419`. Locked-source check: `C:/Users/Administrator/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_ecs-0.20.0/src/schedule/schedule.rs:421` and `src/schedule/executor/mod.rs:44` under that same crate; no feature-resolved execution was run.

[^tests]: `crates/nara_transform/src/snapshot_tests.rs:77`, `:190`, `:409`, `:820`; `crates/nara_tasks/src/tests/scopes.rs:145`, `:477`, `:586`; `crates/nara_app/tests/runtime_close.rs:253`; `crates/nara_identity/tests/reactivation_identity.rs:39`, `:121`. These are inspected test sources, not fresh passing results.

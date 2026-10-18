# Foundation Contract Audit

Date: 2026-10-18

Baseline: `e9c147ffe2e1d85c4493a44940d86af2c6a1dc17`

Repair commits: `ad97c1f` (glyph ownership), `5ae2c83` (Scene asset context)

Companion: [searchable, standalone HTML report](2026-10-18-foundation-contract-audit.html)

Execution: [audit and hardening plan](../plans/2026-10-18-foundation-contract-audit-and-hardening-plan.md)

## Decision summary

The engine has useful foundations to retain: a thin `bevy_ecs` substrate, a frozen composition
recipe with one managed runtime owner, persistent documents separated from native effects, and
semantic Editor operations. The next investment should close owner, identity, value, and
transaction seams before new domains make their assumptions expensive to change.

This is a foundation audit, not a backlog of missing Tilemap, save, animation, physics, or audio
features. It does not promise an architecture without future debt. Proposed directions below are
not new architecture decisions: changes that conflict with Accepted ADRs require an ADR revision
before implementation. In particular, permanent identity claims are intentional today, strict
lifecycle-free transactions remain valid until their consumers migrate, and native failure must
not be disguised as a promise of arbitrary rollback.

## Reading the findings

- **P1:** repair a confirmed defect or settle a contract before expanding its consumers. This is
  sequencing priority, not a claim that every entry is a reproduced production failure.
- **P2:** close a bounded extension seam when its named consumer is introduced, or measure a scale
  assumption before optimizing it.
- **Confirmed defect:** the described existing behavior is incorrect and has a failing regression.
- **Structural contract risk:** source analysis establishes a coupling or mismatch; a proposed
  consumer or static counterexample is not represented as a passing or failing executed test.
- **Scale assumption:** correctness is preserved; cost follows total world size rather than work
  changed. No frame-budget failure is claimed without measurement.

Statuses are **Fixed**, **Proposed**, and **Budget gate**. The validation register at the end
separates execution evidence from suggested acceptance work.

## Dependency paths

These are independent work lanes, not one mandatory linear rewrite.

| Lane | Order and reason |
| --- | --- |
| Immediate ownership defects | **F01 + F02**: close existing asset-context and glyph-owner defects; these can proceed independently. |
| Persistent values and editing | Correct **F03** final-candidate transaction boundaries first. In parallel, design **F04 + F05** together, then integrate their effective-value/reference closure into final validation. Keep per-operation structural and budget checks. |
| Reusable worlds and native domains | Revise **F06** durable identity versus runtime incarnation; then prove **F07** with real physics and audio owners before extracting shared coordination. |
| Execution and spatial domains | Establish **F09** thread ownership; close **F08** sub-schedule execution when needed; use the real consumer to settle **F10** current-pose reads. |
| Domain-facing presentation | **F15** before multi-endpoint input; **F13** before multipass runtime/Editor rendering; **F14** before optional device capabilities; **F16** before custom-font presentation. |
| Scale | Authority and invalidation ownership precede **F11** optimization. Trigger **F12** from a measured static-map/active-actor budget. |

## Findings

### F01 — Advanced scene replacement loses admitted asset context

**P1 · Confirmed defect · Fixed · High confidence**

- **Evidence:** baseline [`nara_scene/src/spawn.rs:1034`](../../crates/nara_scene/src/spawn.rs#L1034)
  prepares product replacement with `database = None` at line 1052, unlike initial startup. The
  added regression is `product_replacement_preserves_stable_asset_resolution`.
- **Counterexample:** initial spawn resolves a stable asset UUID with the project database; a
  supported advanced in-game replacement/Retry using the same content then fails resolution.
  This is not an assertion that Editor Restart fails.
- **Direction:** require an explicit borrowed database/context at replacement. Borrow the complete
  admitted source through `StartupSceneSourceView`, preserving its existing retention owner.
- **Migration:** `nara_scene` product replacement, startup source/view plumbing, reference-game
  Retry, and direct callers/tests. Do not add a second root-content owner or clone its authority.
- **Prerequisite:** none; keep the accepted source-retention and atomic document contracts.
- **Acceptance:** regression fails on the baseline and passes after the change; path and UUID
  references agree across initial spawn and replacement; rejected replacement preserves the old
  product. Focused scene tests and affected reference-game consumers must pass.

### F02 — Private glyph atlases collide in a shared GPU cache

**P1 · Confirmed defect · Fixed · High confidence**

- **Evidence:** [`nara_render_wgpu/src/texture.rs:266`](../../crates/nara_render_wgpu/src/texture.rs#L266)
  derives a shared cache identity from values whose atlas ownership is private to presentation
  targets/planners. Distinct atlases can both begin at font revision 1, page 0, generation 1.
- **Counterexample:** two targets, or separate planners for one target, produce different pixels
  with equal cache keys. The second packet can reuse the first atlas's GPU texture.
- **Direction:** include a checked private atlas incarnation and page revision in identity. Clones
  of the same snapshot preserve identity; font/reset creates a new owner incarnation.
- **Migration:** `nara_ui_render` atlas and packet ownership, GPU texture cache keys, tests of
  target switching, planner reuse, retirement, and device recreation.
- **Prerequisite:** none; independent of F01 and the custom-font design in F16.
- **Acceptance:** cross-target and same-target/cross-planner collisions fail before the fix and
  pass afterward; unchanged snapshots reuse uploads; resets retire the previous incarnation.
  Record GPU/native checks at their actual scope; these checks do not establish complete native
  Editor qualification.

### F03 — Patch transactions validate business invariants between operations

**P1 · Structural contract risk · Proposed · High confidence; static counterexample**

- **Evidence:** [`nara_scene/src/patch.rs:260`](../../crates/nara_scene/src/patch.rs#L260),
  [`:377`](../../crates/nara_scene/src/patch.rs#L377), and
  [`:1197`](../../crates/nara_scene/src/patch.rs#L1197) validate intermediate candidates;
  [`validation.rs:287`](../../crates/nara_scene/src/validation.rs#L287) reaches native component
  invariants. No new runtime regression for this entry has been executed.
- **Counterexample:** an invariant requires `x == y`. A patch changing `(1, 1)` to `(2, 2)` through
  two field operations is rejected in either operation order despite a valid final document.
- **Direction:** validate path access, permission, value shape, and work budgets per operation;
  validate component/business/reference closure on the final candidate before publication.
- **Migration:** scene/prefab patch preparation, registry validation entry points, inverse/undo
  tests, and Editor patch consumers. Do not relax final validation or publish partial changes.
- **Prerequisite:** align final effective-value/reference closure with F04/F05. The transaction
  boundary itself can be corrected without waiting for a generic schema framework.
- **Acceptance:** coupled-field patches succeed in either order; an invalid final state fails
  atomically; bad paths and excessive work remain rejected; inverse application restores the
  original document. Test both scene and prefab paths.

### F04 — Flat field paths cannot describe references inside variable containers

**P1 · Structural contract risk · Proposed · High confidence**

- **Evidence:** [`nara_reflect/src/schema.rs:472`](../../crates/nara_reflect/src/schema.rs#L472)
  and [`registry.rs:2462`](../../crates/nara_reflect/src/registry.rs#L2462) expose flat fields with
  opaque Map/List kinds. [`entity_reference.rs:191`](../../crates/nara_reflect/src/entity_reference.rs#L191)
  constructs absolute declared reference paths and checks the complete value tree against them.
- **Counterexample:** deriving a component with a variable `Vec<EntityRef>` cannot declare every
  runtime element through fixed absolute schema paths. Adding derive syntax alone cannot close
  serialization, reference remapping, migration, and Inspector access.
- **Direction:** design a bounded recursive value shape with matching field locators and reference
  visitors. Migrate lineage/fingerprints and traversal together. Avoid an arbitrary validation DSL
  or stable identity for every list element without a real requirement.
- **Migration:** reflect schema/registry/derive, entity and asset visitors, schema export and
  compatibility, scene/prefab validation and rewrite, Editor Inspector path operations.
- **Prerequisite:** co-design with F05; settle the schema contract before extending collection
  authoring broadly. F03 must validate the resulting final effective value.
- **Acceptance:** nested lists/maps of references round-trip, remap, validate, and export with one
  schema; traversal limits reject excessive depth/work; fingerprints and migrations distinguish
  incompatible shape changes; existing scalar schemas migrate explicitly.

### F05 — Reference validity differs across spawn, export, prefab, and defaults

**P1 · Structural contract risk · Proposed · High confidence**

- **Evidence:** baseline [`nara_scene/src/tests.rs:3718`](../../crates/nara_scene/src/tests.rs#L3718)
  admits a missing scene-local reference that later fails export; prefab coverage at
  [`nara_scene/src/tests.rs:1263`](../../crates/nara_scene/src/tests.rs#L1263) rejects it during
  expansion. Default handling in [`nara_reflect/src/registry.rs:2435`](../../crates/nara_reflect/src/registry.rs#L2435)
  is not the same effective-value traversal used by
  [`asset_reference.rs:167`](../../crates/nara_reflect/src/asset_reference.rs#L167) and
  [`entity_reference.rs:223`](../../crates/nara_reflect/src/entity_reference.rs#L223).
- **Counterexample:** equivalent authored data is accepted through one entry point and rejected
  through another; a non-null default reference can be omitted from reference visitation.
- **Direction:** define required presence separately from target existence; distinguish local,
  durable, unloaded, and deleted targets. Normalize one effective value, including defaults, and
  use one visitor/closure contract at all admission and export boundaries.
- **Migration:** reflect default normalization and reference visitors, scene spawn/export, prefab
  expansion, patch validation, and Editor diagnostics. Do not solve this with entry-point exceptions.
- **Prerequisite:** F04 shares the value model; F06 supplies durable/runtime resolution semantics.
  Close existing scalar inconsistencies before waiting for every recursive-schema feature.
- **Acceptance:** a matrix of explicit/default, required/optional, present/missing/local/external
  references produces consistent outcomes through spawn, export, prefab, and patches. Unloaded
  durable references remain distinct from invalid local references.

### F06 — Permanent identity claims couple durable identity to one runtime incarnation

**P1 · Structural contract risk · Proposed · High confidence; intentional current contract**

- **Evidence:** [Accepted ADR 0058](../architecture/adr/0058-stable-runtime-identity-and-entity-references.md)
  requires permanent claims. [`nara_identity/src/domain.rs:1011`](../../crates/nara_identity/src/domain.rs#L1011)
  rejects previously claimed references; retirement around
  [`:1611`](../../crates/nara_identity/src/domain.rs#L1611) does not reclaim them. Claim capacity
  measures world history, with a default near one million claims, rather than active residency.
- **Counterexample:** an NPC with a durable UUID is unloaded and later restored in the same world;
  rebinding the same semantic identity is rejected. Allocating fresh durable UUIDs instead breaks
  the identity expected by saved references.
- **Direction:** revise the ADR first. Separate durable ID, active binding/runtime incarnation,
  and stale-token rejection. Do not merely allow old tokens to resolve to a newly spawned entity.
- **Migration:** identity domain and references, scene instance replacement/retirement, gameplay
  command targets, serialization/export, diagnostics, and later streaming/save consumers.
- **Prerequisite:** product semantics for unloaded versus deleted references from F05; perform the
  identity decision before broad F07 native lifecycle or streaming/save implementation.
- **Acceptance:** unload/reload preserves durable references while invalidating old runtime tokens;
  cross-world tokens still fail; residency cycles do not exhaust history-sized claims; deletion,
  duplicate active bindings, and exhausted incarnation counters fail explicitly.

### F07 — Native reconciliation is proved by a fixture, not by independent runtime owners

**P2 · Structural contract risk · Proposed · High confidence**

- **Evidence:** [`module-consumer/.../domain_reconciliation.rs:377`](../../tests/fixtures/module-consumer/tests/support/domain_reconciliation.rs#L377)
  manually retires `NativeState`/required/hook-owned state and synchronizes it. Strict scene
  retirement still rejects lifecycle hooks at
  [`nara_scene/src/spawn.rs:1324`](../../crates/nara_scene/src/spawn.rs#L1324) and
  [`nara_ecs/src/__private.rs:155`](../../crates/nara_ecs/src/__private.rs#L155).
- **Counterexample:** independently developed physics and audio owners must coordinate unload,
  pending native work, stop, and retry while the scene owner retains strict retirement rules.
  The fixture coordinates these steps by hand; it does not prove the public multi-owner workflow.
- **Direction:** build a real physics-plus-audio lifecycle slice with explicit stop, cleanup,
  reconcile, and degraded/unavailable states; then extract only the shared coordination it needs.
  Keep document publication separate from fallible native effects.
- **Migration:** scene/identity retirement consumers, native domain owners, runtime close and
  reconciliation fixtures, Editor stop/restart diagnostics. Existing strict guards remain until
  each affected consumer migrates.
- **Prerequisite:** F06 identity/incarnation semantics and relevant thread ownership from F09.
- **Acceptance:** unloading/replacing/stopping with both domains releases each native owner once;
  pending work retains its owner; partial native failure is visible and retryable; stale entities
  cannot be reactivated accidentally. This is not a universal native rollback facility.

### F08 — Custom schedules have configuration but no managed execution path

**P2 · Structural contract risk · Proposed · High confidence; intentional current behavior**

- **Evidence:** [`nara_app/src/lib.rs:1244`](../../crates/nara_app/src/lib.rs#L1244) and
  [`:1281`](../../crates/nara_app/src/lib.rs#L1281) register arbitrary schedule labels, but the
  registry lives outside World at [`runtime.rs:2812`](../../crates/nara_app/src/runtime.rs#L2812).
  The private driver at [`lib.rs:1832`](../../crates/nara_app/src/lib.rs#L1832) runs the fixed stage
  list. [`tests/schedule_registry.rs:25`](../../crates/nara_app/tests/schedule_registry.rs#L25)
  intentionally asserts that custom schedules stay inert.
- **Counterexample:** a plugin declares an extensible physics substep schedule and wants N managed
  executions per fixed tick. A system cannot use World scheduling to execute this same registry;
  it needs a second registry, a privately owned Schedule, or an App change. Ordinary FixedUpdate
  SystemSet-based physics remains possible.
- **Direction:** when the consumer arrives, add a configuration-time declared sub-schedule driver
  that preserves managed fault/time/safe-point ownership. Do not expose unrestricted host execution.
- **Migration:** App schedule registry/driver and tests; the domain's substep composition. Preserve
  existing CoreStage and GameplayCommandSet consumers.
- **Prerequisite:** F09 owner thread; a real repeated or independent-clock consumer. Do not build a
  generic schedule graph only to make the existing configuration surface appear complete.
- **Acceptance:** external plugins can add ordered systems to the same sub-schedule; exact step,
  pause, errors, deferred commands, and configuration freeze have explicit, tested behavior.

### F09 — Runtime thread ownership is weaker than its NonSend and retirement assumptions

**P2 · Structural contract risk · Proposed · Send trait confirmed; runtime failure inferred**

- **Evidence:** [`nara_app/src/runtime.rs:2222`](../../crates/nara_app/src/runtime.rs#L2222) and
  [`:2812`](../../crates/nara_app/src/runtime.rs#L2812) have no visible owner-thread marker, while
  raw World configuration can install NonSend data. Retirement uses thread-local quarantine at
  [`:3289`](../../crates/nara_app/src/runtime.rs#L3289) and requires owner-thread draining at
  [`:1243`](../../crates/nara_app/src/runtime.rs#L1243). Close context's mutable access is limited
  to Resource types at [`:925`](../../crates/nara_app/src/runtime.rs#L925).
- **Counterexample:** a platform audio controller is inserted as NonSend, then an admitted runtime
  is moved to another thread for driving or drop. A metadata-only `rustc` probe confirms
  `RuntimeInstance: Send`. Access panic and wrong-thread resource forgetting are inferred from the
  substrate; this review did not execute a cross-thread NonSend SDK scenario.
- **Direction:** keep data-only recipes transportable. Give App/sealed/admitted states capable of
  retaining NonSend values an explicit thread-ownership contract, including before admission, and
  preserve it through retirement. Add bounded NonSend close access if the concrete SDK needs
  polling; do not create a universal cross-thread executor.
- **Migration:** App/runtime admission and retirement owner types, project_host holders, native
  platform shutdown integration, and compile-time ownership tests.
- **Prerequisite:** settle intended mobility using the confirmed auto-trait; before thread-affine domains
  and F08 substep execution. A report alone does not change the supported threading contract.
- **Acceptance:** compile-time tests establish which configuration and execution states are Send;
  moving a configured NonSend value before admission is covered too. Wrong-thread movement is
  prevented or rejected before effects; close/quarantine polling and destruction preserve the
  creating thread; normal independent runtime construction on other threads remains supported.

### F10 — Completed transforms do not cover same-tick read-then-write spatial stages

**P2 · Structural contract risk · Proposed · Medium-high confidence**

- **Evidence:** [`nara_hierarchy/src/lib.rs:181`](../../crates/nara_hierarchy/src/lib.rs#L181)
  completes after Simulate, followed by transform propagation at
  [`nara_transform/src/propagation.rs:126`](../../crates/nara_transform/src/propagation.rs#L126).
  The valid public helper [`lib.rs:75`](../../crates/nara_transform/src/lib.rs#L75) schedules
  consumers after propagation; it is not a missing public read anchor.
- **Counterexample:** animation updates a parent, physics needs the current world pose, then
  writes local transforms in the same tick. Reading in Simulate observes the prior completion;
  ordering a Simulate system after final propagation creates a cycle; writing afterward requires
  another completion. The domain otherwise duplicates ancestor-matrix computation.
- **Direction:** use a real consumer to add an owner-provided on-demand current-pose helper,
  distinguishing it from cached completed globals. Preserve the final global projection and
  immutable derived components; avoid arbitrary global completion barriers.
- **Migration:** transform read helpers, bounded hierarchy traversal, physics/animation integration
  and tests. Existing post-propagation consumers and render extraction can remain unchanged.
- **Prerequisite:** F08 if the consumer uses substeps; defined mutation/current-pose semantics.
- **Acceptance:** animation-to-physics reads the current ancestor chain without reading partial
  cached projection; physics writeback is visible at final completion; malformed hierarchy,
  stale entities, and non-finite transforms fail consistently.

### F11 — Each ordinary hierarchy operation validates and copies the whole topology

**P2 · Structural contract risk · Proposed · High confidence; cost established, budget unmeasured**

- **Evidence:** [`nara_hierarchy/src/writer.rs:310`](../../crates/nara_hierarchy/src/writer.rs#L310)
  turns each single operation into a batch; [`:351`](../../crates/nara_hierarchy/src/writer.rs#L351)
  validates and copies all roots/children/parents; [`:728`](../../crates/nara_hierarchy/src/writer.rs#L728)
  repeats this baseline for every batch. Deferred single-operation Commands use this same path at
  [`:332`](../../crates/nara_hierarchy/src/writer.rs#L332).
- **Counterexample:** independent audio, attachment, and streaming systems each issue normal
  single-operation commands. K operations can cost O(KH) over a hierarchy of H members despite
  touching small local branches. `apply_batch` mitigates one owner's batch, not all plugin writes.
- **Direction:** retain failure-atomic ordered batches, but use an owned/validated generation plus
  a candidate overlay for affected edges and ancestor chains. Keep full validation for raw dirty
  boundaries and diagnostics rather than every ordinary write.
- **Migration:** hierarchy writer/validation/completion, scene hierarchy transactions, Editor
  hierarchy changes, and invalidation tests. Keep Parent/Children ownership and serialized shape.
- **Prerequisite:** trustworthy mutation authority and invalidation; do not remove validation first.
- **Acceptance:** large-world small edits and independent command producers have a measured bounded
  cost; invalid operations remain atomic; raw mutation invalidates completion; ordered sibling
  behavior and scene/editor transactions remain correct.

### F12 — Transform work scales with all loaded transforms

**P2 · Scale assumption · Budget gate · High confidence; no measured budget failure**

- **Evidence:** [`nara_transform/src/propagation.rs:183`](../../crates/nara_transform/src/propagation.rs#L183)
  scans all transform participants at each completion; any change rebuilds the whole candidate
  forest at [`:228`](../../crates/nara_transform/src/propagation.rs#L228). Interpolation scans all
  samples at [`presentation.rs:107`](../../crates/nara_transform/src/presentation.rs#L107) and
  rebuilds render matrices at [`:130`](../../crates/nara_transform/src/presentation.rs#L130).
  [`lib.rs:931`](../../crates/nara_transform/src/lib.rs#L931) explicitly tests unchanged full scans.
- **Counterexample:** a large static map with a few moving actors still pays for all loaded
  transforms; optional interpolation adds full scans and a full render projection.
- **Direction:** retain the simple correct baseline. Measure a static-map/active-actor consumer;
  introduce dirty-subtree propagation and interpolation membership only when its budget requires it.
- **Migration:** transform internals and performance evidence; render consumers continue reading
  owner-provided projections. Keep the full algorithm as a correctness oracle where useful.
- **Prerequisite:** stable authority/invalidation; a representative scene and explicit CPU/memory
  budget. This does not block the preceding ownership fixes.
- **Acceptance:** record loaded/active transforms, hierarchy depth, fixed steps, interpolation,
  allocations, and completion time; compare incremental output against baseline after reparent,
  remove, pause, and exact step. Do not claim an optimization from traversal counts alone.

### F13 — Runtime/Editor rendering needs a shared multipass owner before a public graph

**P2 · Structural contract risk · Proposed · High confidence; reasonable first-version scope**

- **Evidence:** [`nara_render/src/frame.rs:48`](../../crates/nara_render/src/frame.rs#L48)
  describes a packet for one exclusive target with multiple ordered views and closed sprite/UI
  payload. That scope is reasonable and multi-view is supported. The gap is a shared normal-runtime
  execution owner for dependencies across independent targets/stages, also consumed by Editor.
- **Counterexample:** pixel canvas to offscreen color, then lighting/post-processing, then present,
  with the same behavior inside Editor game view. Adding passes independently to each execution
  path duplicates ordering and target lifetime decisions.
- **Direction:** prove a private shared stage owner through both real consumers; expose a public
  graph or provider interface only after its necessary extension points are known.
- **Migration:** render frame planning/target lifetimes, wgpu execution, normal runtime, Editor game
  view, and readback/presentation verification. Do not replace the renderer with a generic graph now.
- **Prerequisite:** a concrete multipass consumer; F14 when the passes require optional capabilities.
- **Acceptance:** identical stage order and target ownership in runtime/Editor; resize, skip,
  readback, target removal, and device recreation retain correct dependencies and diagnostics.

### F14 — Device requirements are not declared before device creation

**P2 · Structural contract risk · Proposed · High confidence**

- **Evidence:** [`nara_render_wgpu/src/backend.rs:1152`](../../crates/nara_render_wgpu/src/backend.rs#L1152)
  lazily requests a surface-compatible adapter, then uses `DeviceDescriptor::default()` at line
  1161. There is no composition-level request for optional features/limits before creation.
- **Counterexample:** a compute/storage-backed domain needs a capability unavailable in the
  already-created default device. Enabling the plugin later cannot retroactively request it.
- **Direction:** add a frozen composition `DeviceProfile` with required/optional capabilities,
  limits, and explicit fallback/rejection. This is not an automatic device-recovery bug.
- **Migration:** backend/device construction, render plugin recipe, runtime/Editor device owners,
  capability diagnostics, and device recreation.
- **Prerequisite:** a real capability consumer; resolve requirements before creating its device.
- **Acceptance:** unsupported required capabilities reject before resource creation; optional
  fallback is explicit; runtime/Editor and recreated devices apply the same profile.

### F15 — Input endpoints are lost when raw input becomes gameplay state

**P2 · Structural contract risk · Proposed · High confidence**

- **Evidence:** `nara_input` preserves window/seat/pointer identity in InputTimeline, but its
  ButtonInput, PointerState, and [`ActionOutcome` at lib.rs:874](../../crates/nara_input/src/lib.rs#L874)
  projection does not retain the complete endpoint. The ownership seam is
  [`project_timeline_input:987`](../../crates/nara_input/src/lib.rs#L987) and
  [`entry.action_transition:1064`](../../crates/nara_input/src/lib.rs#L1064).
- **Counterexample:** two seats press the same key; releasing one seat can conflict with the
  other seat's held action after projection has merged their identity. Routing cannot recover an
  endpoint that the state layer has discarded.
- **Direction:** retain endpoint identity through state and action outcomes; define held-action
  aggregation explicitly before adding gamepad/IME consumers. Avoid a new general input bus.
- **Migration:** nara_input timeline/state/actions/action transitions, gameplay action mapping,
  Editor game-view routing, replay/capture, and device-aware consumers.
- **Prerequisite:** endpoint and aggregation semantics; before multi-seat/device expansion.
- **Acceptance:** overlapping same-key presses/releases from different endpoints remain correct;
  focus loss and device/window removal clear only their own state; gameplay and Editor observe
  the same endpoint-aware action transition.

### F16 — Custom font ownership does not close shaping through rasterization

**P2 · Structural contract risk · Proposed · High confidence**

- **Evidence:** [`nara_ui_kernel/src/compiler.rs:36`](../../crates/nara_ui_kernel/src/compiler.rs#L36)
  accepts a FontFace, while [`nara_ui_render/src/planner/mod.rs:41`](../../crates/nara_ui_render/src/planner/mod.rs#L41)
  constructs the bundled face. [`nara_text/src/raster.rs:57`](../../crates/nara_text/src/raster.rs#L57)
  rejects a glyph run whose face differs from the raster face.
- **Counterexample:** a consumer successfully shapes UI with a custom face, then the default
  presentation planner cannot rasterize its glyphs.
- **Direction:** share an immutable FontSet owner across shaping, presentation, rasterization,
  atlas identity, and retirement. Increasing glyph budgets cannot repair face identity.
- **Migration:** text/FontFace ownership, UI compiler and planner construction, atlas/cache keys,
  and runtime/Editor presentation composition. Preserve convenient bundled-font defaults.
- **Prerequisite:** F02 cache ownership fix; a custom-font consumer defines the necessary FontSet
  surface. Avoid a speculative dynamic font-provider framework.
- **Acceptance:** a custom face shapes and presents through the same owner; incompatible runs fail
  before uploads; mixed faces, clone/reset, target reuse, and font retirement preserve cache identity.

## Boundaries to retain

1. **Thin ECS access, domain-owned writes.** `nara_ecs` intentionally re-exports mature storage,
   queries, commands, schedules, and change detection. Do not build a parallel ECS facade. Restrict
   writes where persistence or native ownership requires it, rather than wrapping every query.
2. **Frozen recipe and one managed execution owner.** Plugin composition need not become mutable
   at runtime to support substeps, pause, or Editor restart. Preserve typed SystemSet composition
   and make new execution paths participate in the same owner.
3. **Persistent document acceptance separated from native reconciliation.** Keep atomic document
   changes and explicit degraded/unavailable native states. Strict lifecycle-free guards are
   useful proofs until affected consumers migrate; they are not a universal domain lifecycle API.
4. **Semantic Editor operations and explicit channel lifetimes.** Keep Editor operations aligned
   with owned product contracts. [ADR 0036](../architecture/adr/0036-event-message-and-resource-queue-lifetime.md)
   correctly distinguishes frame/fixed observations from retained requests. Frame tracker cleanup
   is not a durable native-cleanup queue; physics/audio must own retention across pause/zero ticks.

## Reference-engine checks

These are comparisons of concrete ownership mechanisms, not instructions to copy entire engines.

| Question | Local reference evidence | Implication |
| --- | --- | --- |
| Recursive reflected values and reference remapping | Bevy [`type_info.rs:34`](../../repo-ref/bevy/crates/bevy_reflect/src/info/type_info.rs#L34), [`list.rs:138`](../../repo-ref/bevy/crates/bevy_reflect/src/list.rs#L138), [`reflect/component.rs:303`](../../repo-ref/bevy/crates/bevy_ecs/src/reflect/component.rs#L303); Godot [`editor_properties_array_dict.cpp:902`](../../repo-ref/godot/editor/inspector/editor_properties_array_dict.cpp#L902) | Carry container shape and reference traversal across serialization and editing; derive syntax alone is not the contract. |
| Runtime incarnation versus durable resource identity | Bevy [`entity/mod.rs:646`](../../repo-ref/bevy/crates/bevy_ecs/src/entity/mod.rs#L646); Godot [`resource_uid.h:74`](../../repo-ref/godot/core/io/resource_uid.h#L74), [`object_id.h:41`](../../repo-ref/godot/core/object/object_id.h#L41) | Keep durable identity and runtime validity separate. Resource UID and ObjectID illustrate different lifetimes, not a ready-made Nara entity-persistence implementation. |
| Schedule configuration reaches execution | Bevy [`main_schedule.rs:240`](../../repo-ref/bevy/crates/bevy_app/src/main_schedule.rs#L240), [`:290`](../../repo-ref/bevy/crates/bevy_app/src/main_schedule.rs#L290), [`:393`](../../repo-ref/bevy/crates/bevy_app/src/main_schedule.rs#L393) | Close a real sub-schedule execution seam while retaining Nara's managed owner. |
| Current pose before cached propagation | Bevy [`helper.rs:14`](../../repo-ref/bevy/crates/bevy_transform/src/helper.rs#L14), [`:28`](../../repo-ref/bevy/crates/bevy_transform/src/helper.rs#L28) | An on-demand read helper can be smaller than more global completion barriers. |
| Thread-affine resource safety | Bevy [`non_send.rs:34`](../../repo-ref/bevy/crates/bevy_ecs/src/storage/non_send.rs#L34), [`:74`](../../repo-ref/bevy/crates/bevy_ecs/src/storage/non_send.rs#L74); App [`app.rs:1514`](../../repo-ref/bevy/crates/bevy_app/src/app.rs#L1514); Godot [`node.cpp:1712`](../../repo-ref/godot/scene/main/node.cpp#L1712) | World being Send does not establish that admitted runtime/native ownership can move. |
| Local hierarchy mutation cost | Godot [`node.cpp:1664`](../../repo-ref/godot/scene/main/node.cpp#L1664), [`:1711`](../../repo-ref/godot/scene/main/node.cpp#L1711) | Avoid unconditional whole-world copies without discarding Nara's stronger batch guarantees. |
| Static transform scaling | Bevy [`systems.rs:111`](../../repo-ref/bevy/crates/bevy_transform/src/systems.rs#L111), [`:135`](../../repo-ref/bevy/crates/bevy_transform/src/systems.rs#L135) | Dirty subtree optimization exists, but should follow a measured Nara consumer. |

Reference revisions: Bevy `85503941e488152711a4695bbe248e9ed9f453fc`; Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. `repo-ref/` remains read-only background evidence.

## Watchlist, not immediate redesign

- Image-specific project-content/reload routing needs a second real asset domain before generic
  loader expansion. Live UUID relocation must preserve identity and last-good content; reopening a
  fresh database does not prove live relocation works.
- Default task-pool composition needs a process/host budget when multiple consumers are combined;
  do not infer overload from thread-count formulas alone.
- Linux filesystem directory behavior is a platform-consumer gap, not a reason to replace the
  entire filesystem abstraction.
- Tilemap, save format, scene streaming, gamepad, IME, and advanced rendering are consumers that
  expose these foundations. Their absence is not itself a defect in the audited foundation.

## Validation register

The read-only review lanes did not run Cargo. Only the coordinating implementation lane may add
executed results below. Proposed acceptance bullets above are not execution evidence.

| Check | Actual result | Evidence / remaining work |
| --- | --- | --- |
| F01 regression before repair | Failed, then repaired-owner and consumer suites passed | Red run `cc35e95f-47ee-4663-b933-4f0b15d5255d`; green run `f6bfd071-3de4-4453-9d91-34aeaa9b2e3a`. Root startup and real project Retry results are recorded below. |
| F02 regression before repair | Failed, then focused repaired suite passed | Red run `e9caedac-3687-400d-8d49-56b32162015f`; same green run as F01. GPU scope recorded separately below. |
| F03 coupled-field counterexample | Static analysis only | Add and execute scene/prefab transaction regression before claiming a reproduced test. |
| F09 RuntimeInstance Send trait probe | Confirmed | `rustc 1.99.0 --edition=2024 --emit=metadata` against the current `target/deps` nara_app rlib compiled `assert_send::<RuntimeInstance>()`, exit 0. Cross-thread NonSend failure was not executed. |
| Focused repaired-owner suites | 144 passed | Green run above: nara_scene 114 + nara_ui_render 30 library tests; Windows, Rust 1.99.0, 7.722 s. This does not qualify GPU or all public consumers. |
| Root startup source consumers | 4 passed | Run `8ab2683b-0009-4fef-96d6-308b6cbb1afd`; focused startup_scene checks only, not native Editor or full product qualification. |
| WGPU ui-submitter library suite | 95 passed | Run `1c2384ef-5d72-43cf-aa50-47ca80449891`; Windows, `WGPU_BACKEND=dx12`, Rust 1.99.0, 12.664 s. Includes shared-cache GPU upload/replay and nonblank glyph readback; not complete native Editor qualification. |
| Reference-game first_wave consumer suite | 21 passed | Run `69c2a08b-6b85-4386-8de7-fb22361fba7a`; Windows, Rust 1.99.0, 2.593 s. A guarded project copy uses a stable image UUID, retires the initial player, and successfully applies Retry with the same image handle after the loader snapshot has been released. |
| Scoped code review and formatting | Passed | Independent Scene ownership/API and glyph/cache/Retry reviews found no blocking P1/P2 findings. Scoped reuse, quality, efficiency, test-value and comment checks required no further abstraction. `cargo fmt --all`, the excluded reference-game formatter, and `git diff --check` passed. |
| Report paths and structure | Passed | Inline Python: all referenced local files exist; 119 HTML links checked after integration; 16 finding IDs match Markdown; no duplicate IDs or invalid in-page links; no external assets; default visibility and control labels checked. |
| Report interaction logic | Passed, DOM harness only | Inline Node: severity/class/status conjunction, case-insensitive multiword search, empty/reset, matching navigation, and Escape/search shortcut handlers. Print-all CSS and no-JS fallback are present. |
| Report visual browser / printed pages | Not executed | No browser is available in the current environment. Layout, native keyboard interaction and actual printed pages are not visually qualified. |

All four passing suites ran serially with `CARGO_BUILD_JOBS=1` and the existing root `target`:

```powershell
cargo nextest run --locked -p nara_scene -p nara_ui_render --lib --test-threads=1 --no-fail-fast
cargo nextest run --locked -p nara --lib --no-default-features --features runtime-2d,serde,project-tooling --test-threads=1 -E 'test(startup_scene::tests)'
# WGPU_BACKEND=dx12 for the GPU suite.
cargo nextest run --locked -p nara_render_wgpu --features ui-submitter --lib --test-threads=1 --no-fail-fast
# CARGO_TARGET_DIR points to the existing root target for this excluded workspace.
cargo nextest run --locked --manifest-path reference-game/Cargo.toml --test first_wave --test-threads=1 --no-fail-fast
```

These 264 tests establish the selected repair scope. No MSRV, full workspace, native Editor
qualification, cross-platform release, or large-world performance claim is added. The initial
new consumer test had an optional-image fixture access compile error, corrected before its
successful execution; no production failure is inferred from that test-authoring error.

Accepted sources governing changes include [ADR 0058](../architecture/adr/0058-stable-runtime-identity-and-entity-references.md),
[ADR 0084](../architecture/adr/0084-executable-runtime-ownership-and-isolation.md),
[ADR 0117](../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md), and the
[architecture index](../architecture/README.md). The report records findings and proposed work;
implementation status and release authority remain in their owning sources.

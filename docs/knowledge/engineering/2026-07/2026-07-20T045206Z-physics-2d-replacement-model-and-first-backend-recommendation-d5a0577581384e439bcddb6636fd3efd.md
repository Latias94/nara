---
type: "Engineering Research"
title: "Physics 2D replacement model and first-backend recommendation"
description: "Non-normative design research for a user-replaceable 2D physics domain, backend selection, fixed-tick authority, queries, events, and ecosystem pressure tests."
timestamp: 2026-07-20T04:52:06Z
record_id: "d5a0577581384e439bcddb6636fd3efd"
resource: "docs/architecture/open-questions.md"
tags: ["architecture", "physics", "plugins", "ecosystem", "bevy", "godot", "unity"]
status: "research"
producer_id: "codex-root"
run_id: "20260720-physics-replacement-model"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bca2eeb86f086989bbe1df2fe09a836c414f43ee"
---

# Summary

Nara should make 2D physics replaceable through a domain protocol, not through a speculative
`dyn PhysicsBackend2d` trait and not by accepting backend-native components as the durable game
model.

The recommended Trial direction is:

- `nara_physics2d` owns portable authoring intent, fixed-tick participation, bounded contact
  outcomes, capability diagnostics, and the eventual common query interface.
- A Rapier adapter built directly on `rapier2d` is the first Trial backend. Nara should not depend
  on `bevy_rapier2d`, because that integration owns Bevy App, schedule, transform, message, and
  plugin behavior that Nara deliberately owns itself.
- Box2D 3 is the second real Adapter pressure test before any backend compatibility promise is
  frozen. An audited engine-independent binding such as `boxdd`, or a narrow binding over the
  official C API, is a candidate implementation route; the wrapper itself must be verified
  independently from Box2D's production history.
- Avian is valuable architecture evidence but is not currently a clean Nara Adapter dependency:
  its public crates target the full Bevy product stack and Bevy schedules, transforms, scenes,
  picking, and diagnostics.
- Jolt is a strong future 3D candidate, not a 2D candidate. Its shipped-game evidence and Godot
  integration make it useful when Nara starts a real 3D vertical slice.

This record is non-normative. ADR 0019 already owns the accepted high-level direction, while
OQ-005 deliberately leaves transform authority, query freshness, contact ordering, and the first
backend open. No Accepted ADR should change until a named physics vertical slice and a reviewable
Adapter spike supply that evidence.

# Repository Evidence

- ADR 0019 is accepted but `not-started` in the implementation ledger. No physics domain crate,
  portable body/collider intent, query/event contract, or backend Adapter exists.
- ADR 0016 and ADR 0042 require stable ECS intent, Adapter-owned native state, equal first-party
  and external selection, and a second real Adapter before compatibility freeze.
- ADR 0039 already owns fixed time, pause, exact one-tick stepping, and runtime generation
  lifecycle. Physics must join that transaction rather than creating another clock.
- OQ-005 is the correct decision owner for body/control modes, transform writers, query freshness,
  contact ordering, determinism, and deployment requirements.
- The reference game currently performs projectile and enemy contacts with direct squared-distance
  tests. `Player`, `Enemy`, and `Projectile` each persist their own `position` and `velocity`
  fields instead of using `Transform2d`. A physics migration therefore exposes a real dual-
  authority problem; adding a solver without resolving it would let gameplay, rendering, tooling,
  and physics disagree about spatial state.

# Competitor Lessons

| Product | Physics composition model | Lesson for Nara |
|---|---|---|
| Bevy | Physics is supplied by ecosystem plugins such as Rapier and Avian. Each plugin owns its ECS components, schedules, queries, and events. | Maximum code freedom, but changing solver usually means changing authored components and gameplay integration. Copy the plugin ergonomics and explicit system sets, not the lack of a portable product vocabulary. |
| Godot | Stable body/shape nodes lower through a startup-selected PhysicsServer. Godot 3D and Jolt share much of the product surface, while unsupported and backend-specific properties remain visible. Runtime server switching is not supported. | This is the closest replacement model: project-level selection, restart, shared authoring intent, explicit compatibility differences, and no behavioral-equivalence promise. |
| Unity | Built-in object-oriented 2D and 3D physics are fixed to Box2D and PhysX. DOTS uses a separate authoring/runtime stack where Unity Physics and Havok can share ECS-authored scenes. | Do not force every solver behind one universal abstraction. Replacement may be supported inside a deliberately scoped domain while different paradigms remain different products. |

None of the three engines promises that changing a solver preserves contact order, tuning,
stability, or replay behavior. Nara should not promise it either.

# Alternatives

## Backend-native ECS components

Let Rapier, Avian, or another package define all public physics components directly.

- Strength: closest to Bevy; exposes every backend feature with minimal glue.
- Cost: scene data and gameplay systems become solver-specific, so the product cannot offer a
  truthful switch or compatibility preview.
- Verdict: valid only for the exact-version interop tier, not the first-party portable path.

## Universal backend trait from the first implementation

Define `PhysicsBackend2d` with create, destroy, step, query, and event methods before implementing
multiple solvers.

- Strength: appears clean and testable.
- Cost: exposes a shallow mirror of solver APIs, places dynamic dispatch or generic plumbing in
  hot paths, and freezes assumptions learned from one backend.
- Verdict: reject. The Adapter seam is initially a protocol of data, schedule, lifecycle, and
  diagnostics. Extract a Rust trait only if two implementations prove that a trait adds leverage.

## One hard-coded solver

Put Rapier types and handles into core physics components.

- Strength: fastest initial implementation.
- Cost: replacement becomes a scene, save, editor, and gameplay migration rather than an Adapter
  change.
- Verdict: reject, consistent with ADR 0019.

## Portable domain plus graduated escape hatches

Keep common authoring intent Nara-owned, allow namespaced backend extensions, expose exact-version
raw access at declared schedule points, and select exactly one backend Adapter per runtime.

- Strength: supports coherent defaults without reducing advanced users to the common denominator.
- Cost: requires explicit authority, capability, diagnostics, and compatibility UX.
- Verdict: recommended.

# Recommended Module Shape

```text
Scene / Prefab / Rust gameplay
        |
        v
nara_physics2d portable intent and outcomes
        |
        | fixed-tick Adapter protocol
        v
selected Adapter private runtime-generation session
        |
        v
Rapier / Box2D / future custom solver
```

The deep Module is `nara_physics2d`. Its Interface is everything a game author must know:
portable body and collider semantics, authority rules, schedule visibility, event retention,
query freshness, failure behavior, configuration, and performance limits. Solver synchronization,
native handles, broadphase state, callback state, entity mappings, and teardown remain inside each
Adapter implementation.

The Adapter seam does not initially require one public trait. A conforming Adapter supplies:

- the exclusive `physics.2d.backend` capability through normal plugin composition;
- one runtime-generation-scoped private solver session;
- the complete physics transaction inside declared physics system sets;
- structured capability, validation, overflow, and backend-fault diagnostics;
- deterministic retirement of native state through the runtime close contract; and
- the shared conformance fixture without a first-party ID allowlist.

Exactly one backend may own the 2D physics capability in one runtime. Switching backends is a new
runtime construction operation, never a mid-step or in-place solver swap.

# Portable Authoring Surface

The first portable surface should be deliberately small. The following names are illustrative,
not authorized public symbols:

- body mode: static, dynamic, and kinematic;
- collider shapes: circle, box, and capsule first;
- sensor intent;
- friction, restitution, density, and collision layers;
- initial linear/angular velocity;
- force, impulse, kinematic target, and explicit teleport intents;
- collision started/ended outcomes; and
- gravity, substep, sleep, continuous-collision, and determinism settings only where the Trial
  workflow proves a portable meaning.

Do not make the portable surface a lowest-common-denominator prison. A backend package may register
namespaced persistent extension components such as solver-specific CCD, contact, or joint tuning.
Project inspection must classify them as backend-specific. A switch preview reports required
unsupported data and semantic differences; it never silently drops required extension data.

# Spatial Authority

| Body mode or operation | Recommended authority |
|---|---|
| Static body | ECS global pose seeds and updates the solver at the named pre-step boundary; the solver does not write it back. |
| Dynamic body | ECS pose and velocity seed admission. After admission the solver owns fixed-tick pose and velocity; writeback updates observable ECS state after each successful step. |
| Kinematic body | Gameplay writes an explicit target or velocity during the drive phase; the solver resolves motion and writes back the resulting pose. |
| Force or impulse | A tick-scoped typed intent consumed exactly once by the Adapter transaction. |
| Teleport | An explicit discontinuous command, not an arbitrary dynamic-body `Transform2d` write with hidden semantics. |
| Render interpolation | Reads previous/current completed physics poses but never changes authoritative simulation state. |

The first 2D slice should reject parented physics bodies and non-identity effective scale unless
the hierarchy/transform proposal has first become authoritative. Collider dimensions own physical
size; silently interpreting arbitrary `Transform2d.scale` differently across solvers would destroy
portability. This fail-closed restriction can be expanded by later evidence.

The reference-game migration should remove gameplay-owned duplicate position fields. Gameplay
components retain health, movement policy, damage, targeting, and similar domain state, while
`Transform2d` plus physics intent/result data become the spatial authorities.

# Fixed-Tick Transaction

A candidate user-facing phase model is:

```text
GameplayCommandSet::Consume
  -> Physics2dSet::Drive
  -> Physics2dSet::Simulate
  -> Physics2dSet::ConsumeResults
  -> GameplayCommandSet::Capture
```

The names are candidates. Their required semantics are more important:

- `Drive`: gameplay systems may update kinematic targets and submit forces, impulses, or
  teleports for the current fixed tick.
- `Simulate`: the selected Adapter exclusively performs validation, ECS-to-solver sync, one
  solver step, solver-to-ECS writeback, query-view publication, and contact publication.
- `ConsumeResults`: gameplay observes the current completed tick's poses and contact batch.

Adapter-internal sync, step, writeback, and publish sets may remain private. Public authors need a
small semantic Interface, while an exact-version Adapter can use a more detailed internal chain.
Any public set must receive the full participation, deferred-flush, skip, fault, and cleanup
documentation required by ADR 0003.

Contact outcomes are authoritative gameplay input and therefore cannot use silent lossy queues.
The batch must be bounded, tagged with the completed fixed tick and runtime generation, ordered by
stable runtime physics identity rather than hash or ECS query order, consumed only in the results
phase, and retired by the domain in fixed finalization. Overflow or an invariant violation faults
the runtime generation instead of continuing with missing contacts.

# Query Freshness

The stable query contract should eventually expose a synchronous, read-only view of the last
successfully completed physics snapshot:

- before the current physics transaction completes, it reports the previous completed tick;
- in `ConsumeResults`, its completed tick equals the current `FixedTime` tick;
- it never claims to include uncommitted ECS mutations; and
- query results use Nara entities/runtime identity and portable hit data, not solver handles.

Do not freeze the Rust query Interface from Rapier alone. The Rapier Trial may expose an
exact-version `RapierQueryAccess` escape hatch. A Box2D tracer must then pressure-test ray cast,
shape cast, overlap, filters, hit ordering, and callback/early-exit behavior before Nara freezes a
portable facade. This preserves advanced usefulness without pretending one implementation proved
the common contract.

# Graduated Extension Freedom

| Tier | User promise |
|---|---|
| Portable semantic tier | Nara body/collider intent, named fixed-tick phases, common contacts and later common queries survive a supported backend switch when capability validation passes. |
| Backend extension tier | Namespaced solver-specific components and settings expose capabilities absent from the portable model. Switching may require explicit migration or removal. |
| Exact-version interop tier | Advanced Rust systems may access the selected Adapter's raw context at documented schedule points. This surface follows that backend crate's version. |
| Replacement tier | A third-party Adapter can claim the exclusive physics slot, use the same selection/lifecycle/diagnostic protocol, and pass the same conformance suite without editing Nara core. |

Stable-tier freedom may be narrower than Bevy's direct plugin surface. Overall outcome freedom must
not be narrower: a user can choose the default, use backend-specific features, access raw solver
state, or replace the backend. The cost and compatibility guarantee become explicit at each tier.

# Backend Assessment

| Candidate | Evidence and integration shape | Recommended role |
|---|---|---|
| `rapier2d` | Pure Rust 2D/3D solver with contacts, sensors, queries, snapshots, and optional cross-platform determinism. Direct core integration avoids Bevy product ownership. Public shipped-game evidence is weaker than Box2D/Jolt. | First Trial/default Adapter because it minimizes integration and delivery risk while preserving Rust-native implementation. Do not market it as the permanent winner yet. |
| `bevy_rapier2d` | Mature Bevy integration with SyncBackend, StepSimulation, and Writeback phases, but it depends on Bevy App, Transform, messages, reflection, and plugin lifecycle. | Reference its integration lessons; do not depend on it from Nara. |
| Avian | Rust, ECS-driven, well documented, and closely aligned with Bevy 0.19. Its public crates intentionally depend on the full Bevy stack and own physics schedules/transform synchronization. Production validation remains younger. | Design reference and possible future independent pressure only if a clean solver-level Adapter can avoid importing Bevy product ownership. |
| Box2D 3 | C17 2D solver with extensive shipped-game history and cross-platform determinism in 3.1. Rust wrapper quality varies. `boxdd` currently presents a separated core binding and Bevy integration, but requires an audit and Nara conformance proof. | Second real Adapter before compatibility freeze. Its different FFI, callback, threading, and ID model is valuable counterevidence. |
| Jolt | C++ 3D solver used by Horizon Forbidden West and Death Stranding 2; built into current Godot and selected by default for new 3D projects. Available Rust bindings are less mature than the engine itself. | Future 3D Adapter candidate, not part of the first 2D decision. |

# Required Tracers

1. Migrate one reference-game path from hand-written distance collision to physics while removing
   duplicate spatial authority.
2. Prove root-only static, dynamic, kinematic, sensor, projectile CCD/sweep, contact start/end,
   despawn, explicit teleport, pause, exact one-tick step, and headless behavior.
3. Prove bounded contact overflow, invalid shape/body combinations, unsupported capability,
   backend fault, and runtime retirement diagnostics.
4. Benchmark representative enemy/projectile counts and record fixed-step tail cost, sync cost,
   allocation behavior, and event volume.
5. Implement a Box2D clean-room Adapter against the same authored fixture. Compare portable
   invariants and tolerances, not numerical equality.
6. Build one renamed-dependency external-Adapter fixture with no Nara source edit or first-party
   allowlist.
7. Prove an exact-version raw-access example and a backend-switch compatibility report that flags
   namespaced extension data.

# ADR Timing

Do not revise ADR 0019 from this research alone. After the user accepts or changes the choices
below and a physics Trial is admitted by an active plan, create a Proposed ADR that refines ADRs
0019 and 0042 and resolves OQ-005 for the Trial:

1. Rapier direct integration as the first 2D backend, with Box2D as the required second Adapter.
2. Solver authority for admitted dynamic bodies; explicit kinematic/teleport intent.
3. Root-only, identity-scale physics participation until hierarchy evidence lands.
4. The fixed-tick Drive/Simulate/ConsumeResults transaction and bounded contact lifecycle.
5. Last-completed-snapshot query freshness, with the common query Rust shape frozen only after the
   second Adapter.
6. Portable authoring components plus namespaced backend extensions and exact-version raw access.

Acceptance should wait for the tracer results named by OQ-005. A Proposed ADR can guide the spike;
it cannot turn unimplemented behavior into a product claim.

# Next Action

Discuss the six ADR choices above. If they hold, write the Proposed ADR and a separately activated
physics Trial plan. Do not add crates, root features, plugin slots, or public schedule sets under
the currently active reference-game foundation plan unless that plan explicitly admits the new
unit.

# Citations

- `STRATEGY.md`
- `docs/architecture/README.md`
- `docs/architecture/open-questions.md#oq-005-physics-integration-authority-and-backend-selection`
- `docs/architecture/adr/0016-extension-seams-for-backends-and-domain-modules.md`
- `docs/architecture/adr/0018-coordinate-units-and-time.md`
- `docs/architecture/adr/0019-physics-strategy.md`
- `docs/architecture/adr/0039-main-loop-time-pause-and-runtime-state.md`
- `docs/architecture/adr/0042-runtime-service-and-backend-boundary.md`
- `docs/architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md`
- `docs/architecture/adr/implementation-status.md`
- `reference-game/src/components.rs`
- `reference-game/src/systems.rs`
- Rapier overview and determinism: <https://rapier.rs/docs/>,
  <https://rapier.rs/docs/user_guides/templates/determinism>
- Rapier scene queries and events: <https://rapier.rs/docs/user_guides/rust/scene_queries/>,
  <https://rapier.rs/docs/user_guides/rust/collider_active_events>
- Bevy Rapier integration phases: <https://docs.rs/bevy_rapier2d/latest/bevy_rapier2d/plugin/>
- Avian package and scheduling: <https://docs.rs/avian2d/latest/avian2d/>,
  <https://docs.rs/avian2d/latest/avian2d/schedule/>
- Box2D documentation and determinism: <https://box2d.org/documentation/>,
  <https://box2d.org/documentation/md_faq.html>
- `boxdd` binding status: <https://docs.rs/boxdd/latest>
- Godot Jolt behavior differences: <https://docs.godotengine.org/en/4.6/tutorials/physics/using_jolt_physics.html>
- Godot PhysicsServer2D selection: <https://docs.godotengine.org/en/4.7/classes/class_physicsserver2dmanager.html>
- Unity physics stack overview: <https://docs.unity3d.com/cn/current/Manual/PhysicsSection.html>
- Jolt production evidence: <https://github.com/jrouwe/JoltPhysics>

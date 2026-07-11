# ADR 0058: Stable Runtime Identity and Entity References

**Status**: Accepted
**Date**: 2026-07-11
**Refines**: ADR 0034: Editor Play Mode World Boundary; ADR 0038: Scene/Prefab Authoring
Identity and Provenance; ADR 0056: Headless Runtime and Dedicated Server Readiness
**Related**: ADR 0057: Authoritative Fixed-Tick and Command Ingress; ADR 0076: Play Runtime
Debug Control and Observation

## Context

nara currently has three incompatible identity paths:

- `nara_scene::SceneEntityId` is a durable ID inside one scene document;
- every `SceneSpawner` privately allocates `SceneInstanceId`, so independent spawners and the
  convenience spawn functions can issue the same instance ID in one world;
- `nara_gameplay` defines separate `SceneStableId` and `PersistentRuntimeId` command vocabulary,
  while `nara_tooling::WorldSnapshot` exposes allocator-local Bevy `Entity` values.

`SceneEntityMap` is also a second mutable `SceneEntityId -> Entity` authority beside the world.
Insertion silently overwrites collisions, an instance counter saturates and then reuses `u64::MAX`,
and export manufactures IDs by concatenating `instance_N/` with authored IDs. Equal `Entity` bit
patterns in two worlds therefore have no explicit world scope, late commands cannot distinguish a
missing target from an unloaded one, and a future fork or checkpoint restore has no semantic remap
contract.

Persistent documents, authoritative commands, editor observations, and future replay checkpoints
need related but deliberately different identity values. A universal ID would collapse namespace,
lifetime, and serialization rules that must remain explicit.

## Decision

Create a dedicated `nara_identity` crate. It is below scene, gameplay, reflection, and tooling and
depends only on shared limit scalars, the ECS substrate and derive support, and identity-format
support. `nara_ecs` remains the thin Bevy ECS boundary; identity is a product domain rather than an
ECS storage primitive.

`nara_identity` owns the canonical types, the world-owned allocator/index, lookup outcomes, remap
records, and bounded tombstone evidence. No higher crate may define a competing scene or persistent
runtime identity type.

### Identity layers

| Type | Namespace and lifetime | Serialization contract | Lookup authority |
|---|---|---|---|
| `SceneEntityId` | Unique inside one source scene/prefab document | Allowed in project data | Document index |
| `PersistentRuntimeNamespaceId` | Validated public name for one runtime/save namespace | Allowed beside persistent runtime IDs | Owning persistence/runtime policy |
| `PersistentRuntimeId` | UUID unique inside its declared runtime/save namespace | Allowed where the owning format grants entity-reference capability | Never looked up without its namespace |
| `PersistentRuntimeReference` | Namespace plus persistent runtime ID | Allowed where the owning format grants entity-reference capability | World identity domain plus persistence restore/remap policy |
| `EntityReference` | Durable semantic reference: document-local scene entity or persistent runtime entity | Allowed in component/document values; never contains a runtime instance or `Entity` | Requires document/spawn context or persistent lookup |
| `SceneInstanceId` | Non-zero, monotonic, never-reused value inside one `WorldIdentityDomain` lifetime | Runtime/replay records only; forbidden in project documents | World identity domain |
| `RuntimeEntityReference` | Resolvable scene-instance/entity pair or persistent runtime ID | Allowed in command/replay records that explicitly belong to one runtime timeline; forbidden as project authoring data | World identity domain |
| `WorldIdentityDomainId` | Process-unique opaque ID for one world identity domain | Observation/session records only; not project identity | Owning world resource |
| `WorldEntityLocator` | World domain ID plus `RuntimeEntityReference` | Observation/session records only | Matching world identity domain |
| `WorldEntityToken` | Opaque capability for one entity minted by its bound world identity domain | Never serialized | Matching bound world plus private identity marker |
| `SceneIdentitySnapshot` | Bounded authoritative scene-group projection containing every active scene and persistent axis | Never serialized as project data | Matching world identity domain |
| Bevy `Entity` | Generational allocator slot inside one `World` | Never serialized or exposed as stable observation identity | Bevy `World` only |

Serde support is not itself permission to place a value in every persistent format. Scene, prefab,
patch, and component document schemas accept `EntityReference`, not `RuntimeEntityReference`,
`SceneInstanceId`, `WorldIdentityDomainId`, or `WorldEntityLocator`. Command/replay adapters may
encode runtime references only inside their declared timeline and existing ADR 0049 parse budgets.

### World identity domain

Each world that needs semantic identity owns exactly one `WorldIdentityDomain` ECS resource. Domain
creation is fallible and binds the resource to the target Bevy `WorldId`. Moving that resource into
another world does not transfer authority: every mutation, lookup, reverse lookup, retirement, and
snapshot entry point validates the binding and fails with a typed world-binding error. A checked
process allocator issues non-zero domain IDs and returns a typed exhaustion error without advancing
or wrapping; there is no infallible `Default` path that could silently alias a live domain. The
resource owns:

- one process-unique world domain ID;
- the sole scene-instance allocator for that world;
- active scene-reference and persistent-reference indexes;
- an entity-to-identity reverse index that may carry both axes for one entity;
- bounded lifetime claim sets for instance IDs and entity-reference axes that prevent reuse;
- a bounded recent-tombstone detail window and monotonic retirement sequence.

Identity-aware spawn mints an opaque, non-serializable `WorldEntityToken` and attaches a crate-private
domain marker to the entity. Registration accepts the token plus the target `World`, never a bare
`Entity`, and validates the world binding, token domain, current entity generation, and private
marker before consuming a claim. Equal entity allocator bits, a moved domain resource, a dead token,
or an unrelated replacement entity therefore cannot acquire another world's identity.

Creating a `SceneSpawner` never creates an allocator. Every spawn entry point obtains the allocator
from the target world's identity domain, so `SceneSpawner::new`, `SceneSpawner::default`, and all
convenience functions share one sequence.

Scene-instance allocation rejects zero and exhaustion without changing allocator state. It uses
checked advancement; wrapping, saturation, and reuse are forbidden. Every instance ID is itself a
lifetime claim, including an empty scene instance, so restore cannot claim the same empty instance
twice. Restoring an explicit instance ID into a fresh same-timeline domain reserves that value and
advances the next allocation past it, or marks the allocator exhausted when the restored value is
the maximum.

Registration is a preflight-then-commit operation. A wrong-world binding, stale token, reference
collision, entity-axis collision, tombstoned claim, or budget failure changes neither allocator nor
forward or reverse index. Input collection stops at the remaining lifetime-claim capacity instead
of materializing an unbounded rejected batch. An entity may have one scene identity and one
persistent identity, but never two identities on the same axis.

### Claims, tombstones, and lookup

The domain has two related budgets:

1. A lifetime claim budget charges one item for each scene-instance claim and one item for each
   scene or persistent entity-reference axis accepted by the domain. Claims are not evicted, which
   makes no-reuse exact. An empty scene still costs one instance claim. Once exhausted, allocation,
   restore, or registration that would add claims fails atomically.
2. A recent tombstone-detail budget bounds retained cause/sequence metadata. Older details may be
   evicted, but their claims remain and lookup still returns `Tombstoned` rather than `Missing`.

Lookup always receives the target `World` and returns a typed outcome:

- `Resolved(Entity)` when the reference is active;
- `Tombstoned` with optional recent detail when the reference existed and retired;
- `Missing` when the domain has never claimed the reference.
- `WrongWorldBinding` when the installed domain resource belongs to another Bevy world;
- `StaleRegistration` when the registered entity generation or private ownership marker is absent.

`resolve_in_world` additionally validates the world-scoped locator and installed domain ID. It can
return `WrongDomain`, `WrongWorldBinding`, or `StaleRegistration` without collapsing any of them into
`Missing` or `Tombstoned`. A durable scene-local `EntityReference` also returns `ContextRequired`
unless the caller supplies its owning scene instance; the domain never guesses by scanning for a
matching local ID.

Retirement removes every active axis for an entity atomically, records tombstones, and never makes
the claims reusable. Callers retire identity before or as part of despawn/unload. Direct external
despawn that bypasses the identity domain is detectable as a stale registration and must be
reconciled diagnostically; it is not silently treated as a valid entity.

### Spawn, fork, duplicate, and restore

- A normal scene spawn allocates a fresh `SceneInstanceId` and registers every local
  `SceneEntityId` under it. The returned scene-instance handle contains stable references, not a
  copied map of runtime entities. Consumers resolve through the world domain.
- Duplicating or parallel-forking scene content starts from a bounded authoritative
  `SceneIdentitySnapshot` produced by the source domain. The target transaction supplies one token
  per source entity plus an explicit target persistent reference exactly where the source snapshot
  has that axis. It preflights both axes, target claims, budget, and a planned target snapshot;
  constructs the complete locator remap; and only then allocates and commits the target instance.
- A parallel world fork receives a new `WorldIdentityDomainId`. Equal Bevy `Entity` bits can never
  make its `WorldEntityLocator` equal to the source locator.
- A same-timeline checkpoint restore creates a new world identity domain ID and fresh Bevy
  entities. It uses the same preflight/remap/commit transaction while reserving the recorded scene
  instance ID. It explicitly preserves or rewrites semantic `RuntimeEntityReference` values before
  command replay resumes and publishes a locator remap that replaces the old domain ID. The source
  and restored worlds may coexist without locator aliasing. Recorded commands resolve through the
  restored domain, never through recorded `Entity` bits.
- Persistent IDs are preserved only for an authoritative fork/restore policy. Duplicated content
  must allocate or receive new persistent IDs and publish the remap.

The complete group remap produces both runtime-reference and durable `EntityReference` mappings.
Reflection rewrites declared component-reference candidates into a new value, and gameplay rewrites
submission targets into a new submission before admission. A failed or incomplete rewrite leaves
the source value/submission unchanged and cannot publish a partial candidate. U16 owns cloning the
complete runtime host and deciding when those candidates commit; U8 supplies the bounded rewrite
primitive rather than a partial world-clone API.

No clone/fork helper may partially publish a remap. Generic caller-self-attested remap builders are
not public. Duplicate source references, incomplete scene groups, mismatched identity axes, target
collisions, stale registrations, or budget failures reject the entire operation.

### Reflected values and command targets

`nara_reflect::ComponentValue` gains an explicit `EntityReference` variant and
`ComponentValueKind::EntityRef`. It is not represented as an ad hoc string or generic map. Schema
capability still gates whether a field may save, inspect, edit, replicate, or script the reference.
Persistent variants carry `PersistentRuntimeNamespaceId` plus `PersistentRuntimeId`; lookup never
assumes that a bare UUID is globally authoritative.

`nara_gameplay::GameplayCommandTarget` uses the shared `RuntimeEntityReference` for entity targets
and retains its separate named-target vocabulary for non-entity routing. The gameplay-only
`SceneStableId` and duplicate `PersistentRuntimeId` are deleted. A bare `SceneEntityId` cannot be a
runtime command target because it is ambiguous when the same scene is spawned more than once.

Command consumers resolve entity targets only through the current world's identity domain.
Missing, tombstoned, domain-unavailable, wrong-world-binding, and stale-runtime outcomes remain
distinguishable typed results. A command target is scoped by its replay/runtime timeline and does
not embed `WorldIdentityDomainId`; therefore `WrongDomain` is intentionally a
`WorldEntityLocator`/`resolve_in_world` result, not a command-target result. Parallel-fork replay
must rewrite the runtime target through the complete group remap before admission.

### Observation boundary

Tooling observations contain no Bevy `Entity`, scheduler `NodeId`, backend handle, or process
pointer. A bounded identity snapshot records:

- the world domain ID when the world has an identity domain;
- stable `WorldEntityLocator` values up to a declared item limit;
- total, identified, unidentified/runtime-only, and omitted counts.

Runtime-only/internal entities are count-only in this slice. They do not receive persistent IDs
solely for tooling. Identified locators sort by semantic identity, not allocator/query order.
`SceneInspectorEntityRow` carries an optional stable locator or live-state flag rather than a raw
`Entity`.

Observation capture is a projection, never a second lookup authority. Runtime resolution always
returns to the owning world identity domain.

### Export remap

Scene export builds an explicit injective ID assignment before encoding records. It first collects
all authored candidates, detects duplicates, and then assigns deterministic generated IDs while
skipping every already-claimed authored or generated ID. It does not concatenate instance numbers
into authored namespaces.

The export report exposes the stable source-to-document remap needed to rewrite internal
`EntityReference` fields. Every active locator axis on an exported entity maps to the same assigned
document ID, so a scene locator and persistent locator may intentionally be aliases in the remap;
assigned IDs remain injective by exported entity. A locator collision or incomplete rewrite fails
the export rather than silently overwriting a record or emitting a dangling reference. Prefab
expansion's documented `anchor/source_entity` authoring namespace is unchanged; this rule removes
only the runtime export shortcut.

A persistent reference that resolves to an entity in the same export set is rewritten to that
entity's assigned scene-local ID because the exported document does not recreate its source runtime
persistent axis. A persistent reference that resolves to a live entity outside the export set stays
persistent. Missing, tombstoned, stale, or otherwise invalid persistent targets fail the complete
export; they are never preserved as unchecked document references.

## Ownership and Dependency Direction

```text
nara_identity -> { nara_core, nara_ecs, bevy_ecs (derive support), uuid }
nara_gameplay -> nara_identity
nara_reflect  -> nara_identity
nara_scene    -> { nara_identity, nara_reflect }
nara_tooling  -> { nara_identity, nara_reflect, nara_scene }
```

- `nara_identity` must not depend on scene, gameplay, reflection, tooling, render, window, or
  backend crates.
- `nara_scene` owns authoring provenance and the `SceneEntitySource` component, but uses canonical
  identity values and the world domain.
- `nara_reflect` owns value/schema participation, not identity allocation or lookup.
- `nara_tooling` owns observation budgets and presentation models, not runtime lookup authority.

## Alternatives Considered

### Option A: Keep identity in `nara_scene`

**Pros**: Fewer crates and minimal migration.

**Cons**: Gameplay, persistence, and tooling would depend on a document domain for runtime identity;
scene convenience would remain the architectural owner by accident.

**Decision**: Rejected.

### Option B: Add an identity module to `nara_ecs`

**Pros**: Direct access to `World` and `Entity`; no new workspace crate.

**Cons**: Turns the intentionally thin Bevy re-export boundary into a product identity domain and
makes future non-scene identity policy look like ECS mechanics.

**Decision**: Rejected after the dependency spike showed a dedicated crate introduces no cycle.

### Option C: Use persistent UUIDs for every entity

**Pros**: One apparent identifier for documents, runtime, tooling, and replay.

**Cons**: Assigns durable identity to internal entities, obscures scene-instance lifetime, increases
persistent data and index cost, and still needs world scope for parallel worlds.

**Decision**: Rejected.

### Option D: Dedicated layered identity domain (Chosen)

**Pros**: Keeps namespaces and serialization honest, removes duplicate lookup authorities, supports
server commands and tooling without raw entities, and makes fork/restore remaps explicit.

**Cons**: Requires a breaking migration across scene, gameplay, reflection, tooling, and the facade.

**Decision**: Chosen. nara is pre-1.0, so obsolete identity APIs are removed rather than wrapped.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Allocation safety | Zero, exhaustion, and all collision paths are failure-atomic; no wrap/saturation reuse | `nara_identity` unit tests |
| World isolation | Equal Bevy entity bits in two worlds never yield equal observation locators | two-world tests |
| Spawn authority | Independent spawners and convenience calls share one world allocator/index | scene integration tests |
| Fork/restore | Parallel fork remaps a complete group; same-timeline restore resolves recorded references to fresh entity slots | identity/scene tests |
| Retirement | Unloaded identities remain typed tombstones and can never be reused within the domain budget | identity tests |
| Persistent safety | Serialized component, command, and observation shapes contain no `Entity` or `AssetId` | serde boundary tests |
| Tooling safety | Runtime-only entities are bounded count-only; stable observations are domain-scoped | tooling tests |
| Vocabulary convergence | No duplicate `SceneStableId`, `PersistentRuntimeId`, or raw `SceneEntityMap` authority remains | dependency-boundary searches |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Lifetime claim budget eventually exhausts in a very long world | High | Low | Make the budget explicit and observable; fail closed; replace the world/timeline instead of reusing identity. |
| Direct Bevy despawn leaves a stale active index | High | Medium | Provide retirement helpers, validate resolved entities against the world, and surface a typed stale-registration outcome. |
| Domain resource is moved into a different Bevy world | High | Low | Bind domains to `WorldId`; validate all entity-bearing operations; require private entity markers and opaque tokens. |
| Runtime references leak into project documents because they implement serde | High | Medium | Keep project component values typed as `EntityReference`; add negative fixtures and capability validation. |
| Fork remap misses an internal reference | High | Medium | Require complete group preflight and atomic publication; test multi-entity cyclic references. |
| Observation volume grows with large worlds | Medium | High | Hard item limits, stable truncation, and count-only unidentified entities. |
| Export renames make diffs noisy | Medium | Medium | Preserve unique authored IDs first; assign deterministic collision-free generated IDs and publish the remap. |

## Consequences

- `SceneEntityId`, `SceneInstanceId`, and `PersistentRuntimeId` move to `nara_identity` and are
  re-exported from higher-level preludes only where appropriate.
- `SceneSpawner` becomes stateless with respect to identity allocation. Scene spawn reports expose
  an instance handle whose resolution delegates to the world domain.
- `SceneEntityMap`, gameplay `SceneStableId`, raw `WorldSnapshot`, and inspector raw live entities
  are deleted rather than retained behind compatibility wrappers.
- U9 can attach conservative save/inspect/edit/replicate capability policy to typed entity
  references without inventing their identity semantics.
- U10 can extend the explicit remap with prefab projection provenance while preserving the existing
  authoring `anchor/source_entity` rule.
- U16 and future replay work can restore semantic references into fresh worlds and can scope every
  observation by world domain without serializing ECS allocator state.

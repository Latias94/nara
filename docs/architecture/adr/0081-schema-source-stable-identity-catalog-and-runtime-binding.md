# ADR 0081: Schema Source, Stable Identity, Catalog, and Runtime Binding

**Status**: Accepted
**Last Revised**: 2026-10-24
**Date**: 2026-07-12
**Implemented Slices**: RGF-U1 canonical catalog and native binding boundary on 2026-07-12; RGF-U2
native Rust derive authoring and public headless consumer on 2026-07-13; RGF-U12 World-independent
content/fingerprint handoff on 2026-07-18; RGF-U29 explicit persistent composition and guarded
target-World eligibility on 2026-07-18
**Refines**: ADR 0011, ADR 0045, ADR 0051
**Refined By**: [ADR 0095](0095-plugin-owned-specialized-domains-and-project-configuration.md) and
[ADR 0098](0098-schema-owner-lineage-and-active-runtime-composition.md), and
[ADR 0118](0118-editor-workspaces-frontends-and-creation-templates.md)
**Related**: ADR 0034, ADR 0058, ADR 0076
**R5a Design**: [Durable Property Addressing](../durable-property-addressing-design.md)

## Semantic Authority And Representation Scope

[ADR 0119](0119-schema-first-semantic-metadata-authority.md) owns the Schema-first semantic
authority, replacing ADR 0004's Bevy-reflection substrate assumption. This ADR's executable
preparation, freeze and one-to-one World binding requirements describe the current **native
representation**. They remain mandatory for every Runtime path implemented today.

[Proposed ADR 0120](0120-schema-publications-and-execution-representations.md) compares complete
owner-managed representations and narrower immutable publications for runtime-defined Behavior
Schemas. It does not authorize skipping current native bindings, mutating a frozen registry or
turning the native bijection into a multimap. A new representation needs its own complete
read/absence, grouped preparation, encoding, reference and retirement proof before adoption.

## ADR 0095 Refinement

A schema owner's durable identity/version/tombstone lineage is distinct from one product recipe's
composed catalog fingerprint. Omitting an optional plugin from a composition does not tombstone its
types and must not prevent later reactivation of the same compatible owner lineage. Runtime still
requires a complete frozen native binding registry.
Lossless authoring with known-but-unbound or unknown schema is now governed by Accepted ADR 0090 and its refinements; Runtime still rejects missing native bindings rather than creating placeholders.

## ADR 0118 Refinement

[ADR 0118](0118-editor-workspaces-frontends-and-creation-templates.md) admits optional complete creation-only schema templates.
Catalog identity includes present template metadata, while same-version durable decode semantics exclude it.
Field missing-value defaults, strict native binding and owned schema lineage retain their existing roles.

## S3 Component Authoring Refinement

Finite scalar intervals follow ADR 0045 and participate in saved schema identity. Tightening or
relaxing a range requires an explicit component successor; migrations decide how historical values
are handled rather than silently clamping them. The UI owner preserves its generation-two catalog
and advances UiText to version three with its complete native F32 `(0, 256]` size constraint.

A component-domain failure may retain a checked static diagnostic code and summary, plus an
optional stable root field ID. The Scene bridge publishes that field only when an explicitly
supplied admitted schema contains it; no schema means no field claim. Dynamic codec messages,
value paths, map keys, locators and payloads remain private fallback detail. Data-only interval
failures use engine-defined identity and the known root field, without executing native codecs.
The diagnostic owner remains independent of reflection; checked code/summary primitives may be
consumed by reflection without adding reverse dependencies or a second diagnostics bus.

A source-built component inventory may generate the existing pure generation-one catalog/schema
callbacks and native registration callback. It does not install a package, freeze a registry,
discover types or infer historical catalogs/migrations. Successors reuse the current schema list
while retaining explicit predecessor snapshots, owner/provider identity and binding versions.
The existing atomic provider admission still compares declared and native schemas.

## Context

ADR 0011 made `ComponentTypeId` independent from Rust type names, but the first implementation still
mixes four different concerns in `ComponentRegistry`:

- persistent component identity and field paths;
- exported schema metadata;
- Rust `TypeId` and `rust_type_path` bindings;
- Bevy reflection, codecs, and migration closures.

That shape is not suitable for the canonical version-1 schema catalog required by ADR 0051. A Rust
type path is process/toolchain binding metadata rather than persistent identity, and a name-based
field path changes when an author renames a field. Nara's Rust-first product direction still needs
scene, prefab, save, inspector, and patch data to survive refactors and runtime reconstruction.

The schema catalog must therefore become stable before it becomes persistent. This ADR only closes
the identity, authority, catalog, binding, and freeze boundaries needed by RGF-U1. It does not
select a scripting language or implement dynamic ECS components.

## Decision

Nara separates schema declaration authority, compiled semantic authority, and runtime binding
authority.

```mermaid
flowchart TD
    Source[Rust declarations or project data schemas] --> Identity[Explicit stable IDs and tombstones]
    Identity --> Compile[Schema collection and validation]
    Source --> Compile
    Compile --> Catalog[Immutable runtime-independent schema catalog]
    Catalog --> Native[Native Rust and codec bindings]
    Native --> Runtime[RuntimeInstance]
    Catalog -. concrete adapter requirement .-> Optional[Optional adapter-specific projection]
    Optional -.-> Runtime
```

### Authority Layers

1. Native Rust declarations and project data formats own semantic declarations. Stable IDs and
   tombstones should be explicit in their source when practical; an engine-managed,
   version-controlled sidecar is allowed only when an authoring format cannot retain them safely.
2. A validated immutable catalog is the semantic authority consumed by runtimes, tools, validators,
   and code generators. The catalog is derived and rebuildable from the declaration source plus its
   identity sidecar.
3. A runtime binding registry maps catalog identities to Rust/Bevy types, codecs, migration code, or
   a future dynamic representation. Runtime binding metadata is not persistent schema identity.

No layer may silently manufacture a replacement identity from a Rust path, display name, field
name, or current runtime `ComponentId`.

### Stable Type and Field Identity

- `ComponentTypeId` remains the stable identity for the component-role slice of the schema catalog.
  It is an opaque, bounded string. Existing built-in reverse-domain IDs become permanent tokens;
  their resemblance to names does not make them renameable aliases.
- Every root catalog field has a `ComponentFieldId`, unique within its component and retained as a
  component-level tombstone after deletion. A nested Record member may opt into an opaque, bounded
  sibling-scoped ID. Its identity includes its complete stable parent path, allowing identified
  nested shapes to be reused. Root-relative declaration-path tombstones retain retired member
  scopes even when an intermediate value shape disappears; a tombstoned prefix cannot reactivate.
  This does not make every nested member durable by default or create a global shape registry.
- Type and field display names are mutable aliases. Renaming an alias does not change an ID and does
  not require a persistent field-reference migration.
- `ComponentFieldPath` locates data inside the current version's `ComponentValue`. It is a codec and
  value-layout locator, not durable field identity.
- Persistent root-field references use `ComponentTypeId + ComponentFieldId`; the nested
  property selector extends this with tagged member, keyed-item and map-entry segments. The active
  schema resolves the selector to a current value location only for the exact admitted Schema and
  current-version value. It must not persist a Bevy reflection path or raw list index.
- Moving a field between owning types, splitting or merging fields, or changing semantics requires
  an explicit migration. A storage-path change that preserves one field ID is handled by the
  component value migration and current catalog mapping.
- Durable field writes carry the component schema version that defines their value semantics.
  Stable identity alone cannot upgrade an old field value: until a dedicated field-value migration
  contract exists, writes from an older component version are rejected. Whole-component values use
  the registered component migration chain; identity-only field removal may resolve against the
  current catalog.
- A keyed list's item identity is an explicit Schema declaration over an exact String/I64/U64
  member value. A keyed-item selector also names that stable identity member; a different current
  identity policy refuses it rather than silently changing its meaning. Keyed items are distinct
  from list positions and map entries. Current uniqueness is
  sufficient for current lookup, but this ADR does not manufacture arbitrary value history: a
  reused key asserts continuity. A future overlay/rebase owner must retain lineage or require an
  explicit remap if it needs to distinguish restoration from accidental reuse.
- Identity allocation and tombstone persistence for generated or adapter-owned schemas belong to
  the concrete generator or adapter. RGF-U1 establishes the catalog contract but does not invent a
  universal source language or sidecar format.

The version-1 wire representation uses validated strings for type and field IDs. This ADR does not
require UUIDs or numeric IDs; a later encoding change requires a new catalog format version rather
than reinterpretation of version 1.

### Runtime-Independent Persistent Catalog

The persistent catalog contains only stable semantic data:

- catalog format version and a separate catalog generation/fingerprint seam;
- type IDs, aliases, schema versions, roles, and capabilities;
- field IDs, aliases, current value locators, recursive value shapes, defaults, and capabilities;
- retained type and field tombstones needed to prove non-reuse.

The catalog must not contain Rust `TypeId`, `rust_type_path`, Bevy `ComponentId`, codec closures,
function pointers, VM handles, or backend-native values. Rust paths may remain non-persistent debug
metadata on a native binding.

The unshipped catalog envelope and the authoring bundle embedding it move to canonical version 3
for scoped nested-member identity, declaration-path tombstones and keyed-list policy. Versions 1/2
reject explicitly; no automatic compatibility reader is retained. The fingerprint domain moves to
version 3 with the new tagged semantic encoding. Component schema versions, stable IDs, tombstones
and lineage remain distinct from this envelope reset. Fixtures preserve their component histories
and rewrite fingerprints/predecessor links using the new canonical encoding. Root-only schemas do
not gain member IDs or new value semantics through that rewrite. Unrelated Scene, Prefab, patch
and asset envelope versions remain unchanged.

The [R5a qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T024954Z-r5a-final-owner-diagnostics-and-workspace-qualification-f58a0ab130cb47a8bc2829e32f1d0a86.md)
executes metadata, lineage, resolver and independent Scene/Prefab edit workflows under this v3
contract. It does not qualify complete source/product provenance, durable overlays or package
scope. Those remain separate owning contracts rather than additional Reflect responsibilities.

Shape fingerprints include recursive tags, member/key ordering, scalar numeric domains,
presence/default semantics and reference capabilities. Editor-only presentation remains separate
from persistent semantic compatibility. Derive obtains finite field shapes through Rust codec
traits, allowing rustc to resolve aliases and container arguments instead of a macro name resolver.

The first RGF-U1 catalog contains only component-role declarations. Plain values, data assets,
commands, and events may later reuse the same stable identity discipline when a real persistent
format needs it; this ADR does not require one universal project type system.

The version-1 catalog does not encode Bevy required-component closures, component lifecycle hooks,
or observers. Its generation/fingerprint therefore cannot certify a persistent binding whose
durable composition, defaults, or construction side effects depend on those mechanisms.

Eligibility is therefore two-phase:

1. provider validation/freeze registers the native component in an isolated scratch ECS registry
   when necessary and rejects required-component or intrinsic `ComponentHooks` metadata that can
   participate in persistent insertion/removal; and
2. every actual persistent apply flushes deferred work, records the post-flush rejection baseline,
   and takes one exclusive target `World` borrow. A fresh-target apply rechecks real `ComponentInfo`
   values plus matching event-global/component-global lifecycle observers before allocation and
   retains exclusivity through allocation and persistent insertion. An apply to an already existing
   or reserved target additionally checks entity and entity+component scopes before its first
   persistent mutation. Rejection leaves the applicable post-flush baseline unchanged.

The second check rejects late required/hook registration and active `Add`/`Insert`/`Discard`/
`Remove`/`Despawn` observers that insertion, replacement, removal, or despawn could trigger for the
affected persistent apply. Across fresh- and existing-target paths it covers event-global,
component-global, entity, and entity+component observer scopes rather than inspecting only
component-targeted entries, and it inspects target-World
`ComponentInfo` hooks so `World::register_component_hooks*` cannot bypass provider-freeze checks.
In the current Bevy substrate, replacement is not a sixth lifecycle event: it triggers `Discard`
for the old value followed by `Insert` for the new value, so both caches are part of the check.
Post-publication World-local hooks and runtime observers are allowed. A later persistent apply
rejects while a matching hook remains installed; for a matching observer it may instead wait for an
explicit Host safe point that disables that observer. Runtime-only ECS components, unrelated
custom-event observers, and post-spawn runtime projections remain free to use Bevy-local mechanisms
outside document truth.

A content snapshot may certify bounded document/schema decoding and the explicit persistent set; it
does not certify a future target `World`'s observer or component-registration topology. Runtime
construction and every direct persistent-spawn path own the second check.

Scene materialization separates fallible native-value preparation from runtime target binding.
After the existing topology and identity-capacity admission, the component batch prepares all native
values in its original scratch asset context before allocating Scene target entities. A preparation
failure disposes the partial owned values and scratch catalogue without consuming Scene target
allocations. Values and their originating batch stay inseparable; binding targets does not rerun
codecs or transfer prepared handles to another catalogue. Final World, target, asset and lifecycle
admission remains mandatory. This does not make the entire Scene publication allocation-free, undo
ordinary deferred World work, or roll back arbitrary codec/destructor effects outside its context.

Any future catalog-derived persistent composition must add stable `ComponentTypeId` dependencies,
bounded deterministic closure and cycle rules, default/override/removal semantics, migrations, and
unavailable-provider behavior to a versioned fingerprint. It must agree across Scene, Prefab,
Inspector, migration, and direct persistent-spawn paths. ADR 0006 defines the current explicit-set
rule and OQ-043 owns the future carrier decision.

File `format_version`, catalog generation/fingerprint, per-type schema version, and
`engine_min_version` are distinct axes. None may be reused as another axis.

### Capabilities and Whole-Value Access

Component and field capability defaults are empty. Canonical version 1 contains exactly `scene`,
`inspect`, `edit`, `asset_ref`, and `entity_ref`. The first three are component-and-field
eligibility gates. `asset_ref` and `entity_ref` are field-only value-kind markers and are rejected
at component scope. Save, animation, replication, scripting, diagnostics, and runtime-only state do
not have speculative wire capabilities; a concrete domain consumer must admit each future value.

An operation is eligible only when both the component and every touched field grant the required
capability. Whole-value encode/apply paths must prove complete declared-field coverage and reject
unknown or ineligible fields. A mixed-capability value returns a projection-required error rather
than silently filtering fields. Field-level patch operations are the explicit projection path.

`inspect` is eligibility only. It does not replace the host disclosure/redaction policy required by
ADR 0076.

### Freeze and Runtime Ownership

Each runtime-owned registry has a one-way state transition:

```mermaid
stateDiagram-v2
    [*] --> Building
    Building --> Building: registration rejected without mutation
    Building --> Frozen: full validation and snapshot publication succeeds
    Building --> Building: freeze validation fails
    Frozen --> Frozen: every mutation is rejected
```

- Component owners register schemas and bindings only during plugin `build`.
- The reflection/schema registry plugin owns `freeze` in plugin `finish`. Schema provider plugins
  require that owner, so its build creates the registry before providers register and its finish
  freezes before the app can become `Ready`.
- Freeze atomically validates identity uniqueness/non-reuse, aliases, fields, capability scope,
  defaults, migrations, and required runtime bindings. Failure publishes no frozen snapshot and
  causes normal fallible plugin finish failure.
- Frozen registries expose no unchecked mutable Bevy type registry, codec table, migration table,
  schema, or capability path.
- Native codec execution belongs to the immutable frozen snapshot. Registration and freeze remain
  on the registry; a building registry exposes no encode or preflight operation. Obtaining a
  snapshot before freeze returns `NotFrozen`, and freeze rejects missing native bindings before
  publishing an execution authority. A retained snapshot keeps its exact validated definition;
  taking or using it does not certify the current target World's registry or lifecycle topology.
- Preflight returns a prepared component or an explicit execution error. Encoding returns an
  encoded value, actual entity/component absence, or an explicit execution error. Unknown schema,
  unavailable native binding and codec failure must not collapse into the same absence result.
  Context-aware operations preserve one caller-owned decode/encode context for the complete batch.
- A structural catalog change builds and validates a fresh `RuntimeInstance`; an active runtime is
  never unfrozen or modified in place. Presentation-only UI state may live outside the frozen
  catalog, but catalog aliases themselves change only in a new catalog generation.

### Implemented RGF-U2 Rust Authoring Slice

Native Rust components may derive `PersistentComponent` beside `Component`. The declaration
provides explicit component and field IDs, aliases, schema version, capabilities, and retained field
tombstones once; the derive generates the native `PersistentComponentProvider`, schema, decoder,
and encoder. Runtime-only components continue to derive only `Component` and remain absent from
the persistent catalog.

Component-owning plugins call `validate_persistent_component::<T>` during read-only preflight and
`register_persistent_component::<T>` during build. Validation performs the same complete schema
and binding checks as registration without mutation. The registry owner still freezes the complete
candidate during plugin finish; the derive cannot publish, replace, or unfreeze a catalog.

The first admitted field types are `i64`, `u64`, `Vec2`, and `EntityReference`.
Entity references require `entity_ref`; `asset_ref`, collections, generics, enums, tuple
structs, and arbitrary nested Rust types remain rejected until a production consumer proves their
codec and schema contract.

The recursive-shape successor under ADR 0045 admits the supported scalar/reference containers.
Its Vec2 correction declares required x/y finite-F32 members under the existing stable root field,
including inside containers. Moving an existing owner from opaque Vec2 maps to this closed shape
requires a component successor, explicit migration and exact predecessor catalog; unchanged wire
encoding alone is insufficient. The Reference Game retains catalog v4 and publishes catalog v5
with InitialVelocity2d v2 and its explicit v1-to-v2 migration.

External manual codecs register through the registry's closure-based codec methods, with public
encode/decode contexts and typed errors. The erased codec trait and storage are private registry
implementation details; they do not constitute a second external codec installation interface.

The independent `reference-game` is the public production consumer. Its four components use only
root exports, freeze against a committed predecessor catalog, round-trip through canonical
`SceneDocument` and stable-ID `ScenePatchDocument` files, and materialize into a live world.
Locked renamed-dependency fixtures verify root and ECS derive path resolution.

This slice does not admit dynamic non-Rust component storage, a universal schema compiler, generated
schema sidecars, a scripting ABI, or adapter-owned catalog projection.

### Implemented RGF-U12 Content Fingerprint Slice

`ProjectContentLoader` consumes the frozen `SchemaValidationInput` carried by the matching
`RuntimePlan`. Scene and prefab component values decode and validate against that registry, while
`collect_declared_asset_references` traverses only fields whose frozen schema declares
`asset_ref`. Unknown fields, undeclared reference shapes, incompatible providers, and a changed
catalog reject before snapshot publication.

The immutable `ProjectContentSnapshot` carries the same opaque project lineage, catalog
fingerprint, and schema generation together with the explicit expanded stable-ID document set,
content revision, and content digest. It stores no native type/codec binding and does not certify a
future target `World`'s required-component, hook, or observer topology. RGF-U29 now supplies the
separate provider-freeze and per-apply target-World eligibility proof; U24 later compares the fresh
runtime registry before materialization.

### Implemented RGF-U29 Persistent Apply Slice

Persistent codecs produce `PreparedComponentCandidate<T>`, which has no public apply operation.
Registration for native component `T` requires a candidate that produces exactly `T`; deferred
and asset-aware `Send + 'static` owning `FnOnce` closures retain that type relationship until a
private registry adapter erases them for heterogeneous batches. Registering `T` with a candidate
for another type is a compile error. The adapter carries the registration function and
target-World validator for the same `T`, without caller-supplied runtime type proofs.

The registry owns component selection during encoding. Encoders receive the selected `&T` and
return one `ComponentValue`; only the registry reports component absence. Context-aware encoders
also receive read-only `World` and asset export context, so resource-dependent references do not
require repeated component lookup. This does not authorize codecs to mutate the target World.

A frozen `ComponentRegistrySnapshot` alone may bind the candidate's stable component identity into
`PreparedComponent`. Building registries expose no executable codec interface, and compile-fail coverage
prevents external construction of the bound type. Deferred preparation still shares one scratch
`AssetServer` for the entire batch. A failed later candidate publishes neither earlier components
nor scratch assets; commit revalidates the target World, lifecycle topology and captured asset
server baseline before publication. Static component typing does not prove these mutable facts.

Provider validation rejects Scene-capable component types with Bevy required-component metadata or
intrinsic lifecycle hooks. Each real persistent apply then flushes deferred registration and checks
the target World under one exclusive borrow. Fresh targets cover real component metadata plus
event-global/component-global Add, Insert, Discard, Remove, and Despawn observers before allocation;
existing or reserved targets also cover entity and entity+component scopes before mutation.

Support metadata is registered before the permitted flush, but support insertion requirements and
stable/runtime binding eligibility are derived from the resulting World. Queued first application
may already have installed those resources; pre-flush absence cannot authorize a second insertion.
The flush does not renew an original asset baseline: preparation carrying that baseline revalidates
the captured asset authority, rejecting interference rather than recapturing it as legitimate input.

Private target receipts and a World-global bidirectional stable/runtime binding authority preserve
identity across target retirement and reject missing authority, collisions, temporal rebinding, and
cross-World use. Asset preparation separately declares whether it may need `AssetServer`, so an
asset-free value neither rejects an unrelated resource observer nor inserts the resource.

The Bevy-version-coupled hook and observer probes remain in `nara_ecs::__private`. Runtime-only
components and post-publication runtime behavior remain valid; later persistent work rechecks the
current topology. This slice proves pre-mutation rejection, not rollback of arbitrary lifecycle or
native-service side effects.

### Persistent File Boundary

A complete file envelope belongs only to a top-level file. Embedded semantic records do not repeat
file generator or minimum-engine metadata. In particular, `PrefabInstance` embeds a versioned patch
record, while a standalone scene-patch file wraps that record in the shared ADR 0051 envelope.

The RGF-U1 compatibility matrix contains exactly four version-1 kinds: scene, prefab, scene patch,
and component schema catalog. There is no speculative document migration registry while all
matrices are version-1-only.

## Alternatives Considered

### Option A: Keep the Rust-bound, name-path catalog

**Pros**: Smallest immediate change and preserves current tests.

**Cons**: Freezes `rust_type_path` and rename-sensitive paths into canonical version 1 and
guarantees another persistent-format reset after an ordinary Rust refactor.

**Decision**: Rejected.

### Option B: Use stable type IDs but retain name-based durable field paths

**Pros**: Preserves most patch APIs and component registrations.

**Cons**: A field rename changes durable addresses, forces unnecessary migrations, and contradicts
the non-reused field identity contract.

**Decision**: Rejected.

### Option C: Separate stable field IDs from current value locators

**Pros**: Alias and storage-path changes can preserve identity, catalogs remain independent from
process-local Rust/Bevy bindings, and patches resolve through one validated authority.

**Cons**: Requires a breaking patch/schema API migration and explicit IDs for every built-in field.

**Decision**: Chosen.

### Option D: Implement dynamic ECS lowering and a universal scripting host in RGF-U1

**Pros**: Proves the full product direction immediately.

**Cons**: Couples persistence foundation work to unproven non-Rust component, query, scripting,
and reload requirements and exceeds RGF-U1's dependency evidence.

**Decision**: Rejected for RGF-U1 and retained as a later evidence-driven slice.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Rename-safe identity | Type/field alias changes preserve stable IDs and durable patch targets | Catalog and patch tests |
| Runtime independence | Canonical catalog fixtures contain no process-local Rust/Bevy/codec identity | Golden-file tests and stale search |
| Non-reuse | Duplicate or tombstoned type/field IDs fail before catalog publication | Registry freeze tests |
| Frozen authority | Every schema, binding, codec, and migration mutation fails after freeze | Registry state-machine tests |
| Whole-value safety | Mixed-capability whole values never reach encode/apply | Projection gate tests |
| Runtime isolation | Structural catalog changes require a fresh runtime snapshot | Plugin/runtime lifecycle tests |
| Composition truth | A catalog fingerprint cannot remain valid while an unmodeled Bevy requirement or hook changes persistent spawn semantics | Persistent-binding eligibility and scene composition tests |
| Scope control | Dynamic ECS and universal scripting-host APIs are absent from RGF-U1 | API and dependency review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Human-readable IDs are accidentally treated as aliases | High | Medium | Validate and document IDs as opaque permanent tokens; expose aliases separately. |
| Field-ID registration becomes verbose | Medium | High | Allow explicit named presets/builders, but never infer identity or capability from Rust fields. |
| Freeze order depends on plugin installation accidents | High | Medium | Require the registry-owner plugin through plugin metadata and reject registration outside build. |
| Catalog and native bindings diverge | High | Medium | Freeze validates complete binding coverage for native component declarations. |
| Required components or hooks change runtime meaning without changing the catalog | Critical | Medium | Reject hook/require-dependent persistent bindings until the behavior is stable-ID modeled and fingerprinted. |
| Future dynamic representation pressures the v1 catalog | Medium | Medium | Keep runtime layout and `ComponentId` outside the persistent catalog. |
| Tombstone policy grows without a source compiler | Medium | Low | RGF-U1 models and validates tombstones; the later compiler owns allocation and sidecar persistence. |

## Consequences

- ADR 0011's namespaced strings remain valid only as opaque permanent component IDs, not as mutable
  type names.
- `ComponentSchema` and the persistent catalog must lose `rust_type_path`; native binding metadata
  retains it outside the wire model.
- `ComponentFieldId` becomes the durable patch and schema reference; `ComponentFieldPath` becomes a
  current value-layout locator.
- Existing built-in registrations must declare IDs, aliases, capabilities, and value locators
  explicitly before RGF-U1 freezes the registry.
- Existing bare scene/prefab/patch readers and the Rust-bound catalog are prototype shapes and must
  not survive as hidden version-1 compatibility readers.
- Dynamic non-Rust components, per-World dynamic `ComponentId`, `RuntimeSchemaRecord`, generated
  schema sidecars, scripting adapters, and adapter-specific hot replacement require separate,
  evidence-backed decisions.

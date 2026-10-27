# Persistent Rust Components

Rust is Nara's complete first-party game-authoring language. Ordinary runtime-only ECS data needs
only `Component`. Add `PersistentComponent` when a component must participate in versioned scene,
prefab, patch, inspection, or editor workflows.

## Declare a Persistent Component

```rust
use nara::prelude::{Component, PersistentComponent, Vec2};

#[derive(Component, PersistentComponent)]
#[nara(
    id = "example.Player",
    version = 1,
    alias = "Player",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Player {
    #[nara(id = "position", alias = "Position")]
    pub position: Vec2,
    #[nara(id = "hit-points", alias = "Hit points")]
    pub hit_points: i64,
}
```

The declaration is the single Rust source for the generated
`PersistentComponentProvider`, schema, decoder, and encoder. Do not duplicate a
`ComponentFieldSchema` table or hand-write a `ComponentValue` conversion for a supported type.

## Register Components from a Plugin

A component-owning plugin publishes one explicit owner/provider definition. Its source describes
the owner-local catalog without touching a `World`, registry, filesystem, network, or mutable
global state. The executable callback registers the native Rust binding into the private
owner-local candidate:

```rust
use nara::app::PluginSchemaProviderId;
use nara::reflect::{
    ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
    ComponentSchemaProviderDefinition,
};

const PLAYER_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("example.player.components");
const PLAYER_SCHEMA_PROVIDER: ComponentSchemaProviderDefinition =
    ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("example.player.components"),
        PLAYER_SCHEMA_PROVIDER_ID,
        ComponentSchemaProviderBindingId::new("example.player.components.native", 1),
        player_types::initial_catalog,
        player_types::register,
    );

nara::persistent_components! { pub mod player_types = [Player]; }
```

Expose this definition through the component-owning crate's `SchemaContribution`. The plugin
declaration lists `PLAYER_SCHEMA_PROVIDER_ID` in `provides_schema`; its runtime `build` method owns
runtime behavior rather than registering that provider again. Follow
[Rust Product Recipes](rust-product-recipes.md#persistent-schema-contributions) for the single
composition path shared by runtime Hosts and strict project tools.

Product composition loads and validates every trusted owner source before it runs selected native
callbacks. Each selected validation and registration callback runs once against a private
owner-local candidate. A failed source, callback, freeze, or merge leaves the aggregate registry
unchanged; a frozen registry rejects every mutation.

At App sealing, the registry owner checks the final installed `provides_schema` union against
actual provider receipts. Missing declarations or registrations reject before schema authority
is published. Several plugins may select the same provider, and a recipe may register it on
their behalf. Standalone `ComponentRegistry` users can still register and freeze directly.

Codec execution uses the `ComponentRegistrySnapshot` returned by `registry.snapshot()` after a
successful freeze. The building registry has no encode or preflight methods, and requesting its
snapshot returns `ComponentRegistryError::NotFrozen`. Freeze rejects missing native bindings before
publishing a snapshot. Retain one snapshot for a complete operation or batch so all component work
uses the same validated schema and native definition.

The inventory generates `schemas`, `initial_catalog` and `register` callbacks from one explicit
list of type identifiers. Import or alias qualified component types into the invocation module
before listing them. It performs no discovery or installation. `initial_catalog` is a generation-one
convenience only. When evolving a package, retain the old catalog snapshot, construct the current
catalog with `ComponentSchemaCatalog::successor_of`, fill its components from `player_types::schemas()?`,
and keep explicit migration registration around `player_types::register`. Adding a new current
type changes the inventory; it must not change historical catalog membership. Owner/provider IDs,
binding versions and migration meaning remain package decisions.

The [independent component consumer](../../tests/fixtures/module-consumer/tests/component_inventory.rs)
demonstrates a successor catalog, a new current component, explicit migration and the ordinary
Editor Create/Edit/Save/Reopen workflow using one product contribution.

## Data-Only Owner Metadata

Schema tools can admit an already-owned owner ID, current catalog and optional predecessor with
`ComponentSchemaOwnerSnapshot::admit`, without `serde` or a native codec. This reuses the ordinary
catalog/lineage validation and retains no executable callbacks. File/transport callers must apply
their input/work limits first; bounded `AuthoringSchemaBundle` loading does so and exposes these
same admitted owner snapshots through `owners()`.

Cloning a snapshot retains its exact admission identity (`ptr_eq`). Re-admitting equivalent data
does not. Bundle equality remains persistent-content equality, including lineage, not runtime
authority. Converting a snapshot to `AuthoringSchemaBundleOwner` copies content into an unpublished
candidate and does not transfer that identity. The [backend-free
tests](../../crates/nara_reflect/tests/schema_owner.rs) cover these distinctions. Metadata admission
alone does not install dynamic Scene storage, a language module or an executable binding; ordinary
native materialization still requires its complete frozen native registry.

For source compilation, `SceneComponentBindings::new(native_snapshot, data_owners)` explicitly
selects disjoint native and data catalogs. Duplicate owners and overlapping active/retired type
claims reject, including predecessor declarations. `new_with_limits` bounds the data-owner and
declaration indexes. Data records need a matching owner and the current version; a missing native
binding never chooses this family implicitly.

Use the selection's `expand_prefabs` or `try_expand_prefabs_with_loader` for the same production
compiler with data Schemas. Defaults, references, declarative overlays, source demand and bounded
work retain their existing semantics. See the [public compiler
tests](../../crates/nara_scene/tests/component_bindings.rs).

For runtime storage, use the selection's `prepare_scene` or `prepare_scene_replacement`; each
returns the same move-only `PreparedScene` token as native preparation. The complete native/data
candidate preflights before target allocation and commits through one guarded publication tail.
`prepare_scene_with_limits` and `prepare_scene_replacement_with_limits` apply one aggregate
logical allowance across both families. Data Schemas do not acquire independent Rust types.

For an ordinary product, pass owner snapshots through `SchemaContribution::data_for_default`,
`data_configured` or a native contribution's `with_data_owners`. `ProductRecipe` installs the Scene
selection and carries the same admissions into content checking and runtime publication; see
[Product Recipes](rust-product-recipes.md#persistent-schema-contributions).

An advanced direct App without a recipe installs `SceneComponentBindingsPlugin::new(data_owners)`
alongside the native registry plugin. Obtain `scene_component_bindings(world)` at
`StartupStage::Scene` or a later safe point. The plugin captures the actual frozen native definition
at seal. Independently re-admitted catalogs with the same bytes cannot replace that installed
authority. Several engine domain guards coexist at the existing App/runtime boundaries.

```rust,ignore
let bindings = scene_component_bindings(world).ok_or(MissingSceneDefinitions)?.clone();
let report = bindings.prepare_scene(world, candidate, occurrence, database)?.commit(world);
let current = report.instance.ok_or(report.diagnostics)?;

for (component, record) in bindings.data_components(world, entity)? {
    inspect(component, record);
}
bindings.set_data_component(world, entity, component, value, database, limits)?;
bindings.remove_data_component(world, entity, component)?;
```

Guarded reads distinguish absent members from unavailable definitions or stale targets. Full-value
replacement and removal preserve siblings and validate the complete retained group, exact
owner/version, declared references and current Scene identity before mutation. Runtime writes
change the live carrier, not source documents or authoring history. Access and change tracking are
currently at carrier granularity; this is not per-Schema ECS query/scheduler presence.

`bindings.export_scene` snapshots all current live registered Scene identities, flattening Prefabs.
`export_scene_with_options` additionally accepts explicit project metadata for `Path` or
`StableIdWhenKnown` asset lowering. Native free functions reject encountered data members rather
than omit them; `export_scene_with_asset_database` provides the same metadata lowering for a
native-only caller, including assets supplied by defaults after encoding. Managed native export
uses the installed native definition. The independent preview publication exception does not
transfer managed export authority. Source Save remains the authoring document owner's operation.

The carrier retains only data and exact owner-local Schema snapshots. Strict entity/instance
retirement releases it without a hook exemption. A product owning native retirement work can use
the advanced mixed claims preparation method and retain its existing commit/retry/discard custody.
Plain data materialization does not install a language module, prove gameplay callbacks available,
or replace the product recipe's required-contribution checks. Those execution-owner integrations
remain separate from this storage family.

## Prepare And Materialize A Scene

`spawn_scene(world, registry, candidate, occurrence, database)` is the immediate convenience. Use
`prepare_scene` when a caller needs to hold actual native values before deciding to publish:

```rust,ignore
let prepared = prepare_scene(world, registry, candidate, occurrence, database)?;
// Keep preparation across other compatible World work, or cancel with drop(prepared).
let report = prepared.commit(world);
let current = report.instance.ok_or(report.diagnostics)?;

let replacement = prepare_scene_replacement(world, registry, successor, &current, database)?;
let report = replacement.commit(world);
```

Both operations are exported from `nara::prelude` and `nara_scene`. The required
`ExpandedSceneCandidate` binds its containing source to immutable expanded content and provenance.
First placement declares a World-local `SceneOccurrenceKey`; replacement obtains that slot from
its exact predecessor. A receipt is not permission to replace a later activation of the same slot.

The move-only `PreparedScene` owns decoded native values and a frozen registry snapshot, not
runtime Scene entities. Preparation can register ECS metadata and flush ordinary deferred work;
it does not reserve a slot or runtime instance number. Dropping the token cancels private work,
even after dropping its World. Commit consumes it once, rechecks current facts and never reruns
codecs. A failed commit leaves the predecessor active and publishes no candidate. Native
destructors and checked process-local identity allocation retain their ordinary effects.

Retained preparation rejects a foreign World before flushing it, an occupied slot or ended
predecessor, incompatible managed Schema authority, late unsupported lifecycle work, and a
changed asset catalogue if preparation read or allocated asset identities. Unrelated compatible
publications and asset changes that the batch did not read do not automatically invalidate it.
The standalone registry variable may be dropped after preparation; a managed World must retain
the exact admitted definition and its authority. This is not a promise to retain asset payload
residency or every ProjectAssetDatabase revision. The advanced native-retirement integration owns
target-bound scratch entities and still requires its explicit `discard(world)` lifecycle.

In an `App`, materialize initial Scenes from `StartupStage::Scene`, after the managed definition
has been sealed. Read `component_registry(world)`, obtain its frozen `snapshot()`, and use
`ComponentRegistry::from_snapshot` for the Scene operation. Do not substitute a separately frozen
registry during App configuration, even for an empty Scene. The
[interaction lab](../../crates/nara_ui/examples/interaction_lab/fixture.rs) demonstrates this path
and publishes its entity handles only after successful materialization.

The [independently compiled consumer](../../tests/fixtures/module-consumer/tests/scene_preparation.rs)
exercises prepare/cancel, reverse-order commits, exact replacement, and retained-state rejection.

## Declare Numeric Constraints

Declare finite scalar bounds once; Editor feedback, data-only validation and native preflight use
the same schema rule:

```rust
#[derive(Component, PersistentComponent)]
#[nara(id = "example.Cooldown", version = 1, alias = "Cooldown",
    component_capabilities(scene, inspect, edit), field_capabilities(scene, inspect, edit),
    authoring_initial)]
pub struct Cooldown {
    #[nara(id = "seconds", alias = "Seconds", initial = 0.5_f32,
        range(min_inclusive = 0.0_f32, max_inclusive = 30.0_f32))]
    pub seconds: f32,
}
```

Each endpoint is optional and may be inclusive or exclusive. Bounds are numeric literals checked
against the field's Rust type, including aliases; expressions, nonnumeric fields and container
range attributes are rejected. Handwritten recursive scalar shapes can use `ComponentNumericInterval`.
F32 validation compares the materialized F32 value without rewriting saved bytes. Empty intervals
and nonfinite/out-of-domain bounds reject at catalog admission. These are validation constraints,
not slider hints. Cross-field/domain rules still belong to the native codec.

Adding, changing or removing a constraint requires a component version successor and explicit
catalog lineage/migration. Preserve valid historical values or reject them for repair; never
silently clamp authored data. A successful data-only edit still does not prove every native invariant.

For native domain failures, return `ComponentDomainError::new(checked_code, checked_summary)` and
optionally `.with_field(ComponentFieldId::new("seconds"))`. Checked `DiagnosticCode` and
`SafeSummary` are static public facts. The Scene bridge retains them and publishes the field only
when an explicitly admitted schema proves membership. Keep payloads, nested map keys, paths and
locators in private detail, never in codes or summaries. Without a schema the safe code remains,
but the field claim is omitted. Existing free-form codec errors retain their redacted fallback.

## Manual Codecs and Apply Preparation

Use the derive for supported Rust field types. When a domain needs a hand-written codec,
`register_persistent_component_with_codec` takes a decoder returning the native component and an
encoder borrowing that component:

```rust
use nara::prelude::Component;
use nara::reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};

#[derive(Component)]
struct Health {
    current: i64,
}

fn register_health(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("example.Health"),
        "Health",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("current"),
        "Current",
        ComponentFieldPath::from_fields(["current"]),
        ComponentValueKind::I64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);

    registry.register_persistent_component_with_codec::<Health, _, _>(
        schema,
        |value| {
            let current = value
                .get("current")
                .and_then(ComponentValue::as_i64)
                .ok_or_else(|| ComponentCodecError::invalid_field("current", "i64"))?;
            Ok(Health { current })
        },
        |health| {
            Ok(ComponentValue::map([(
                "current",
                ComponentValue::I64(health.current),
            )]))
        },
    )?;
    Ok(())
}
```

After registration, execute through the frozen snapshot:

```rust
let mut registry = ComponentRegistry::new();
register_health(&mut registry)?;
registry.freeze()?;
let execution = registry.snapshot()?;

let component_id = ComponentTypeId::new("example.Health");
let source = ComponentValue::map([("current", ComponentValue::I64(42))]);
let prepared = execution.preflight_component(&component_id, &source)?;
let mut world = nara::ecs::World::new();
let entity = world.spawn_empty().id();
prepared.apply(&mut world, entity)?;
let encoded = execution.encode_component(&component_id, &world, entity)?;
```

Preflight returns a prepared component or an explicit error. Encoding returns `Ok(None)` only
when the entity or selected native component is absent. Unknown schema, missing native binding and
codec failures return errors. A retained snapshot keeps its exact definition; it does not certify
the current target World's lifecycle topology, asset baseline or installed runtime authority.

The same registration and a frozen-snapshot decode, guarded apply, and native-value encode round
trip compile in the independent
[`manual_component_codec` consumer](../../tests/fixtures/module-consumer/tests/manual_component_codec.rs).
Production composition places this registration in the provider callback described above.

For fallible delayed preparation, use `register_persistent_component_codec` or its
`_with_context` variant instead. These lower-level APIs return a `PreparedComponentCandidate<T>`
from preflight for the registered component `T`. The encoder receives `&T` and returns a
`ComponentValue`; `_with_context` additionally supplies `&World` and `&ComponentEncodeContext`.
The registry selects the component and handles absence before invoking the encoder.
Choose the candidate constructor by the work it can perform:

- `insert(value)` for a fully decoded component;
- `deferred(|| ...)` for fallible delayed preparation that cannot access target-World resources;
- `with_asset_server(|context| ...)` only when apply-time work may resolve an `AssetRef` through
  `ComponentApplyContext`.

The distinction is enforced by the closure signature. Do not use the asset-server constructor for
an asset-free value: fresh-scene admission must know every resource insertion that may occur before
it allocates targets. A candidate becomes a `PreparedComponent` only through a frozen snapshot;
game code never constructs the bound apply value directly.

Persistent components describe the complete stored component set. They may not declare Bevy
`#[require]` metadata or intrinsic component hooks. A target `World` that later registers required
components, lifecycle hooks, or matching lifecycle observers rejects the affected persistent apply
before its first persistent mutation. Runtime-only components may continue to use those Bevy
features normally.

## Stable IDs, Aliases, Versions, and Tombstones

- Component and field `id` values are opaque persistent identities. Never derive them from Rust
  paths, Rust field names, or display text, and never reuse them.
- `alias` values are user-facing names. They may change while the stable ID remains unchanged.
- A semantic or persistent layout change increments the component `version` and supplies the
  required migration chain.
- A removed field keeps its ID in `tombstone = "..."`. Removed component IDs remain catalog
  type tombstones. Tombstones cannot be reactivated.
- Durable field patches use `ComponentTypeId + ComponentFieldId + ComponentSchemaVersion`.
  `ComponentFieldPath` is only the current value locator.

The committed reference-game lineage is preserved by
[`component-schema-v1.json`](../../reference-game/schema/component-schema-v1.json),
[`component-schema-v2.json`](../../reference-game/schema/component-schema-v2.json), and
[`component-schema-v3.json`](../../reference-game/schema/component-schema-v3.json).
Its tests prove that every predecessor remains loadable, field deletion requires both a versioned
migration and a tombstone, and an unversioned semantic change, missing tombstone, or ID reactivation
cannot freeze.

## Capabilities

Capabilities are explicit eligibility gates, not behavior implementations:

- `scene`: eligible for scene/prefab documents and patches;
- `inspect`: eligible for local tooling inspection;
- `edit`: eligible for editor or authoring commands;
- `entity_ref`: required exactly when the field's recursive shape contains `EntityReference`;
- `asset_ref`: required exactly when the field's recursive shape contains `AssetRef`.

Save, animation, replication, scripting, diagnostics, and remote disclosure remain
domain-owned contracts and are not inferred from these capabilities.

## Supported Field Types

The current native Rust authoring slice supports:

- `bool` and owned `String`;
- `i8`, `i16`, `i32`, and `i64`;
- `u8`, `u16`, `u32`, and `u64`;
- finite `f32` and `f64`;
- `Vec2`;
- `EntityReference`, with the `entity_ref` field capability;
- `AssetRef`, with the `asset_ref` field capability;
- recursive `Vec<T>`, `BTreeMap<String, T>`, and `Option<T>` over these supported types.

Narrow integer codecs check their declared range before conversion. Float codecs reject non-finite
values and `f32` overflow; ordinary finite `f32` rounding is allowed. Integer wire kinds remain
`I64` and `U64`, and both float types use the finite `F64` wire kind.

Type aliases and qualified type paths use ordinary Rust trait resolution. `Option<T>` has a
required stored root and encodes `None` as explicit `Null`; it does not permit an absent root.
Nested numeric domains apply to each element. Reference capabilities belong to the stable root
field, while traversal reports the concrete nested value path. `Vec2` declares a closed record
with required `x` and `y` finite-F32 members stored as F64 wire scalars. The same shape works in
`Vec<Vec2>`, `Option<Vec2>` and supported maps. Missing axes, extra members and F32 overflow reject
in both data-only authoring and native decoding; the shape is never inferred from example values.

If an existing owner used the older opaque-map Vec2 helper, retain its historical catalog and
advance each affected component's version, including components containing Vec2 in containers.
Publish the next owner catalog with the exact predecessor fingerprint and register an explicit
old-to-new component migration. A migration may preserve the x/y value unchanged when it already
satisfies the closed shape; the successor's schema validation still rejects malformed values.
Regenerate the current authoring bundle and prove saved predecessor data through strict Check
before explicitly saving migrated documents. Do not rewrite historical catalogs or merely replace
their fingerprints. The Reference Game's InitialVelocity2d v1-to-v2/catalog-v5 successor is a
concrete example. Provider owner candidates inherit existing type tombstones from their exact
predecessor; a registration callback declares newly retired IDs rather than re-declaring retained
tombstones on every successor.

`HashMap`, non-string map keys, enums, tuple structs, and arbitrary nested Rust structs remain
unsupported. Manual schemas can declare closed records with `ComponentValueShape::record`:
every declared member must be present, optional members encode `Null`, and extra members reject.
The catalog and authoring-bundle envelopes use version 3; versions 1/2 are rejected. Scene, Prefab,
and Patch formats retain their own versions. The unreleased R5a metadata reset preserves historical
component definitions/versions and migration chains, while re-encoding catalogs and recomputing
their predecessor fingerprints. It is not permission to rewrite historical component semantics.

Schema-description limits bound metadata; Scene, Prefab, Patch, and reference traversal own the
runtime value work budgets. Direct in-memory validation does not grant file-ingest or allocation
admission. Opaque collections retain their native semantics and do not acquire a hidden 16K-item
limit from the schema description.

Prefab materialized-value limits count the resulting product. Its private reference-work allowance
also accounts for retained root IDs and paths: at most twice the configured value-node allowance,
and twice its byte allowance plus a per-node allowance for one root ID, depth-bounded indices,
and persistent UUID text. Repeated long map-key prefixes can exceed this work ceiling even when
the product fits; traversal rejects before publishing a remap or invoking rewrite callbacks.

Data-only Add Component creation is an explicit opt-in, separate from decoding existing data.
Add `authoring_initial` to the component's `#[nara(...)]` declaration and an
`initial = <literal>` to every field. The current derive admits scalar boolean, string, integer,
and finite float literals (including negative numeric literals), not `Vec2`, entity references,
callbacks, or aggregate expressions. Missing fields in a stored component still fail decoding;
creation metadata does not supply decode defaults.

The Inspector shows `Reset to default` for a field with a declared missing-field default and
`Remove field` for an optional field without one. Both remove an explicitly stored map-member
override; they do not store a copy of the default. Already omitted fields and required fields
without defaults cannot be removed. Root-value and terminal list-index removal are unavailable.
Explicit `Null` is a stored value and differs from omission. Undo restores the exact authored
value, including Null, and Save/Reopen preserves the omitted key. The same Scene/Prefab document
owner validates data-only field edits and their inverses without loading project code.

Hosts that prepare persistence away from the document owner can capture
`session.authoring_readmission()` alongside their intended document snapshot. The owned
`AuthoringDocumentReadmission` retains the exact immutable schema/source basis and unavailable
record expectation without retaining the working session or its history. Re-admit encoded bytes
through the matching Scene/Prefab candidate method and check `preserves_scene` / `preserves_prefab`
before publication. This validates preservation; it grants no permission to replace a Workspace
document or mark it saved. Those operations still require the original owner and persistence receipt.

## Complete Prefab Conversion

With `nara_scene/serde`, an admitted Scene or Prefab session can prepare complete conversion of
one directly owned instance anchor. The source owner allocates fresh local IDs, materializes the
selected effective values, rewrites declared local references and removes all Prefab links in
that selected projection. It preserves the anchor's own records, ordinary source children and
unrelated instances. Selected unavailable data or unresolved references refuse atomically.
Copying current semantic data requires its admitted Scene adapter and matching Schema owner;
it does not require a creation template, Edit capability or native component codec.

```rust,ignore
let request = session.request_complete_prefab_conversion(
    session.revision(),
    &anchor,
    PrefabExpansionOptions::default(),
)?;
// The owned request can move to an existing background task.
let prepared = request.prepare_with_loader(None, |reference, working| {
    load_current_authoring_prefab(reference, working)
})?;
review(prepared.document(), prepared.entities());
let receipt = session.commit_complete_prefab_conversion_with_loader(
    prepared,
    |reference| reload_current_authoring_prefab(reference),
)?;
assert!(receipt.report().applied);
```

Scene requests produce `PreparedScenePrefabConversion`; Prefab requests produce
`PreparedPrefabConversion`. Each move-only token keeps its exact candidate, typed
`LocalizedPrefabEntity` mapping and all actually demanded semantic source snapshots through
review. `effective_asset_references()` observes typed references, including materialized
defaults, for the selected result. It does not load resources. Dropping a request or candidate
publishes no source mutation or history entry.

Commit requires the original source incarnation/revision and exact Schema admission. Its loader
must resolve every original request currently: a changed canonical key, body, authority or missing
dependency rejects the entire candidate. `PrefabConversionError::Loader` preserves the provider's
concrete error. Dependencies needed only for selected structural meaning remain part of this
check even when no entity from them survives. Unrelated broken instances are not loaded.

A filesystem or Editor Host additionally owns current file/metadata receipts, stable-lookup
ambiguity and file-versus-working-copy selection. Semantic snapshot checks alone do not prove
those physical facts. The Host must retain that evidence with the same reviewed token and
revalidate it before publication. Native preview selection supplies no conversion authority.

An accepted conversion occupies one entry in the existing source history. Undo/Redo replays
the exact admitted records and declaration without loading a newer Prefab or generating new
IDs. Encode and persist through the existing nominal Scene/Prefab Save workflow; Reopen starts
a new source incarnation and invalidates earlier conversion tokens.

The independent consumer is
`tests/fixtures/module-consumer/tests/prefab_conversion.rs`. It exercises direct Scene and renamed
root Prefab exports, background preparation, reference remapping, exact replay and disk Reopen.
The integrated Editor's Host operation, paged review and correlated result are described in
[the panel guide](editor-panels.md#convert-a-complete-instance).

## Stable Nested Properties

Manual semantic Schemas can opt into nested identity without reflecting a Rust type. Use
`record_members` and explicit member IDs; plain `record` remains appropriate when only whole-root
replacement is needed. Member IDs are unique among siblings, not across a component: the same
identified value shape may be reused under multiple stable parents.

```rust
use nara::reflect::{
    ComponentFieldId, ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyStep,
    ComponentShapeMember, ComponentValueKind, ComponentValueShape,
};

let items = ComponentValueShape::keyed_list(
    ComponentValueShape::record_members([
        ComponentShapeMember::new("wire_id", ComponentValueKind::String)
            .with_id(ComponentFieldId::new("item-id")),
        ComponentShapeMember::new("health", ComponentValueKind::U64)
            .with_id(ComponentFieldId::new("hp")),
    ]),
    ComponentFieldId::new("item-id"),
);
let health = ComponentPropertyPath::new(ComponentFieldId::new("inventory"))
    .with_steps([
        ComponentPropertyStep::ListItem {
            by: ComponentFieldId::new("item-id"),
            key: ComponentPropertyKey::String("potion".to_owned()),
        },
        ComponentPropertyStep::Member(ComponentFieldId::new("hp")),
    ]);
```

The root field's ID/path/shape/capabilities still come from its admitted `ComponentFieldSchema`.
Keyed items support exact String/I64/U64 identities; duplicate keys reject. Reordering does not
change a selector, but changing the key member policy rejects old `by` selectors. A reused key
asserts identity continuity; Reflect does not invent historical item tombstones. Unkeyed list
indices are not durable selectors. Map entries use separately tagged exact string keys.

`resolve_component_property` reads an existing target only in a current-version value. It does not
choose a catalog, migrate data, materialize omitted defaults, create optional containers, or grant
mutation authority. Retired member declaration paths stay in the root field's `member_tombstones`;
retiring a parent protects descendants, and the metadata survives intermediate shape changes.
Wire renames still require explicit component value migration while retaining semantic IDs.

Use `resolve_component_property_with_limits` when a document or compiler owner admits a value
larger than the standalone arbitrary-value profile. It accepts that owner's
`ComponentValueMaterializationLimits` for logical node, byte and depth admission while retaining
the same selector and complete value semantics. Schema/selector admission remains independent.
Keep typed resource refusals distinct from invalid selectors and replacement values; a budget
failure does not establish a conflicting source intent.

For editing, pass ordered `AuthoringPropertyEdit` replacements to
`prepare_authoring_property_edit` on a Scene/Prefab session, Workspace, project session or Editor
Application, then use the existing `commit_authoring_edit`. The owner coalesces writes to each root
and creates the ordinary inverse/history entry. Original operation count, selector bytes and
replacement payload are admitted before coalescing; final root patches and inverses retain their
existing limits. Exact source revision/Schema checks, unavailable-record preservation and one Undo
remain unchanged. This path replaces existing stored targets; root omission/default reset still
uses the existing root-field operations. It is not a complete document/Prefab-occurrence address
and remains separate from the declarative Prefab overlay namespace and actual composition.

Workspace, Project Session and Application preparation takes the complete retained
`EditorDocumentQueryBasis`, including exact Schema admission. Source-local Scene/Prefab
preparation takes its source revision. Retain the original observation when preparing later;
capturing a new basis cannot authorize values planned from an older one.

Resolution failures retain `operation-index`, a static `property-error-kind`, and a
`property-step-index` when the failing selector step is known. Tools can distinguish missing
optional containers, retired targets and changed list identity policy without reparsing values.
These diagnostics do not publish actual map/list keys or replacement payloads.

The first-party inline Inspector collection controls still support scalar items only. A keyed
Record list is available through the public property-edit API, not those inline controls; this
Schema capability does not imply a new collection UI.

## Runtime-Only Components

```rust
use nara::prelude::Component;

#[derive(Component)]
pub struct RuntimeCache {
    pub frame_generation: u64,
}
```

Runtime-only components do not enter the persistent catalog and need no stable IDs, codecs,
capabilities, versions, or tombstones.

## Dependency Renaming and Crate Overrides

`PersistentComponent` resolves a direct `nara` or `nara_reflect` dependency, including renamed
packages. The ECS derives resolve a direct `nara` or `nara_ecs` dependency, such as
`engine = { package = "nara", ... }` or `substrate = { package = "nara_ecs", ... }`.
The supported ECS derives are `Component`, `Resource`, `Bundle`, `SystemParam`, `ScheduleLabel`
and `SystemSet`. A root-only consumer can import Component/Resource/Bundle/SystemParam from
`nara::prelude`; schedule labels and sets are available through `nara::ecs`. Direct `nara_ecs`
consumers use its root or prelude. Module paths `nara::ecs::bundle::Bundle` and
`nara::ecs::system::SystemParam` expose the same facade-aware derives. No direct Bevy dependency
or `use nara::ecs as bevy_ecs` alias is required.

The [renamed-root fixture](../../tests/fixtures/derive-dependencies/renamed-root/src/lib.rs) uses
only the renamed root dependency. The shared
[derive contract](../../tests/fixtures/derive-dependencies/facade_contract.rs) covers generic and
nested bundles, parameter lifetimes, const generics, field attributes and deferred commands through
both root and direct-ECS fixtures. `tests/derive_dependency_fixtures.rs` owns their independent
locked compilation/execution checks. These selected derives do not promise full Bevy API compatibility.

Use `#[nara(crate = "path::to::reexport")]` for `PersistentComponent` only when a custom
re-export prevents normal direct dependency discovery; it is not an ECS derive path attribute.

## Catalog Lineage and Schema Evolution

The derive emits one native provider. It does not publish catalogs, mutate a frozen registry,
select a predecessor, synthesize migrations, allocate tombstones, or implement dynamic non-Rust
storage. Those responsibilities remain explicit in `ComponentRegistry` and the owning project or
plugin workflow.

See [`reference-game/src/components.rs`](../../reference-game/src/components.rs) for
production-shaped declarations and [`reference-game/tests/authoring.rs`](../../reference-game/tests/authoring.rs)
for registry, canonical scene, stable patch, and live-world round trips.

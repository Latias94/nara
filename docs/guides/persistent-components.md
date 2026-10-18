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
    ComponentRegistry, ComponentRegistryError, ComponentSchemaCatalog,
    ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
    ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
    PersistentComponentProvider,
};

const PLAYER_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("example.player.components");
const PLAYER_SCHEMA_PROVIDER: ComponentSchemaProviderDefinition =
    ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("example.player.components"),
        PLAYER_SCHEMA_PROVIDER_ID,
        ComponentSchemaProviderBindingId::new("example.player.components.native", 1),
        player_schema_catalog,
        register_player_components,
    );

fn player_schema_catalog(
) -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![Player::persistent_component_schema()],
        ..ComponentSchemaCatalog::default()
    })
}

fn register_player_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component::<Player>().map(|_| ())
}
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

Only a frozen registry may turn codec output into an applicable component. A building registry
returns no runtime preflight result, so plugin preparation cannot publish a value before the final
schema, native binding, and stable identity are fixed.

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

The same registration and a frozen-registry decode, guarded apply, and native-value encode round
trip compile in the independent
[`manual_component_codec` consumer](../../tests/fixtures/module-consumer/tests/manual_component_codec.rs).
Production composition places this registration in the provider callback described above.

For fallible delayed preparation, use `register_persistent_component_codec` or its
`_with_context` variant instead. These lower-level APIs return a `PreparedComponentCandidate`
from preflight and encode from a `World` and entity (plus codec context for `_with_context`).
Choose the candidate constructor by the work it can perform:

- `insert(value)` for a fully decoded component;
- `deferred(|| ...)` for fallible delayed preparation that cannot access target-World resources;
- `with_asset_server(|context| ...)` only when apply-time work may resolve an `AssetRef` through
  `ComponentApplyContext`.

The distinction is enforced by the closure signature. Do not use the asset-server constructor for
an asset-free value: fresh-scene admission must know every resource insertion that may occur before
it allocates targets. A candidate becomes a `PreparedComponent` only through a frozen registry;
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
field, while traversal reports the concrete nested value path. `Vec2` retains its existing opaque
map schema and native codec; its member layout is not inferred from example values.

`HashMap`, non-string map keys, enums, tuple structs, and arbitrary nested Rust structs remain
unsupported. Manual schemas can declare closed records with `ComponentValueShape::record`:
every declared member must be present, optional members encode `Null`, and extra members reject.
The catalog and authoring-bundle envelopes use version 2; Scene, Prefab, and Patch formats retain
their own versions. Historical component versions, stable IDs, and migration chains remain intact.

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

The derives resolve renamed direct dependencies such as
`engine = { package = "nara", ... }` and
`substrate = { package = "nara_ecs", ... }`. The root integration test
`tests/derive_dependency_fixtures.rs` compiles both independent locked fixtures.

Use `#[nara(crate = "path::to::reexport")]` only when a custom re-export prevents normal direct
dependency discovery.

## Catalog Lineage and Schema Evolution

The derive emits one native provider. It does not publish catalogs, mutate a frozen registry,
select a predecessor, synthesize migrations, allocate tombstones, or implement dynamic non-Rust
storage. Those responsibilities remain explicit in `ComponentRegistry` and the owning project or
plugin workflow.

See [`reference-game/src/components.rs`](../../reference-game/src/components.rs) for
production-shaped declarations and [`reference-game/tests/authoring.rs`](../../reference-game/tests/authoring.rs)
for registry, canonical scene, stable patch, and live-world round trips.

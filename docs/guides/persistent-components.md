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

A component-owning plugin validates registration during read-only `preflight` and performs it
during `build`:

```rust
registry.validate_persistent_component::<Player>()?;
registry.register_persistent_component::<Player>()?;
```

The registry owner freezes the complete catalog during plugin `finish`. A failed registration or
freeze leaves the building candidate repairable; a frozen registry rejects every mutation.

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

The committed predecessor catalog for the reference game is
[`reference-game/schema/component-schema-v1.json`](../../reference-game/schema/component-schema-v1.json).
Its tests prove that rename/deletion lineage remains valid and that an unversioned semantic change,
a missing tombstone, or ID reactivation cannot freeze.

## Capabilities

Capabilities are explicit eligibility gates, not behavior implementations:

- `scene`: eligible for scene/prefab documents and patches;
- `inspect`: eligible for local tooling inspection;
- `edit`: eligible for editor or authoring commands;
- `entity_ref`: required on an `EntityReference` field.

`asset_ref` is part of the canonical catalog vocabulary but is not admitted by the current derive
tracer. Save, animation, replication, scripting, diagnostics, and remote disclosure remain
domain-owned contracts and are not inferred from these capabilities.

## Supported Field Types

The current native Rust authoring slice supports:

- `i64`;
- `u64`;
- `Vec2`;
- `EntityReference`, with the `entity_ref` field capability.

Generic fields, collections, enums, tuple structs, `asset_ref`, and arbitrary nested Rust types are
rejected with compile-time diagnostics until a production consumer admits their schema and codec
contract.

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

See [`reference-game/src/components.rs`](../../reference-game/src/components.rs) for four
production-shaped declarations and [`reference-game/tests/authoring.rs`](../../reference-game/tests/authoring.rs)
for registry, canonical scene, stable patch, and live-world round trips.

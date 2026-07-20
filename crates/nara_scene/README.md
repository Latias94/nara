# Direct `nara_scene` consumption

`nara_scene` can be used directly for the supported scene parse, validation, and spawn slice. The
consumer owns its registry and ECS world; it does not need the integrated root `nara` facade.

The checked-in `module-consumer/Cargo.toml` uses these exact paths from its location at the
repository root:

```toml
[dependencies]
nara_reflect = { path = "../crates/nara_reflect" }
nara_scene = { path = "../crates/nara_scene", default-features = false, features = ["serde"] }

[dev-dependencies]
bevy_ecs = "0.19"
```

- `nara_scene` owns the canonical scene file candidate, scene component registration, stable scene
  identity, and spawn operation. Enable `serde` to decode canonical scene files.
- `nara_reflect` supplies the caller-owned `ComponentRegistry`. Register the required component
  providers and freeze the registry before publishing, validating, or spawning a scene.
- `bevy_ecs` supplies the caller-owned `World`. Its version must match the version supported by the
  selected `nara_scene` release.

An external project should select the corresponding published, Git, or local package locations;
the package names, `nara_scene/serde` feature, and direct prerequisite roles remain the same. The
checked-in [`module-consumer`](../../module-consumer/) workspace is the executable contract. It uses
its own manifest and lockfile, parses a committed RON scene, validates it against a frozen registry,
and spawns it into a fresh world.

This evidence covers the documented direct scene-module workflow only. It does not promise
arbitrary cross-engine compatibility, arbitrary combinations of Nara modules, or a stable dynamic
ABI. Other module workflows must document and prove their own prerequisites.

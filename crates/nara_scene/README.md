# Direct `nara_scene` consumption

`nara_scene` can be used directly for the supported scene parse, validation, and spawn slice. The
consumer owns its registry and ECS world; it does not need the integrated root `nara` facade.

The checked-in `tests/fixtures/module-consumer/Cargo.toml` uses these paths relative to its
fixture directory:

```toml
[dependencies]
nara_reflect = { path = "../../../crates/nara_reflect" }
nara_scene = { path = "../../../crates/nara_scene", default-features = false, features = ["serde"] }

[dev-dependencies]
bevy_ecs = "0.20"
```

- `nara_scene` owns the canonical scene file candidate, scene component registration, stable scene
  identity, and spawn operation. Enable `serde` to decode canonical scene files.
- `nara_reflect` supplies the caller-owned `ComponentRegistry`. Register the required component
  providers and freeze the registry before publishing, validating, or spawning a scene.
- `bevy_ecs` supplies the caller-owned `World`. Its version must match the version supported by the
  selected `nara_scene` release.

An external project should select the corresponding published, Git, or local package locations;
the package names, `nara_scene/serde` feature, and direct prerequisite roles remain the same. The
checked-in [`module-consumer`](../../tests/fixtures/module-consumer/) workspace is the executable contract. It uses
its own manifest and lockfile, parses a committed RON scene, validates it against a frozen registry,
spawns it into a fresh world, and ends the exact instance through the Scene owner.

## Instance Lifetime

Retain the successful `SpawnedSceneInstance` receipt. Use
`retire_and_despawn_scene_instance(&mut world, &instance)` to unload that activation and remove
its remaining runtime entities. It also ends an empty instance, including one whose entities were
individually retired. Deleting the final entity or dropping the receipt does not end the instance.

The operation rejects foreign or stale receipts and unsupported native lifecycle work before
Scene retirement. Owning native domains must complete their cleanup first; the function does not
run arbitrary destruction callbacks. Pending World commands are flushed before admission, so
rejection does not promise to undo those commands. Ordinary identity history remains retained;
ending one instance does not reset the World domain or affect independently resident instances.

`SceneAuthoringSession::clear_live_world` uses this same retirement path while updating its session
receipt. A `SceneProjection` owner instead uses `SceneProjection::clear`, which additionally retires
its complete disposable domain epoch. Do not end their retained receipts behind those owners.

This evidence covers the documented direct scene-module workflow only. It does not promise
arbitrary cross-engine compatibility, arbitrary combinations of Nara modules, or a stable dynamic
ABI. Other module workflows must document and prove their own prerequisites.

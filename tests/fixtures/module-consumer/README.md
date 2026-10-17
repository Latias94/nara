# Independent module consumers

This workspace exercises public Nara modules without importing engine-private implementation
details. `tests/domain_reconciliation.rs` is a small native-effect plugin consumer; its support
module contains the complete domain implementation.

It owns its manifest and lockfile independently of the engine workspace. This catches missing
direct dependencies, accidental workspace inheritance and unsupported facade shortcuts.
The filesystem, project-workflow, material and editor-panel members exercise their own declared
public surfaces; only the project-workflow member uses the root facade for that workflow.

Run from the repository root, using the shared `target` directory through `CARGO_TARGET_DIR`:

```text
cargo nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --workspace --test-threads=1
```

Run Cargo commands serially. See the [fixture guide](../README.md) for isolation and platform scope.

`editor-panel-consumer/` independently compiles a trusted selection-summary panel.
It installs a semantic descriptor and a Dear ImGui painter through the same paths as built-ins,
reads selection and emits existing navigation actions without mutable document or GPU access.
Its default integration test emits an action through the frontend and preserves the dirty source
document, selection and history across panel close and reopen, with independent duplicate instances.
This is static trusted source composition, not a dynamic plugin ABI or project-time code loader.
Only this Editor consumer selects GUI dependencies; runtime and headless module consumers do not.

## Persistent configuration and native reconciliation

The plugin registers a plain persistent `Configuration` component and ordinary startup/update
systems. That same component is its runtime configuration; native ownership alone needs a separate
`NativeState`. `NativeState` lives on the configuration entity, declares a required component, and
creates another transient component through `on_add`.

The domain owns these phases:

1. Decode and validate a candidate through `SceneDocumentCandidate` and accept it through
   `SceneAuthoringSession::try_replace_file_candidate`. Rejection preserves the accepted document,
   revision, and existing native state.
2. Mark the accepted revision pending, then retire the old native owner and its required and
   hook-created components. This is an ordinary domain operation with real lifecycle effects.
3. Call `SceneAuthoringSession::sync_world` to publish the pure configuration projection through
   the existing strict scene transaction.
4. Create the native state and mark that exact accepted revision ready. A creation failure keeps
   the new document accepted and reports its revision unavailable. An explicit Retry reconstructs
   the state; idle frames do not retry it.

Replace and Remove use the same retirement phase. The plugin also owns physical cleanup during
managed runtime shutdown. Failed startup returns a retained failure owner and never publishes a
`RuntimeInstance`.

Native cleanup cannot restore arbitrary earlier effects. If projection or creation fails after
cleanup, the domain reports the accepted revision unavailable instead of claiming that the old
native world was rolled back. This fixture's native allocation has infallible RAII release; a
domain with fallible cleanup must retain its real cleanup owner and define its own retry policy.

The legacy scene and ECS lifecycle-free guards remain intact. Direct scene synchronization or
clear still rejects a configuration entity with an attached hook-owning native component. Domain
retirement must happen before that transaction; adding hooks to its rollback path is not a supported
shortcut. This concrete consumer does not introduce a universal hydrator or native-state manager.

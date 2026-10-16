# Collect Game

A complete small public-API consumer created with `nara new desktop-2d/v1`.
The generated project owns its sources; the starter is not a runtime dependency.

Move the blue gem with WASD and collect all three gold gems. The background turns
green when you win. Press R to restore the initial player position, gold gems and
background. The window title carries the controls; collection and victory also
print to the console. No physics, audio or runtime UI package is required.

## Run

From this directory, with the first-party `nara` and `nara-editor` tools available:

```text
nara schema . --write
nara schema . --check
nara check .
cargo run --bin headless
cargo run --bin headless -- --verify-reset
cargo run --features desktop --bin desktop
cargo run --features desktop --bin desktop -- --verify-first-present
nara-editor .
```

The ordinary headless demonstration walks right through the same authored level
and requires `tick=62 collected=3 targets=3 won=true resets=0`. The reset variant
requires `tick=66 collected=0 targets=3 won=false resets=1 player=(-120.0,0.0)`.
These are bounded gameplay checks, not an alternate headless level.

Run the consumer's tests separately from the engine workspace:

```text
cargo nextest run --manifest-path samples/collect-game/Cargo.toml --workspace
```

That command is relative to the engine root. Reuse the engine's target directory
with `CARGO_TARGET_DIR` and one build job when working in the repository.

## Composition

- `src/lib.rs` extends the generated bootstrap with one `product_recipe()`.
- `gameplay-plugin/` is an independent Cargo package contributing gameplay and
  persistent schemas together. Its optional desktop plugin maps keys to the
  same semantic commands consumed by headless execution.
- `prefabs/player.prefab.json` supplies the player sprite, initial position and
  authored speed. `scenes/startup.scene.json` instances it and owns the camera
  and three collectible entities.
- `assets/textures/token.png` is an original geometric image. Its committed
  metadata supplies a stable image identity. Inspector image assignment uses
  `textures/token.png` as its logical asset path.
- Runtime held keys, collection state and reset baselines stay in resources.
  Collected authored entities remain alive with transparent sprites. Reset does
  not rewrite project source or reconstruct a second hard-coded level.

The independent workspace has `nara = { path = "../..", ... }` in its root
manifest and `path = "../../.."` in the plugin manifest. An external project
must retain the dependency coordinate selected by its own `nara new` call;
adjust the nested plugin's Nara coordinate to the same checkout. Do not copy
these repository-relative paths unchanged into a different directory layout.

The generic Editor reads the explicitly generated schema bundle. Custom records
are KnownUnbound and remain read-only there; native gameplay uses their compiled
binding. Edit custom speed/radius in the ordinary source files, refresh schema
after Rust schema changes, and reopen. Built-in Camera, Transform and Sprite
fields support the ordinary Inspector, Undo/Redo and Save workflow.

## A lightweight content variant

`variants/fast-player/player.prefab.json` is a normal replacement Prefab with
speed 300 and an adjusted tint. Stop Play, copy it over `prefabs/player.prefab.json`,
reopen the document, check, and Play again. The game code and native plugin are
unchanged. Keep the original file in version control to restore the default.
This demonstrates explicit data replacement; it is not a dynamic plugin ABI,
mod discovery service or conflict resolver.

## Export

```text
nara export . --parent .. --destination collect-candidate
```

The destination must be absent. Run its `bin/headless` and
`bin/desktop --verify-first-present` from an unrelated working directory; add
`.exe` on Windows. Both resolve the sibling `project/` content tree. The image,
player Prefab and scene are in the startup closure. The inactive variant is not
part of that closure. This is a source-carrying development/verification
candidate, not formal release packaging or a complete editable project backup.

Follow the [first-game guide](../../docs/guides/first-2d-game.md) from the engine
repository for independent creation, editing and supervised Play.

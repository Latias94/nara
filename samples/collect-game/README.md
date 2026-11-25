# Collect Game

A complete small public-API consumer based on the authored `desktop-2d/v1` starter.
The generated project owns its sources; the starter is not a runtime dependency.

Move the blue gem with WASD and collect all three gold gems. The background turns
green when you win. Press R to restore the initial player position, gold gems and
background. The window title carries the controls; collection and victory also
print to the console. No physics, audio or runtime UI package is required.

## Run

From this directory, with the `nara` CLI available:

```text
nara check .
cargo run --bin headless
cargo run --bin headless -- --verify-reset
cargo run --features desktop --bin desktop
cargo run --features desktop --bin desktop -- --verify-first-present
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
- Movement speed, level bounds and pickup radii use world units. A scaled,
  rotated or reflected Prefab parent can change the authored pose. Right still
  moves along world X at the configured speed. A singular parent keeps the
  player at its current pose. A spawn beyond the bounds can walk back at the
  configured speed. Each fixed tick can collect gems crossed along its walking
  segment. Reset starts a new segment at the restored pose.
- `assets/textures/token.png` is an original geometric image. Its committed
  metadata supplies a stable image identity. Sprites refer to it by the logical
  asset path `textures/token.png`.
- Runtime held keys, collection state and reset baselines stay in resources.
  Collected authored entities remain alive with transparent sprites. Reset does
  not rewrite project source or reconstruct a second hard-coded level.

The independent workspace has `nara = { path = "../..", ... }` in its root
manifest and `path = "../../.."` in the plugin manifest. An external project
must retain the dependency coordinate selected when publishing its starter;
adjust the nested plugin's Nara coordinate to the same checkout. Do not copy
these repository-relative paths unchanged into a different directory layout.

Edit a gem's fractional `Collectible.radius` in `scenes/startup.scene.json` or the
player's `PlayerConfig.speed` in `prefabs/player.prefab.json`. Both fields are `f32` in Rust
and use F64 wire values in their version-2 records. Refresh the schema bundle after
Rust schema changes by calling `nara::project_tooling::invoke_project_tool` with
`ProjectToolRequest::Schema { write: true, .. }`. Run `nara check .` after saving content.

Both custom components define literal creation templates. `PlayerConfig` starts
with speed 240 and `Collectible` with radius 20.
Catalog generation 2 adds these templates and retains the frozen generation-1 catalog.
The component records still use wire version 2.

## A lightweight content variant

`variants/fast-player/player.prefab.json` supplies speed 300.5 and an adjusted tint.
Close the game, copy the variant over `prefabs/player.prefab.json`, run `nara check .`,
and launch the desktop game again. Keep the original file in version control to
restore the default.

Follow the [first-game guide](../../docs/guides/first-2d-game.md) to start a new code-first game.

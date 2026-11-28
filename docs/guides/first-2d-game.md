# Your first 2D game

From the Nara checkout, create a standalone Cargo workspace using Rust 1.95 or newer. Its scene
is loaded from RON and displays a rotating sprite driven by a Rust system.

```text
cargo run --locked -p nara_project --bin nara -- new ../my-game --package my-game --nara-checkout .
cargo run --locked -p nara_project --bin nara -- check ../my-game
cargo run --locked -p nara_project --bin nara -- play ../my-game --trusted
```

Choose a new directory under an existing parent. The generated `Cargo.toml` uses the local Nara
checkout. `check` validates the project manifests and discovers missing asset sidecars. Play runs
Cargo and the game. Use `--trusted` for project code you trust to execute. Close the game window or press Ctrl+C
in the terminal to stop Play.

## Change the scene

Open `assets/main.scene.ron` and set `radians_per_second: 2.0` on the `game.Spin` component.
Rerun Play. The startup system instantiates the authored sprite, `DefaultPlugins` supplies the
desktop runner, and `Update` calls `spin` once per frame with
`VirtualTime`. Use `FixedUpdate` for movement that should advance at a fixed simulation interval.

The project's `src/lib.rs` defines `Spin`. Both the game and the `nara_schema` binary register that
type. After changing its component or field schema, refresh the cache from the Nara checkout.

```text
cargo run --locked -p nara_project --bin nara -- schema ../my-game --trusted
cargo run --locked -p nara_project --bin nara -- schema ../my-game
```

The first command builds the exporter and updates `.nara/schema_cache.json`. The second reads that
cache. Keep the generated `Cargo.lock` after the first build so later builds use the same dependency
versions.

## Cook and export

The generated `nara.toml` lists the entry scene's `.meta` UUID under `[delivery]`. Cook expands its
prefabs and imports its referenced assets into `.nara/game.npak`. Run with `--trusted` to refresh
the release Schema before cooking. Offline Cook keeps the asset directory read-only and requires
existing `.meta` sidecars. It uses the cached Schema for the selected delivery profile and falls
back to `.nara/schema_cache.json` when that release catalog is absent.

```text
cargo run --locked -p nara_project --bin nara -- cook ../my-game --trusted
cargo run --locked -p nara_project --bin nara -- export ../my-game --out ../my-game-dist --trusted
```

Choose a new export directory. Export builds the release game with Cargo defaults disabled and the
game target's required features enabled. It runs the Schema exporter with the same profile plus the
exporter's required features. Keep the executable and `game.npak` together when moving or
distributing the exported game. The development game continues reading project source assets.

## Bind a physical key

Insert an `ActionMaps` resource before the game starts. Physical keys such as `KeyW`, `Space`,
and `ArrowLeft` stay independent of the text typed by the player.

```rust
let jump = ActionBinding::key(ActionId::new("jump")?, KeyCode::Space);
app.insert_resource(ActionMaps::new(ActionMap::from_bindings([jump])?))?;
```

Gameplay systems read the binding through `Res<ActionState>` with `actions.button("jump")`.

## Explore the supplied games

- Run [the compact code-first example](../../examples/first_game.rs) to see fixed-step movement,
  sprite animation, a tilemap, collision audio, and a HUD together. Its build command is in the
  [repository README](../../README.md#build-and-test).
- Play the [reference game](../../reference-game/README.md) to see the desktop HUD and controls.
  Its headless command exercises the same game simulation.

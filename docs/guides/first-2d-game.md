# Your first 2D game

From the Nara checkout, create a standalone Cargo workspace using Rust 1.95 or newer. Its scene
starts in Rust and displays a rotating sprite.

```text
cargo run --locked -p nara_project --bin nara -- new ../my-game --package my-game --nara-checkout .
cargo run --locked -p nara_project --bin nara -- check ../my-game
cargo run --locked -p nara_project --bin nara -- play ../my-game --trusted
```

Choose a new directory under an existing parent. The generated `Cargo.toml` uses the local Nara
checkout. `check` reads `nara.toml` and validates the local Cargo package declaration. Play runs Cargo and the
game. Use `--trusted` for project code you trust to execute. Close the game window or press Ctrl+C
in the terminal to stop Play.

## Change the scene

Open `src/main.rs` in the generated project and find `setup`. The two `commands.spawn` calls create
the camera and sprite. Change the sprite's `Spin { radians_per_second: 0.8 }` to `2.0` and rerun
Play. `DefaultPlugins` supplies the desktop runner, and `Update` calls `spin` once per frame with
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

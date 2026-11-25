# Your first 2D game

From the Nara checkout, create a standalone Cargo workspace using Rust 1.95 or newer. Its scene
starts in Rust and displays a rotating sprite.

```text
cargo run --locked -p nara_project --bin nara -- new ../my-game --package my-game --nara-checkout .
cargo run --locked -p nara_project --bin nara -- check ../my-game
cargo run --locked -p nara_project --bin nara -- play ../my-game --trusted
```

Choose a new directory under an existing parent. The generated `Cargo.toml` uses the local Nara
checkout. `check` reads `nara.toml` and any declared scene and schema files. Play runs Cargo and the
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

## Author an action map

A source-hosted game can select `input/default.actions.ron` with `[input]` in `nara.toml`.
Use physical key names such as `KeyW`, `Space`, or `ArrowLeft`. Each binding names an action and
its context. The file's `source` UUID identifies the map across edits.

```toml
[input]
action_map = "input/default.actions.ron"
```

```ron
(
    kind: "action_map",
    format_version: 1,
    engine_min_version: "0.1.0",
    generator: (name: "my-game", version: "0.1.0"),
    payload: (
        source: "b23d8f3a-9770-5c1a-bf4d-81fb93171257",
        contexts: [(context: "gameplay", enabled: true)],
        bindings: [(action: "jump", input: Key(Space), context: "gameplay")],
    ),
)
```

Run `nara check` to validate the selected map. Gameplay systems read its actions through
`Res<ActionState>`, for example `actions.button("jump")`.

## Explore the supplied games

- Run [the compact code-first example](../../examples/first_game.rs) to see fixed-step movement,
  sprite animation, a tilemap, collision audio, and a HUD together. Its build command is in the
  [repository README](../../README.md#build-and-test).
- Play [First Wave](../../reference-game/README.md) to see a complete game with authored scenes,
  prefabs, and HUD. Its headless command runs the same gameplay systems as the desktop build.
- Follow [the two-level game's delivery workflow](../../samples/two-level-game/README.md#source-free-cooked-player)
  to cook and export a project using a project-owned `cook` target.

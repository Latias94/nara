# Nara

Nara is an open-source game engine written in Rust. It is under active pre-1.0 development and is
validated through a small 2D reference game. APIs and project formats are expected to change.

Rust is the complete first-party game-authoring language, with `bevy_ecs` as the simulation substrate.
The v2 refactor develops the standalone game runtime and desktop editor as separate products.
See the [strategy](STRATEGY.md), [architecture](docs/architecture/overview.md), and
[v2 master plan](docs/plans/v2-refactor-master-plan.md).

## Current Product Slice

- `App`, `DefaultPlugins`, deterministic fixed stepping, and a native Winit runner;
- ECS hierarchy, transform propagation, typed input, 2D physics, and audio;
- versioned scene, prefab, schema, asset, and project-manifest data;
- a headless project CLI for project creation, validation, Schema discovery, supervised Play, Cook,
  and Export;
- a deterministic 2D reference game with sprites, input, HUD, and headless and desktop entry points;
- shared format and image data in `tests/fixtures/`, with game-facing coverage in `reference-game`
  and `samples/two-level-game`.

## Requirements

- Rust 1.95 or newer
- `cargo-nextest` 0.9.138 for the documented test workflow
- A Vulkan- or Direct3D-12-capable adapter for the desktop reference game

## Build And Test

Run the root workspace checks:

```text
cargo check --workspace --locked
cargo nextest run --locked
```

Run the headless reference game:

```text
cargo run --manifest-path reference-game/Cargo.toml --locked --bin headless
```

Run the desktop reference game:

```text
cargo run --manifest-path reference-game/Cargo.toml --locked --features desktop --bin desktop
```

See [`reference-game/README.md`](reference-game/README.md) for the gameplay controls, project data,
and standalone candidate layout.

The [short code-first game](examples/first_game.rs) demonstrates movement, sprite animation, a
tilemap, collision, sound, and a HUD. Run it with the features below.

```text
cargo run --locked --example first_game --features physics-2d,audio-device,presentation-2d,presentation-ui,desktop-winit,render-wgpu
```

On Linux, a desktop build with audio requires the ALSA development headers.

Create a standalone project from the engine checkout. The destination's parent must exist.

```text
cargo run --locked -p nara_project --bin nara -- new ../my-game --package my-game --nara-checkout .
cargo run --locked -p nara_project --bin nara -- check ../my-game
cargo run --locked -p nara_project --bin nara -- schema ../my-game --trusted
cargo run --locked -p nara_project --bin nara -- play ../my-game --trusted
```

Use `--trusted` for projects whose Rust and Cargo build code you trust to run. Follow the generated
README for the project layout and [examples](examples/) for game code.

For cross-level travel and game-owned checkpoint saves, see the independent
[`samples/two-level-game`](samples/two-level-game/README.md) consumer.

## Repository Layout

- `crates/`: engine modules and native adapters
- `src/`: the game facade and the current authored and cooked runtime integrations
- `reference-game/`: the first complete headless and desktop game tracer
- [`tests/fixtures/`](tests/fixtures/README.md) contains shared format and image test data.
- [`docs/architecture/`](docs/architecture/overview.md) contains the v2 architecture and accepted ADRs.
- [`docs/plans/`](docs/plans/v2-refactor-master-plan.md) contains the active roadmap and phase gates.

The `legacy/v0-imgui-reference` tag contains the previous editor, guides, and architecture records.

## License

Nara is available under either the Apache License 2.0 or the MIT License, at your option. See
`LICENSE-APACHE` and `LICENSE-MIT`.

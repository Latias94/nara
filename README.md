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
- code-first worlds, compact RON scenes and prefabs, and typed assets with UUID sidecars;
- a headless project CLI for project creation, Schema discovery, supervised Play, Cook, and Export;
- a deterministic 2D reference game with sprites, input, HUD, and headless and desktop entry points;
- a native editor with a shared WGPU Scene View, dockable panels, Schema editing, and external Play.

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
cargo run --manifest-path reference-game/Cargo.toml --locked --features simulation --bin headless
```

Run the desktop reference game:

```text
cargo run --manifest-path reference-game/Cargo.toml --locked --features desktop --bin game
```

See [`reference-game/README.md`](reference-game/README.md) for the gameplay controls and project layout.

Follow the [native editor guide](docs/guides/native-editor.md) to open Night Relay, edit its authored
scene, and play the current edit using a sibling Open-GPUI checkout.

The [short code-first game](examples/first_game.rs) demonstrates movement, sprite animation, a
tilemap, collision, sound, and a HUD. Run it with the features below.

```text
cargo run --locked --example first_game --features physics-2d,audio-device,presentation-2d,presentation-ui,desktop-winit,render-wgpu
```

The [platformer demo](examples/platformer_demo.rs) uses merged tile colliders, a slope, and one-way
platforms. Move with **A/D** or the **arrow keys**, and jump with **Space**. Add `-- --verify` to
replay a slope climb and one-way landing and exit after the window presents the result.

```text
cargo run --locked --example platformer_demo --features scene-cooked,physics-2d,presentation-2d,desktop-winit,render-wgpu
```

On Linux, audio device support requires the ALSA development headers. Optional gamepad support
requires the libudev development headers.

Create a standalone project from the engine checkout. The destination's parent must exist.

```text
cargo run --locked -p nara_project --bin nara -- new ../my-game --package my-game --nara-checkout .
cargo run --locked -p nara_project --bin nara -- check ../my-game
cargo run --locked -p nara_project --bin nara -- schema ../my-game --trusted
cargo run --locked -p nara_project --bin nara -- play ../my-game --trusted
cargo run --locked -p nara_project --bin nara -- export ../my-game --out ../my-game-dist --trusted
```

Use `--trusted` for projects whose Rust and Cargo build code you trust to run. Follow the generated
README for the project layout and [the first-game guide](docs/guides/first-2d-game.md) for edits
to the starter.

## Repository Layout

- `crates/`: engine modules and native adapters
- `src/`: the game facade
- `reference-game/`: the first complete headless and desktop game tracer
- [`docs/architecture/`](docs/architecture/overview.md) contains the v2 architecture and accepted ADRs.
- [`docs/plans/`](docs/plans/v2-refactor-master-plan.md) contains the active roadmap and phase gates.

The `legacy/v0-imgui-reference` tag contains the previous editor, reference game, two-level game,
guides, and architecture records.

## License

Nara is available under either the Apache License 2.0 or the MIT License, at your option. See
`LICENSE-APACHE` and `LICENSE-MIT`.

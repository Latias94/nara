# Nara

Nara is an open-source game engine written in Rust. It is under active pre-1.0 development and is
validated through a small 2D reference game. APIs and project formats are expected to change.

Rust is the complete first-party game-authoring language, with `bevy_ecs` as the simulation substrate.
The v2 refactor develops the standalone game runtime and desktop editor as separate products.
See the [strategy](STRATEGY.md), [architecture](docs/architecture/overview.md), and
[v2 master plan](docs/plans/v2-refactor-master-plan.md).

## Current Product Slice

The repository currently includes:

- a fallible `App`, plugin composition, schedules, fixed time, tasks, and managed runtime lifecycle;
- versioned scene, prefab, schema, asset, and project-manifest data;
- headless and Winit/wgpu desktop product Hosts;
- a deterministic 2D reference game with sprites, input, HUD, Save/Play authoring evidence, and
  bounded diagnostics;
- an independent `reference-game` workspace and public-API consumer fixtures under `tests/fixtures/`.

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

For cross-level travel and game-owned checkpoint saves, see the independent
[`samples/two-level-game`](samples/two-level-game/README.md) consumer.

## Repository Layout

- `crates/`: engine modules and native adapters
- `src/`: the integrated product facade and Project Host
- `reference-game/`: the first complete headless and desktop game tracer
- `tests/fixtures/`: test data and independent consumer projects; see the [fixture guide](tests/fixtures/README.md)
- [`docs/architecture/`](docs/architecture/overview.md) contains the v2 architecture and accepted ADRs.
- [`docs/plans/`](docs/plans/v2-refactor-master-plan.md) contains the active roadmap and phase gates.

The `legacy/v0-imgui-reference` tag contains the previous editor, guides, and architecture records.

## License

Nara is available under either the Apache License 2.0 or the MIT License, at your option. See
`LICENSE-APACHE` and `LICENSE-MIT`.

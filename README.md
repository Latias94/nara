# Nara

Nara is an open-source game engine written in Rust. It is under active pre-1.0 development and is
currently validated through a small 2D reference game rather than a stable general-purpose SDK.

Rust is the complete first-party game-authoring language. Nara uses `bevy_ecs` as its simulation
substrate while keeping project documents, product Hosts, native backends, and editor state outside
the runtime `World`.

## Current Product Slice

The repository currently includes:

- a fallible `App`, plugin composition, schedules, fixed time, tasks, and managed runtime lifecycle;
- versioned scene, prefab, schema, asset, and project-manifest data;
- headless and Winit/wgpu desktop product Hosts;
- a deterministic 2D reference game with sprites, input, HUD, Save/Play authoring evidence, and
  bounded diagnostics;
- independent `reference-game` and `module-consumer` workspaces that use public APIs.

The engine is not yet a complete Godot- or Unity-level game-development product. Packaging,
hosted cross-platform evidence, editor delivery, and additional real games are still active work.

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

## Repository Layout

- `crates/`: engine modules and native adapters
- `src/`: the integrated product facade and Project Host
- `reference-game/`: the first complete headless and desktop game tracer
- `module-consumer/`: a direct public-module consumer
- `docs/architecture/`: architecture decisions, implementation status, and open questions
- `docs/plans/`: execution plans and verification contracts

## License

Nara is available under either the Apache License 2.0 or the MIT License, at your option. See
`LICENSE-APACHE` and `LICENSE-MIT`.

# Night Relay

Fly through a dark relay station, collect three amber power cells, and reach the lower-right gate
before time runs out. Walls block the courier and red patrol drones drain its shields. The gate turns
green when it has power.

Use **WASD** or the **arrow keys** to fly, **R** or **Enter** to restart, and **Esc** to quit.

## Desktop

From the repository root, run the Winit/WGPU game with audio and the in-game HUD.

```sh
cargo run --manifest-path reference-game/Cargo.toml --features desktop
```

Add `-- --verify-first-present` to exit successfully after the primary window presents a frame with
HUD glyphs. The probe requests exit if that evidence has not arrived within 15 seconds.

## Headless

The `simulation` feature uses the same game plugin with exact fixed ticks and mock audio. The replay
checks wall collision, shield depletion, restart, and a complete escape. Repeated runs compare their
results for deterministic behavior.

```sh
cargo run --manifest-path reference-game/Cargo.toml --features simulation --bin headless -- --stress 8
```

## Project CLI

Run these from the repository root with the Nara CLI on `PATH`.

```sh
nara check reference-game
nara schema reference-game --trusted
nara play reference-game --trusted
nara play reference-game --trusted --bin headless -- --stress 8
```

The `nara_schema` target builds with default features and exports the registered `RunRules`
component. `GamePlugin` uses those rules for the timer, flight speed, and shield count. Game logic,
the arena, the animated courier texture, and the sound cues all live in Rust. `nara.toml` supplies
project metadata to the CLI.

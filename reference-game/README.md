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

## Scene and Editor Play

The editor opens `assets/night-relay.scene.ron` by default. The desktop and headless games find
that same scene by its stable `.meta` ID under the development project's `assets` directory.
Edit the courier spawn, wall shapes, cell and gate positions, patrol placement, or the
`Round rules` entity and relaunch to see the change. World coordinates use +Y up. The arena
needs one `Pilot`, one `ExtractionGate`, three `PowerCell` roles, and three `PatrolDrone` roles,
all on root entities with circular colliders. Cell and drone orders must each cover 1 through 3.
A drone's authored position is its starting position, and moving it shifts the entire patrol.

The three cells share `assets/power-cell.prefab.ron`. Each instance adds its own `PowerCell` role
and order, overrides its position, and inherits the shared color, texture, collider, and starting
rotation. Move source assets with their `.meta` sidecars so the scene, prefab, and texture IDs
continue to resolve. Play can open a temporary scene snapshot while still reading its prefab and
sprite references from the original project assets.

The editor launches `game --editor-play <scene-path>` with piped stdin. It receives
`NARA_EDITOR ready` after the first game frame with HUD. Send `pause`, `step`, or `resume`, one
line at a time. The game replies with `NARA_EDITOR paused`, `NARA_EDITOR stepped`, or
`NARA_EDITOR running` when the runtime applies that control. A step advances one fixed tick while
paused. The supervisor owns process termination.

## Headless

The `simulation` feature uses the same game plugin and checks the same authored assets with exact
fixed ticks and mock audio. The replay checks wall collision, shield depletion, restart, and a
complete escape. Repeated runs compare their results for deterministic behavior.

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

The `nara_schema` target builds with default features and exports `RunRules` and the four arena
roles. `GamePlugin` applies the scene's rules to the timer, flight speed, and shield count. Game
logic, the animated courier texture, and sound cues live in Rust. `nara.toml` supplies project
metadata to the CLI.

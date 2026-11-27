# Night Relay

Fly through a dark relay station, collect three amber power cells, and reach the lower-right gate
before time runs out. Walls block the courier and red patrol drones drain its shields. The gate turns
green when it has power.

Use **WASD** or the **arrow keys** to fly, **R** or **Enter** to restart, and **Esc** to quit.

## Desktop

From the project directory, run the Winit/WGPU game with audio and the in-game HUD.

```sh
cd reference-game
cargo run --features desktop
```

Add `-- --verify-first-present` to exit successfully after the primary window presents a frame with
HUD glyphs. The probe requests exit if that evidence has not arrived within 15 seconds.
Use `-- --verify-gameplay` to fly the packaged arena's route to the first power cell and wait
for the resulting HUD to present before exiting.

The default `hot-reload` feature enables the development `source` feature. For source loading
without a watcher, use `--no-default-features --features source,desktop`. Development launchers
can set `NARA_PROJECT_ROOT` to a project directory containing `nara.toml` and `assets` when they
cannot set the process working directory. Launches from the repository root also find its checked
`reference-game` child directory.

## Standalone export

Run `nara export reference-game --out <directory> --trusted` from the repository root. Export
builds the game with `--release --no-default-features --features desktop` and cooks its entry
scene, prefab expansion, images, and gate material into `game.npak`. The packaged game opens
that archive beside its executable. It can launch from any working directory with the executable
and archive together. Run the exported game with `--verify-first-present` for a native smoke.
Development-capable binaries also prefer a sibling `game.npak` on normal launch. `--editor-play`
always uses development sources.

## Scene and Editor Play

The editor opens `assets/night-relay.scene.ron` by default. The desktop and headless games find
that same scene by its stable `.meta` ID under the development project's `assets` directory.
Edit the courier spawn, wall shapes, cell and gate positions, patrol placement, or the
`Round rules` entity. World coordinates use +Y up. The arena
needs one `Pilot`, one `ExtractionGate`, three `PowerCell` roles, and three `PatrolDrone` roles,
all on root entities with circular colliders. Cell and drone orders must each cover 1 through 3.
A drone's authored position is its starting position, and moving it shifts the entire patrol.

The three cells share `assets/power-cell.prefab.ron`. Each instance adds its own `PowerCell` role
and order, overrides its position, and inherits the shared color, texture, collider, and starting
rotation. Move source assets with their `.meta` sidecars so the scene, prefab, and texture IDs
continue to resolve. The gate uses `gate.material.ron` and `gate.wgsl` to demonstrate live changes
to a sprite material and its shader.

With `hot-reload`, the game watches the project `assets` directory and validates the active scene
and asset changes together. Invalid edits leave the last good scene, pixels, material, and round
in place. A valid reload keeps the current timer, shields, collected cells, and pilot position.
New round rules and the authored pilot spawn take effect on restart.
The game rescans and loads the small dependency set during its next frame after a coalesced change.

Play can open a temporary snapshot while reading assets from the original project. At launch the
game compares the effective snapshot to the expanded saved entry scene by entity UUID. Added or
removed rows, name and hierarchy edits, and edited component fields in the snapshot stay pinned.
Unedited fields follow saves and prefab changes on disk. A snapshot for another document, or one
without a readable baseline at launch, owns the scene rows for that Play session. Textures and
materials still follow their project assets. The game never writes the snapshot or source files.

The editor launches `game --editor-play <scene-path>` with piped stdin. It receives
`NARA_EDITOR ready` after the first game frame with HUD. Send `pause`, `step`, or `resume`, one
line at a time. The game replies with `NARA_EDITOR paused`, `NARA_EDITOR stepped`, or
`NARA_EDITOR running` when the runtime applies that control. A step advances one fixed tick while
paused. The supervisor owns process termination. Closing a piped stdin also retires the game.

Set `NARA_RELOAD_TRACE=1` for measurement. Each changed revision emits `NIGHT_RELAY_RELOAD` with
its application and native presentation frame indices, plus scene, image, and material changes.
Measure from a source save to receipt of this line after the matching game frame presents.

## Headless

The `simulation` feature uses the same game plugin and checks the same authored assets with exact
fixed ticks and mock audio. The replay checks wall collision, shield depletion, restart, and a
complete escape. Repeated runs compare their results for deterministic behavior.

```sh
cd reference-game
cargo run --features simulation --bin headless -- --stress 8
```

## Project CLI

Run these from the repository root with the Nara CLI on `PATH`.

```sh
nara check reference-game
nara schema reference-game --trusted
nara play reference-game --trusted
nara play reference-game --trusted --bin headless -- --stress 8
```

The `nara_schema` target builds with the default `schema` feature and exports `RunRules` and the
four arena roles. It also builds with `--no-default-features --features schema`. `GamePlugin`
applies the scene's rules to the timer, flight speed, and shield count. Game logic, the animated
courier texture, and sound cues live in Rust. `nara.toml` supplies project metadata to the CLI.

## Run the starter

Run these commands from the generated project root.

```text
nara check .
cargo run --bin headless
cargo run --features desktop --bin desktop
```

Focus the game window and hold W, A, S, or D to move the square. Hold Enter to return to the center
and stay there until you release it. On Windows, the `desktop` feature starts the managed Gilrs
backend using Windows Gaming Input (WGI). With a supported gamepad and the window focused, hold a
D-pad direction to move or South to return to the center. Linux and macOS use the keyboard controls.

Use this command to open the desktop product, verify its first presented frame, and exit.

```text
cargo run --features desktop --bin desktop -- --verify-first-present
```

## Swap a binding

1. Open `input/starter.actions.json`. Find the `move.left` binding with
   `"input": {"Key": {"Character": "a"}}`. Change `"a"` to `"j"`.
2. Preserve the `action`, `context`, and `transform` fields and the file's `product` identity.
   Save the file and run `nara check . --profile desktop`.
3. Close the game and rerun `cargo run --features desktop --bin desktop`. Hold J to move left.
   Press A on its own to verify that the square stays in place.

To change a gamepad binding, find its `GamepadButton` entry. For example, change the `confirm`
button code from `0` (South) to `1` (East), then check the desktop profile and restart the game. Standard
codes come from `nara::input::GamepadButton::code()`. The `[input]` section of `nara.toml` selects
this action map for the primary seat. Each new Runtime reads the selected definition at startup.
Run Cook again to deliver an edited map to a new Player installation.

## Change the game in Rust

Open `src/gameplay.rs`. Change `MOVE_SPEED` from `240.0` to `480.0` and rerun
`cargo run --features desktop --bin desktop` to double the speed. Edit the `confirm_held` branch in
`step_player` to change what holding `confirm` does. For example, return `Vec2::new(80.0, 40.0)` to
move to that point when confirm is held. Change the square's size and color in `spawn_player`.

`move_player` reads held actions from the primary seat in the `gameplay` context on each fixed tick.
Opposite directions cancel, and diagonal movement has the same speed as movement along one axis.
`step_player` computes the next position from those actions and the fixed timestep. The desktop,
headless, and cooked Player targets all run this gameplay plugin. The supplied headless and Player
entry points run with neutral input by default. `product_recipe()` in `src/lib.rs` registers the
plugin and the Image and Action Map codecs used by Cook and Player.

## Edit the startup Scene

Run `nara-editor .` from the project root. Select `main-camera`, change `Camera2d.viewport_height`
from `720` to `768`, and use Undo, Redo, Save, and Reopen to try the authoring workflow. The camera
is authored in `scenes/startup.scene.json`, and the player spawns when the game runs. You can also
edit the camera's `clear_color.r` from `0.06` to `0.14` in that file and relaunch to see the change.

Keep `scenes/startup.scene.json.meta` with its Scene. Its stable asset ID and the manifest's
`project.source_universe` preserve authored identity across saves and delivery. The generated
files are your project's copies. The starter and dependency coordinate above identify their
origin. Upgrading either is an explicit migration.

If you add a persistent component, register its schema contribution in `product_recipe()` and
refresh the schema bundle before reopening the project in the Editor.

```text
nara schema . --check
nara schema . --write
nara schema . --check
```

Pass `--profile desktop` to check or write the desktop profile's schema. To include another Scene
in Check and Export, declare its path relative to `paths.scenes` in `nara.toml`.

```toml
[content]
scenes = ["level-b.scene.json"]
```

## Cook a source-free Player

Run the cooker from the generated project root. This PowerShell example selects the installed
native Rust toolchain and reuses the project's Cargo target directory.

```powershell
$projectRoot = (Get-Location).Path
$outputParent = (New-Item -ItemType Directory -Force ../installations).FullName
$targetDirectory = Join-Path $projectRoot 'target'
$cargoPath = (rustup which cargo).Trim()
$rustcPath = (rustup which rustc).Trim()
$targetTriple = ((& $rustcPath -vV | Select-String '^host: ').Line -replace '^host: ', '')
cargo run --bin cook -- --project-root "$projectRoot" --output-parent "$outputParent" --name game-player --target-dir "$targetDirectory" --target "$targetTriple" --cargo "$cargoPath" --rustc "$rustcPath" --profile release
```

Supply absolute directory and tool paths and choose an installation name that is absent under
the output parent. Keep `Cargo.lock` for subsequent locked Player builds. Cook uses the default
project profile and builds the headless `player` binary with the processed startup Scene and
action map packaged in `content.pack`. `STARTER_PACKAGE_PUBLISHED` reports the installation.
Keep `player` (`player.exe` on Windows), `content.pack`, and `package.json` together when moving it.

```powershell
& "$outputParent/game-player/player.exe" --frames 120
```

Use `player` on Linux or macOS. The installed executable needs only its packaged content and
runs complete headless frames. `starter_player: ok` reports success.

## Export a source-carrying candidate

Publish a source-carrying declared-content candidate into an absent destination.

```text
nara export . --parent .. --destination candidate-before-repair
```

The candidate's `bin/headless` and `bin/desktop --verify-first-present` executables use the sibling
`project/` directory and can run from another working directory. Add `.exe` on Windows.

To exercise startup Scene repair, move `scenes/startup.scene.json` and its `.meta` sidecar to
`scenes/repaired.scene.json` and `scenes/repaired.scene.json.meta`. Update the metadata's authored
path to `repaired.scene.json` and preserve its stable asset ID. Then run these commands.

```text
nara check .
nara repair startup-scene . --replacement repaired.scene.json
nara check .
cargo run --bin headless
cargo run --features desktop --bin desktop -- --verify-first-present
nara export . --parent .. --destination candidate-after-repair
```

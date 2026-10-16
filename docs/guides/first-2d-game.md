# Your first independent 2D game

Create a project outside the engine checkout, add a small Cargo gameplay plugin,
edit its content, and run an exported development candidate. The maintained result
is [Collect Game](../../samples/collect-game/README.md): WASD movement, three gems,
a green victory background, and R to reset.

The commands below use PowerShell on Windows. The Rust sources and Cargo workspace
are portable; Unix shells use their own environment assignment and executable
spelling. Native platform acceptance is recorded separately from compilation.

## Build the tools and create a project

From the engine checkout:

```powershell
$engine = (Get-Location).Path
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path $engine 'target'
cargo build --bin nara --no-default-features --features project-workflow-cli --locked
cargo build -p nara-editor --locked
$nara = Join-Path $env:CARGO_TARGET_DIR 'debug/nara.exe'
$editor = Join-Path $env:CARGO_TARGET_DIR 'debug/nara-editor.exe'
$games = Join-Path (Split-Path $engine -Parent) 'my-games'
New-Item -ItemType Directory -Path $games -Force | Out-Null
& $nara new desktop-2d/v1 collect-game --parent $games --local-checkout $engine
$game = Join-Path $games 'collect-game'
```

The destination must be absent. Local creation derives a relative Cargo dependency
from a shared directory capability. Both locations must have a usable common
filesystem ancestor; cross-volume and inaccessible-ancestor cases fail explicitly.
An immutable HTTPS Git coordinate is the portable dependency alternative.

Before changing anything, run the generated project:

```powershell
& $nara check $game
cargo run --manifest-path "$game/Cargo.toml" --bin headless
cargo run --manifest-path "$game/Cargo.toml" --features desktop --bin desktop -- --verify-first-present
```

The first bootstrap may create Cargo.lock. Keep that lockfile and use `--locked`
afterward. Reusing target avoids a second dependency cache; the first build of this
project is consequently not a cold-cache measurement.

## Add the game source

Copy these ordinary project files from `samples/collect-game/` into the new project:

- `src/`, replacing the generated source with the completed game's source.
- `gameplay-plugin/`, `scenes/`, `prefabs/`, `assets/`, `variants/`, and `tests/`.
- `nara.toml`, retaining the new project's own name/identity if you chose another slug.

Keep the **generated Cargo.toml and its selected Nara path**. Add
`members = ["gameplay-plugin"]` to its existing `[workspace]`, add this dependency,
and extend its existing desktop feature:

```toml
[dependencies]
# Keep the generated nara entry here.
collect-gameplay = { path = "gameplay-plugin" }

[features]
default = []
desktop = ["nara/presentation-2d", "nara/desktop-winit", "nara/render-wgpu", "collect-gameplay/desktop"]
```

These are edits to existing sections, not duplicate TOML sections. In
`gameplay-plugin/Cargo.toml`, set its Nara path to `../` followed by the root
project's selected path. For example, if the generated root dependency is
`../../nara`, the plugin uses `../../../nara`. Both packages must select the same
checkout. Do not copy the sample's repository-relative coordinates unchanged.

The library owns one `product_recipe()`. Its contribution carries both native
gameplay and persistent schema. The headless tool entry, headless gameplay and
desktop product use that definition. Adding desktop input does not introduce a
second simulation: keys and headless demonstrations submit the same commands.

Refresh the data-only schema from compiled code:

```powershell
cargo check --manifest-path "$game/Cargo.toml" --workspace # Update Cargo.lock after adding the plugin.
& $nara schema $game --check # Reports the starter bundle as stale.
& $nara schema $game --write
& $nara schema $game --check
& $nara check $game
cargo nextest run --manifest-path "$game/Cargo.toml" --workspace --locked
cargo run --manifest-path "$game/Cargo.toml" --bin headless --locked
cargo run --manifest-path "$game/Cargo.toml" --bin headless --locked -- --verify-reset
```

The first demonstration collects three targets and wins. The reset demonstration
restores the initial player position, three visible targets and original background.
Gameplay changes runtime state and leaves authored source intact. Collected targets
remain alive, so reset does not destroy and reconstruct persistent entities.

## Edit in the ordinary Editor

```powershell
& $editor $game --profile desktop
```

Open does not compile or execute the game's Rust. The Editor reads nara.toml, the
declared schema bundle and project content. After changing Rust schema, run the
explicit schema write command and reopen the project.

1. Select `main-camera`. Change Camera2d's `viewport_height`, press Apply, then
   Undo and Redo. Save, close and reopen; the saved value must remain.
2. Select `gem-left`. The custom Collectible record is identified but read-only
   because the generic Editor has no installed exact native authoring adapter for
   that crate. Saving built-in edits preserves this record. Change custom radius
   or player speed in the source Scene/Prefab when needed.
3. In the Project sidebar, select `textures/token.png` under assets. Import it
   with sRGB, then explicitly Reimport after changing its import recipe. Invalid
   image input reports failure while retaining the last accepted image.
4. A Sprite image field accepts `textures/token.png` or `stable_id:<UUID>` and
   applies through the same document revision transaction. The initial player
   Prefab is supplied as source; this walkthrough does not require a visual Prefab
   creator or assume custom components have editable Inspector fields.
5. Save before Play. Play builds and starts a supervised external game window;
   Running requires the child's accepted first presentation. Focus that window
   and use WASD, collect all three gems, then press R. Stop closes the owned child.

To change gameplay, Stop, edit `gameplay-plugin/src/lib.rs`, save and Play again.
Restart deliberately reuses the current executable generation. A deliberate
`compile_error!("learning the build loop");` in desktop.rs should yield compiler
diagnostics while authoring remains usable. Remove it and use Stop then Play.
The ordinary external lane does not supply Pause or Step.

## Try a content variant

With Play stopped, replace `prefabs/player.prefab.json` with
`variants/fast-player/player.prefab.json`, reopen and Check, then Play. The player
becomes purple and moves faster without changing the plugin. Restore the original
Prefab from your own version control when done.

This is an explicit data mod example. It adds no discovery service, conflict
resolver, package manager or dynamic native-library ABI. All active content must
be reachable from the startup Scene for the current export workflow.

## Export and run independently

```powershell
& $nara export $game --parent $games --destination collect-candidate
$candidate = Join-Path $games 'collect-candidate'
Push-Location $env:TEMP
try {
    & "$candidate/bin/headless.exe"
    & "$candidate/bin/desktop.exe" --verify-first-present
} finally {
    Pop-Location
}
```

The executables use their sibling project/ tree and do not need the development
project's CWD. Keep that tree with them. A published candidate that fails product
acceptance remains present and is reported as such; do not treat it as success.
This is a source-carrying startup-closure development candidate, not a full project
backup or final distribution package. Inactive variant sources are not exported.

## What to measure

Record the engine revision, platform, lockfile, features, build profile, cache state
and monotonic wall time for first project build, data Save through the next READY,
and Rust save through rebuild/READY. Observe the changed value or behavior as well:
process startup alone does not prove a gameplay edit became visible. Keep command
timings separate from intervals that include human interaction. Do not label shared
target runs cold, infer percentiles from one observation, or infer Linux/macOS
release acceptance from Windows results.

An explicit integration check is available for a **disposable external copy**. It
changes the camera and temporarily injects a compiler error before restoring the
source; it exercises the generic action path and real native game children:

```powershell
$env:NARA_AUTHOR_WORKFLOW_PROJECT = $game
cargo nextest run -p nara-editor --test first_game_workflow --run-ignored only --test-threads 1 --nocapture
```

This action-driven check proves editing, persistence, supervision and READY. It does
not claim to click the native Editor controls or play the game through keyboard input.

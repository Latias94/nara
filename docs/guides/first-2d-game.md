# Your first independent 2D game

Create a project outside the engine checkout, add a small Cargo gameplay plugin,
edit its content, and run an exported development candidate. The maintained result
is [Collect Game](../../samples/collect-game/README.md): WASD movement, three gems,
a green victory background, and R to reset.

The commands below use PowerShell on Windows. The Rust sources and Cargo workspace
are portable; Unix shells use their own environment assignment and executable
spelling. Native platform acceptance is recorded separately from compilation.

Use the `nara` CLI and edit the scene and prefab source files in your text editor.

## Build the tools and create a project

Use `desktop-2d/v2` for a new project with editable keyboard and gamepad actions and a running
movement example. Its generated `README.md` explains how to change the bindings and gameplay.
The Collect Game walkthrough below uses `desktop-2d/v1` and adds all gameplay as a local plugin.

From the engine checkout:

```powershell
$engine = (Get-Location).Path
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path $engine 'target'
cargo build --bin nara --no-default-features --features project-workflow-cli --locked
$nara = Join-Path $env:CARGO_TARGET_DIR 'debug/nara.exe'
$games = Join-Path (Split-Path $engine -Parent) 'my-games'
New-Item -ItemType Directory -Path $games -Force | Out-Null
& $nara new desktop-2d/v1 collect-game --parent $games --local-checkout $engine
$game = Join-Path $games 'collect-game'
```

The destination must be absent. Local creation derives a relative Cargo dependency
from a shared directory capability. Both locations must have a usable common
filesystem ancestor; cross-volume and inaccessible-ancestor cases fail explicitly.
An immutable HTTPS Git coordinate is the portable dependency alternative.

The project uses the requested name in its directory, `nara.toml` and window title. Its private
Cargo package is `nara-project-<source-universe-uuid>`. To select the package explicitly, use
`[package].name` from the generated `Cargo.toml`. The `--manifest-path` and `--bin` commands
below select it through the project workspace.

Use `nara new --help`, `nara check --help`, `nara schema --help`, `nara export --help` or
`nara repair startup-scene --help` for scoped syntax and examples. These requests do not open
or build a project. Help is terminal: supplied options still need values and valid preceding
arguments; extra arguments after `--help` reject.

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

First follow **Your first fixed-step system** in the generated `README.md`: create
`src/first_step.rs`, add its ordinary plugin to the existing recipe, Check, and run headless.
The exact snippet prints `first_fixed_step: tick=1` during gameplay; Check must not print it.
This gives one observable Rust edit before introducing persistent components. Keep that file and
the `mod first_step;` declaration while following the recipe below.

Copy only these ordinary files from `samples/collect-game/` into the new project:

- `src/bin/headless.rs` and `src/bin/desktop.rs`, replacing those two demonstrations.
- `gameplay-plugin/`, `prefabs/`, `assets/`, `variants/`, and `tests/`.
- `scenes/startup.scene.json`, replacing its content while keeping the new project's existing
  `scenes/startup.scene.json.meta` byte-for-byte.

Keep the **generated `nara.toml`, `Cargo.toml`, `src/lib.rs`, `build.rs`, `src/bin/cook.rs` and
`src/bin/player.rs`**. Do not copy the sample manifest, schema bundle, lockfile or whole `src/`
directory over them. The new project's `source_universe`, `stable_metadata` policy and startup
sidecar belong to that creation, even when its slug is also `collect-game`. Two same-named games
must not acquire the sample's universe or each other's document identities. A display-name edit
does not change those identities. The generated manifest already provides the required source
and desktop capabilities; merge later settings into their existing sections. The optional window
title can describe WASD movement and R reset without replacing the manifest.

The repository sample uses `project_location` for its historical Prefab source. This generated
project uses `stable_metadata`, so author `prefabs/player.prefab.json.meta` once for its newly
imported Prefab:

```powershell
$prefabMetadata = [ordered]@{
    kind = 'asset_meta'
    format_version = 1
    engine_min_version = '0.1.0'
    generator = @{ name = 'nara'; version = '0.1.0' }
    payload = @{
        asset = @{
            stable_id = [guid]::NewGuid().ToString()
            path = 'player.prefab.json'
            source_kind = 'prefab'
        }
        settings = @{}
    }
}
$prefabMetadata | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8NoBOM "$game/prefabs/player.prefab.json.meta"
```

Keep this sidecar when moving or updating that Prefab, including the fast-player data variant;
do not regenerate it on every Check. The copied token Image already includes its authored
metadata, which stays with that asset. Missing required native metadata rejects source Check
before gameplay rather than selecting a path-derived fallback.

In the **generated Cargo.toml**, add
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
checkout. If creation selected Git instead, give the plugin that same `git` and immutable `rev`
coordinate, retaining its runtime features rather than adding a local path. Do not copy the
sample's repository-relative coordinates unchanged or mix engine sources in one game.

### Keep one product recipe

Replace only `product_recipe()` in the generated `src/lib.rs` with this function. Retain its
other helpers and `mod first_step;`. The stock Image codec and first-step plugin stay selected:

```rust
pub fn product_recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    let recipe = nara::ProductRecipe::new()
        .add_runtime_image_codec::<nara::image::ImageRuntimeCodec>(
            nara::runtime_content::RuntimeCodecDefinitionId::new(
                "nara.starter.desktop-2d.image",
                1,
            ),
        )?
        .add_plugin::<first_step::FirstStepPlugin>()?
        .add_contribution(collect_gameplay::contribution()?)?;
    #[cfg(feature = "desktop")]
    let recipe = recipe.add_plugin::<collect_gameplay::DesktopInputPlugin>()?;
    Ok(recipe)
}
```

The contribution carries native gameplay and persistent schema together. All four generated
targets use this recipe. Adding desktop input does not create another simulation: physical keys
and bounded headless demonstrations submit the same commands. Recipe construction propagates
composition errors to the existing tool and product entry boundaries.

### Declare the local plugin's Cook inputs

The generated Cook inventory covers its own default Rust tree, not Cargo dependencies or
`include_bytes!` discovery. In `src/bin/cook.rs`, replace the single
`project_rust_input_inventory(&project)?,` request argument with:

```rust,ignore
project_rust_input_inventory(&project)?.into_iter().chain([
    "gameplay-plugin/Cargo.toml",
    "gameplay-plugin/src/lib.rs",
    "gameplay-plugin/src/input.rs",
    "gameplay-plugin/src/walking.rs",
    "gameplay-plugin/schema/component-schema-generation-1.json",
].into_iter().map(|path| RelativePath::new(path).expect("fixed local plugin input"))),
```

This names the local plugin source and its embedded predecessor schema while keeping Cargo's
dependency and build decisions authoritative. Extend this explicit list when adding another
local compiler input. The engine's own source/dependency graph stays selected by Cargo, not
copied or resolved by this helper. Keep `build.rs` and the Player's request-owned bootstrap.

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

Schema refresh updates the compiled recipe's owners and preserves complete current/predecessor
catalogs for other admitted project owners. A semantically unchanged bundle keeps its existing
file and spelling. Conflicting owner claims or invalid existing bundle data reject before writing;
restore or explicitly repair invalid input rather than treating `--write` as permission to erase
it. Retaining authoring metadata does not add a Runtime binding or approve installed tool code.

The first demonstration collects three targets and wins. The reset demonstration
restores the initial player position, three visible targets and original background.
Gameplay changes runtime state and leaves authored source intact. Collected targets
remain alive, so reset does not destroy and reconstruct persistent entities.

The generated README's Cook command remains usable after these additions. Its Player has no
scripted collection input and runs a finite headless baseline; the source `headless` demonstration
above separately proves win/reset. A cooked baseline does not establish interactive desktop
distribution. Keep the entire three-file installation together when relocating it.

## Select an optional pixel world

For a fixed pixel world, add this optional setting to `nara.toml`:

```toml
[render.world_canvas]
width = 320
height = 180
```

The world raster stays at 320 by 180 while the native window keeps its real size. IntegerFit
uses whole physical pixels with centered bars when it fits, and fractionally downscales the
complete image in a smaller window. Runtime UI is drawn afterward at native resolution.
The camera's `viewport_height` still controls world units; simulation positions remain fractional.
For a profile that needs native world rendering, use `render.clear_world_canvas = true` in
that profile's patch. A profile cannot both clear and replace the canvas.

Direct App consumers can use `RenderPlugin { world_canvas: Some(WorldCanvas::new(320, 180).unwrap()) }`;
repeatable product groups configure their Render slot with
`nara::render::plugin(Some(WorldCanvas::new(320, 180).unwrap()))`. Project consumers use the
manifest so Check and exported executables share one setting.

Use `nara::render::WorldPointer` as a system parameter for world picking:

```rust,ignore
fn pick_world(pointer: nara::render::WorldPointer, camera: nara::ecs::Query<nara::ecs::Entity,
    nara::ecs::With<nara::render::Camera2d>>) {
    for camera in &camera {
        // Supply a logical window position from the admitted gameplay input route.
        let position = nara::core::Vec2::new(160.0, 90.0);
        if let Some(world) = pointer.window_to_world(camera, position) {
            // Use the world position for this camera.
        }
    }
}
```

The conversion uses the last successfully presented camera. Bars and maximum edges return None;
resize, canvas changes and backend retirement require a matching new presentation. Respect the
input route's UI disposition before applying gameplay actions.

## Edit and verify game content

`PersistentComponent` supports bool, String, f32/f64 and fixed-width integers up to
64 bits. Narrow numeric domains travel with the schema, so an f32 field rejects a
finite value outside its range during validation. The collection game's
speed and radius use f32; their version-2 records use F64 wire values. Changing a
field's wire kind requires an explicit schema version and authored-content update.

In `scenes/startup.scene.json`, find `gem-right` and change the `radius` value of
`collect-game.Collectible` from `14.0` to `30.5`. Keep the `f64` wire type and the
component version. Save the file, validate it, and run the game with the edited data:

```powershell
& $nara check $game
cargo run --manifest-path "$game/Cargo.toml" --locked --features desktop --bin desktop
```

Walk right with D. The final gem is now collected from farther away. Restore the original
radius before rerunning the headless demonstration, which checks the sample's exact win tick.
After changing Rust schema, run `nara schema $game --write` and validate the updated source again.

## Try a content variant

After closing the game, replace `prefabs/player.prefab.json` with
`variants/fast-player/player.prefab.json`, run Check, then launch the desktop game again. The player
changes tint and moves faster without changing the plugin. Restore the original
Prefab from your own version control when done.

This is an explicit data mod example. It adds no discovery service, conflict
resolver, package manager or dynamic native-library ABI. All active content must
be reachable from startup or a declared Scene root for the export workflow.

Declare additional levels in the project manifest, with paths relative to the effective
`paths.scenes` directory:

```toml
[content]
scenes = ["second.scene.json"]
```

Check validates each declared Scene and its transitive dependencies. Export includes their exact
source union and excludes undeclared, unreferenced files. A declaration grants delivery, not
automatic loading or runtime residency. Code-first content consumers can use
`ProjectContentLoader::load_scene` to prepare one selected Scene without changing startup.
An authored Door can retain the destination's primary `AssetProductRef` and pass it directly to
`ProjectScenes`. The preparation worker resolves its actual Scene metadata and checks that its
source is startup or a declared Scene root. A Required reference also captures the destination
document as data, including finite A-to-B-to-A references; that membership alone grants no travel.
Observe preparation and explicitly commit the prepared result to instantiate the selected Scene.

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

Keep the sibling `project/` tree with the executables. It carries the complete declared
Scene deliveries for both the base Headless settings and the `desktop` profile. Each
executable checks its own startup agreement. Extra sources that neither delivery uses
are outside the package. A published candidate that fails product acceptance remains
available for inspection and Export reports the failure.

## Measure iteration time

Record the engine revision, platform, lockfile, features, build profile, and cache state.
Measure the first project build, a saved content change through the next observed game result,
and a Rust change through rebuild and the next result. Record the changed behavior alongside
its time. A run that reused the engine target is a cached observation.

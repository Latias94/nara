## Product workflow

This project was created from an immutable starter. Its generated files are project-owned copies.
The exact starter and Nara dependency coordinate are recorded above and in `Cargo.toml`; upgrades
are explicit migrations, not automatic starter updates.

The project manifest declares an authored `project.source_universe` and the project-wide
`project.native_document_identity = "stable_metadata"` policy. Creation prepares these identities
once, including `scenes/startup.scene.json.meta`; publication uses those exact prepared bytes.
Keep authored metadata with its source. Parsing, reopening and startup selection repair do not
generate replacement identity. The descriptive project `stable_id` is not the source universe.

The project has four explicit Cargo binary targets:

- `headless`, using the base `runtime-2d` product;
- `desktop`, enabled with the Cargo `desktop` feature and using the matching desktop capabilities
  declared by the `desktop` project profile;
- `cook`, a compiled headless adapter that prepares this project's content and publishes its Player;
- `player`, a source-free headless runtime that requires the expectation supplied by Cook.

### Your first fixed-step system

Create `src/first_step.rs` with this complete ordinary plugin:

```rust
use nara::{
    app::{PluginCategory, PluginDeclaration, PluginError, PluginId},
    prelude::{App, CoreStage, FixedTime, Plugin, Res},
};

const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("my-game.first-step"),
    PluginCategory::Runtime,
);

#[derive(Default)]
pub struct FirstStepPlugin;

impl Plugin for FirstStepPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::FixedUpdate, first_step)?;
        Ok(())
    }
}

fn first_step(time: Res<FixedTime>) {
    if time.tick() == 1 {
        println!("first_fixed_step: tick=1");
    }
}
```

Add `mod first_step;` in `src/lib.rs`. Keep its other helpers and replace only `product_recipe()`
with this function:

```rust
pub fn product_recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    nara::ProductRecipe::new()
        .add_runtime_image_codec::<nara::image::ImageRuntimeCodec>(
            nara::runtime_content::RuntimeCodecDefinitionId::new(
                "nara.starter.desktop-2d.image",
                1,
            ),
        )?
        .add_plugin::<first_step::FirstStepPlugin>()
}
```

Run `nara check .`, then `cargo run --bin headless`. Check must not print the gameplay marker;
the game prints `first_fixed_step: tick=1` followed by `starter_headless: ok fixed_ticks=1`.
This registers an ordinary fixed-step system through the existing plugin API. It needs no
persistent component, custom schema, window or Editor. Change the printed message and rerun to
observe a Rust edit. Keep the generated `product_recipe`, Image codec and all entry points.
The same plugin also runs in the cooked Player; its first tick uses the same game code.

Open the complete-schema startup Scene in the first-party Editor from the project root:

```text
nara-editor .
```

Select `main-camera`, edit `Camera2d.viewport_height` from `720` to `768`, then use the fixed
Undo, Redo, Save, and Reopen controls. The Editor writes the same ordinary Scene source consumed
by the commands below; it does not create an Editor-private project format.

For one observable authored-data edit, change the startup Scene's
`Camera2d.clear_color.r` value in `scenes/startup.scene.json` from `0.06` to `0.14`. No Rust source
change is required. Then check the project's declared content and run both products from any process
working directory:

```text
nara check .
cargo run --bin headless
cargo run --features desktop --bin desktop
cargo run --features desktop --bin desktop -- --verify-first-present
```

The final command opens the real Winit/WGPU product, requires one exact presented frame, and exits
through normal runtime cleanup.

Check, schema refresh, Export, and startup-Scene Repair build the existing headless executable and enter its explicit
tool mode before gameplay. Define custom systems, persistent components, and migrations once in
`product_recipe()` with `SchemaContribution`; the same recipe serves all targets and strict tools.
The starter recipe explicitly registers the stock `ImageRuntimeCodec` with definition
`nara.starter.desktop-2d.image`, revision 1. Keep that registration when adding game contributions so
Cook and Player agree on Image decoding.

The generated `package.metadata.nara.project-tool-protocol = 1` declares that entry contract. Superseded
entry/response shapes are unsupported; both the headless entry and CLI use this current contract.
Project Rust and Cargo code remain trusted native code. Default tooling has no window or graphics
backend; checking the explicit `desktop` profile builds its desktop feature set.

After changing persistent component definitions or their contributions, refresh the declared
data-only schema bundle explicitly, then reopen the project in the Editor:

```text
nara schema . --check
nara schema . --write
nara schema . --check
```

Schema Check reports a missing or stale bundle without changing it. Schema Write atomically
publishes the bundle generated from `product_recipe()` at `authoring.schema_bundle`; it does not
start gameplay or rewrite Scene/Prefab records. Pass `--profile desktop` when checking or writing
that profile's schema. Generic Editor Open reads the bundle without compiling project code.

After creating another Scene, such as `scenes/level-b.scene.json`, declare it in `nara.toml` so
Check and Export include it and its admitted Prefab, Image, TileSet, and metadata dependencies:

```toml
[content]
scenes = ["level-b.scene.json"]
```

These project-wide paths are relative to the selected profile's effective `paths.scenes` directory.
The selected startup Scene is always included implicitly. Declaring another Scene does not activate
it at runtime, and undeclared dynamic content is not included automatically.

### Cook and source-free Player

Run the project-owned cooker from the generated project root. This PowerShell example selects the
installed native toolchain, an existing output parent, and the project's reusable Cargo target:

```powershell
$projectRoot = (Get-Location).Path
$outputParent = (New-Item -ItemType Directory -Force ../installations).FullName
$targetDirectory = Join-Path $projectRoot 'target'
$cargoPath = (rustup which cargo).Trim()
$rustcPath = (rustup which rustc).Trim()
$targetTriple = ((& $rustcPath -vV | Select-String '^host: ').Line -replace '^host: ', '')
cargo run --bin cook -- --project-root "$projectRoot" --output-parent "$outputParent" --name game-player --target-dir "$targetDirectory" --target "$targetTriple" --cargo "$cargoPath" --rustc "$rustcPath" --profile release
```

`--project-root`, `--output-parent`, `--name`, `--target-dir`, `--target`, `--cargo`, and `--rustc`
are required. Directory and tool paths must be absolute; the project and output parent must already
exist. `--target-dir` can reuse an existing directory or be created by the build owner. `--name`
selects one absent portable child directory under the output parent. `--profile` selects the Cargo
profile and defaults to `release`; a custom profile must already be defined for Cargo. The selected
toolchain must support `--target`. Paths can contain spaces and are passed as separate arguments.

Cook ingests `nara.toml` with the default source profile and uses the actual `product_recipe()`.
It delegates content preparation, source and code revalidation, Player building, and whole-directory
publication to Nara's shared owners. It selects package `env!("CARGO_PKG_NAME")`, binary `player`,
the explicit target and Cargo profile, no additional features, and disabled default features. The
cooked content profile is `game-shipping`; it is separate from the Cargo profile and the source
project's `desktop` profile. This cooker and Player are headless.

Cook stores reusable artifact data under `<output-parent>/nara-cook-cache`. The shared Rust input
inventory covers `Cargo.toml`, `Cargo.lock`, root `build.rs`, and Rust files under `src/`. The initial
`cargo run` creates `Cargo.lock` if necessary; retain it for subsequent locked Player builds. Projects
that add other compiler inputs must explicitly extend the request's declared input inventory in
`src/bin/cook.rs`. Cargo retains dependency resolution and the original build process owner. Failed
child cleanup keeps that owner's process and target lease until cleanup completes. A recoverable
publication orphan is reported with its original evidence for explicit inspection.

`build.rs` tracks `NARA_CARGO_GENERATED_RUST_SOURCE`, writes the selected bootstrap only into
Cargo's `OUT_DIR`, and enables `nara_cooked_player` only for a request carrying that input. Keep this
reserved variable out of ambient environment and Cargo configuration. Ordinary Player builds refuse
to run, including when `OUT_DIR` still contains an earlier cooked bootstrap. No generated bootstrap
is written into project source.

On success, `STARTER_PACKAGE_PUBLISHED` reports the installation and executable. The installation
contains only `player` (`player.exe` on Windows), `content.pack`, and `package.json`. Move the whole
directory elsewhere and launch its Player from any working directory, for example:

```powershell
& "$outputParent/game-player/player.exe" --frames 120
```

Use `player` on Linux or macOS. Player opens only the executable-relative `content.pack` with the
compiled expected catalog digest, target triple, content profile, and game recipe. It has no path,
expectation, source-profile, or project-tool override and needs no project source, artifact cache,
Nara CLI, or Rust toolchain at runtime. Scene identities and startup selection come from the actual
authored `source_universe`, stable metadata, and declared content rather than sample IDs or counts.
`--frames N` runs complete managed frames, including normal plugin updates and Scene travel, with
the admitted fixed timestep as each frame's elapsed time. It defaults to 1 and accepts 1 through
200000; it does not impose a wall-clock deadline on plugin or operating-system work. Runtime cleanup
continues through the original run owner until it completes. `starter_player: ok` reports success.

### Source-carrying Export

Publish and accept a source-carrying declared-content candidate into an absent destination:

```text
nara export . --parent .. --destination candidate-before-repair
```

The candidate's `bin/headless` and `bin/desktop --verify-first-present` executables resolve the
sibling `project/` tree, so they can be launched from an unrelated working directory without this
checkout or a Rust toolchain. Add `.exe` to the executable names on Windows.

To exercise explicit recovery, move `scenes/startup.scene.json` and its `.meta` sidecar to
`scenes/repaired.scene.json` and `scenes/repaired.scene.json.meta`. Update the metadata's authored
path to `repaired.scene.json` while retaining its stable asset ID, then run:

```text
nara check .
nara repair startup-scene . --replacement repaired.scene.json
nara check .
cargo run --bin headless
cargo run --features desktop --bin desktop -- --verify-first-present
nara export . --parent .. --destination candidate-after-repair
```

Repair updates only the selected startup Scene after a fresh authoritative check. It does not infer
renames or rewrite arbitrary asset references.

This export is a source-carrying declared-content development and verification candidate. It is not
a complete project backup, cooked/source-free build, formal release package, Editor workflow, or
inventory of dynamically requested content. Edit authored content under `scenes/`, `prefabs/`, and
`assets/`; do not hand-maintain generated starter metadata as a second product configuration.

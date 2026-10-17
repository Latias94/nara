# Your first independent 2D game

Create a project outside the engine checkout, add a small Cargo gameplay plugin,
edit its content, and run an exported development candidate. The maintained result
is [Collect Game](../../samples/collect-game/README.md): WASD movement, three gems,
a green victory background, and R to reset.

The commands below use PowerShell on Windows. The Rust sources and Cargo workspace
are portable; Unix shells use their own environment assignment and executable
spelling. Native platform acceptance is recorded separately from compilation.

The ordinary Editor uses Dear ImGui with the same data-only Open, document history,
saved-source Play and export owners. Use Workspace to manage docked panels; native
detachment is enabled only by a supported backend profile. Windows/DX12 has native
workflow evidence; Linux/X11 and macOS release support remain unverified, and Wayland
uses in-window docking. Full native accessibility is not yet provided.

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

`PersistentComponent` supports bool, String, f32/f64 and fixed-width integers up to
64 bits. Narrow numeric domains travel with the schema, so an f32 field rejects a
finite value outside its range before publishing an edit. The collection game's
speed and radius use f32; their version-2 records use F64 wire values. Changing a
field's wire kind requires an explicit schema version and authored-content update.

1. Select `main-camera`. Change Camera2d's `viewport_height`, press Apply, then
   Undo and Redo. Save, close and reopen; the saved value must remain.
2. Select `gem-right`. Change the custom Collectible `radius` from 14.0 to 30.5,
   Apply, Undo and Redo. Save and reopen; the accepted fractional value remains.
   The generic Editor edits this declared scalar through its exact schema adapter.
   Its first-party native preview omits custom configuration without a native
   binding and keeps the source record intact. External Play uses the compiled
   game and consumes the saved radius. Unsupported records remain protected.
3. In the Project sidebar, select `textures/token.png` under assets. Import it
   with sRGB, then explicitly Reimport after changing its import recipe. Invalid
   image input reports failure while retaining the last accepted image.
4. A Sprite image field accepts `textures/token.png` or `stable_id:<UUID>` and
   applies through the same document revision transaction. The initial player
   Prefab is supplied as source; this walkthrough does not require a visual Prefab
   creator.
5. Save before Play. Play builds and starts a supervised external game window;
   Running requires the child's accepted first presentation. Focus that window
   and use WASD, collect all three gems, then press R. Stop closes the owned child.

Run `nara check $game` after saving. A valid schema describes the supported data
shape; strict Check and the compiled game still own runtime admission. The bounded
headless demonstration reaches victory earlier with the larger right gem radius
and reports a lower final player x. This gives an observable data-edit result
without relying only on successful process startup.

To change gameplay, Stop, edit `gameplay-plugin/src/lib.rs`, save and Play again.
Restart deliberately reuses the current executable generation. A deliberate
`compile_error!("learning the build loop");` in desktop.rs should yield compiler
diagnostics while authoring remains usable. Remove it and use Stop then Play.
The ordinary external lane does not supply Pause or Step.

## Try a content variant

With Play stopped, replace `prefabs/player.prefab.json` with
`variants/fast-player/player.prefab.json`, reopen and Check, then Play. The player
changes tint and moves faster without changing the plugin. Restore the original
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

The [committed-source acceptance record](../knowledge/engineering/verification/2026-10/2026-10-16T025031Z-first-author-workflow-u1-u6-closes-with-clean-committed-external-products-and-measured-iteration-7a2024c6678b43a48a069406d6eb729d.md)
records one Windows 11 observation at engine revision `78faa7c`, Rust 1.99.0,
one Cargo job, incremental compilation disabled, and an existing shared target:

| Interval | Observed time |
|---|---:|
| First external starter Check, including compiled tooling | 56.939 s |
| First starter desktop build/run through first presentation | 64.266 s |
| Authored speed save through changed headless outcome | 0.113 s |
| Rust movement expression save through changed headless outcome | 7.843 s |
| First ordinary Editor Play through READY | 67.229 s |
| Repaired ordinary Editor Play through READY | 7.553 s |

The data edit changed the observed final player x from 108.0 to 110.4; the Rust
edit changed it to 108.8. These command/result intervals are distinct from
save-to-visible-pixel interaction latency. The linked record includes exact
source/lockfile identity, features, profile, export, review and platform limits.
They are individual cached observations, not cold builds or percentile estimates.

The subsequent [Dear Editor committed-source acceptance](../knowledge/engineering/verification/2026-10/2026-10-17T102351Z-editor-foundation-u9-and-u1-u8-close-with-selected-dear-frontend-and-exact-external-products-b68c3a9f938a49b7ba63779f2849d0d9.md)
uses source `f4ffdd18`, Rust 1.95.0, Windows/DX12 and one job with the shared cache.
The same publicly created game passes Scene/Prefab creation/composition, compiler
failure/repair and exported-product execution. Its saved speed edit changes the
final player x `108 -> 110.4`; after restoring speed, its radius edit yields
`108 -> 92`. First/repaired Play READY take 113.829s/17.331s.
The first explicit Windows release cohort takes 12m49s to build; public Export
including both product acceptances takes 785.812s. These are separate cached
observations, not directly comparable cold-build timings or percentiles. The
record names the exact lock, features, platform and limitations.

An explicit integration check is available for a **fresh disposable external copy**.
It first creates empty Scene and Prefab entities, adds/edits/removes a custom
component, and verifies Undo/Redo and Save/Reopen through the ordinary document
owners. It restores these disposable source fixtures after closing that proof.
It then edits the Player Prefab speed, custom radius and camera, assigns an
imported image, rejects an out-of-f32-range edit, and exercises Undo/Redo,
Save/Reopen and real native game children. Strict compiled gameplay observes the
saved speed and radius changes. It also temporarily injects a compiler error
before restoring the Rust source:

```powershell
$env:NARA_AUTHOR_WORKFLOW_PROJECT = $game
cargo nextest run -p nara-editor --test first_game_workflow --run-ignored only --test-threads 1 --nocapture
```

This action-driven check proves custom data editing, persistence, runtime agreement,
supervision and READY. It does not claim to click the native Editor controls or play
the game through keyboard input.

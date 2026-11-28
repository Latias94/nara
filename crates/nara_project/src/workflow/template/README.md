# Nara 2D starter

From the project directory, run `nara play . --trusted` or `cargo run --features desktop` to open
a window with a spinning sprite. Change `game.Spin` in `assets/main.scene.ron` to adjust the speed.
The scene mounts `assets/sprite.prefab.ron`, which defines the sprite's size, tint, starting
transform, and PNG image. The development game resolves assets by their `.meta` IDs and cooks
the scene in memory. Move or rename an asset together with its `.meta` file to keep its references
working. Save scene changes before using Play in the editor. Stop closes the game.

## Export

Run `nara export . --out ../my-game-export --trusted` to build a release for the current desktop
platform. Open `game.exe` on Windows or `game` on macOS and Linux. Distribute the executable and
`game.npak` together. The game opens the archive beside the executable, regardless of the working
directory. The export builds with `--no-default-features --features desktop` and loads the cooked
scene and texture. If you change the entry scene, update its asset ID in `[delivery].scenes` in
`nara.toml` and `ENTRY_SCENE` in `src/main.rs`.

## Project checks and Schema

Run `nara check .` to validate the project manifest, Cargo package, and source assets. The project
manifest requires `[project]` with `name` and `version`. If you change `paths.assets`, pass the same
path to `RuntimeAssetSource::from_project_dir` in `src/main.rs`. Tooling can read optional `[window]`
metadata (`title`, `width`, and `height`). The running app configures its window in Rust.

Run `nara schema . --trusted` to build `src/bin/nara_schema.rs` and update `.nara/schema_cache.json`.
Run `nara schema .` to read the last cached catalog. Trust permits Cargo and project code to run.
Run `cargo run --no-default-features --features schema --bin nara_schema` to print the catalog
directly.

The Cargo manifest points at the local Nara checkout used to create this project.

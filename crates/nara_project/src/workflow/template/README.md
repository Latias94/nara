# Nara 2D starter

Run `nara play . --trusted` or `cargo run --features desktop` to open a window with a spinning
sprite. Change `game.Spin` in `assets/main.scene.ron` to adjust the speed. The scene mounts
`assets/sprite.prefab.ron`, which defines its size, tint, starting transform, and PNG image.
`src/main.rs` resolves all three assets by their `.meta` IDs, spawns the camera, and updates the
sprite's rotation. Move or rename an asset together with its `.meta` file to keep its references
working. The loader reads `Transform2d`, `Visibility`, `SpriteSource`, and `Spin`. In the
editor, save scene changes before using Play. Stop closes the game. Projects that implement the
`--editor-play PATH` snapshot and control protocol can opt in with `play = "snapshot"` under
`[editor]` in `nara.toml`.

Run `nara check .` to validate the project manifest, Cargo package, and source assets. The project
manifest requires `[project]` with `name` and `version`. If you change `paths.assets`, update
`ASSET_DIRECTORY` in `src/main.rs` as well. Tooling can read optional `[window]` metadata (`title`,
`width`, and `height`). The running app configures its window in Rust.

Run `nara schema . --trusted` to build `src/bin/nara_schema.rs` and update `.nara/schema_cache.json`.
Run `nara schema .` to read the last cached catalog. Trust permits Cargo and project code to run.

The Cargo manifest points at the local Nara checkout used to create this project.

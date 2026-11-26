# Nara 2D starter

Run `nara play . --trusted` or `cargo run --features desktop` to open a window with a spinning
sprite. `src/main.rs` configures the app, spawns the camera and sprite, and updates its rotation.

Run `nara check .` to validate `nara.toml` and the local Cargo package declaration. The project
manifest requires `[project]` with `name` and `version`. `[paths]` can set the project-relative
`assets` directory, which defaults to `assets`. Tooling can read optional `[window]` metadata
(`title`, `width`, and `height`). The running app configures its window in Rust.

Run `nara schema . --trusted` to build `src/bin/nara_schema.rs` and update `.nara/schema_cache.json`.
Run `nara schema .` to read the last cached catalog. Trust permits Cargo and project code to run.

The Cargo manifest points at the local Nara checkout used to create this project.

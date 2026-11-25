# Nara 2D starter

Run `nara play . --trusted` or `cargo run --features desktop` to open the game. The scene is
set up in code so the project can run before it has any authored scene files or a Schema cache.

Run `nara schema . --trusted` to build the project-owned exporter and update
`.nara/schema_cache.json`. Run `nara schema .` to read the last cached catalog.

The Cargo manifest points at the local Nara checkout used to create this project.

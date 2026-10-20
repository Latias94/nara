## Product workflow

This project was created from an immutable starter. Its generated files are project-owned copies.
The exact starter and Nara dependency coordinate are recorded above and in `Cargo.toml`; upgrades
are explicit migrations, not automatic starter updates.

The project has two explicit product targets:

- `headless`, using the base `runtime-2d` product;
- `desktop`, enabled with the Cargo `desktop` feature and using the matching desktop capabilities
  declared by the `desktop` project profile.

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
`product_recipe()` with `SchemaContribution`; the same recipe serves both binaries and strict tools.
The generated `package.metadata.nara.project-tool-protocol = 2` declares that entry contract. Older
projects, including version 1 entries, must migrate their headless entry before using these commands.
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

Publish and accept a source-carrying declared-content candidate into an absent destination:

```text
nara export . --parent .. --destination candidate-before-repair
```

The candidate's `bin/headless` and `bin/desktop --verify-first-present` executables resolve the
sibling `project/` tree, so they can be launched from an unrelated working directory without this
checkout or a Rust toolchain. Add `.exe` to the executable names on Windows.

To exercise explicit recovery, move `scenes/startup.scene.json` to
`scenes/repaired.scene.json` with your editor or file manager, then run:

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

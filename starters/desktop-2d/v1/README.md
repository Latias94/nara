## Product workflow

This project was created from an immutable starter. Its generated files are project-owned copies.
The exact starter and Nara dependency coordinate are recorded above and in `Cargo.toml`; upgrades
are explicit migrations, not automatic starter updates.

The project has two explicit product targets:

- `headless`, using the base `runtime-2d` product;
- `desktop`, enabled with the Cargo `desktop` feature and using the matching desktop capabilities
  declared by the `desktop` project profile.

For one observable authored-data edit, change the startup Scene's
`Camera2d.clear_color.r` value in `scenes/startup.scene.json` from `0.06` to `0.14`. No Rust source
change is required. Then check the admitted startup closure and run both products from any process
working directory:

```text
nara check .
cargo run --bin headless
cargo run --features desktop --bin desktop
cargo run --features desktop --bin desktop -- --verify-first-present
```

The final command opens the real Winit/WGPU product, requires one exact presented frame, and exits
through normal runtime cleanup.

Publish and accept a source-carrying startup-closure candidate into an absent destination:

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

This export is a source-carrying startup-closure development and verification candidate. It is not
a complete project backup, cooked/source-free build, formal release package, Editor workflow, or
inventory of dynamically requested content. Edit authored content under `scenes/`, `prefabs/`, and
`assets/`; do not hand-maintain generated starter metadata as a second product configuration.

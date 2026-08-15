# Nara desktop-2d/v1 project

This project was created from the immutable `desktop-2d/v1` starter. Its generated files are
project-owned copies. The selected Nara dependency is pinned in `Cargo.toml`; upgrades are explicit
migrations, not automatic starter updates.

The project has two explicit product targets:

- `headless`, using the base `runtime-2d` product;
- `desktop`, enabled with the Cargo `desktop` feature and using the matching desktop capabilities
  declared by the `desktop` project profile.

Run the products from any process working directory:

```text
cargo run --bin headless
cargo run --features desktop --bin desktop
cargo run --features desktop --bin desktop -- --verify-first-present
```

The final command opens the real Winit/WGPU product, requires one exact presented frame, and exits
through normal runtime cleanup. Edit authored content under `scenes/`, `prefabs/`, and `assets/`;
do not hand-maintain generated starter metadata as a second product configuration.

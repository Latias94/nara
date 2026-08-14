# Nara desktop-2d/v1 project

This project was created from the immutable `desktop-2d/v1` starter. Its generated files are
project-owned copies. The selected Nara dependency is pinned in `Cargo.toml`; upgrades are explicit
migrations, not automatic starter updates.

The project has two explicit product targets:

- `headless`, using the base `runtime-2d` product;
- `desktop`, enabled with the Cargo `desktop` feature and using the matching desktop capabilities
  declared by the `desktop` project profile.

Run `cargo check --all-targets --all-features` to verify the generated Rust and product manifests.
Edit authored content under `scenes/`, `prefabs/`, and `assets/`; do not hand-maintain generated
starter metadata as a second product configuration.

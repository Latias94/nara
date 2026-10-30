# Independent UI module consumer

This small project exercises only the public `nara_text`, `nara_ui_kernel` and `nara_ui_widgets`
interfaces. Its tests shape and rasterize a label, compile an immutable status panel, and compile
a controlled button with focus and semantic actions. A fourth test constructs complete binary
persistent identities without Scene types: distinct contexts stay separate, rebuilding the same
root/local composition preserves keys, and both buttons retain focus and semantic actions. The
independent Image consumer compiles an opaque Product-shaped name and two provider-scoped external
tokens into a retained frame without importing Asset, ECS or renderer types. It creates no native
window or GPU surface and cannot manufacture presentation authority or persistent asset bindings.

The separate Cargo workspace and lockfile keep this dependency graph independent of the engine
and the broader module-consumer project. Building alongside their ECS, Scene or renderer
dependencies would weaken this isolation check.

Run from the repository root, using the shared `target` directory through `CARGO_TARGET_DIR`:

```text
cargo nextest run --manifest-path tests/fixtures/ui-module-consumer/Cargo.toml --locked --test-threads=1
```

Run Cargo commands serially. The [fixture guide](../README.md) describes the other consumer projects.

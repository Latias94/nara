# ADR-001 Workspace and dependency boundaries

**Accepted on 2026-11-24**

Organize Nara around the headless core, game runtime, and project and editor layers shown in the
[architecture overview](../overview.md). Group code by the product workflow that owns it. Keep root
`nara` a small facade with `DefaultPlugins` and `prelude`.

- Keep the core dimension-independent and free of GPU and windowing dependencies. Keep shipping
  games free of `nara_project`, `nara_editor*`, and Open-GPUI packages. Keep `nara_editor_core` free
  of UI frameworks. Only the desktop shell wires these pieces together.
- Use the same WGPU source, revision, and version across Nara and Open-GPUI. The v2 baseline pins
  WGPU 30.0.0 to `cddb2f5d80dc0009b2ef6767635140cacf90a007`. Require one `wgpu` package identity
  per lockfile and compare the complete sources. Run
  `cargo tree --locked --workspace --all-features --invert wgpu --depth 1` to detect an ambiguous
  package selection and list the workspace packages that use WGPU.
- Use the normal crate APIs or consolidate the owners when crates need shared internals. Eliminate
  `pub mod __private`, first-party anti-tamper hooks, manual byte budgets, and private fault routing.
- Express library failures as errors and retain ordinary resource lifetimes. Eliminate
  `std::process::abort()` and intentional GPU leaks through `mem::forget`.
- Keep the old baseline in Git at `legacy/v0-imgui-reference` and `legacy/v0-pre-slim`. Delete
  superseded code when the owning phase retires it. Keep the active feature graph free of `legacy-*`.

The Nara target is 16 packages. The named layout in the handoff spans 18 Cargo packages when the
facade and both derive crates are counted. Confirm any final consolidation against the ownership
boundaries and real consumers. Keep the facade within 1,500 lines and review regular crates above
30,000 lines. Target cold CI under 15 minutes and warm CI under 3 minutes. Maintain one active
roadmap, at most 25 active documents, and ADRs of at most 150 lines each.

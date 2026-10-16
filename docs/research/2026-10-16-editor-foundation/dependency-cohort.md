# Editor foundation dependency cohort

Date: 2026-10-16. This is a source-based upgrade recommendation, not an upgraded or compiled dependency graph. No Cargo command, production Rust edit, or Git mutation was performed. The [Dear ImGui integration note](dear-imgui-020.md) retains its exact v0.20.0 research baseline.

## Current requirements and resolved versions

The following facts come from Nara's root `Cargo.toml:5,53,94–104,123–131` and current `Cargo.lock`. A semver requirement is not a lockfile resolution; an exact pin also prevents ordinary patch updates.

| Boundary | Manifest requirement | Current resolution |
| --- | --- | --- |
| Nara MSRV | Rust 1.95 | Declared minimum; not independently tested here |
| Bevy ECS, reflection, macro logic/utils | `0.19` | 0.19.0 across the Bevy packages present |
| egui, egui-wgpu, egui-winit, egui_kittest | `0.36.1` | 0.36.1; semver permits 0.36.2 |
| WGPU | `=30.0.0` | wgpu/core/hal/naga 30.0.0 |
| Winit | `=0.30.13` | 0.30.13 |
| Nara glam | `0.30` | 0.30.10; Bevy reflection separately brings glam 0.32.1 |

The lock also contains wgpu-types 29.0.4 through Bevy reflection and 30.0.0 through WGPU. Duplicate versions are an investigation target, not automatically a bug. Nara publicly re-exports `Vec2`, `Vec3`, and `Mat3` from its own glam version (`crates/nara_core/src/lib.rs:14`), making a glam update an API decision.

The local `repo-ref/bevy` is `v0.16.0-rc.4-3688-g4e6bd141e`, not the source for either the locked 0.19 dependency or the latest release. Do not infer current migration APIs from that checkout. The Dear ImGui reference is tag 0.20.0 plus two later build-support commits, as recorded separately.

## Verified upstream choices

| Dependency | Official release state on the research date | Recommended treatment |
| --- | --- | --- |
| WGPU | Stable 30.0.1 | Upgrade the current patch pin before native editor acceptance |
| Winit | Stable 0.30.13; 0.31.0-beta.3 is a prerelease | Keep the stable route used by the editor backends |
| Bevy | Stable 0.20.0, released October 8 | Evaluate and migrate the ECS/reflection/macro cohort together during foundation work |
| egui | Stable 0.37.0, released October 15; 0.36.2 maintenance patch exists | Patch the retained adapter; make 0.37 a deliberate product decision |
| dear-imgui-rs | Stable 0.20.0, released October 12 | Use the matching core/platform/renderer release train for the probe |

Sources: [WGPU 30.0.1](https://github.com/gfx-rs/wgpu/releases/tag/v30.0.1), [Winit releases](https://github.com/rust-windowing/winit/releases), [Bevy 0.20](https://github.com/bevyengine/bevy/releases/tag/v0.20.0), [egui releases](https://github.com/emilk/egui/releases), [Dear ImGui release](https://github.com/Latias94/dear-imgui-rs/releases/tag/v0.20.0). “Latest” is time-bound and must be checked again when implementation begins.

### WGPU: a concrete platform repair

30.0.1 fixes a non-Windows Vulkan acquire-fence validation error introduced in 30.0.0 and changes Metal color-space constant resolution. These matter directly to Linux/macOS editor presentation. This is a justified patch update, not a reason to redesign the rendering abstraction. It also updates browser bindings, so inspect resulting lock changes even though Web is deferred. [Official fixes](https://github.com/gfx-rs/wgpu/releases/tag/v30.0.1).

Keep one WGPU 30 device lineage shared by Nara and the adapter. Resolve the compatible core/hal/types packages normally; their individual patch numbers need not all equal the public wgpu package. Do not manually force unrelated transitive packages to identical versions. The public wgpu crate declares Rust 1.87, independently of its development workspace's higher minimum. [Package manifest](https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu/Cargo.toml#L12).

### Bevy: migrate a cohort, not the entire engine

Nara's derive crate calls `bevy_ecs_macro_logic::component::DeriveComponent` and `bevy_macro_utils::derive_label` directly. Update those, `bevy_ecs`, `bevy_reflect`, their derive packages, and resolved platform/tasks/utils together. Mixing generated 0.19 implementation details with a 0.20 runtime would undermine the intended facade.

The official migration guide changes lifecycle observer syntax from `On<Remove, T>` to `On<Remove<T>>`, unifies exclusive/ordinary function systems, changes system access metadata, and removes `ComponentInfo::id()`. Nara has lifecycle/observer admission and transaction tests plus a custom system implementation in `src/startup_scene.rs`; these are meaningful review targets rather than a blind dependency edit. [Migration guide](https://bevy.org/learn/migration-guides/0-19-to-0-20/).

The rationale is to settle Nara's macro and lifecycle boundaries on the current stable cohort while they are still being refactored. The upgrade does not supply document creation templates, editor modularity, or arbitrary project-code loading. Preserve existing transaction/lossless-authoring invariants and re-run their behavioral tests; do not add a compatibility layer to maintain both Bevy minors.

Importantly, top-level Bevy and its Dear ImGui renderer integration require Rust 1.97.1, while the directly relevant [bevy_ecs](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_ecs/Cargo.toml#L11) and [bevy_reflect](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_reflect/Cargo.toml#L10) packages declare 1.95.0. Several companion packages declare no explicit MSRV. Do not raise Nara's minimum merely by copying the whole engine's requirement; prove the actually resolved feature graph on the declared minimum. Using only Bevy ECS is not a reason to adopt `dear-imgui-bevy`.

Reflection 0.20's glam feature targets 0.33.2, whereas Dear ImGui's optional glam interoperability targets 0.34. These are distinct type boundaries. For Nara, explicitly decide whether upstream glam reflection is needed: align Nara's public math cohort if that interoperability is required, or remove an unused feature after checking callers. Do not build conversion infrastructure simply to erase harmless lockfile duplication. [Reflection manifest](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_reflect/Cargo.toml#L111), [Dear ImGui compatibility](https://github.com/Latias94/dear-imgui-rs/blob/v0.20.0/docs/COMPATIBILITY.md#L27).

### UI and windowing: avoid two migrations for one result

egui 0.37 brings system-font/glyph integration and accessibility API improvements. It uses Rust 1.95, WGPU 30 and Winit 0.30.13, but also moves its AccessKit cohort. If the egui adapter remains a supported product, those improvements deserve an explicit assessment. If it is about to be retired after the Dear ImGui gate, first take the smaller 0.36.2 fix for focused `DragValue` updates and defer the breaking UI rewrite. [0.37 notes](https://github.com/emilk/egui/releases/tag/0.37.0), [0.37 manifest](https://github.com/emilk/egui/blob/0.37.0/Cargo.toml#L23), [0.36.2 fixes](https://github.com/emilk/egui/releases/tag/0.36.2).

Winit 0.31 beta changes input/window/event-loop APIs. It is not compatible with the published Dear ImGui 0.20 Winit route merely because both provide windows. Its newer popup support also does not establish global Wayland positioning for Dear ImGui native detachment. Keep 0.30.13 until a concrete requirement and compatible SDK route justify another migration. [Prerelease notes](https://github.com/rust-windowing/winit/releases/tag/v0.31.0-beta.3).

## Implementation boundary

Add one dependency-baseline unit before new editor acceptance: apply WGPU fixes; migrate the selected Bevy cohort and resolve the math-feature decision; patch the still-live egui adapter; preserve Winit 0.30.13; then build the Dear ImGui probe on those resolved versions. Verify macro fixtures, lifecycle/transaction behavior, document undo/save/reopen, native first-present and shutdown, plus the declared MSRV. Record the exact lockfile and platform results. An upgrade is complete when those workflows pass, not when version numbers change.

## U9 selected implementation cohort

The sections above preserve the pre-migration research baseline. U9 selects the following resolved graph; verification belongs in the implementation ledger and engineering evidence.

| Boundary | Selected resolution | Reason |
| --- | --- | --- |
| Nara compiler minimum | Rust 1.95 | The standalone Bevy ECS/reflection graph supports this minimum; no top-level Bevy dependency is added |
| Bevy ECS, reflection and macro family | 0.20.0 | Migrate the lifecycle and derive substrate together before changing editor contracts |
| Public Nara math and Bevy reflection math | glam 0.33.12 | Public Vec2/Vec3/Mat3 must participate in the enabled reflection contract; a real reflected-struct round-trip covers the boundary |
| Retained egui family | 0.36.2 | Take the maintenance fix while the replacement gate remains pending; avoid a second breaking widget migration |
| WGPU / WGPU HAL | 30.0.1 / 30.0.1 | Apply both the public patch and the implementation package containing the native platform repairs |
| Winit | 0.30.13 | Retain the stable route shared by the current editor and researched Dear ImGui backend |

All six WGPU-bearing project locks include HAL 30.0.1. Other WGPU subpackages follow ordinary compatible resolution rather than a synthetic equal-patch rule.
The two presentation fixture locks required updating the coupled wasm-bindgen/js-sys/web-sys/futures packages together; this is dependency resolution, not a Web support claim.
The startup Scene source uses a handwritten SystemParam, whose access registration now delegates through SystemAccess; it does not implement a handwritten System.
No ComponentInfo::id migration or macro-logic wrapper was needed in Nara's callers.

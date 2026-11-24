# Nara and Open-GPUI v2 refactor

Run Phases 0 through 5 in order. Verify each exit gate and commit the completed phase with
Conventional Commits before starting the next. [The architecture overview](../architecture/overview.md)
indexes the accepted decisions. Use the detailed specs in the v2 handoff bundle for implementation.
`04_rfc1_rfc3_rfc5_gpu_editor_ui_seams.md` covers GPU and editor integration,
`05_rfc2_core_runtime_api_and_scale_seams.md` covers runtime, and
`06_rfc4_schema_scene_prefab_and_cook_spec.md` covers authoring and cooked formats.

| Repository | Refactor branch | v0 baseline |
| --- | --- | --- |
| Nara | `refactor/v2-clean-architecture` | `legacy/v0-imgui-reference` |
| Open-GPUI | `refactor/v2-slim-wgpu` | `legacy/v0-pre-slim` |

Use Windows and PowerShell for the local desktop gates. Use Rust 1.95 and edition 2024, reuse
`target`, and run one Cargo command at a time per checkout. Set `CARGO_BUILD_JOBS=1` for substantial
local builds. During integrated development, use an explicit local Cargo override for the affected
Open-GPUI crates and keep the resolved WGPU source and revision identical in both repositories.
Keep `repo-ref/` read-only.

Measure reductions against the tagged trees with the same package and source-counting rules for
both revisions. Report tracked project Rust source and workspace package counts separately from
vendored, generated, documentation, and fixture data. Record wall-clock CI time with its cache
state, runner, and job set. Local estimates and measured hosted runs are different evidence.

## Phase 0 — Preserve the baselines and reset the contracts

**Entry.** Inspect both checkouts. Preserve any Dialogue Graph and GPUI prototype work before
creating the refs. On the supplied `main` baseline the prototype is already tracked at
`tests/fixtures/gpui-engine-consumer/`.

- Create both annotated baseline tags and their v2 branches. Remove identified compiler dump and
  untracked trybuild output files.
- Retire v0 ADRs, plans, knowledge, audits, research, and benchmark records from the v2 worktree.
  Preserve the Design Kit v0.4 subtree byte for byte. Introduce the overview, eight short ADRs,
  this roadmap, and the new agent entry point.
- Keep the guide and four benchmark inputs still consumed by v0 root tests until the tests are
  retired. Historical links in retained v0 material refer to paths in the Nara baseline tag.

**Exit.** Both working trees are clean. Both annotated tags resolve to the original commits.
Nara's `docs/` contains at most 20 Markdown files. The root workspace compiles and the tracked
Rust sources, Cargo configuration, and Design Kit match the baseline.

## Phase 1 — Remove superseded code and contract tests

**Entry.** Phase 0 exit gates pass.

### Nara

- Remove the empty audio, tooling, source-tool host, and editor canary directories where present.
- Retire the ImGui and drawn editor frontends, native shells and entry points, and
  `vendor/dear-imgui-wgpu`. Retain reusable typed `value_draft` and field draft logic for Phase 4.
- Remove the unused WGPU test renderer and `history*` implementation and Winit accessibility
  integration. Replace the `vendor/notify` patch with upstream `notify`. Retain `vendor/winit` with
  the native retirement criteria in [ADR-007](../architecture/adr/ADR-007-editor-architecture-and-ui-seam.md).
- Retire synthetic boundary fixtures and reverse trybuild tests that assert removed constraints.
  Keep the Dialogue Graph prototype recoverable from the v0 tag. Preserve useful coverage of
  game behavior in `reference-game`, `samples/two-level-game`, and focused crate tests.
- Remove the v0 editor host and authoring modules from root `nara`. Move reusable headless draft
  parsing and editing state into `nara_editor_core` and preserve the rest of the prototype in Git.
  Eliminate the editor feature path from every game target. Keep the game runtime, project CLI,
  and editor-independent format tests buildable.
- Reduce CI to format, workspace check and focused tests, reference-game, and portable Wasm core.
  Check WGPU identity, runtime dependency direction, and additions of private tunnels, process
  aborts, and teardown leak workarounds. Enforce zero remaining instances after the owning runtime
  and GPU replacements land.

### Open-GPUI

- Remove legacy docking and obsolete Zed, canvas, mobile, and web packages. Remove callers and
  dependencies tied to the retired surfaces. Retain the Windows WGPU native example and the desktop
  text, input, accessibility, and windowing needed in Phase 2.
- Align manifests and lockfiles with the Nara WGPU pin. Keep `dockspace` and the native docking
  path intact for the editor workflow.

**Exit.** Both workspaces resolve with the named obsolete editor and desktop UI code removed.
Nara passes the locked workspace check, focused core tests, portable Wasm compilation, and the
reference-game checks. Open-GPUI's Windows `examples/wgpu-native` example builds and runs with the
same WGPU source and revision. Use the Phase 1 job set to assess the 25-minute cold CI estimate.
Phase 3 owns the 15-minute cold CI target. The renderer, runtime, platform, and widget changes
scheduled in Phases 2–4 account for the remaining package and source reductions.

### Phase 1 baseline comparison

Counts cover Rust files tracked in each repository. Project source includes tests and examples.
Fixture source counts Rust files under `fixtures/` and `.golden.rs` files. Both exclude vendored
code, documentation, local build outputs, and reference checkouts. Workspace packages include
examples and tooling listed as members. The `crates/` column counts only members in that directory.

| Repository | Workspace packages | Packages under `crates/` | Project Rust lines | Fixture Rust lines |
| --- | ---: | ---: | ---: | ---: |
| Nara | 39 → 38 | 37 → 37 | 1,238,943 → 827,547 | 196,631 → 6 |
| Open-GPUI | 47 → 28 | 38 → 22 | 1,051,969 → 678,381 | 12,227 → 10,496 |

Nara removed 21 fixture directories and reduced project and fixture Rust lines by 42.4 percent.
Open-GPUI reduced project and fixture Rust lines by 35.3 percent. Both workspaces and the Nara
game lockfiles resolve WGPU 30.0.0 at `cddb2f5d80dc0009b2ef6767635140cacf90a007`.

### Phase 1 verification

On Windows 11 with Rust 1.95.0, both workspaces passed their locked all-feature, all-target
checks. Nara passed the Wasm `runtime-core` check, 1,116 core tests, 715 focused watcher, render,
window and process tests, 17 two-level continuity tests, 30 reference-game tests with desktop
enabled, and six CLI and reference-game contract tests. The generated-project test built the
independent game. The reference game's real Windows window passed `--verify-first-present`.

Open-GPUI passed 82 core utility tests, three independently locked consumer checks, all seven
native WGPU example smokes, and the Dockspace editor's native and recovery smokes. The native
runs used an NVIDIA GeForce RTX 4090. The Linux and macOS jobs remain part of hosted verification.

On November 25, 2026, the Nara CI commands passed locally on Windows 11 with an i9-13900KF,
Rust 1.95.0, one Cargo build job, and an empty Cargo target directory per job. The commands reused
the downloaded registry and Git sources. Formatting took 47 seconds, the workspace job took
9 minutes 18 seconds, the reference-game job took 6 minutes 3 seconds, and the Wasm check took
1 minute 53 seconds. The serial total was 18 minutes. GitHub-hosted timing, including the Linux
job and runner setup, remains to be measured.

## Phase 2 — Unify GPU and desktop platform integration

**Entry.** Phase 1 passes and RFC-1 defines the shared GPU and color contracts.

- Implement [ADR-003](../architecture/adr/ADR-003-shared-gpu-rendering-and-color.md) in both
  repositories. Replace the Windows D3D11 and macOS Metal renderers with `gpui_wgpu` while retaining
  DirectWrite, CoreText, IME, accessibility, and native window behavior. Consolidate the platform
  shell in `gpui_platform`. Support shared context injection and peer top-level windows.
- Consolidate Nara's renderer, implement texture and frame buffer reuse, and remove synchronous
  frame submission stalls and device teardown workarounds. Enable dirty tile chunk caching,
  direct game swapchain rendering, and zero-copy viewport import with completion-based leases.

**Exit.** Run a Windows two-window Mode A demo with one shared device and a 120 FPS sprite and
tilemap viewport. Verify mixed-DPI dragging, resize, and CJK IME. Capture steady-state GPU evidence
for zero transient texture or buffer creation, zero full-screen copies, and zero synchronous GPU
waits. Verify direct swapchain rendering in a standalone Winit game with no Open-GPUI dependency.
Qualify the macOS and Linux behaviors on their native platforms before claiming parity.

## Phase 3 — Rebuild the core and project workflow

**Entry.** Phase 2 passes and RFC-2 defines the public runtime and project API.

- Adopt the ECS, task, fixed-step, transform, input, audio, and physics decisions in
  [ADR-002](../architecture/adr/ADR-002-ecs-app-spatial-and-audio.md). Consolidate identity,
  diagnostics, and VFS in `nara_core`. Retire the gameplay queue, task pool, fault routes,
  quarantine, and private runtime driver tunnels.
- Consolidate the five game UI and text packages into `nara_ui`. Move manifests, starters, builds,
  Schema discovery, supervised Play, Cook, Export, and the `nara` CLI to headless `nara_project`.
  Keep the root facade within its budget. Use the current data formats until the Phase 5 cutover.

**Exit.** Build and test the core workspace and a new reference game using public APIs. Demonstrate
movement, sprite animation, tilemap, collision, sound, and HUD in at most 100 lines. Eliminate private
crate tunnels, process aborts, and intentional teardown leaks in first-party production Rust. Measure
cold CI below 15 minutes.

## Phase 4 — Deliver the desktop editor workflow

**Entry.** Phase 3 passes and RFC-3 defines the editor and supervised Play behavior.

- Build the unstyled `gpui_base` controls and thin GPUI docking adapter. Keep library primitives,
  document state, and appearance independent of the editor shell.
- Implement synchronous `EditorSession`, multi-select and mixed values, merged gesture Undo, typed
  drafts, registry, picking, and Gizmos in `nara_editor_core`. Integrate project builds, Schema
  refresh, external Play, and streamed Console output using `nara_project`.
- Implement Project Manager, Hierarchy, Inspector, Scene View, Asset Browser, Console, and toolbar
  in the Design Kit shell. Recover the Dialogue Graph prototype from the v0 tag and integrate
  `jellyflow-core` against the current document and Undo path.

**Exit.** In the real reference game, drag an asset into the scene, edit multiple entities and a
Rust component, drag a Gizmo and undo or redo it, float the Inspector onto a second display, name
an entity through CJK IME, save, Play and observe the change, then Stop with no surviving game
process. Pass `nara_editor_core` headless tests and verify its UI-framework-free dependency graph.

## Phase 5 — Cut over formats and qualify delivery

**Entry.** Phase 4 passes and RFC-4 defines Schema, compact authoring formats, and the cooked layout.

- Implement derived Schema, packed fields, `FieldPath`, UUID sidecars, compact RON, nested prefab
  overrides, and unknown-component round trips. Cook the dependency closure into `.nscn` and `.npak`.
  Update `reference-game`, `samples/two-level-game`, and `starters/desktop-2d` at the cutover.
- Qualify data hot reload and measure edit-to-result latency. Integrate retained `gpui-fast` layout,
  read tracking, and frame reuse so texture updates can redraw without relaying out static panels.
- Export the reference game as an executable with cooked assets and verify an independent launch.

**Exit.** A comparable tilemap scene is at least 10 times smaller than v0 and cooked loading takes
under 2 ms. The editor UI thread takes under 0.5 ms per frame with static panels and a 120 FPS
viewport. The exported game runs on a clean machine. Aim for 120,000–180,000 lines of Nara source
and 120,000–160,000 lines of Open-GPUI source with the final workspace boundaries in place.

## Open-GPUI target

| Package | Responsibility |
| --- | --- |
| `gpui` | Entities, elements, layout, events, focus, retained frames, and window model |
| `gpui_wgpu` | Unified renderer, atlas, and external image imports |
| `gpui_platform` | Windows, macOS, and Linux native services |
| `gpui_macros` | Core derives and action macros |
| `gpui_base` | Unstyled interaction primitives |
| `gpui_docking` | Thin adapter to the pure Rust `dockspace` model |
| `gpui_testing` | Headless platform, input simulation, and rendering tests |

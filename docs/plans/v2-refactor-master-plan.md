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

### Phase 2 verification

The renderer consolidation reduced the Nara workspace from 38 to 30 packages. Open-GPUI's
platform consolidation reduced its workspace from 28 to 24 packages. Both use WGPU 30.0.0 at
`cddb2f5d80dc0009b2ef6767635140cacf90a007`, including the independent shared-viewport workspace.
Using the Phase 1 counting rules, Nara project Rust source moved from 827,547 to 830,883 lines,
and Open-GPUI project Rust source moved from 678,381 to 668,470 lines. The workspaces contain
6 and 10,496 lines of Rust fixtures, respectively.

On Windows 11 with a GeForce RTX 4090 and DX12, the release shared-viewport example presented
animated sprites and 480 tile cells in two peer native windows. A two-second sample on a 144 Hz
display recorded 291 viewport submissions at 144.5 FPS and zero Inspector submissions. The viewport
replayed the same GPUI scene after each image update. The native smoke also verified resize,
paused refresh, and shared-queue completion. A resource-pressure run reproduced a deferred
texture allocation and verified that the viewport keeps its previous image and resumes after
resize. [Example and measurement procedure](../../examples/shared-viewport/README.md)

PIX exported 12 warmed-up Mode A frames with zero in-frame texture or buffer creations and zero
texture copies in the application commands. The symbolized release timing trace found no GPU wait
stacks in steady-state frame work. Forcing WGPU's DX12 `Wait` option on the same executable
produced 83 surface-acquisition wait samples. Frame rate counts native submissions. Scanout is
paced by the compositor.

Windows manual testing exercised Microsoft Pinyin composition and candidate commit, and a viewport
move from 150% to 125% DPI updated its physical render extent. During later automated retesting,
Windows deactivated the secondary outputs just after temporary extension, including with no demo
running. Win32 DPI readback changed for both windows, and the viewport resumed presenting at the
primary monitor's scale. A sustained final-build mixed-DPI drag remains to be repeated on a stable
extended desktop.

The standalone Winit reference game passed six desktop tests and presented its first playable
frame in a real Windows window. Its stock scene and HUD drew into the same swapchain image with
one submission. A separate six-frame PIX capture recorded zero texture-to-texture copies and two
glyph uploads. The standalone dependency tree contains no Open-GPUI packages.

Both locked Windows workspaces passed all-feature, all-target checks. Nara passed 1,059 renderer
tests and 11 targeted native GPU tests, and its portable Wasm core compiled. The Open-GPUI Linux
feature matrix and 212 tests passed on WSL2. Six additional native X11/Wayland tests passed with
isolated Xvfb and Weston displays and Mesa llvmpipe. The macOS platform passed the Apple Silicon
all-feature, all-target cross-check. Native AppKit and hardware Linux results remain separate
qualifications.

## Phase 3 — Rebuild the core and project workflow

**Entry.** RFC-2 defines the public runtime and project API. The Phase 2 native checks listed
above remain open as platform qualifications. Phase 3 work proceeds with those checks deferred.

- Adopt the ECS, task, fixed-step, transform, input, audio, and physics decisions in
  [ADR-002](../architecture/adr/ADR-002-ecs-app-spatial-and-audio.md). Consolidate identity,
  diagnostics, and VFS in `nara_core`. Retire the gameplay queue, task pool, fault routes,
  quarantine, and private runtime driver tunnels.
- Consolidate the five game UI and text packages into `nara_ui`. Move manifests, starters, builds,
  Schema discovery, supervised Play, and the `nara` CLI to headless `nara_project`. Keep the root
  facade within its budget. Phase 3B retires the v0 Scene and pack formats before Phase 5.

**Exit.** Build and test the core workspace and a new reference game using public APIs. Demonstrate
movement, sprite animation, tilemap, collision, sound, and HUD in at most 100 lines. Eliminate private
crate tunnels, process aborts, and intentional teardown leaks in first-party production Rust. Measure
cold CI below 15 minutes.

### Phase 3B: remove the v0 runtime

The Phase 3 package consolidation kept the old hosted scene and pack system alive while its
owners changed. Phase 3B retires it. The v0 tag preserves the two-level game and the previous
reference game. The live reference game, starter, and examples construct their worlds in Rust.

- Delete the v0 root project host, full scene and prefab stack, cooked pack and publication
  contracts, and the NT filesystem capability backend. Keep native Bevy relationships and
  incremental spatial propagation, VFS paths and atomic writing, and typed asset storage.
- Keep the game runnable through Winit and the shared viewport runnable through the same WGPU
  device and queue as Open-GPUI. Retain one-shot GPU ownership and completion where native
  resources depend on it. Publish UI hit-test geometry only after the backend presents it.
- Keep project creation, Check, Schema refresh, and supervised Play. Phase 5 owns the new data
  formats and Cook and Export. There is no v0 project migration feature in the v2 runtime.
- Check the retained features and games with Rust 1.95. Measure the live Rust source against
  `850d419` using the Phase 1 counting rules, and report regular crates above 30,000 source lines.

### Phase 3B source and verification

| Scope | `850d419` | Phase 3B |
| --- | ---: | ---: |
| Project Rust lines, including tests and examples | 804,533 | 144,099 |
| Rust fixture lines | 6 | 0 |
| Root facade | 65,625 | 367 |
| `nara_scene` | 194,014 | 746 |
| `nara_asset` | 42,235 | 5,074 |
| `nara_app` | 26,431 | 4,954 |
| `nara_ui` | 59,974 | 13,926 |
| `nara_render` | 112,337 | 79,204 |
| `reference-game` | 18,954 | 1,356 |

The workspace has 18 packages including the facade and the derive crates. `nara_render` is the
only crate over the review threshold, with 76,796 lines under `src/`. Its stock frame submit and
present skip outer error scopes. Resource creation and custom encoders still have local validation.
Native error-scope completion is polled once, without blocking the frame to await a pending result.

On Windows 11 with Rust 1.95.0, the locked all-feature, all-target workspace check and 1,153
workspace tests passed. Five additional native probes covered HUD pixels, alpha blending, dirty
tile chunks, device loss and GPU resource retirement, and keyboard focus loss. The Wasm core check
and macOS arm64 gamepad cross-check passed. Night Relay drew the first 1152×768 frame with 110 HUD
glyphs. The headless simulation repeated eight identical rounds over 9,320 fixed ticks, including
collision, shield depletion, restart, and escape. The CLI created and checked an independent
starter, refreshed and read its Schema cache, and supervised a two-round run of the reference
game. The independent starter passed a desktop all-target check. The 98-line `first_game`
example opened a native window and exited successfully on close.

The shared viewport completed its two-window DX12 smoke on a GeForce RTX 4090. At 144 Hz, it
recorded 286 retained viewport submissions over 2.015 seconds (141.9 FPS) and zero Inspector
submissions during the sample. Nara's steady-state counters recorded zero new textures, buffers,
and texture copies. Resize, paused refresh, and shared-queue completion passed. This run used
the development profile and application counters. The Phase 2 PIX and manual display results
above describe the earlier build.

GitHub-hosted CI passed all seven jobs at `85dad83` in 3 minutes 35 seconds from workflow creation
to the last job completion. The workflow used Windows and Ubuntu runners without a Cargo cache.
The [Phase 3B run](https://github.com/Latias94/nara/actions/runs/45775718470) includes the restored
all-target workspace checks and Linux gamepad build coverage.

### Phase 3C — Replace the hosted renderer with direct passes

Use one `Renderer` for swapchain attachments and leased offscreen images. Retire the old GPU
host, policy, program, and graph. Draw ordered sprite and tilemap views, then load the same color
attachment for the HUD. Let the editor install an overlay after the HUD through `RenderPass`.
Keep the fixed-resolution canvas path, dirty tile chunks, custom materials, frame uploads, and
the shared-queue texture lease. Publish picking and HUD geometry after the matching surface
image presents.

The exit gate is at most 28,000 Rust lines in `nara_render` including its tests and examples.
Run the desktop and headless Night Relay, the 98-line `first_game`, and the two-window shared
viewport on Windows. Verify warmed-up allocation and texture copies with the [shared viewport
procedure](../../examples/shared-viewport/README.md). Verify resize, paused refresh, and GPU
completion. Run the locked workspace checks and hosted CI.

### Phase 3C source and verification

| Scope | `4293f60` | `56c9c7f` |
| --- | ---: | ---: |
| Project Rust lines, including tests and examples | 144,099 | 85,261 |
| `nara_render`, including tests and examples | 79,204 | 20,407 |
| `nara_render/src` | 76,796 | 18,530 |
| `nara_render/src/hud` | 8,425 | 1,239 |

On Windows 11 with Rust 1.95.0, the locked all-feature, all-target workspace check passed without
compiler warnings. All 623 workspace tests, eight DX12 probes, seven Vulkan renderer probes,
four reference-game tests, and the Wasm core check passed. The GPU readbacks covered scene and
HUD composition, BGRA direct and RGBA leased targets, editor overlay ordering, fallback glyphs,
incremental glyph and tile updates, and reuse after the producer and sampler both release a texture.

Night Relay presented its first 1152 × 768 frame with 110 HUD glyphs on DX12 and Vulkan. The
headless replay finished eight rounds over 9,320 fixed ticks. The 98-line `first_game` opened a
native window and exited on close. The clear example also presented its first frame.

The shared viewport development build passed on DX12 with an RTX 4090 and a 144 Hz display at
150% scale. In 2.007 seconds it submitted 286 viewport frames at 142.5 FPS, with zero Inspector
submissions and zero new Nara textures or buffers. Resize, one paused refresh, and shared-queue
completion passed. A separate 12-present PIX capture on the same build found zero in-frame GPU
resource creations and zero texture copies in the application commands. Native macOS and Linux
desktop presentation remain separate platform qualifications.

The [hosted CI run](https://github.com/Latias94/nara/actions/runs/45792564236) passed all seven
jobs, including the offscreen WGPU probes on Mesa software Vulkan. The workflow took 12 minutes
24 seconds including runner queues. The longest job ran for 3 minutes 26 seconds.

## Phase 4 — Deliver the desktop editor workflow

**Entry.** Phase 3C passes and RFC-3 defines the editor and supervised Play behavior.

- Replace the former Open-GPUI UI packages with unstyled `gpui_base` controls. Keep the retained
  native docking adapter and its shared `dockspace` session. Keep widget behavior, document state,
  and appearance independently owned.
- Implement synchronous `EditorSession`, multi-select and mixed values, merged gesture Undo, typed
  drafts, registry, picking, and Gizmos in `nara_editor_core`. Integrate project builds, Schema
  refresh, external Play, and streamed Console output using `nara_project`.
- Implement Project Manager, Hierarchy, Inspector, Scene View, Asset Browser, Console, and toolbar
  in the Design Kit shell. The Dialogue Graph remains a follow-up that can use the same document
  and Undo path after its GPUI integration migrates to `gpui_base`.

**Exit.** In the real reference game, drag an asset into the scene, edit multiple entities and a
Rust component, drag a Gizmo and undo or redo it, float the Inspector onto a second display, name
an entity through CJK IME, save, Play and observe the change, then Stop with no surviving game
process. Pass `nara_editor_core` headless tests and verify its UI-framework-free dependency graph.

### Phase 4 source and verification

| Scope | Phase 4 entry | Phase 4 checkout |
| --- | ---: | ---: |
| Nara engine workspace packages | 18 | 19 |
| Nara native editor workspace packages | — | 2 |
| Nara project Rust lines | 85,329 | 97,786 |
| Open-GPUI workspace packages | 24 | 13 |
| Open-GPUI library crates under `crates/` | 18 | 6 |
| Open-GPUI project Rust lines | 668,467 | 493,711 |
| Open-GPUI fixture Rust lines | 10,496 | 9,522 |
| `gpui_base` Rust lines | — | 4,709 |

The entry revisions are Nara `d247193` and Open-GPUI `48db6cd19f6`. The editor adds 4,663 Rust
lines in Core, 1,111 in Gizmo, 1,186 in Viewport, and 5,638 in the shell, including tests.

Open-GPUI merged the `3480cc0fafa` native fixes in `2c8c63e231e` and retained the docking
adapter with `dockspace` pinned to `129e802abc516fe6aa24075f44aa9323a13ff04b`. The engine,
native editor, reference game, shared viewport, and Open-GPUI use WGPU 30.0.0 at
`cddb2f5d80dc0009b2ef6767635140cacf90a007`. The boundary check keeps the editor core free
of GPUI, WGPU, Winit, and dockspace, and keeps runtime and game graphs free of the editor.

Windows 11 with Rust 1.95.0 passed the locked all-feature, all-target checks for the Nara
engine, Nara editor, and Open-GPUI workspaces without compiler warnings. The Nara engine
workspace passed 651 tests, with 34 ignored in that invocation. The editor passed all 15 tests
including the native WGPU texture identity, resize, queue completion, and warm resource-reuse
probe. Both overlay GPU tests passed on DX12 and Vulkan, including cache clearing and rebuilding
after a simulated device-loss condition. The six reference-game tests and Wasm core build passed. Open-GPUI
passed 443 base-control, docking, macro, example, and repository-tool tests. Its native and
recovery smokes passed, and its real HWND placement test passed both directions between 125%
and 150% displays, retiring three native generations in each direction. All three independent
consumers compiled, and the public Vulkan consumer passed three native surface lifetime tests.
The Phase 4 results here are local checks.

Night Relay's native editor smoke rendered 20 authored sprites, refreshed its Rust Schema,
edited Shield charges, and checked Undo, Redo, save, reopen, and PNG placement. The native
game reported the edited 4-shield value, acknowledged Pause, Step and Resume, and completed
Stop with its process reaped and both editor and source scenes intact. The smoke also floated
the Inspector and round-tripped its layout. On the interactive desktop a Gizmo drag and
single Undo, asset drag, two-entity size edit and Undo, sRGB edit and Undo, cross-display
Inspector actions, and Windows Pinyin candidate selection and save were exercised on temporary
project copies. Controlled HWND placement covers mixed-DPI geometry. The system-pointer
capture scenario suite and native macOS and Linux editor sessions remain separate qualifications.

On the RTX 4090 using DX12, the editor reused its imported image over 60 requested frames
after resize with zero Nara texture or buffer creations, stayed idle, and presented one
requested refresh. The separate two-window shared viewport submitted 121 retained viewport
frames at 60.2 FPS on the current 60 Hz display, with zero Inspector submissions or Nara
texture and buffer creations. Resize, paused refresh, and shared-queue completion passed.
PIX captured 12 warmed-up shared-viewport presents with zero application texture copies
and zero in-frame native resource creations. The editor's WGPU test verifies that GPUI imports the
original Nara texture, and its native smoke measures Nara's allocations.
[Native editor verification](../guides/native-editor.md)

The Phase 4 closeout protected unapplied Inspector drafts at close, required a pane in each
restored peer window, and made a multi-selection transform one undoable gesture. Snapshot Play
requires project opt-in. Night Relay repeated Schema refresh, layout restoration, and supervised
snapshot Play with the modified scene. The generated starter remained running past the former
30-second handshake deadline and stopped under process supervision. The Windows editor tests
exercise close prompts, Inspector drafts, and failed saves through the GPUI test platform.

Viewport counters now update independently of workspace notifications. The single-window
`--capture-frames` mode submitted 60 Nara frames with zero new textures or buffers on the RTX 4090
with DX12. A separate PIX capture of 12 warmed editor frames recorded zero application texture
copies and zero in-frame native resource creations.
[Capture and Play instructions](../guides/native-editor.md)

## Phase 5 — Cut over formats and qualify delivery

**Entry.** Phase 4 passes and RFC-4 defines Schema, compact authoring formats, and the cooked layout.

- Implement derived Schema, packed fields, `FieldPath`, UUID sidecars, compact RON, nested prefab
  overrides, and unknown-component round trips. Cook the dependency closure into `.nscn` and `.npak`.
  Move the `reference-game` scene and `nara_project` starter onto the final format at the cutover.
- Qualify data hot reload and measure edit-to-result latency. Integrate retained `gpui-fast` layout,
  read tracking, and frame reuse so texture updates can redraw without relaying out static panels.
- Export the reference game as an executable with cooked assets and verify an independent launch.

**Exit.** A comparable tilemap scene is at least 10 times smaller than v0 and cooked loading takes
under 2 ms. The editor UI thread takes under 0.5 ms per frame with static panels and a 120 FPS
viewport. The exported game runs on a clean machine. Aim for 120,000–180,000 lines of Nara source
with the final workspace boundaries in place. Open-GPUI retains native desktop platform services,
AccessKit, and docking. Its measured source count below exceeds the original 120,000–160,000-line
estimate.

### Phase 5A verification

- **Tile data size.** The [`nara_scene` authoring gate](../../crates/nara_scene/tests/authoring.rs)
  writes 10,000 `PackedU16` cells with repeating values `0..31`. The complete compact RON document
  is 27,155 bytes. The equivalent cell array alone takes 436,872 bytes with v0-style pretty JSON
  and per-cell `u32` tags, a 16.09× reduction. The historical `meadow.scene.json` file at
  `legacy/v0-imgui-reference` is 35,792 bytes and does not carry that tile grid. The test compares
  the same 10,000 cell values and checks exact decode and stable reserialization.
- **Night Relay on Windows.** The headless replay completed eight rounds across 9,320 fixed ticks.
  Each round kept 28 wall contacts and exercised shield loss, restart, and escape. Desktop
  first-present reported a 1152 × 768 frame with 110 HUD glyphs. The authored scene keeps all
  21 entity UUIDs; three of the entities share one prefab with sparse local overrides.
- **Native editor on Windows.** `--verify` rendered the 20 authored sprites, edited and reopened
  a scene, reused its viewport texture across 60 frames, and moved the Inspector to a peer window.
  The supervised game saw the edited four-shield snapshot and acknowledged Pause, Step, and
  Resume. Stop reaped the game, and the original scene bytes stayed unchanged. The separate
  60-frame capture recorded zero new textures or buffers on the RTX 4090 DX12 adapter.
- **Editor I/O at the 5A cut.** Asset discovery and confirmed prefab publication performed disk
  work on the UI thread. Phase 5B moves discovery and reload preparation to a background job and
  checks the active document revision before publishing. Explicit prefab publication still runs
  in the editor transaction that updates the document and its history.

### Phase 5B verification

- **Cooked loading.** On an Intel Core i9-13900KF running Windows, Rust 1.95.0 release
  `cooked_scene_benchmark` parsed a 132,161-byte NSCN and instantiated 1,000 entities with
  10,000 packed `u16` cells in **0.488 ms p50 / 0.579 ms p95** (101 timed runs). Each run
  inserted into a fresh Bevy world and verified the packed cell checksum. Timing starts from
  in-memory NSCN bytes and excludes the asset archive read and one-time native registry setup.
- **Incremental Cook.** Night Relay's entry produced four assets in a 2,519-byte archive. Two
  runs on the same source cut went from four misses to four hits and produced identical archive
  bytes. A prefab supplies the flattened instances, and the archive carries their used image
  plus the gate material and its shader.
- **Native hot reload.** On the same Windows host, the editor and supervised Night Relay
  alternated real source edits in disposable project copies, with 20 samples per input and per
  process. Timing starts just before the save operation and ends at the matching accepted
  viewport frame or receipt of the game's matching presentation record. The editor also resolved
  a PNG after moving it together with its sidecar. Play started from a four-shield editor
  snapshot and exited on control-pipe EOF.

| Changed source | Editor p50 / p95 (ms) | Dev Play p50 / p95 (ms) |
| --- | ---: | ---: |
| PNG texture | 112.350 / 123.218 | 32.354 / 33.123 |
| Scene | 113.786 / 128.825 | 31.956 / 33.431 |
| Prefab | 120.986 / 131.517 | 32.440 / 33.024 |
| Material | 113.824 / 124.082 | 32.411 / 32.706 |
| WGSL shader | 110.687 / 123.817 | 32.368 / 32.768 |

The final Cargo-built binaries also passed all five reload cases. During a concurrent Starter
build, per-case p95 ranged from 136.688 to 172.667 ms in the editor and 36.203 to 63.312 ms in Play.
After the review fixes, both native probes passed all five cases again at 30 Hz with 20 samples
per case. Per-case p95 ranged from 144.136 to 154.510 ms in the editor and from 99.231 to
99.547 ms in Play. Test coverage includes same-ID moves across separate source and sidecar
events, recovery after a Windows sharing violation, and asset directory replacement.
A follow-up Windows test held a newly created asset directory under an exclusive share lock.
After release, the queued mount retry attached the watch and observed a new source write.
The native editor also presented all five kinds of hot reload with five samples each and
resolved the texture under the same asset ID after a move.

- **Standalone Export on Windows.** A fresh trusted export produced a 20,063,744-byte executable
  and the 2,519-byte archive. A copy containing only those two files ran from a new directory
  outside the checkout with a system-only `PATH` and no Cargo, Rustup, or Nara environment
  overrides. Both `--verify-first-present` and `--verify-gameplay` exited successfully. The
  1152 × 768 window presented 110 HUD glyphs, then presented the updated HUD after collecting a
  power cell. The shipping dependency graph excludes the editor, Open-GPUI, RON, PNG decoding,
  and file watching. The review rebuild repeated both clean-directory checks with the same
  file sizes and four Cook cache hits.
- **Editor regression on Windows.** The final editor and development game passed `--verify`.
  It reopened the edited scene, preserved Undo and Redo, retained 60 viewport frames with zero
  new textures or buffers, floated the Inspector, and drove the four-shield Play snapshot
  through Pause, one fixed Step, Resume, and contained Stop.
- **Regression suites.** The engine passed 509 tests with five configured skips. Night Relay
  passed all 12 tests and the eight-round fixed-step replay. The root, editor, and shared viewport
  all-target checks passed on Windows, as did the Wasm core check and the dependency boundary audit.

### Phase 5C verification

Windows 11 release measurements used Rust 1.95.0, an Intel Core i9-13900KF, and an RTX 4090
with DX12. The editor window was 1460 × 940 logical pixels at 150% scale. The original
144 Hz run submitted about 143 viewport and GPUI frames per second with AccessKit active.
Static panels recorded 0.303 ms p50 / 0.412 ms p95 for GPUI draw and platform submission.
Publishing Console status on each frame raised those values to 1.746 / 1.968 ms. These
measurements excluded the synchronous Nara viewport callback.

The review follow-up pairs each viewport callback with its native GPUI submission. Combined
time sums the callback, accepted GPUI draw, and synchronous platform submission for each
sample, including platform wait. Idle time and other event dispatch are outside the sample.
Each case requested 240 viewport frames after warmup. The active display was 30 Hz in this
session, and both the viewport and GPUI submitted about 30 frames per second.

| AccessKit | GPUI view retention | Workload | GPUI draw + submit p50 / p95 (ms) | Viewport callback p50 / p95 (ms) | Combined p50 / p95 (ms) |
| --- | --- | --- | ---: | ---: | ---: |
| Active | Enabled | Static panels | 0.345 / 0.435 | 0.337 / 0.419 | 0.686 / 0.848 |
| Active | Disabled | Static panels | 0.341 / 0.429 | 0.335 / 0.402 | 0.683 / 0.836 |
| Disabled | Enabled | Static panels | 0.341 / 0.408 | 0.334 / 0.404 | 0.678 / 0.805 |
| Disabled | Disabled | Static panels | 0.339 / 0.393 | 0.337 / 0.428 | 0.679 / 0.808 |
| Active | Enabled | Console status | 1.970 / 2.171 | 0.342 / 0.387 | 2.322 / 2.536 |
| Active | Disabled | Console status | 2.053 / 2.333 | 0.333 / 0.405 | 2.398 / 2.665 |
| Disabled | Enabled | Console status | 1.804 / 2.095 | 0.337 / 0.428 | 2.146 / 2.468 |
| Disabled | Disabled | Console status | 1.890 / 2.128 | 0.333 / 0.394 | 2.230 / 2.522 |

All four initial review probes completed and exited cleanly. Static panels issued zero GPUI
draws and used `request_retained_presentation()` with either setting of `GPUI_VIEW_RETENTION`.
Console status triggered 241 GPUI draws for 240 updates in each run. With AccessKit disabled,
enabling view retention reduced the measured Console p95 in this comparison. The combined
static-panel median was above 0.5 ms in every configuration. The full UI-thread target at
120 FPS remains unverified.

The final Windows rerun used a 60 Hz display and measured about 30–60 submissions per second.
In each of the four accepted configurations, 240 static frames produced 240 retained-only
submissions and zero GPUI draws. The 240 Console status updates produced 240 draws and
240 submissions. The probe now requires viewport callback timing for every GPUI submission.
Combined static work ranged from **0.637 to 0.679 ms p50** and **0.799 to 1.057 ms p95**.
The probe rejected four attempts with unexpected static draws or incomplete draw-to-submission
pairing before the accepted runs.

AccessKit permits replay of eligible unclipped roots. It falls back to full draws for a dirty
splice or when accessibility capture owns a subtree clip, including the clipped Dockspace
path in the editor. The retained/full-redraw tests compare paint, hitboxes, input, focus,
and the accepted accessibility tree. They also exercise clean subtree replay, dirty-leaf
splicing, font registration during a splice, group hover, and element-state retirement.
Follow-up tests cover pointer exit during deferred frame preparation and discard, and retained
list queries before the first layout and after remeasurement.
Redrawing after a pointer exit preserves the exit until a new pointer event arrives.
Explicit `Window::draw` rebuilds the root, preserving Dockspace's frame-local publications
and direct consumers' drawing callbacks. A release shutdown regression also verified that
closing the last window during critical cleanup completes the original quit generation.

The Windows review suites for Open-GPUI Core, Dockspace, and WGPU passed all 2,854 tests.
The Windows platform package passed all 328 enabled tests with leak detection, with 12 native
tests configured to run separately. The native accessibility inspection test drives paint
callbacks for the observed HWND and checks the tree, draws, and submissions.
Three native DX12 probes covered the owned host, deferred external delivery, and owned logical-root
close. The Apple Silicon cross-check compiled all platform targets with all features. Nara's editor
and viewport suite passed all 51 ordinary tests and the separate DX12 allocation reuse test
after warmup and resize.
The standalone 60-frame editor capture recorded 60 Nara renders, 61 GPUI presents, and zero new
textures or buffers.
The final native editor smoke edited and reopened Night Relay, floated the Inspector, and
reaped the game after Pause, one Step, and Resume with an edited four-shield snapshot.
The separate two-window shared-viewport smoke submitted 287 retained frames at 143.1 FPS,
with zero Inspector submissions and zero new textures or buffers during steady state. The
review rerun submitted 60 retained frames at 29.9 FPS on the 30 Hz display with the same
steady-state submission and allocation invariants.
The locked all-feature, all-target checks completed without compiler warnings for both root
workspaces, the Nara editor workspace, Night Relay, and the shared viewport. The portable Nara
runtime core and the Open-GPUI core passed their `wasm32-unknown-unknown` checks. Every resolved
`wgpu` entry across the two repositories uses 30.0.0 at
`cddb2f5d80dc0009b2ef6767635140cacf90a007`.

#### Maintained Rust source

The Phase 1 counting rules include project tests and examples and count fixture Rust separately.

| Repository | Workspace packages | Packages under `crates/` | Project Rust lines | Fixture Rust lines |
| --- | ---: | ---: | ---: | ---: |
| Nara | 39 → 19 | 37 → 18 | 1,238,943 → 122,018 | 196,631 → 0 |
| Open-GPUI | 47 → 13 | 38 → 6 | 1,051,969 → 501,583 | 12,227 → 9,527 |

Nara's largest crate is `nara_render` at 20,640 lines. The root facade has 321 lines, including its
104-line library entry point. The project total includes 12,078 lines in the separate editor
workspace.

Open-GPUI keeps 273,482 lines in `gpui`, 125,391 in `gpui_platform`, 52,471 in `gpui_docking`,
21,175 in `gpui_wgpu`, 5,414 in `gpui_base`, and 3,763 in `gpui_macros`, including their tests.
These packages retain native windowing, input, focus, accessibility, and Dockspace integration.

#### Hosted CI

Nara passed all seven jobs on GitHub-hosted Windows and Ubuntu before and after the review.
The review run at `16111ff` took 22 minutes 30 seconds from creation through completion,
including runner queue time. Its longest job was Windows core engine tests at 4 minutes
41 seconds. The previous run at `5041f54` took 11 minutes 16 seconds, and its longest job took
4 minutes 43 seconds. The workflow has no repository-level Cargo cache.
[Review CI](https://github.com/Latias94/nara/actions/runs/46039412516) ·
[Earlier CI](https://github.com/Latias94/nara/actions/runs/45933417713)

Open-GPUI's pre-review Verify run passed all 12 jobs on GitHub-hosted Windows, Linux, and
macOS, including native Windows and GPU host jobs. Workflow wall time was 67 minutes
7 seconds, including queue time. The Linux Verify job took 44 minutes 16 seconds. The Windows
consumer and workspace jobs restored Cargo caches, while the Windows Rust 1.95 docking and
tooling job had a cache miss. These workflow and job times include setup and tests.
[Earlier Verify](https://github.com/Latias94/open-gpui/actions/runs/46026905673) ·
[Review Verify](https://github.com/Latias94/open-gpui/actions/runs/46044324386)

## Open-GPUI packages

| Package | Responsibility |
| --- | --- |
| `gpui` | Entities, elements, layout, events, focus, retained frames, window model, and headless test support |
| `gpui_wgpu` | Unified renderer, atlas, and external image imports |
| `gpui_platform` | Windows, macOS, and Linux native services |
| `gpui_macros` | Core derives and action macros |
| `gpui_base` | Unstyled interaction primitives |
| `gpui_docking` | Thin adapter to the pure Rust `dockspace` model |

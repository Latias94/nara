---
type: "Engineering Research"
title: "Nara functional comparison with Bevy, Godot, and Unity"
description: "User-workflow comparison of Nara's implemented, accepted-target, and candidate capabilities against current official Bevy, Godot, and Unity product surfaces."
timestamp: 2026-07-20T04:11:14Z
record_id: "f9f13a5f4842409ebd62a537299ea385"
resource: "STRATEGY.md"
tags: ["architecture", "product", "comparison", "bevy", "godot", "unity"]
status: "research"
producer_id: "codex-root"
run_id: "20260720-engine-feature-comparison"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "9969471f9e8359ac0ba31a9d241f7b60fca6e723"
---

# Summary

This note compares Nara with Bevy, Godot, and Unity by author-visible outcomes rather than by the
number of engine subsystems or public types. It separates implemented repository evidence from
Accepted targets and non-normative candidates. It is research evidence, not architecture or
implementation authority.

The short verdict is:

- Unity is the strongest complete production product and ecosystem reference.
- Godot is the strongest open-source integrated editor, scene, and distribution reference.
- Bevy is the strongest Rust-native composition and low-friction replacement reference.
- Nara is currently less feature-complete and less production-proven than all three. Its credible
  differentiation is a future combination of Bevy-grade Rust control, Godot-like integrated
  authoring, and explicit data, lifecycle, headless, and replacement contracts.

Nara must not claim that architectural rigor already compensates for missing physics, text, audio,
animation, export, or a usable editor. The reference game and clean-room package workflows are the
evidence gates that can turn this architecture into a product.

# Comparison Method

Nara status vocabulary:

- **Implemented**: repository evidence exists in the ADR implementation ledger.
- **Partial**: a real vertical slice exists, but the user workflow or accepted decision remains
  incomplete.
- **Accepted target**: an Accepted ADR constrains future work; the capability is not available.
- **Candidate**: a design harness, Proposed ADR, open question, or inactive plan only.

For the other engines, `built-in` means present in the official engine/product surface and
`ecosystem` means normally supplied by a package or community project. These labels do not imply
equal production maturity.

External capability facts were checked against official sources on 2026-07-20. Official showcases
are used only as production-validation evidence, not as a complete census of shipped games.

# Product And Authoring Surface

| User-facing capability | Nara | Bevy | Godot | Unity | Assessment |
|---|---|---|---|---|---|
| Official authoring language | Rust is the intended complete path; the ECS/App foundation is implemented | Rust is the complete code-first path | GDScript, C#, and C++ are official; Rust bindings are community-owned | C# is the normal gameplay and Editor path; native plugins cover lower-level code | Nara and Bevy own the Rust-native position; Nara still has to prove the complete production path |
| Runtime composition model | `bevy_ecs` data plus Nara-owned App, fallible plugins, schedules, and sealed composition | Bevy ECS and broad `Plugin::build(&mut App)` composition | Node/Scene tree with script and Server APIs | GameObject/Component/MonoBehaviour by default; Entities is a separate data-oriented package | Nara is intentionally closer to Bevy internally but wants a more bounded product composition phase |
| Project entry | `nara.toml` and project-content ingestion exist; executable generation and ordinary discovery are partial | Cargo application plus code and assets | Project Manager plus `project.godot` | Unity Hub, project templates, Editor, and Build Profiles | Nara is still infrastructure-first; Godot and Unity provide a complete first-run product |
| Integrated editor | UI-neutral workspace, inspector, patch, Play, and egui adapter foundations are partial; no complete product editor | No first-party integrated production editor in the official authoring path; community editors and inspectors exist | Mature scene, 2D, 3D, UI, script, animation, audio, debugger, profiler, and import workspaces | Mature scene, game, inspector, animation, profiler, build, package, and specialized workspaces | This is Nara's largest product gap and its main opportunity over Bevy |
| Scene and prefab authoring | Stable scene/prefab documents, schema IDs, migrations, atomic patches, inverse undo, and spawn are implemented; visual workflows remain partial | ECS Scene loading, instancing, saving, and hot reload; authoring is mainly code or community tooling | PackedScene/Node composition, instancing, inheritance, inspector editing, and live scene workflows | Mature Scenes plus nested Prefabs, overrides, and Prefab Variants | Nara has a strong persistence foundation but cannot claim parity until common editing tasks are ergonomic |
| Asset import and identity | Stable IDs, metadata, importer records, dependency/reload generations, image import, and render preparation are implemented; format and editor coverage are narrow | Flexible AssetServer, custom loaders, runtime generation, and hot reload; less integrated import-database UX | Automatic import, editor configuration, many built-in formats, and custom import plugins | Mature Asset Database, `.meta` identity, import settings, custom importers, caches, bundles, and Addressables | Nara is unusually rigorous for its age, but Unity and Godot dominate format breadth and product tooling |
| Data and code iteration | Asset/data reload and rebuild-oriented isolated Play foundations exist; measured classification, last-good executable activation, and code patching remain partial/candidate | Asset hot reload, fast Rust compile configuration, and community code hot-patching | Live scene/script synchronization, remote scene inspection, and script reload | Mature Play Mode and asset/script iteration, with platform and domain reload tradeoffs | Nara's layered and honest model is sound, but latency and recovery need measured proof |
| Build and export | Windows/Linux Trial direction; packaging, cooking, checkout-free runtime use, and reproducible export are incomplete | Cargo builds for native/web/mobile; product packaging is largely application/community owned | Integrated export presets, templates, PCK/ZIP, platform exports, and dedicated-server export | Broad platform modules, Build Profiles, dedicated servers, and mature deployment ecosystem | Export is a release blocker for Nara, not a later polish item |
| Package discovery and lifecycle | Cargo/Git/local is the target first phase; source-package design exists, but install/update/disable/remove UX is unproven | crates.io plus the Bevy Assets catalogue; users manage Cargo compatibility | Asset Library is integrated into the editor; addons and project templates have engine-version metadata | Package Manager plus Asset Store supports discovery, versions, install, update, and removal | Nara should aim to exceed Bevy's manual UX before considering a custom registry |
| Production validation | One open-source reference game is under construction; no released Nara game proves the full path | Community games and plugins exist, but official evidence is smaller and more code-first than Godot/Unity | Official showcase requires released or public-demo projects and includes many commercial games | Extensive official showcases and case studies across genres, scales, and platforms | Nara must use the reference game and external packages as evidence rather than positioning claims |

# Deep Runtime Domains

| Domain | Nara current and target | Bevy | Godot | Unity | Nara priority |
|---|---|---|---|---|---|
| 2D rendering | Textured sprites, tilemaps, material-aware batches, runtime-UI batches, static pass ordering, and one wgpu surface transaction exist; culling, upload policy, effects, and broad scale proof remain partial | Strong built-in 2D renderer, materials, shaders, cameras, atlases, and render graph | Mature dedicated 2D renderer, lighting, particles, TileMap, shaders, and editor | Mature 2D workflow, sprites, tilemaps, animation, physics, URP lighting, effects, and editor | Complete the reference-game visual slice before generalizing the render architecture |
| 3D rendering | Accepted coordinate direction only; no Transform3d, Camera3d, mesh importer, or 3D renderer | Built-in PBR, lighting, shadows, glTF, animation, and custom pipelines | Mature Forward+, Mobile, and Compatibility renderers with full 3D editor | URP/HDRP/Built-in pipelines, extensive graphics tooling, and custom SRP | Explicitly deferred during Trial; do not compete on 3D breadth |
| Render extensibility | Backend-neutral packet and phase baseline exists; Feature, Family, exact-wgpu interop, graph/compiler, and Host replacement are candidates | Highest ambient Rust flexibility: custom shaders, pipelines, graph nodes, render-world access, and wgpu-facing integrations | RenderingServer provides low-level use, but arbitrary renderer replacement is not a normal addon workflow | SRP supports complete custom pipelines; low-level native rendering plugins support device-specific work | Prove one portable post-process, one Dear ImGui overlay, and only then exact-version interop |
| Physics | Accepted strategy only; no body/collider data, queries, events, or Adapter exists | Physics is primarily ecosystem-owned, with multiple ECS-native integrations such as Avian and Rapier | Built-in 2D/3D physics plus startup-time alternate `PhysicsServer3D` registration through engine/GDExtension paths | Built-in PhysX/Box2D plus Unity Physics and Havok packages for Entities | First missing gameplay-critical domain; prove both direct-crate freedom and product-compatible Adapter UX |
| Runtime UI | ECS panel/layout/pointer/clipping/batching foundation exists; text, widgets, focus, navigation, accessibility, responsive layout, and dogfooding are incomplete | ECS-native UI is built in and can be omitted; egui and other UI frameworks are common plugins | Mature Control nodes, themes, text, focus, keyboard/controller navigation, and visual editing | Multiple mature UI systems, UI Builder, styling, events, runtime and Editor support | Deliver one polished reference-game HUD and retain the ability to omit `nara_ui` |
| External/editor UI toolkit | Tooling models are toolkit-neutral and egui is isolated; Dear ImGui needs richer input and a public render contribution/interop path | External UI plugins attach directly to App/input/render APIs; swapping Bevy UI is explicitly supported | EditorPlugin integrates custom docks/inspectors using Godot UI; native toolkits need deeper extension work | Editor extensions and multiple official UI systems are mature; packages/native plugins can add specialized tooling | Dear ImGui primary-window overlay is the named clean-room tracer; multi-viewport comes later |
| Input and platform | Winit boundary, keyboard/mouse state, action map, and desktop runner exist; IME, text, wheel, gamepad, pen, clipboard, cursor policy, file drop, and multi-window are incomplete | Broad window/input support and direct plugin access | Mature keyboard, mouse, gamepad, pen, action mapping, focus, cursor, and IME-related platform workflows | Mature Input System and broad device/platform integrations | Rich normalized input is a prerequisite for UI/editor and accessibility, not an optional UI detail |
| Animation | Accepted strategy only; no clips, field targets, player, blending, or consumer | Built-in skeletal, morph-target, property, graph, blend, mask, and event examples | Mature property, skeletal, blend-tree, method, audio, IK, and editor timelines | Mature Animator, clips, state machines, rigging, Timeline, and import pipeline | Add the smallest sprite/property animation needed by the reference game before skeletal scope |
| Audio | Placeholder crate only; no asset, emitter, mixer, streaming, spatial, device, or backend | Built-in asset playback and spatial audio with extensible sources | Mature 2D/3D playback, buses, effects, procedural audio, input, and multiple backends | Mature clips, sources, listeners, mixer, effects, import, spatial and platform integration | Required for a complete game; first slice should include lifecycle-safe device ownership and headless exclusion |
| Text, font, localization | No text domain, importer, shaping, fallback, bidi, glyph cache, or localization contract | Text rendering and UI text exist; localization is ecosystem/application territory | Mature font, shaping, bidi, localization, UI mirroring, fallback, and editor workflows | TextCore/TextMeshPro/UI systems plus official localization packages and editor tooling | Text is a blocker for runtime UI and editor dogfooding; multilingual shaping should be in the first real contract |
| Navigation and AI | No owned navigation domain or accepted first implementation | Primarily ecosystem plugins | Built-in 2D/3D navigation meshes, runtime generation, avoidance, and A* | AI Navigation package and large ecosystem | Defer general AI, but explicitly track navigation as a missing deep module |
| Particles and VFX | No product domain | Built-in rendering examples and ecosystem tools | Mature 2D/3D particles and shader effects | Mature Particle System and Visual Effect Graph | A small effect path may become necessary for reference-game quality before a general VFX architecture |
| Save/runtime persistence | Accepted strategy only; stable identity/schema foundations exist, but no save artifact or restore workflow | Application/plugin owned | Generic file/resource APIs; game-specific save schema remains application owned | Serialization, files, services, and ecosystem packages; game-specific save schema remains application owned | Nara does not need a universal save engine, but the reference game must prove versioned save/restore |
| Networking/replication | Identity, schema, commands, and headless readiness exist; no replication model or transport Adapter | Primarily ecosystem owned | Built-in low-level protocols and high-level ENet/RPC replication | Official Netcode packages and services for GameObjects and Entities | Correctly deferred for Trial; preserve commands and stable identity without freezing replication vocabulary |
| Headless/server | Exact fixed stepping, semantic gameplay commands, strict server bundle rules, managed runtime, and a headless reference slice exist; production deployment is incomplete | Remove render/window plugins and compose a headless App | `--headless`, dedicated-server export, visual-resource stripping, and high-level multiplayer | Dedicated-server build target plus Netcode/services | This is a credible Nara architectural strength, but it still lacks deployment and shipped-load evidence |
| Diagnostics and debugging | Bounded structured runtime diagnostics, stable identities, patch/undo, isolated Play direction, and deterministic replay direction exist; producer coverage and editor tools are partial | Built-in diagnostics plus community inspectors, graph tools, and debuggers | Mature debugger, profiler, visual profiler, remote scene tree, monitors, and live synchronization | Mature profiler, frame debugger, memory tools, console, debugger integration, and Play workflows | Preserve the structured/headless advantage while rapidly building an ordinary usable debugger |

# Replacement And Ecosystem Freedom

| User intent | Nara | Bevy | Godot | Unity |
|---|---|---|---|---|
| Omit a default subsystem | Plugin groups and product features support omission in principle; current cross-domain proof is partial | Core design goal; omit individual plugins/features and build headless combinations | Possible for some servers/custom builds, but normal projects use the integrated engine stack | Packages/pipelines vary, but many core engine services remain fixed product assumptions |
| Replace physics | Target: direct crate path plus semantic Adapter path; not implemented | Add Avian, Rapier, or another plugin directly; integration conventions vary | Register an alternate physics server at startup through deep extension paths | Choose among supported built-in/DOTS implementations; arbitrary replacement is less uniform |
| Replace or deeply extend rendering | Graduated Feature, exact-version interop, and Host model is selected as a product requirement but only the minimal baseline is Accepted | Broadest direct access and easiest experimentation, with a larger unstable compatibility surface | Low-level server API and engine/GDExtension paths; full renderer replacement is not ordinary addon UX | SRP is a mature full-pipeline seam; native rendering extensions provide a lower-level path |
| Replace runtime UI | `nara_ui` must remain optional; coexistence protocols are a target, current input/render seams are insufficient | Explicitly supported by disabling Bevy UI and adding another plugin | Custom UI can be authored from Controls; replacing the UI foundation is rarely necessary or seamless | Multiple official UI systems and custom frameworks can coexist, with varying editor integration |
| Integrate native/backend SDK | Candidate exact-version, capability-, order-, epoch-, and close-aware interop; no public role today | Direct Rust/native dependencies and backend access are easy, but compatibility is package-owned | GDExtension loads native libraries without rebuilding the engine, with version compatibility rules | Native plug-in interfaces and low-level rendering callbacks are mature and platform-aware |
| Install and recover an extension | Target package lifecycle includes preview, compatibility, disable/remove, migration, and last-good recovery; unproven | Cargo resolves code, while project UX and recovery are mostly manual | In-editor Asset Library and addon conventions | Package Manager and Asset Store provide the strongest integrated lifecycle |

The strategic implication is not that Nara should expose every internal object. It should guarantee
that important user tasks remain reachable through the lowest sufficient authority, with a
documented exact-version escape hatch when a stable semantic contract is not yet known.

# Representative User Journeys

| Journey | Current leader | Nara state | Required Nara proof |
|---|---|---|---|
| Write the complete game in Rust | Bevy | Runtime foundation is credible, but production coverage is incomplete | Reference game uses only public Rust/project/editor APIs |
| Let a designer edit scenes without Rust | Godot/Unity | Persistent model exists; product editor does not | Scene tree, viewport, inspector, asset browser, undo, Play, and conflict UX |
| Swap the physics implementation | Bevy for freedom; Godot/Unity for integrated defaults | Not implemented | One first-party Adapter, one independent replacement, and direct-crate escape path without core edits |
| Integrate Dear ImGui for tools | Bevy | Blocked by public input/render gaps | Primary-window overlay, texture/view integration, docking later, multi-viewport last |
| Build a completely custom game UI | Bevy for replacement; Godot/Unity for finished controls | Intended but unproven | Omit Nara UI, retain focus/text/accessibility/render-layer interoperability, coexist with tooling overlay |
| Install an extension without understanding build internals | Unity/Godot | No workflow | Cargo/Git/local package selection, compatibility explanation, update, disable, remove, and recovery |
| Ship a desktop game | Unity/Godot | Incomplete | Reproducible Windows/Linux export from a clean machine and release artifact consumer |
| Run the same authoritative gameplay headlessly | Godot/Unity for product delivery; Nara has a strong early contract | Headless first wave exists; packaging and scale proof do not | Server export, long-running diagnostics, load/fault evidence, and desktop/server semantic parity |
| Migrate durable project data safely | No simple cross-engine winner | Schema, migration, parse-budget, and stable-ID foundations are unusually explicit | Real non-empty migrations, asset moves, prefab evolution, save evolution, and editor recovery |

# Strategic Position

Nara should not position itself as "Bevy with more features" or "Godot in Rust." The defensible
position is:

> A Rust-native integrated game engine that keeps the complete public Rust control of a library
> engine while making scenes, assets, editing, debugging, packaging, and delivery one coherent,
> data-driven product.

The three references contribute different lessons:

- **From Bevy, retain** direct Rust composition, ECS data ownership, small Cargo crates, replaceable
  defaults, and ecosystem experimentation. **Do not copy** the assumption that Cargo assembly and
  public engine internals alone constitute a production workflow.
- **From Godot, retain** scene-centric authoring, editor coherence, inspector discoverability,
  headless/export simplicity, low-level Server escape hatches, and integrated addon discovery.
  **Do not copy** Node/Scene ownership as the universal runtime, document, editor, and service model,
  or accept Rust as a community-only authoring path.
- **From Unity, retain** Prefab ergonomics, importer and metadata UX, package lifecycle, Asset Store
  reach, build profiles, progressive disclosure, SRP-style renderer authority levels, and deep
  production examples. **Do not copy** a product model where the primary authoring language and
  runtime object lifecycle are outside Nara's Rust/ECS/data contracts or where multiple overlapping
  systems accumulate without a clear default path.

# Recommended Product Sequence

1. Finish one public-API-only 2D reference-game path before adding broad architecture: input,
   gameplay commands, 2D physics, sprites/tilemaps, text, runtime UI, audio, minimal animation/VFX,
   save/settings, headless parity, and Windows/Linux export.
2. Turn existing authoring foundations into a usable minimum editor: project opening, asset browser,
   hierarchy, viewport, inspector, transactional undo, Play/Stop, diagnostics, and conflict/reload.
3. Prove ecosystem freedom through external workflows rather than traits: replace physics, integrate
   Dear ImGui, omit Nara runtime UI, add one importer, and package the result through install/update/
   disable/remove/recovery.
4. Measure Rust edit-to-result latency and public production coverage continuously. Architectural
   elegance without iteration speed will not beat Bevy; strong Rust APIs without editor/export UX
   will not beat Godot.
5. Keep 3D, full networking, a second authoring language, marketplace infrastructure, and generalized
   renderer families outside Trial unless the reference game or an external package produces direct
   pressure.

# Product Risks

- **Blueprint-to-product gap**: Nara has unusually broad Accepted contracts for an early engine, but
  several basic game domains are still absent. More ADR breadth can reduce rather than increase the
  probability of shipping a game.
- **Over-bounded ecosystem**: sealed composition and Host authority are valuable only if external
  authors still have a documented path to achieve Bevy-like outcomes.
- **Editor architecture before editor use**: UI-neutral models are useful, but Godot and Unity win
  through fast, visible everyday tasks, not through toolkit neutrality alone.
- **Rust iteration gap**: a complete Rust authoring language is not a product advantage if structural
  changes make common iteration slower or less recoverable than competitor scripting workflows.
- **Store-before-package mistake**: the first ecosystem milestone is a reliable local/Git/Cargo
  package lifecycle, not a registry or marketplace service.
- **Premature parity language**: no target design should be described as parity until a renamed,
  clean-room external package and the reference game pass the same public contract.

# Nara Sources

- `STRATEGY.md`.
- `docs/architecture/README.md`.
- `docs/architecture/nara-foundation.md`.
- `docs/architecture/adr/implementation-status.md`.
- `docs/architecture/adr/0016-extension-seams-for-backends-and-domain-modules.md`.
- `docs/architecture/adr/0019-physics-strategy.md`.
- `docs/architecture/adr/0025-runtime-ui-system.md`.
- `docs/architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md`.
- `docs/architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md`.
- `docs/architecture/render-extension-capability-interface-design.md`.
- `docs/architecture/source-extension-package-interface-design.md`.
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`.
- `docs/plans/2026-07-15-001-feat-render-extension-parity-tracers-plan.md`.

# External Official Sources

## Bevy

- [Bevy Engine feature overview](https://bevy.org/).
- [Bevy introduction and design goals](https://bevy.org/learn/quick-start/introduction/).
- [Plugins and default groups](https://bevy.org/learn/quick-start/getting-started/plugins/).
- [Building Bevy's ecosystem](https://bevy.org/learn/quick-start/plugin-development/).
- [Bevy Assets community catalogue](https://bevy.org/assets/).
- [Official examples](https://bevy.org/examples/).

## Godot

- [Official feature list](https://docs.godotengine.org/en/latest/about/list_of_features.html).
- [Nodes and Scenes](https://docs.godotengine.org/en/stable/getting_started/step_by_step/nodes_and_scenes.html).
- [Scripting languages](https://docs.godotengine.org/en/stable/tutorials/scripting/index.html).
- [GDExtension](https://docs.godotengine.org/en/latest/engine_details/engine_api/gdextension/what_is_gdextension.html).
- [Editor plugins](https://docs.godotengine.org/en/stable/tutorials/plugins/editor/index.html).
- [PhysicsServer3DManager](https://docs.godotengine.org/en/stable/classes/class_physicsserver3dmanager.html).
- [Asset Library usage](https://docs.godotengine.org/en/stable/community/asset_library/using_assetlib.html).
- [Dedicated-server export](https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_dedicated_servers.html).
- [Debugger and profiler overview](https://docs.godotengine.org/en/stable/tutorials/scripting/debug/overview_of_debugging_tools.html).
- [Godot Showcase](https://godotengine.org/showcase/) and
  [submission criteria](https://godotengine.org/showcase/submissions/).

## Unity

- [Unity Engine overview](https://unity.com/products/unity-engine).
- [Unity 6 manual](https://docs.unity3d.com/6000.1/Documentation/Manual/UnityManual.html).
- [Prefab system](https://docs.unity3d.com/6000.0/Documentation/Manual/Prefabs.html).
- [Asset workflow](https://docs.unity3d.com/6000.0/Documentation/Manual/AssetWorkflow.html).
- [Package Manager](https://docs.unity3d.com/Manual/upm-ui.html).
- [UI Toolkit](https://docs.unity3d.com/6000.0/Documentation/Manual/UIElements.html).
- [Render pipelines](https://docs.unity3d.com/6000.0/Documentation/Manual/render-pipelines.html).
- [Scriptable Render Pipeline introduction](https://docs.unity3d.com/2021.1/Documentation/Manual/scriptable-render-pipeline-introduction.html).
- [Low-level native rendering extensions](https://docs.unity3d.com/6000.0/Documentation/Manual/low-level-native-plugin-rendering-extensions.html).
- [Physics systems](https://docs.unity3d.com/6000.0/Documentation/Manual/PhysicsSection.html).
- [Build Profiles](https://docs.unity3d.com/6000.0/Documentation/Manual/BuildSettings.html).
- [Dedicated-server builds](https://docs.unity3d.com/6000.0/Documentation/Manual/dedicated-server-build.html).
- [Made with Unity](https://unity.com/made-with-unity).

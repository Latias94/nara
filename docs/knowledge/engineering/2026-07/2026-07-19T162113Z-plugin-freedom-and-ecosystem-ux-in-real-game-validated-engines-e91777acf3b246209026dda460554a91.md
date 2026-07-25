---
type: "Engineering Research"
title: "Plugin freedom and ecosystem UX in real-game-validated engines"
description: "Primary-source comparison of extension freedom, replacement workflows, tooling UI integration, and store-ready package UX for Nara."
timestamp: 2026-07-19T16:21:13Z
record_id: "e91777acf3b246209026dda460554a91"
tags: ["architecture", "plugins", "ecosystem", "packages", "ui", "physics"]
status: "research"
producer_id: "codex-root"
run_id: "20260719-deep-modules-dialogue"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "24b42dc15fa62314a7948735f62a16e89e95508b"
---

# Summary

This note evaluates extension freedom from the game author's workflow rather than from the number
of abstract interfaces exposed by an engine. It is research evidence, not implementation authority.

The primary ecosystem references should be Unity, Unreal Engine, and Godot because they combine
shipped-game evidence with integrated package or asset discovery. Defold is a useful focused-engine
reference because its shipped games, Asset Portal, URL libraries, native extensions, and hosted
build path show how a smaller product can make extensions approachable. Bevy remains the Rust-side
freedom and ergonomics baseline. Fyrox, O3DE, and Blender remain useful subsystem references, but
this review does not use them as primary evidence for ecosystem governance.

Nara's ordinary trusted Rust `Plugin` path is already close to Bevy in the capabilities that matter
to gameplay modules: an external plugin can add ECS data, resources, systems, sets, schedules, and
runtime-local registrations through public APIs. Nara deliberately removes ambient authority to
install hidden plugins, select the runner, or acquire Host-owned native services from `build`.

The material freedom gaps are elsewhere:

1. A Nara plugin slot currently names one expected `PluginId`, so a different implementation cannot
   occupy the same product role even when it could satisfy that role.
2. The stock wgpu backend exposes neither Device/Queue access nor a public render-feature/pass
   contribution path, so a third-party Dear ImGui renderer cannot integrate through public APIs.
3. Normalized platform input is not yet rich enough for complete toolkit integration: text/IME,
   wheel, clipboard, file drop, cursor control, and multi-window lifecycle remain incomplete.
4. The target package design is extensive, but no clean-room package workflow yet proves that a
   user can install, configure, update, disable, or remove an external multi-role extension without
   understanding Nara's internal admission phases.

The recommended direction is not unrestricted ambient access. It is a graduated extension model:
stable semantic plugins, typed domain contributions, explicit version-bound backend interop, and
whole-Host replacement for the rare integration that truly needs ownership. A same-version escape
hatch is important: ecosystem experimentation must not wait for every useful seam to become a
long-term compatibility promise. Nara's render-extension design already contains a candidate form
of scoped encoding and exact-version wgpu interop; the missing work is to prove that candidate and
decide whether the same compatibility class should become an explicit cross-domain product policy.

# Research Question

How can Nara retain Bevy-like Rust composition freedom while preserving an integrated product,
supporting physics replacement, Dear ImGui and other tooling UI, custom game UI, and a future asset
store without giving every plugin unrestricted process, runner, filesystem, or GPU authority?

# Evaluation Standard

Repository stars are a discovery signal, not sufficient architecture evidence. This review weights:

- shipped games across multiple teams and platforms;
- third-party packages that survive engine upgrades and project reuse;
- an integrated install, enable, disable, update, and recovery workflow;
- coverage of runtime code, content, importers, editor tools, and native integrations;
- the ability to extend or replace high-value domains without modifying engine source;
- honest compatibility and trust boundaries.

Unity's official showcase includes commercial games and case studies, and its Asset Store exposes
large tool categories including GUI and physics. Godot's current showcase requires released or
publicly playable work and its in-editor Asset Library filters content for the running engine
version. Unreal plugins can contain code, content, dependencies, and multiple runtime/editor modules,
and Fab is integrated into the acquisition workflow. Defold publishes shipped games and an Asset
Portal spanning libraries, native extensions, templates, tools, UI, physics, rendering, and platform
integrations. These are stronger product signals than popularity alone. [U1][U2][G1][G2][E1][E2][D1][D2]

# Reference Roles

| Reference | What it can prove for Nara | What it should not define for Nara |
|---|---|---|
| Unity | Store/package UX, multi-asset packages, editor/runtime separation, Scriptable Render Pipeline, mature commercial workflows | Managed reflection assumptions, a second dependency solver that ignores Cargo, unrestricted editor callbacks |
| Unreal Engine | One descriptor over code and content, multiple typed modules, project enable/disable, dependency warnings, integrated marketplace acquisition | UObject ownership, global engine singletons, the assumption that every deep subsystem is replaceable |
| Godot | Low-friction editor addons, Asset Library UX, native GDExtension, specialized importer/inspector contracts, replaceable physics server registration | Variant/property-hint strings as durable schema, Node as Nara's universal object model, manual paired cleanup as the ideal package transaction |
| Defold | Small-engine extension UX, content libraries, hosted native builds, simple catalog discovery, shipped mobile/web/desktop games | URL-order last-wins dependency collisions, Lua/native ABI as Nara's Rust package model |
| Bevy | Concise Rust plugins, editable default groups, public ECS/render integration, community-driven alternatives | `TypeId` as durable identity, immediate mutation as package admission, README compatibility as complete product UX |
| Fyrox | Concrete Rust implementation ideas for editor/runtime integration, animation, audio, and scene tooling | Primary evidence for marketplace governance or broad independent ecosystem compatibility |
| O3DE / Blender | Asset source-product pipelines, dependency graphs, baking, invalidation, and tooling architecture | Nara-wide product complexity or extension UX |

# What Mature Ecosystems Actually Show

Successful ecosystems do not make every subsystem equally replaceable.

- Unity provides a deliberately replaceable render pipeline through SRP, while other domains use
  narrower packages, components, native plugins, and editor contracts. [U3]
- Godot exposes editor plugins, import plugins, GDExtension, direct RenderingServer operations, and
  a manager for registering alternate PhysicsServer implementations. A full renderer replacement
  remains a different authority level from adding a compositor effect or direct rendering work.
  [G3][G4][G5][G6]
- Unreal plugins can add and modify engine/editor features, but the default physics product is
  Chaos and the engine presents domain-specific extension systems rather than one universal plugin
  replacement contract. [E1][E3]
- Defold native extensions become part of a project-specific engine build through the ordinary
  project workflow; users do not manually distribute a custom engine to every teammate. [D3]

The transferable lesson is domain-specific reachability plus a clear escalation path, not a single
maximally powerful plugin callback.

# Nara Versus Bevy Today

## Ordinary Runtime Plugins

Nara intentionally preserves Bevy-like caller ergonomics: direct plugin, group, tuple,
`add_plugins`, type-directed disable/configure, and relative insertion. Accepted ADR 0046 also
requires first-party and third-party runtime plugins to use the same public App/domain interfaces
without a first-party allowlist. The current `App` exposes resource, system, set, observer, and
custom-schedule configuration to trusted plugins. [N1][N2][N3]

For a normal gameplay, AI, state-machine, or data-processing plugin, the practical reduction from
Bevy is small. Nara adds a static declaration and fallible lifecycle, but does not whitelist plugin
behavior inside the public runtime surface.

## Deliberately Removed Ambient Authority

Compared with Bevy, Nara plugin hooks cannot install another plugin/group or select the runner.
Dependencies must be declared and top-level product/Host code selects exclusive process authority.
This is a real reduction in callback power, but it is not normally a reduction in user-facing
feature reachability. It prevents hidden order, partially installed dependency graphs, and runner
replacement after product admission. [N1][N4]

## Material Replacement Gap

Bevy groups key entries by Rust type and allow callers to disable, set, add, and reorder plugins.
Nara's builder offers equivalent same-plugin edits, but a `PluginSlot` stores an
`expected_plugin: PluginId`; resolution rejects a different plugin for that slot. ADR 0046 leaves
cross-plugin replacement as an explicit open question. This means Nara currently supports:

- omit a first-party group and build a custom runtime from lower-level plugins;
- add a separate third-party runtime plugin;
- reconfigure or disable an admitted first-party plugin;

but it does not yet support a product-compatible statement such as "this external plugin replaces
the default physics role" through the same stable slot. [N1][N2]

That limitation is acceptable before a real replaceable domain exists, but it must not become the
permanent product model. The first physics integration is the right tracer for selecting a
cross-implementation role/conformance contract.

# Dear ImGui As A Freedom Tracer

The local `dear-imgui-bevy` backend is valuable evidence because it integrates with an engine-owned
loop rather than owning winit and wgpu itself. Its primary-window path requires:

- explicit begin-frame, UI-pass, and end-frame scheduling;
- translated input and post-UI capture intent;
- extraction of owned draw data into the render side;
- a camera/target overlay point;
- texture registration for scene views and tool images;
- renderer access sufficient to prepare buffers, textures, pipelines, and encode draws.

Docking stays inside the primary window. Native multi-viewport adds a substantially larger contract:
OS-window creation/retirement, focus, cursor, DPI, IME, per-window input, surface routing, and
per-window presentation. Clipboard, accessibility, file drop, gamepad navigation, and browser IME
remain separate limitations even in the Bevy backend. [I1]

Nara currently provides normalized window lifecycle events plus keyboard, mouse-button, and pointer
position state. The Winit adapter ignores the remaining event variants, and the normalized
`WindowEvent` vocabulary has no text/IME, wheel, file-drop, or cursor-command channel. The stock
`WgpuRenderBackend` keeps Device and Queue private, and its surface render pass only consumes the
current built-in packet/batch path. [N5][N6][N7]

Therefore a third-party Dear ImGui package cannot currently render a normal overlay through public
Nara APIs without one of these unsupported actions:

- editing `nara_render_wgpu` to add a first-party path;
- acquiring private Device/Queue/surface state;
- replacing the whole render owner and likely duplicating platform integration.

This is the clearest current ecosystem-freedom gap. It does not justify public ambient Device/Queue
resources. It justifies a focused post-RGF tracer that compares the lowest-authority viable paths.

Recommended trial sequence:

1. **Primary-window overlay**: one external package, normalized input, one semantic overlay phase,
   owned draw snapshot, and no multi-viewport.
2. **Texture and docking**: show a Nara render target/image in Dear ImGui and preserve input capture
   and frame ownership.
3. **Native multi-viewport**: defer until platform/Editor Host ownership is proven; treat it as a
   platform-window and surface-lifecycle problem, not a small UI feature.

The tracer should live in an independent or clean-room workspace and compile using only public
Nara APIs. A first-party-private integration would not prove ecosystem freedom.

# Physics And Custom Game UI

Physics and UI need different composition cardinalities.

## Physics

A physics simulation owner is normally exclusive per physics world/domain, while debug draw,
character controllers, authoring tools, and query helpers may be additive. Nara should support two
honest user paths:

1. **Direct solver path**: a game may omit Nara's default physics integration and use Rapier,
   Avian, or custom solver components/systems directly. This path is version-local and need not
   preserve Nara scene, editor, save, or solver portability.
2. **Product-compatible Adapter path**: an admitted implementation satisfies Nara-owned authoring,
   synchronization, query, event, schedule, fault, and shutdown semantics. Advanced solver-specific
   features remain available through the implementation crate rather than being forced into a
   mirrored universal API.

The first product-compatible physics slice should prove both paths and must not require a Nara core
source edit for the direct path. Only independent implementation pressure should freeze the common
Adapter surface, consistent with ADR 0016. [N8]

## Custom Game UI

Runtime UI frameworks should usually be additive layers, not one exclusive `UiBackend` slot. A game
may use Nara UI, a custom ECS UI, a sprite/mesh-driven HUD, an immediate-mode debug overlay, or more
than one of these at once.

Nara's current separation of `Runtime2dPlugins` and `RuntimeUiPlugins` is the right start. The
remaining shared contracts should be lower level:

- input focus, capture, routing, text/IME, clipboard, and accessibility publication;
- semantic render phases and target/layer ordering;
- text/font assets and shaping where reused;
- viewport, scale, safe-area, and lifecycle observations;
- explicit diagnostics when a UI layer does not provide accessibility or text capabilities.

A custom game UI must be able to omit `nara_ui` entirely. Coexistence with tooling overlays should
not require either UI framework to become the other framework's backend.

# Recommended Extension Freedom Ladder

| Level | User intent | Compatibility | Authority |
|---|---|---|---|
| Game-owned code/plugin | Add ECS behavior, data, systems, schedules, and resources | Normal semver/public Rust API | App/runtime public APIs only |
| Stable semantic contribution | Add an importer, inspector, render feature, UI layer, physics Adapter, or service integration | Versioned domain contract and conformance suite | Narrow domain-owned inputs and outputs |
| Version-bound backend interop | Integrate a toolkit, vendor SDK, GPU algorithm, or raw platform feature before a portable contract exists | Explicit exact Nara/backend version; migration expected | Borrow-scoped raw/backend-specific access with epoch, ordering, and close rules |
| Exclusive Host replacement | Own a render device domain, platform driver, process runner, or another truly exclusive authority | Separate admission and full lifecycle conformance | Complete authority for the selected domain, never ambient plugin escalation |

The third level elevates and generalizes a candidate that already exists in Nara's render-extension
design; it is not a newly authorized API. Without a proven form of this level, Nara risks making
ecosystem authors wait for stable abstractions that can only be discovered by building the
integration. With it, experimental freedom remains high while stable product APIs stay coherent.

Version-bound interop must be visibly honest:

- opt-in through an advanced/backend-specific crate or feature;
- tied to exact backend and Nara versions;
- unavailable from persistent project data and gameplay preludes;
- borrow-scoped where possible, with generation/epoch identity;
- no implicit ownership of surfaces, submit/present, runner, or shutdown;
- eligible to graduate only after real repeated use identifies a stable subset.

# Store-Ready Package Model

The evidence supports Nara's existing conclusion that a distribution package is not a runtime
`Plugin`. A useful package may contain runtime plugins, editor/tooling contributions, importers,
schemas, content, templates, samples, documentation, build/cook providers, and native adapters.
Unity and Unreal make this multi-role unit visible to users; Godot and Defold make addon/library
content discoverable inside the product. [U4][E1][G2][D2]

Nara does not need a custom registry or marketplace now. It does need a store-ready package view
before third-party conventions fragment. Cargo, Git, and local paths can remain the initial source
and dependency mechanisms while Nara records:

- stable package identity, version, engine compatibility, targets, and feature requirements;
- typed contribution inventory and runtime/editor/build inclusion;
- license, provenance, documentation, samples, and content ownership;
- build script, proc-macro, native library, network, filesystem, and process trust disclosures;
- install/update/remove preview, expected rebuild/reimport/migration, and last-good recovery;
- references that block unsafe removal and rules that preserve copied or modified user content.

A future catalog or store can index these data-only descriptors. Discovery, ratings, moderation,
payments, signing, and ranking are later product services; they should not determine runtime plugin
lifecycle or force content-only packages to invent empty Rust code.

# Golden Ecosystem Tracers

These are proposed evidence cases, not active implementation units:

| Tracer | User-visible success |
|---|---|
| External runtime plugin | A renamed-dependency crate adds components, resources, systems, a custom schedule, and diagnostics with no Nara source edit or allowlist |
| Physics freedom | One game runs a direct third-party solver path; a separate product-compatible Adapter can replace the selected default role through public composition |
| Dear ImGui overlay | One package adds a primary-window overlay, input capture, texture display, and render ordering using only public APIs |
| Custom game UI | A desktop game omits `RuntimeUiPlugins`, supplies its own UI/data/render path, and coexists with a tooling overlay |
| Multi-role package | One operation adds runtime code, an importer, an editor tool, schemas, samples, docs, and content while shipping excludes editor-only code |
| Version-bound GPU interop | An exact-version package encodes custom wgpu work through a scoped epoch-aware API without owning submit/present or editing the stock backend |
| Safe disable/remove | Disabling or removing a referenced package produces a preview, preserves user-modified content, and leaves the last-good project/runtime recoverable |

# Recommendations

1. Keep ADR 0046's closed plugin commit and static declarations. They reduce hidden authority, not
   ordinary runtime expressiveness.
2. Make user-task reachability the compatibility test: add behavior, replace a domain, coexist with
   another toolkit, own game UI, and distribute a multi-role package.
3. Treat physics replacement as the named trigger for ADR 0046's unresolved cross-plugin slot
   question. Do not generalize the slot carrier before that tracer.
4. Treat Dear ImGui primary-window integration as concrete pressure for OQ-017 and ADR 0094's
   candidate render feature/pass or scoped encoding path. Do not begin with multi-viewport or full
   Render Host replacement.
5. Use the existing render-specific exact-version interop hypothesis as the first tracer for a
   possible cross-domain compatibility tier. Promote it only after clean-room evidence; stable and
   version-bound paths should coexist instead of forcing every experiment into either a permanent
   API or a source fork.
6. Preserve `RuntimeUiPlugins` as optional. Define shared focus/text/accessibility/render-layer
   protocols below UI frameworks rather than selecting one universal UI backend.
7. Keep Cargo/Git/local packages as the first distribution path, but require store-ready metadata
   and clean install/update/remove UX before claiming ecosystem readiness.
8. Do not use first-party-private integration as evidence. Every claimed extension role needs a
   clean-room external-package fixture and a no-core-edit gate.

# Document Implications

- No new ADR is justified by this research alone.
- ADR 0046 already owns ordinary runtime plugin freedom and the open cross-plugin replacement
  question.
- ADR 0016 already owns the evidence threshold for stable Adapter contracts.
- OQ-017 owns raw platform-event pressure; Dear ImGui gives it a named, bounded tracer.
- ADR 0094 and its render harness own the comparison between a portable render feature/pass,
  scoped backend encoding, version-bound wgpu interop, and full Render Host replacement.
- OQ-031 and the source-package harness already own distribution/package topology and should absorb
  store-ready UX evidence when activated.
- The active RGF plan remains the sole implementation order. These recommendations should not be
  inserted into RGF-U13 while its desktop parity work is in progress.

# Details

The direct conclusion is that Nara is not currently over-restrictive for ordinary runtime plugins.
It is currently under-proven for deep third-party integrations. The remedy is not to weaken App and
Host ownership globally. It is to admit narrow stable roles plus an honest version-bound escape
hatch, then prove both with external user workflows.

# Next Action

Discuss and either accept, revise, or reject elevating the existing render-specific version-bound
interop candidate into a product compatibility class. If accepted as a requirement to investigate,
use the Dear ImGui primary-window tracer to determine the smallest render and platform contribution
shape after RGF-U13 closes. Capture a durable decision only after that tradeoff and tracer scope are
agreed.

# Citations

## Nara And Local Primary Sources

- **[N1]** `docs/architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md`, especially
  lines 85-122, 185-199, and 232-244.
- **[N2]** `crates/nara_app/src/plugin/group.rs`, `PluginSlot`, `EditedPluginGroup`, and typed edit
  methods; `crates/nara_app/src/plugin/resolve.rs`, slot validation and duplicate-slot rejection.
- **[N3]** `crates/nara_app/src/lib.rs`, `App::insert_resource`, `App::add_systems`,
  `App::configure_sets`, and `App::add_plugins`; `crates/nara_app/src/plugin.rs`, `Plugin`.
- **[N4]** `docs/architecture/adr/0010-plugin-lifecycle-dependencies-and-failure.md`, closed commit,
  forbidden nested install, runner ownership, poisoning, and shutdown rules.
- **[N5]** `crates/nara_window/src/lib.rs`, `WindowEvent`; `crates/nara_input/src/lib.rs`, input
  primitives; `crates/nara_winit/src/lib.rs`, `WinitWindowEvent` translation.
- **[N6]** `crates/nara_render_wgpu/src/backend.rs`, private Device/Queue and packet execution;
  `crates/nara_render_wgpu/src/lib.rs`, stock surface render pass.
- **[N7]** `docs/architecture/open-questions.md`, OQ-017 and OQ-022;
  `docs/architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md`.
- **[N8]** `docs/architecture/adr/0016-extension-seams-for-backends-and-domain-modules.md`,
  production-shaped pressure and independent implementation evidence threshold.
- `docs/architecture/render-extension-capability-interface-design.md`, especially its separate
  scoped encoding, exact-version wgpu/native interop, and Render Host permission levels.
- `docs/architecture/source-extension-package-interface-design.md`, especially PX-01/PX-02,
  PX-13/PX-14, PX-17/PX-19, PX-42/PX-43, and the typed contribution matrix.
- `repo-ref/dear-imgui-rs/backends/dear-imgui-bevy/README.md` and its referenced backend source.

## External Primary Sources

- **[U1]** [Made with Unity](https://unity.com/made-with-unity).
- **[U2]** [Unity Asset Store tools](https://assetstore.unity.com/tools).
- **[U3]** [Unity Scriptable Render Pipeline](https://docs.unity3d.com/2019.4/Documentation/Manual/ScriptableRenderPipeline.html).
- **[U4]** [Unity custom package layout](https://docs.unity3d.com/6000.0/Documentation/Manual/cus-layout.html).
- **[G1]** [Godot Showcase](https://godotengine.org/showcase/) and
  [showcase criteria](https://godotengine.org/showcase/submissions/).
- **[G2]** [Godot Asset Library](https://docs.godotengine.org/en/stable/community/asset_library/using_assetlib.html).
- **[G3]** [Godot editor plugins](https://docs.godotengine.org/en/stable/tutorials/plugins/editor/making_plugins.html) and
  [import plugins](https://docs.godotengine.org/en/stable/tutorials/plugins/editor/import_plugins.html).
- **[G4]** [Godot GDExtension](https://docs.godotengine.org/en/latest/engine_details/engine_api/gdextension/what_is_gdextension.html).
- **[G5]** [Godot PhysicsServer3DManager](https://docs.godotengine.org/en/stable/classes/class_physicsserver3dmanager.html).
- **[G6]** [Godot RenderingServer](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html).
- **[E1]** [Unreal Engine plugins](https://dev.epicgames.com/documentation/en-us/unreal-engine/plugins-in-unreal-engine).
- **[E2]** [Installing Unreal plugins from Fab](https://dev.epicgames.com/documentation/en-us/unreal-engine/working-with-plugins-in-unreal-engine).
- **[E3]** [Physics in Unreal Engine](https://dev.epicgames.com/documentation/unreal-engine/physics-in-unreal-engine).
- **[D1]** [Defold games showcase](https://defold.com/showcase).
- **[D2]** [Defold Asset Portal](https://defold.com/assets/).
- **[D3]** [Defold native extensions](https://defold.com/manuals/extensions/) and
  [Defold libraries](https://defold.com/manuals/libraries/).
- **[I1]** `repo-ref/dear-imgui-rs/backends/dear-imgui-bevy/README.md`, frame lifecycle, render
  targets, docking/multi-viewport, and input-policy sections.

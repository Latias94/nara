---
type: "Engineering Research"
title: "Standalone generic editor product model"
description: "Cross-engine and repository-grounded research for a Unity-, Unreal-, and Godot-class Nara Editor product model."
timestamp: 2026-08-18T10:54:49Z
record_id: "889830d8600a4f239fdea916381d9d47"
tags: ["editor", "authoring", "play", "extensions", "schema", "product"]
status: "research"
producer_id: "codex-root"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "a6afc4bfb64ac87dbd086baaa071ff2a9608a457"
---

# Standalone Generic Editor Product Model

## Status and Evidence Policy

This note is research evidence, not implementation authority. It does not activate a plan, accept an
ADR, or authorize a public extension API.

The review combines:

- the current Nara Editor, Scene, Prefab, UI Canvas, Game View, schema, project-content, and Host
  contracts at the commit recorded above;
- earlier Nara research on packages, missing types, content graphs, and extension ecosystems;
- current official documentation and source for Godot, Unity, Unreal Engine, Fyrox, Bevy Remote,
  and their editor/runtime boundaries.

The product question is not whether Nara can open one first-party game through a compiled Rust
composition root. It can. The question is whether Nara can become a long-lived productivity tool
that opens projects, edits durable content, survives broken game code, runs the game, and hosts
bounded editor extensions without rebuilding a project-specific editor executable.

## Decisive Verdict

Nara should ship one project-generic `nara-editor` product with four distinct planes:

1. a non-executable authoring plane that opens and preserves project source without compiling or
   loading project Rust;
2. a supervised project Runtime plane that builds and runs as a child process for normal Play and
   Run;
3. a staged Editor extension plane that begins with declarative contributions and later admits
   isolated executable extensions through capability-specific protocols;
4. an optional trusted native companion plane for first-party or expert projects, never required
   for ordinary open, recovery, or source editing.

The current in-process Game View and compiled `EditorProjectIntent` are valuable proven Adapters.
They should not define the product identity.

The default product promise should be:

> Install Nara Editor once. Open any compatible Nara project immediately. Scene, Prefab, UI,
> assets, animation data, materials, and diagnostics remain available when project code does not
> compile. Building is required to execute changed behavior, not to browse or repair authored
> source.

## Why the Current Reference Game Failure Matters

The current standalone binary is real, but its project-open path is still runtime-composition
driven:

- `nara-editor` opens with an empty `ProductRecipe`;
- project-owned schema providers and native bindings only exist when compiled into the caller;
- project content and the startup Scene require a complete resolved registry before the native
  shell starts;
- Reference Game persists project-owned component types such as `reference_game.WaveSpawn`;
- the generic binary therefore exits before opening the shell when those providers are absent.

This behavior is consistent with the current complete-schema contract. It also proves that the
current binary is not yet a mature generic editor. Adding a Reference Game-specific binary would
hide the missing product contract instead of solving it.

## What Mature Editors Actually Separate

The engines differ in implementation language and trust model, but the useful pattern converges:

| Concern | Mature product behavior | Nara implication |
|---|---|---|
| Editor identity | One reusable editor application or installation opens many projects. | `nara-editor` remains the primary product, not a generated per-project editor. |
| Source authority | Scene, resource, asset, and metadata files remain durable independently of a live game instance. | Authoring must consume a non-executable project description, not a native Runtime registry. |
| Derived data | Imports and compiled artifacts are caches or generated products, not the source of truth. | Add an authoring asset/index layer without turning import artifacts into persistent authority. |
| Code iteration | Data edits avoid compilation; behavior or structural code changes may compile. | Classify edits instead of promising universal hot reload. |
| Play | The editor supervises a distinct game lifecycle, even when its output is embedded in an editor panel. | Make child-process Play the normal mature path; embedding is a presentation Adapter. |
| Broken code | The editor offers recovery or degraded operation instead of becoming unusable. | Missing or stale executable bindings must not silently delete source or force total process exit. |
| Domain tools | Animation, materials, UI, assets, and profiling have dedicated product Modules. | A generic property grid is not a substitute for first-party domain editors. |
| Extensions | Extensions are explicit products with lifecycle and trust boundaries. | Runtime plugins, editor panels, importers, and packages must not be one ambient Rust plugin concept. |

### Godot

Godot is the closest product-shape reference. A generic editor and Project Manager open projects,
the editor owns source/import state, and game execution is supervised as a separate process. Its
embedded game view still uses a separate game process. `EditorPlugin`, tool scripts, and
GDExtension provide high extension power, but they also execute trusted code in the editor's fault
domain. `MissingNode` and `MissingResource` show the value of preserving original identity and
properties when implementations are unavailable.

### Unity

Unity demonstrates the productivity value of separating serialized project data and Asset Database
state from compiled managed assemblies. Project scripts compile into reloadable assemblies rather
than rebuilding the Editor executable. Safe Mode keeps the editor available while scripts fail.
Custom inspectors, windows, importers, property drawers, packages, animation, and shader tools are
important product surfaces, but Unity's managed in-process model cannot be copied directly into
Rust without accepting a very different ABI and crash model.

### Unreal Engine

Unreal demonstrates the value of deep domain editors, generated reflection, an asset registry,
derived-data caches, Play-In-Editor modes, and explicit Runtime versus Editor modules. It also shows
the cost of native coupling: project modules, Editor targets, Live Coding, object reinstancing, and
version compatibility remain build- and ABI-sensitive. Nara should copy the workflow decomposition,
not require a native project Editor target before ordinary authoring is possible.

### Fyrox and Rust-native approaches

Fyrox has a generic editor executable, but its normal project workflow builds a project editor crate
that statically links or dynamically loads the game plugin. This gives excellent direct Rust access
and makes custom types easy when the build is healthy. It also makes compile latency, missing native
constructors, compiler compatibility, and project-code failures part of the editor-open path.

Bevy Remote and similar runtime inspection tools are useful for a running game, but they cannot
solve offline first open, source recovery, or editing while the project is unbuildable.

## Recommended Product Architecture

```text
                       one installed product
                    +--------------------------+
                    |       nara-editor        |
                    | workspace / documents    |
                    | undo / save / recovery   |
                    | shell / windows / GPU    |
                    +------------+-------------+
                                 |
               +-----------------+------------------+
               |                                    |
     +---------v----------+              +----------v-----------+
     | Authoring Project  |              | Execution Coordinator|
     | schema bundle      |              | build / run / stop   |
     | source asset index |              | diagnostics / logs   |
     | scene/prefab data  |              | generation handshake |
     | degraded records   |              +----------+-----------+
     +---------+----------+                         |
               |                            supervised process
               |                                      |
     +---------v----------+              +------------v----------+
     | First-party domain |              | project game/runtime  |
     | Scene / UI / asset |              | normal Nara product   |
     | animation/material |              | optional frame/input  |
     | authoring Modules  |              | protocol Adapter      |
     +---------+----------+              +-----------------------+
               |
     +---------v-----------------------------------------------+
     | Editor extension ladder                                 |
     | declarative -> isolated Wasm/process -> trusted native  |
     +---------------------------------------------------------+
```

### 1. Standalone Editor application

The long-lived Editor remains the sole owner of:

- workspace and document sessions;
- selection, history, dirty state, save receipts, conflict and recovery state;
- shell layout, windows, input, GPU execution, Scene View, Canvas, and Game View placement;
- extension admission, activation, diagnostics, disable, and restart;
- project build, Play, Run, Stop, crash, and orphan cleanup orchestration.

Project code must not receive unrestricted mutable access to this state.

### 2. Authoring Project Module

Introduce a private deep Module, conceptually `AuthoringProject`, that can open a project from:

- `nara.toml` and project identity;
- a versioned non-executable Authoring Schema Bundle;
- source documents and a project-wide source asset index;
- source-to-derived import metadata and diagnostics;
- explicit freshness and provenance facts.

It should not require `RuntimePlan`, native component registration, Bevy `World`, project Rust, or a
project-specific editor binary.

The existing stable component and field IDs, schema catalogs, predecessor fingerprints,
tombstones, generic values, Scene/Prefab documents, patches, and authoring sessions are strong
building blocks. The missing contract is how a project publishes those facts as a non-executable
artifact and how the Editor handles stale or unavailable implementation bindings.

Do not add an `authoring_mode` flag to the current runtime-oriented `ProjectContentLoader`. The
authoring and executable loaders have different roots, completeness rules, retention, and failure
semantics. Share only concrete low-level readers and value/schema codecs.

### 3. Lossless degraded authoring

A mature generic editor cannot require all project code to be available before it can preserve
source. Nara needs explicit record readiness, for example:

- `Complete`: schema and authoring behavior are current;
- `KnownUnbound`: schema is known but native Runtime behavior is unavailable;
- `UnknownSchema`: the Editor cannot interpret the record but can preserve its canonical payload;
- `FutureOrMigrationBlocked`: the record is recognized but cannot be safely transformed;
- `Rejected`: structural integrity or dependency safety is insufficient even for preservation.

Each state needs operation gates. Displaying metadata, moving an entity, editing a known sibling
field, deleting an unknown record, duplicating it, applying migrations, and saving the containing
document are different authorities. Unknown data must never become silent deletion authority.

This requires accepting, revising, or superseding Proposed ADR 0090 before implementation.

### 4. Play and Run

Normal mature Play should build and launch the game as a supervised child process:

1. select the exact project target and profile;
2. run a bounded, cancellable Cargo build and consume structured JSON diagnostics;
3. publish an immutable executable generation;
4. start the child with exact project, schema, content, executable, and protocol identities;
5. reject stale handshakes before accepting input or frames;
6. own logs, Pause, Step, Stop, Restart, crash reporting, timeout, and orphan cleanup;
7. retain the last-good executable when a new build fails.

External-window Play should be the first complete Adapter. Embedded Game View should later consume
the same Runtime session through a bounded frame/input transport. It must not require a second game
or product model.

The existing in-process Game View remains useful for first-party canaries and latency-sensitive
trusted workflows, but it should become an optional Adapter rather than the only Play architecture.

### 5. First-party authoring domains

Mature productivity is not created by a universal Inspector alone. Nara should build domain Modules
with their own document, projection, validation, preview, and action semantics:

- Scene and Prefab: already the strongest area;
- Runtime UI: current Prefab and Canvas work is a credible foundation;
- assets and import settings: project browser, dependency facts, reimport, diagnostics, and cache
  state;
- animation: clips, tracks, curves, events, target identity, preview, and write arbitration;
- materials and shaders: source assets, parameters, diagnostics, preview, specialization, and
  fallback;
- audio, navigation, physics, localization, and profiling only when concrete product workflows
  require them.

These should be deep first-party Modules before they become generic registries. A second real
implementation is the evidence threshold for extracting a shared Interface.

## Editor Extension Model

Nara should not expose one universal `EditorPlugin` Rust trait that grants access to workspace,
toolkit, filesystem, process, and GPU state. That would combine unrelated authority and create an
unstable native ABI problem.

Use a staged capability ladder.

### Tier 1: declarative contributions

No executable project code is required. A versioned contribution bundle may describe bounded
facts such as:

- component labels, grouping, ranges, units, enums, asset filters, and read-only reasons;
- icons, menu metadata, document associations, and simple Inspector layout hints;
- asset/import type descriptors and configuration schemas;
- gizmo descriptors whose behavior is entirely expressible through existing typed actions;
- diagnostics and help links.

Keep this vocabulary intentionally finite. Do not grow a second general programming language or an
expression/callback DSL inside schema metadata.

### Tier 2: supervised process Extension Host

After one concrete extension proves the need, introduce capability-specific protocols for custom:

- Inspector panels;
- dockable windows;
- scene gizmos and overlays;
- asset importers and processors;
- build or validation tools.

The ordinary executable form should be a supervised child process over a bounded, versioned,
generation-stamped semantic protocol. Each protocol should pass immutable snapshots and receive
revision-bound typed actions. It must not pass Rust trait objects, Bevy `World`, egui contexts, WGPU
handles, or raw filesystem paths.

Different extension kinds need different capabilities and failure policy. An importer, Inspector,
and dockable analytics window should not share one god-object host API. A custom importer is the
best first executable tracer because its input capability, scratch output, dependencies, timeout,
validation, and atomic publication create a concrete and testable fault boundary.

Process isolation is a crash-containment boundary, not automatically an operating-system security
sandbox.

### Tier 3: optional Wasm Adapter

Wasm should be evidence-gated rather than mandatory. It is a good fit for portable validation,
deterministic transformations, procedural gizmo geometry, and small Host-rendered panel reducers.
It is a poor fit for direct toolkit/GPU/window access, large native SDKs, or high-frequency callback
traffic.

If introduced, Wasm must reuse the same semantic contribution and command model as the process
Host. It is another Adapter, not a second plugin product model or mandatory author language.

### Tier 4: trusted native companion

A statically compiled project/Editor companion remains useful for first-party games and expert
tools that require direct Rust performance or engine-private integration. It should:

- be opt-in and visibly trusted;
- use immutable build generations and last-good activation;
- never be required for generic open or recovery;
- remain a thin composition root over the shared Editor product;
- restart cleanly for structural changes rather than promise unsafe universal unloading.

Avoid arbitrary Rust dynamic-library loading as the default. Rust has no stable language ABI, and
allocator, compiler, panic, dependency, and trait-object compatibility would become product
correctness concerns.

## User Workflow Target

The first complete trajectory should be:

```text
Install/launch nara-editor
  -> Project Manager: create, import, or open a project
  -> open project without building game code
  -> browse assets and open Scene/Prefab/UI documents
  -> edit, undo, save, close, reopen
  -> break the project's Rust build deliberately
  -> keep browsing and safely editing supported source in degraded mode
  -> press Play
  -> receive structured build diagnostics
  -> fix code and rebuild only the game artifact
  -> run a supervised child Runtime
  -> inspect logs and remote Runtime facts
  -> Pause / Step / Stop / Restart
  -> crash the child and recover without losing the Editor session
  -> export from the same accepted project source
```

Reference Game should become one consumer of this trajectory, not the owner of the Editor shape. A
second clean-room project with different schema and content structure must open through the same
installed Editor without compiling a bespoke Editor binary.

## Candidate Architectures

### A. Standalone schema-first Editor plus external Runtime — selected base

Strengths:

- opens and repairs source without a project build;
- strongest crash isolation and recovery;
- fits Nara's stable IDs, generic values, Host-owned workspace, typed actions, and fallible
  lifecycle;
- cleanly separates data-edit latency from Rust build latency.

Cost:

- requires a real non-executable authoring artifact and degraded authoring contract;
- cannot alone express arbitrary executable custom tools.

### B. Generated/static project Editor companion — optional trusted tier

Strengths:

- easiest path from current `EditorProjectIntent` and `ProductRecipe`;
- direct Rust types and highest custom-tool power;
- current in-process Game View remains available.

Cost:

- compilation can block current authoring;
- shared editor crash domain;
- tight version coupling and restart latency.

It is useful as an optional power tool, not as the default product.

### C. Native Rust dynamic modules in the Editor — rejected default

This combines high privilege with the weakest ABI and unload safety. It does not solve first open
when the project cannot build, and it puts native project faults inside the Editor process. A future
explicit C ABI for trusted extensions may be considered only after a concrete need and compatibility
budget exist.

### D. Standalone Editor plus isolated extension host — selected mature destination

This closes the custom-tool gap while preserving open/recovery and crash isolation. It is also the
most expensive architecture, so it must be introduced one capability and one real extension at a
time rather than as a speculative universal package framework.

## What to Preserve, Refactor, and Avoid

### Preserve

- standalone `nara-editor` product identity;
- Host-owned workspace, persistence, Play lifecycle, windows, and GPU;
- UI-neutral immutable frame models and ordered typed actions;
- stable schema, entity, component, field, asset, document, and generation identities;
- Scene/Prefab nominal types, bounded patch history, exact receipts, and failure atomicity;
- Scene View, UI Canvas, and Game View as private presentation Adapters;
- `ProductRecipe` as compiled Runtime composition authority.

### Refactor

- separate authoring project open from complete executable Runtime admission;
- extract or deepen a data-only schema Module that does not depend on native ECS binding;
- add project-wide authoring asset/index ownership rather than using only the startup Runtime
  closure;
- move normal Play toward a child-process execution coordinator;
- treat compiled Editor intent as an optional trusted Adapter;
- make the shell survive project-open failure and expose recovery/degraded diagnostics.

### Avoid

- a Reference Game-specific Editor product;
- rebuilding a project Editor just to inspect a Scene;
- discovering arbitrary Rust plugins by string from `nara.toml`;
- a universal public plugin/registry/transport before two real consumers exist;
- raw Rust dylib hot reload as the default extension model;
- passing egui, WGPU, ECS `World`, or unrestricted filesystem/process authority to extensions;
- claiming that schema metadata can replace animation, shader, material, importer, or profiling
  product Modules.

## Required Architecture Decisions Before Coding

The next implementation plan should not start until focused decisions settle:

1. lossless degraded authoring and operation gates, by accepting or replacing ADR 0090;
2. the Authoring Schema Bundle owner, format, predecessor/migration metadata, freshness, and
   relationship to Runtime binding under ADR 0081 and the schema open questions;
3. child-process Play identity, lifecycle, transport, last-good artifact, and how it refines ADR
   0112 without erasing the proven in-process placement;
4. the first concrete Editor extension contribution and its trust/disable/recovery semantics under
   OQ-031;
5. a first-party animation workflow and a material/shader workflow before freezing generalized
   authoring Interfaces;
6. measurable cold-open, data-edit, code-build, Play-start, recovery, and extension-failure product
   scenarios.

## Recommended Planning Sequence

1. **Editor Product Model decision** — standalone generic Editor, non-executable authoring plane,
   supervised child Runtime, extension ladder, and compatibility with existing Scene/Canvas/Game
   View Adapters.
2. **Generic Project Open and Recovery** — Project Manager, Authoring Schema Bundle, project-wide
   source index, complete/degraded/rejected states, shell survives open failure.
3. **External Play v1** — build coordinator, exact artifact handshake, external window, logs,
   Pause/Step/Stop/Restart, crash/orphan cleanup, last-good executable.
4. **Embedded Game View process Adapter** — reuse the same session protocol only after external Play
   is product-complete.
5. **First isolated Editor extension** — choose one real custom Inspector or importer; do not build a
   universal extension kernel first.
6. **Domain productivity lanes** — asset browser/import status, animation, material/shader, profiler,
   and other tools as separate product plans.

## Primary Sources

### Nara

- [`STRATEGY.md`](../../../STRATEGY.md)
- [`docs/architecture/README.md`](../../architecture/README.md)
- [ADR 0047: Editor Workspace and Scene Document State](../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0081: Schema Source, Stable Identity, Catalog, and Runtime Binding](../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Proposed ADR 0090: Unavailable Schema and Lossless Authoring](../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [ADR 0104: Host-Owned Editor Workspace Installation](../../architecture/adr/0104-host-owned-editor-workspace-installation.md)
- [ADR 0111: Editor Scene View Execution and Picking Authority](../../architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md)
- [ADR 0112: Editor Game View Runtime Presentation and Input Authority](../../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md)
- [ADR 0113: Editor UI Prefab Authoring and Canvas Authority](../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
- [Extension ecosystem research](extension-ecosystem-engine-research.md)
- [Godot and Unity package/extension lifecycle research](godot-unity-package-extension-lifecycle-research.md)

### Godot

- [Project Manager](https://docs.godotengine.org/en/4.7/tutorials/editor/project_manager.html)
- [Import process](https://docs.godotengine.org/en/4.7/tutorials/assets_pipeline/import_process.html)
- [Game embedding](https://docs.godotengine.org/en/4.7/tutorials/editor/game_embedding.html)
- [Making plugins](https://docs.godotengine.org/en/4.7/tutorials/plugins/editor/making_plugins.html)
- [EditorInspectorPlugin](https://docs.godotengine.org/en/4.7/classes/class_editorinspectorplugin.html)
- [EditorNode3DGizmoPlugin](https://docs.godotengine.org/en/4.7/classes/class_editornode3dgizmoplugin.html)
- [EditorImportPlugin](https://docs.godotengine.org/en/4.7/classes/class_editorimportplugin.html)
- [GDExtension](https://docs.godotengine.org/en/4.7/engine_details/engine_api/gdextension/what_is_gdextension.html)
- [MissingNode source](https://github.com/godotengine/godot/blob/4.7.1-stable/scene/main/missing_node.h#L35-L70)
- [Editor child-process runner](https://github.com/godotengine/godot/blob/4.7.1-stable/editor/run/editor_run.cpp#L40-L232)

### Unity

- [Manage projects](https://docs.unity3d.com/hub/manual/ManageProjects.html)
- [Code lifecycle](https://docs.unity3d.com/6000.5/Documentation/Manual/programming-code-lifecycle.html)
- [Asset Database](https://docs.unity3d.com/6000.5/Documentation/Manual/asset-database-contents.html)
- [Asset Database refresh](https://docs.unity3d.com/6000.5/Documentation/Manual/AssetDatabaseRefreshing.html)
- [Safe Mode](https://docs.unity3d.com/6000.2/Documentation/Manual/SafeMode.html)
- [Game view](https://docs.unity3d.com/6000.5/Documentation/Manual/GameView.html)
- [EditorWindow](https://docs.unity3d.com/6000.5/Documentation/ScriptReference/EditorWindow.html)
- [Custom Inspector](https://docs.unity3d.com/6000.5/Documentation/Manual/UIE-HowTo-CreateCustomInspector.html)
- [ScriptedImporter](https://docs.unity3d.com/6000.5/Documentation/ScriptReference/AssetImporters.ScriptedImporter.html)

### Unreal Engine

- [Compiling Game Projects](https://dev.epicgames.com/documentation/en-us/unreal-engine/compiling-game-projects-in-unreal-engine-using-cplusplus)
- [Unreal Engine Modules](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-modules)
- [Asset Registry](https://dev.epicgames.com/documentation/en-us/unreal-engine/asset-registry-in-unreal-engine)
- [Derived Data Cache](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-derived-data-cache-in-unreal-engine)
- [Playing and Simulating](https://dev.epicgames.com/documentation/en-us/unreal-engine/playing-and-simulating-in-unreal-engine)
- [Live Coding](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-live-coding-to-recompile-unreal-engine-applications-at-runtime)
- [Plugins](https://dev.epicgames.com/documentation/en-us/unreal-engine/plugins-in-unreal-engine)
- [Details customization](https://dev.epicgames.com/documentation/en-us/unreal-engine/API/Editor/PropertyEditor/IDetailCustomization)
- [Asset editor toolkit](https://dev.epicgames.com/documentation/en-us/unreal-engine/API/Editor/UnrealEd/Toolkits/FAssetEditorToolkit)

### Rust ecosystem

- [Fyrox Book: Editor workflow](https://fyrox-book.github.io/fyrox/editor/overview.html)
- [Bevy Remote](https://docs.rs/bevy_remote/latest/bevy_remote/)
- [Bevy Editor tracking issue](https://github.com/bevyengine/bevy/issues/85)

## Uncertainties and Evidence Gates

- Cross-process embedded rendering cost and portability require a prototype; external-window Play
  must remain a complete fallback.
- A schema-first Editor cannot provide arbitrary custom tool behavior without an executable
  extension tier.
- Process isolation contains crashes but does not sandbox Cargo build scripts, proc macros, or
  trusted native tools from the user's operating-system authority.
- Lossless preservation must be proven for nested unknown records, references, overrides, and
  migrations before degraded editing is allowed to save broadly.
- Animation and shader/material architecture should be selected from concrete authoring workflows,
  not inferred from the generic Inspector.
- The first extension protocol should be extracted only after two independently useful Adapters or
  consumers prove the seam.

---
type: "Engineering Research"
title: "Godot and Unity package extension lifecycle research"
description: "Primary-source audit of Godot editor addons, GDExtension lifecycle, Asset Store behavior, Unity package governance, and implications for Nara extension placement."
timestamp: 2026-07-20T12:02:43Z
record_id: "713c5643b8f54ea2b9a2c7eedc2aaccf"
tags: ["nara", "godot", "unity", "package", "editor-extension", "gdextension", "native", "coreclr"]
producer_id: "codex-root"
run_id: "package-extension-product-contract-20260720"
related_plan: "docs/plans/2026-07-20-001-feat-package-extension-product-contract-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5bc321d41aba59072a1f97ccc0473f91e0b2c161"
---

# Summary

Godot and Unity demonstrate two different strengths that Nara should combine rather than copy as one mechanism.
Godot proves that a native C++ Editor can accept dynamically enabled script, managed, and native UI extensions through a language-neutral object and registration model, while still requiring restarts for some native changes.
Unity Package Manager provides the stronger model for package identity, dependency resolution, lock state, update, removal, and editable embedded packages.

The feasible Nara direction is a language-neutral Package above typed Contributions.
Ordinary executable Editor Contributions should use a replaceable isolated Extension Host, while source/static Rust runtime plugins retain full typed ECS freedom in fresh Runtime generations and privileged native integrations use separately admitted lifecycle and ABI contracts.

This record is non-normative research evidence.
The associated requirements-only Product Contract records the product decisions, while OQ-007, OQ-031, and OQ-045 remain the architecture decision gates.

# Research Question

Can a Rust Editor with a future Nara-owned UI provide Godot/Unity-class Package and extension freedom, including install, update, removal, dynamic editor UI, C# modules, and DLL/SO integrations, without making arbitrary in-process hot unloading the correctness baseline?

# Evidence Scope

The local Godot source snapshot is `repo-ref/godot` at commit `c939bf3791ce40ff70e0ee29f06486da1ebb6a84`.
The review followed Editor addon activation, `EditorPlugin` UI registration, GDExtension discovery/load/reload/unload, desktop library placement, Asset Store installation, and .NET project assembly reload.
Unity facts are based on Unity 6 first-party Package Manager documentation.

This note extends `docs/knowledge/engineering/extension-ecosystem-engine-research.md`, whose original Godot Asset Library policy gap is addressed here.
Detailed CoreCLR, Roslyn/MSBuild, Schema, debugger, and export findings remain in `docs/knowledge/engineering/godot-csharp-integration-research.md` and are summarized here only where they affect Package activation.

# Findings

## Distribution, Installation, Enablement, and Activation Are Different States

Godot's Asset Library and current Asset Store download an archive, show a file list, and copy selected files into a project.
The audited path does not record a resolved dependency graph, lock state, installed Package identity, owned-file ledger, user modifications, or a package-level rollback generation.
Existing destination files are treated as conflicts rather than an atomic replacement transaction (`repo-ref/godot/editor/asset_library/editor_asset_installer.cpp:578-590`).

Godot's Installed Plugins page does not derive state from Asset Store receipts.
It recursively discovers `res://addons/**/plugin.cfg`; enabling a plugin stores its path under `editor_plugins/enabled` and instantiates its `EditorPlugin`, while disabling destroys that instance (`repo-ref/godot/editor/editor_node.cpp:4460-4575`).
Installing an archive therefore does not automatically activate executable editor code.

One archive may contain ordinary resources and scripts, an Editor addon, GDExtension binaries, or a complete project template.
Their behavior comes from specialized consumers after extraction, not from one universal Package runtime callback.

The useful Nara precedent is the state separation:

```text
available release
  -> resolved and installed release
  -> enabled Contributions
  -> built or acquired executable artifacts
  -> active Host and Runtime generations
```

Nara should exceed Godot's archive installer when it promises update and removal.
Package identity, dependency/lock evidence, file ownership, target artifacts, activation state, and active generation must remain separately inspectable.

## Godot Dynamically Extends an Object and Registration Model, Not Its C++ Editor Binary

Godot has three distinct editor extension paths:

1. Built-in C++ plugins are compiled into the Editor and constructed from static factories during startup (`repo-ref/godot/editor/editor_node.cpp:9451-9482`).
2. Script addons are discovered through `plugin.cfg`, loaded as a language-neutral `Script`, checked for tool mode and `EditorPlugin` inheritance, instantiated, and added to the Editor tree (`repo-ref/godot/editor/editor_node.cpp:4500-4575`).
3. GDExtension libraries register classes through the extension interface; classes inheriting `EditorPlugin` are instantiated and routed through the same add/remove lifecycle (`repo-ref/godot/editor/editor_node.cpp:4436-4457`).

An enabled plugin enters the Editor tree and can register UI from `_enter_tree`; disabling removes it, invokes the paired lifecycle, and destroys the instance (`repo-ref/godot/editor/editor_node.cpp:4405-4433`).
EditorPlugin exposes paired add/remove operations for docks, named Editor containers, menus, inspectors, gizmos, importers, exporters, debugger plugins, and custom types (`repo-ref/godot/editor/plugins/editor_plugin.cpp:94-238`, `241-251`, `444-510`).

This works because extension code participates in a stable ClassDB/Object/Variant/Node/Control model.
It does not share arbitrary C++ templates, vtables, or widget implementation layouts with third-party code.
The Editor keeps window, tree, input, layout, and registry authority.

Godot relies heavily on extension authors to call the matching remove operations.
Nara can improve this boundary by making registrations generation-owned tokens or leases, so disabling or replacing one Contribution automatically revokes everything registered by that generation.

## GDExtension Proves Native Dynamic Loading, but Not Universal Hot Unloading

A `.gdextension` descriptor selects a target-specific native library and entry symbol.
Godot loads the symbol and hands the extension a versioned C-compatible function lookup interface rather than a C++ ABI (`repo-ref/godot/core/extension/gdextension_library_loader.cpp:178-242`; `repo-ref/godot/core/extension/gdextension_interface.cpp:237-260`).
Initialization proceeds through Core, Servers, Scene, and Editor levels as applicable, and deinitialization reverses those levels (`repo-ref/godot/core/extension/gdextension.h:101-148`; `repo-ref/godot/core/extension/gdextension.cpp:841-865`).

Native reload has several gates:

- Reload is Editor-only and must be enabled globally and by the descriptor (`repo-ref/godot/core/extension/gdextension_manager.cpp:158-174`; `repo-ref/godot/core/extension/gdextension_library_loader.cpp:215-218`).
- Every instantiable extension class must supply recreation support or the extension ceases to be reloadable (`repo-ref/godot/core/extension/gdextension.cpp:497-507`).
- An extension requiring an initialization level earlier than the current safe level returns `LOAD_STATUS_NEEDS_RESTART`; Core/Servers-level additions to an already initialized Editor are a representative case (`repo-ref/godot/core/extension/gdextension_manager.cpp:44-56`).
- Parent-class and runtime/editor classification changes can also require restart (`repo-ref/godot/core/extension/gdextension.cpp:470-482`).

Instance reconstruction preserves storage-marked Variant properties where compatible.
It does not preserve arbitrary heap state, static variables, threads, callbacks, GPU objects, native handles, or third-party library sessions (`repo-ref/godot/core/extension/gdextension.cpp:924-1083`).

Godot's reload path unloads and closes the predecessor before successfully opening the replacement (`repo-ref/godot/core/extension/gdextension_manager.cpp:175-207`).
The Windows temporary DLL copy avoids build-output file locking, but it is not a last-good rollback protocol (`repo-ref/godot/platform/windows/os_windows.cpp:480-515`, `564-578`).

Nara can provide a stronger failure model by staging a versioned artifact, validating and starting an unpublished candidate, retaining the predecessor until publication, and replacing the owning Host or Runtime generation when in-process retirement is uncertain.

## Raw Rust Dynamic Libraries Are Not the GDExtension Model

Rust can produce `dylib` and `cdylib` outputs, and a loader can open them.
That does not make Rust trait objects, generics, enums, allocation ownership, panic behavior, or dependency versions a stable ecosystem ABI.
The Rust Reference explicitly states that the Rust ABI has no stability guarantees.

Nara's current runtime `Plugin`, Schema providers, component codecs, ECS systems, component drop glue, callbacks, and background work can retain code pointers into the module.
Dropping the original plugin object or receiving success from `FreeLibrary`/`dlclose` does not prove those references have retired.

A future Nara Native Extension therefore requires a separate contract with versioned C-compatible tables, opaque generation handles, explicit allocator ownership, structured errors, callback leases, thread and affinity rules, quiescence, and a finite retirement result.
The first version should be allowed to keep the module loaded until its disposable Native Host or Runtime generation exits.

## Managed Reload Is Also Cooperative

Godot keeps CoreCLR and its engine tooling in process while loading project assemblies into a collectible `AssemblyLoadContext` in the Editor.
Disabling one C# EditorPlugin destroys its plugin instance and UI registrations, but it does not unload a per-Package runtime; project assemblies and dependencies share the project load context.

Collectible ALC unload is cooperative.
Static references, delegates, tasks, running threads, GC handles, event subscriptions, P/Invoke libraries, and native callbacks can retain the old generation.
Godot performs forced GC/finalizer cycles with a timeout and eventually asks the user to restart the Editor after repeated failures (`repo-ref/godot/modules/mono/glue/GodotSharp/GodotPlugins/Main.cs:232-282`; `repo-ref/godot/modules/mono/mono_gd/gd_mono.cpp:788-831`).

This evidence supports a replaceable CoreCLR Extension Host or Player as Nara's correctness baseline.
An in-Editor CoreCLR plus collectible ALC may later reduce latency for trusted Contributions, but failure to unload must degrade to replacing the owning Host or restarting the Editor rather than silently accumulating generations.

## Target Artifacts Are a Matrix, Not One DLL

Godot `.gdextension` descriptors select libraries using feature tags such as operating system, architecture, debug/release, and Editor context.
Export separately chooses artifacts for the product target (`repo-ref/godot/core/extension/gdextension_library_loader.cpp:74-175`, `273-385`; `repo-ref/godot/editor/export/gdextension_export_plugin.h:68-154`).

Windows loads a temporary copy and manages the dependent-library search directory.
Linux and BSD use `dlopen` and depend on explicit paths and runtime search layout.
macOS must account for dylib/framework layout, architecture slices, `@rpath`, `@loader_path`, signing, and bundle placement (`repo-ref/godot/platform/windows/os_windows.cpp:480-578`; `repo-ref/godot/drivers/unix/os_unix.cpp:1044-1088`; `repo-ref/godot/platform/macos/os_macos.mm:367-409`).

Desktop dynamic loading does not generalize to every target.
Godot's Web templates require explicit extension support, while iOS plugins commonly use static libraries or XCFrameworks.
Nara Package metadata must therefore represent structured Host and subject-target selectors and reject missing or ambiguous matches rather than treating `.dll/.so/.dylib` as the Package definition.

## Unity Package Manager Provides the Stronger Distribution Baseline

Unity Package Manager separates the project manifest, Package manifests, and lock file.
Direct and transitive dependencies form a resolved graph in which one Package version is selected, and lock state preserves deterministic resolution including Git commit identity.

UPM supports registry, scoped registry, local directory, local tarball, Git, and embedded editable sources.
Removing a direct Package preserves dependencies still required elsewhere, while embedded Packages are editable project copies that override cached versions.

These capabilities are materially stronger than archive extraction for Nara's stated product goal:

- dependency and reverse-dependency awareness;
- deterministic lock and provenance;
- installed identity independent from copied files;
- update and removal semantics;
- local/editable package development;
- one Package containing distinct Runtime and Editor roles.

Nara should borrow these governance properties without copying Unity's managed reflection model or making a custom registry an early prerequisite.
Cargo, Git, and local Packages can expose the first product workflow while the Package abstraction preserves a future registry source.

# Cross-Engine Comparison

| Concern | Godot evidence | Unity evidence | Nara implication |
|---|---|---|---|
| Distribution | Archive browsing and selective extraction | Manifest, dependency graph, lock state, multiple sources | Use Unity-like identity and ownership semantics rather than archive-only installation |
| Editor activation | `plugin.cfg` enablement instantiates and removes `EditorPlugin` | Editor assemblies and extension classes load with the project | Keep installation separate from enabled Contribution and active generation |
| Dynamic UI | Script or GDExtension classes add native `Control` objects and specialized providers | Managed Editor APIs add windows, inspectors, importers, and build callbacks | Expose one language-neutral semantic Editor API owned by the Editor Shell |
| Native code | GDExtension versioned C interface with conditional reload and restart outcomes | Native plugins are target-specific and below managed/SRP extension levels | Admit a separate privileged Native Extension tier, never raw Rust `Plugin` ABI |
| Managed code | Project ALC reload is cooperative and Editor-specific | Managed assemblies are the normal authoring path | Make managed code an optional Contribution family with Host replacement fallback |
| Deep engine integration | Built-in C++ modules require engine rebuild | Engine/platform modules remain product-installed components | Preserve an explicit Editor-restart level for process-level changes |

# Disposition for Nara

The accompanying Product Contract records these selected boundaries:

1. Package is a language-neutral distribution and installation identity above typed Contributions.
2. Installation and activation are separate; the UI exposes immediate, Extension Host replacement, Runtime replacement, and Editor restart effects.
3. Ordinary executable Editor Contributions default to an isolated Extension Host.
4. Same-process managed or native Contributions are explicit trusted privileges and may require restart.
5. Source/static Rust remains the complete typed runtime path and rebuilds into a fresh executable or Runtime generation for structural changes.
6. C# is a first-class optional Contribution family, not the Package definition or a mandatory second author language.
7. A future native binary API is versioned and C-compatible, not Rust `dyn Plugin`; its exact surface waits for a concrete tracer.
8. `nara.toml` requests semantic capabilities and settings but does not choose arbitrary providers by string identity.
9. Package governance should converge on dependency, lock, ownership, update, and removal behavior comparable to Unity UPM while retaining Cargo and NuGet as their source-graph authorities.

These decisions do not admit a Package Manager implementation, CoreCLR dependency, Native Extension ABI, Widget protocol, registry, marketplace, or stable compatibility promise.

# Alternatives Considered

## Copy Godot Asset Store and addon activation

This is simple and proven for distributing project files, but it cannot satisfy dependency-aware update, owned-file removal, deterministic lock, or atomic last-good publication.
It remains useful precedent for distribution/activation separation, not the target Package Manager.

## Make all executable Packages in-process

This maximizes direct access and minimizes IPC, but one extension can retain Editor state, crash the process, or make unloading unverifiable.
It remains an opt-in privileged tier, not the default.

## Make all executable Packages isolated

This gives the strongest reclamation and crash boundary, but it cannot efficiently provide every high-frequency renderer, physics, window, or native SDK integration.
It is the ordinary Editor extension default, complemented by source/static Runtime and privileged native paths.

## Use one graduated Package model

One Package identity contains Contributions whose placement follows their actual authority and latency needs.
This is selected because it provides one product workflow without hiding lifecycle differences.

# Remaining Research Gates

- Measure whether standard panel, Inspector, importer, and build workflows remain responsive through an isolated Extension Host.
- Complete one Nara UI panel before selecting the public widget/model diff or custom-surface protocol.
- Run one C# Behavior plus Editor Dock trial only after OQ-007's Schema, reference-game, Editor Host, and packaging baselines permit it.
- Choose one real native SDK or precompiled domain integration before drafting a public C ABI.
- Measure per-Package, per-trust-group, and per-project Extension Host startup, memory, crash isolation, and update behavior before selecting Host grouping.
- Validate local/Git/Cargo Package install, lock, ownership, update, and removal before investing in a remote registry or marketplace.

# Citations

## Nara Sources

- `docs/plans/2026-07-20-001-feat-package-extension-product-contract-plan.md`
- `docs/architecture/open-questions.md` OQ-007, OQ-031, and OQ-045
- `docs/architecture/extension-package-concept-guide.md`
- `docs/architecture/source-extension-package-interface-design.md`
- `docs/knowledge/engineering/extension-ecosystem-engine-research.md`
- `docs/knowledge/engineering/godot-csharp-integration-research.md`

## Godot Sources

- `repo-ref/godot/editor/editor_node.cpp:4405-4575`, EditorPlugin add/remove and addon activation.
- `repo-ref/godot/editor/plugins/editor_plugin.cpp:94-238`, Dock and Editor container registration/removal.
- `repo-ref/godot/editor/plugins/editor_plugin.cpp:241-510`, menu, importer, gizmo, export, debugger, and custom provider registration/removal.
- `repo-ref/godot/core/extension/gdextension_library_loader.cpp:40-385`, descriptor resolution, target library selection, entry loading, dependencies, and reloadability.
- `repo-ref/godot/core/extension/gdextension_manager.cpp:44-230`, staged initialization, load, unload, reload, and restart outcomes.
- `repo-ref/godot/core/extension/gdextension.cpp:470-507`, class compatibility and recreation gates.
- `repo-ref/godot/core/extension/gdextension.cpp:924-1083`, instance property reconstruction.
- `repo-ref/godot/editor/asset_library/editor_asset_installer.cpp:578-590`, installation conflict behavior.
- [Godot Asset Library](https://docs.godotengine.org/en/stable/community/asset_library/using_assetlib.html)
- [Godot Asset Store](https://docs.godotengine.org/en/stable/community/asset_store/what_is_asset_store.html)
- [Godot Editor plugins](https://docs.godotengine.org/en/stable/tutorials/plugins/editor/making_plugins.html)
- [Godot GDExtension descriptor](https://docs.godotengine.org/en/stable/engine_details/engine_api/gdextension/gdextension_file.html)
- [Godot GDExtensionManager](https://docs.godotengine.org/en/stable/classes/class_gdextensionmanager.html)
- [Godot Web extension support](https://docs.godotengine.org/en/stable/engine_details/development/compiling/compiling_for_web.html)
- [Godot iOS plugins](https://docs.godotengine.org/en/stable/tutorials/platform/ios/ios_plugin.html)

## Unity Sources

- [Unity package dependencies](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-dependencies.html)
- [Unity lock files](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-conflicts-auto.html)
- [Unity package installation sources](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-ui-install.html)
- [Unity Git dependencies](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-git.html)
- [Unity package update](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-ui-update.html)
- [Unity package removal](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-ui-remove.html)
- [Unity embedded dependencies](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-embed.html)

## Runtime and Language Sources

- [Rust Reference ABI](https://doc.rust-lang.org/reference/items/external-blocks.html#abi)
- [Rust linkage](https://doc.rust-lang.org/stable/reference/linkage.html)
- [.NET assembly unloadability](https://learn.microsoft.com/en-us/dotnet/standard/assembly/unloadability)

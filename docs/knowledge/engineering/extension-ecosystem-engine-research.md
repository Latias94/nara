---
type: "Research Note"
title: "Extension ecosystem research: packages, plugins, and editor contributions"
description: "Cross-engine evidence for Nara's package and extension contribution boundaries."
timestamp: 2026-07-13T09:51:55Z
record_id: "1fa0f67fa5b34c89b73b5237e7db2f3f"
producer_id: "codex-root"
git_commit: "23fb5af23abc4458b1cba4379f3b4ed75e07ad0d"
---

# Extension Ecosystem Research: Packages, Plugins, and Editor Contributions

**Status**: Research note, not an accepted architecture decision

**Date**: 2026-07-13

**Question**: Which extension model gives Nara a credible path toward Bevy-level Rust
composition and Godot/Unity/Unreal-level editor, package, import, and build extensibility?

## Evidence Policy

This note prioritizes engine-owned documentation and source code. The local reference snapshots
are:

- Bevy commit `f6c6e6eebb94e81c090614f19039319e9acb3c85` (`repo-ref/bevy`).
- Godot commit `c939bf3791ce40ff70e0ee29f06486da1ebb6a84` (`repo-ref/godot`).

The LogLog Games article is deliberately treated as experienced user testimony, not as a
specification, benchmark, or proof of a technical property. The author makes the same limitation
explicit: it is a personal account, not a scientific evaluation or A/B study. [L1]

Negative claims are bounded to the inspected surface. For example, this note does not claim that
no Bevy editor project exists. It claims that Bevy's inspected first-party core contracts are
runtime/code composition contracts and that Bevy's own introduction still warns that important
features are missing. [B1][B8]

## Executive Finding

Nara should not make a distribution package and a runtime `Plugin` the same abstraction.

The strongest model across the mature engines is:

```text
Package
  -> declarative identity, version, compatibility, dependencies, provenance, and content
  -> zero or more typed contributions
       -> runtime plugin
       -> editor extension
       -> asset importer or processor
       -> inspector, gizmo, or authoring adapter
       -> schema or project-setting contribution
       -> build, cook, export, or deployment hook
       -> content, samples, documentation, and migration data
```

Bevy demonstrates an ergonomic Rust runtime composition seam, but its `Plugin` is an in-process
`App` mutation/lifecycle hook and its `PluginGroup` is an ordered set of plugin instances. Neither
is a package manifest, dependency lock, editor extension catalog, or trust boundary. [B1][B2]

Godot, Unity, and Unreal all separate at least two levels:

- a discoverable distribution or descriptor unit; and
- specialized runtime, editor, import, reflection, or build contributions.

Unity and Unreal make the separation especially explicit: one package/plugin can contain multiple
runtime and editor assemblies/modules. [U1][U2][U3][E1]

For Nara, the recommended initial execution model remains statically compiled Rust plus a generated
or otherwise preflighted contribution catalog. A universal native dynamic-plugin ABI should remain
deferred. Rust's native `"Rust"` ABI has no stability guarantee, and Bevy's dynamic-linking feature
is documented as an iterative compile optimization that should not be enabled for release builds,
not as a third-party ABI. [R1][B7]

## Terms That Must Stay Distinct

| Term | Responsibility | Must not imply |
|---|---|---|
| Distribution package | Discovery, acquisition, versioning, compatibility, provenance, license, content, and contribution declarations | A running `World`, an `App` mutation, or native authority |
| Cargo package | Rust source dependency and feature unit resolved by Cargo | The complete Nara product/package UX |
| Contribution | A declared capability supplied to a specific host and lifecycle | That all contributions share one Rust trait |
| Runtime plugin | Trusted in-process Rust code that configures one runtime candidate | Package installation, editor UI, importer execution, or a stable binary ABI |
| Editor extension | Commands, models, panels, inspectors, gizmos, menus, and authoring integrations owned by an editor host | Inclusion in a shipped game |
| Import contribution | Source recognition, options, versioning, dependencies, artifact production, and diagnostics | Runtime asset loading or arbitrary editor mutation |
| Build contribution | A declared, ordered, fallible build/cook/export step | Permission to mutate an already published runtime |
| Native extension ABI | A versioned binary protocol and loader contract | Ordinary Rust trait-object compatibility across compiler versions |

This vocabulary is not cosmetic. Package discovery must work before third-party code runs, while a
runtime plugin necessarily runs after code has been compiled and admitted. Combining them makes
safe preview, dependency solving, editor-only exclusion, recovery mode, and uninstall analysis
either impossible or dishonest.

## Cross-Engine Capability Matrix

| Capability | Bevy | Godot | Unity | Unreal | Implication for Nara |
|---|---|---|---|---|---|
| Runtime composition | `Plugin::build/ready/finish/cleanup` configures `App`; groups order, replace, enable, and disable plugins. [B1][B2] | GDExtension supplies staged native classes and services and is separate from editor addon discovery. [G5][G6] | Runtime code lives in package runtime assemblies; packages are not themselves `MonoBehaviour` instances. [U1][U2] | A plugin may contain Runtime modules with their own startup/shutdown and load policy. [E1][E2] | Keep `Plugin` narrow and in-process. |
| Distribution identity | Community projects are normally Cargo crates and are indexed by the Bevy Assets collection with explicit supported Bevy versions. [B8][B9] | `res://addons` discovery reads `plugin.cfg` metadata for editor addons. [G1] | Root `package.json` carries package identity, version, compatibility, dependencies, and user-facing metadata. [U1] | `.uplugin` is an automatically discovered JSON descriptor and may include code, content, and dependencies. [E1] | Add a product-facing package descriptor/catalog above runtime plugins. |
| Runtime/editor separation | No equivalent split is encoded by the core `Plugin` trait; separation is left to crates/features and application composition. [B1][B6] | `EditorPlugin` is editor-only and GDExtension has an explicit Editor initialization level. [G2][G6] | Recommended package layout has separate `Runtime` and `Editor` assemblies; `.asmdef` can constrain platforms. [U2][U3] | One `.uplugin` can contain a mixture of Runtime, Developer, Editor, and Program modules. [E1] | Host scope must be contribution metadata, not naming convention alone. |
| Asset import | `AssetLoader` is typed, async, extension-aware, and registered into `AssetServer`; asset processors are separate registrations. [B3][B4] | `EditorImportPlugin` recognizes extensions, exposes options/presets and format version, orders imports, and creates cached imported resources. [G3] | `ScriptedImporter` associates source extensions with importer code, serialized per-asset options, and `OnImportAsset`. [U4] | Interchange exposes customizable, ordered import pipelines; editor modules can register asset type actions through `IAssetTools`. [E5][E6] | Importers need a dedicated job/data contract, not a generic plugin callback. |
| Inspector/editor UX | Reflection and ECS type data make inspection possible, but the inspected core APIs do not define a package/editor contribution model. [B5][B6][B8] | `EditorInspectorPlugin` selects objects/properties and inserts or replaces property editors; `EditorPlugin` also exposes docks, menus, viewport input, gizmos, debugger, import, and export registration. [G2][G4] | `CustomEditor` maps an editor class to the object type it edits. [U5] | Editor modules register tools and asset actions; reflected metadata drives editor-visible types and properties. [E1][E4][E5] | Expose typed editor contribution points over stable schemas and commands. Do not give every package unrestricted editor internals by default. |
| Reflection/class registration | `#[derive(Reflect)]`, `TypeRegistry`, and type data support runtime metadata; ECS adds `ReflectComponent`. [B5][B6] | ClassDB/GDExtension registration covers classes, methods, properties, signals, and constants. [G5] | Serialized fields and editor attributes provide editor/import metadata. [U4][U5] | UHT and `UCLASS`/`USTRUCT`/`UFUNCTION`/`UPROPERTY` generate engine/editor metadata. [E4] | Treat generated Rust metadata as one provider of a stable Nara schema catalog, not as package identity. |
| Build/cook/export | Cargo/build scripts and application code own most build integration; core `Plugin` lifecycle is runtime startup, not a product build pipeline. [B1][R3] | `EditorPlugin::_build` can veto project run and it registers export plugins. [G2] | `IPreprocessBuildWithReport` is a dedicated pre-build callback contract. [U6] | UBT consumes `.Build.cs`; module type, platform, dependencies, and loading phase are declared separately from module code. [E1][E2][E3] | Build contributions need their own ordered, fallible host contract and diagnostics. |
| Native binary extension | Bevy normally compiles plugins as Rust dependencies; `bevy_dylib` optimizes development linking but warns against release use. [B7] | `.gdextension` selects platform libraries, requires an entry symbol and compatibility bounds, and uses staged initialize/deinitialize callbacks. [G5][G6][G7] | Managed code is split into package assemblies whose definitions control editor/runtime/platform compilation. [U2][U3] | Binary plugin modules are described by `.uplugin` and built/loaded through UBT/module rules. [E1][E2] | Do not equate `dyn Plugin` with a cross-build ABI. Admit a binary ABI only as a separate future extension technology. |
| Ecosystem UX | Official Bevy Assets is a community catalog; entries expose supported Bevy versions. Bevy warns users that important features are missing and recommends Godot for a large project today. [B8][B9] | Addons are visible and enable/disable in the editor; the inspected `plugin.cfg` path is simple metadata, not a general dependency solver. [G1] | Package Manager reads manifests, presents metadata, and resolves declared dependencies. [U1][U7] | Editor discovers, displays, enables, disables, packages, and distributes plugins. [E1] | Compatibility, provenance, install/update/remove preview, and diagnostics are product features, not README conventions. |

## Engine Findings

### Bevy: A Strong Runtime Seam, Not a Complete Package Model

Bevy's `Plugin` lifecycle is deliberately direct. `build` mutates `App` immediately; `ready`,
`finish`, and `cleanup` complete startup. Uniqueness defaults to a Rust type-derived name. [B1]

`PluginGroupBuilder` keeps plugin instances in a `TypeId` map and a separate order vector. It can
replace, reorder, enable, or disable entries and ultimately invokes their builds against an
`App`. This is useful prior art for an ordered runtime plan, but the keys are process-local Rust
types and the builder owns executable plugin instances. [B2]

Bevy's asset and reflection surfaces are separate, specialized extension contracts:

- `AssetLoader` has associated asset, settings, and error types, reads asynchronously, and declares
  recognized extensions. [B3]
- `AssetApp` separately registers loaders, processors, sources, asset types, and reflected asset
  access. [B4]
- `TypeRegistry` stores type registrations by Rust `TypeId` and type path, and derives can generate
  registrations plus type data. `App::register_type_data` can attach additional behavior metadata.
  [B5][B6]

This specialization is a strength. It also demonstrates why one universal `Plugin` callback is
not enough for package discovery, importer options, editor inspectors, artifact invalidation, or
build hooks.

Bevy's first-party introduction says the engine remains in development, important features are
missing, APIs change on an approximately three-month cadence, and users selecting an engine for a
large project should also consider Godot. Its official Assets page is a curated community catalog,
and entries advertise supported Bevy versions. [B8][B9]

The Nara lesson is to keep Bevy-like Rust ergonomics inside a stronger product boundary:

- preserve a small typed `Plugin` authoring surface;
- resolve identity, compatibility, slots, capabilities, and host scope before plugin construction;
- use stable Nara IDs rather than `TypeId` for package/editor/persistent contracts;
- make importer, editor, schema, and build contributions explicit; a host may construct their
  implementations only after the corresponding contribution was resolved, and a runtime plugin
  must not hide non-runtime contributions.

### Godot: Editor Extensions and Native Extensions Are Different Systems

Godot discovers editor addons recursively under `res://addons` by locating `plugin.cfg`. The
inspected settings code requires `name`, `author`, `version`, `description`, and `script`, displays
the result, and lets the user enable or disable it. This code path does not declare a semver
dependency graph. [G1]

`EditorPlugin` is an editor host gateway, not a runtime game plugin. It has editor state and layout
hooks, build/run hooks, menus, docks, viewport input and overlays, and explicit add/remove methods
for importers, exporters, inspectors, gizmos, debugger plugins, context menus, and other extension
types. [G2]

The child contracts are more precise:

- `EditorImportPlugin` declares a unique importer name, recognized extensions, output resource
  type, presets/options, import order, thread-safety, and a format version used for incompatible
  artifact changes. [G3]
- `EditorInspectorPlugin` selects supported objects and may insert or replace editors for specific
  properties, categories, groups, or the whole object. [G4]

GDExtension is a separate native boundary. Its configuration loader requires an entry symbol and
minimum compatibility version, optionally checks a maximum version, selects a platform library,
collects dependencies, and records whether editor reload is allowed. [G7]

Its ABI exposes Core, Servers, Scene, and Editor initialization levels with matching initialize and
deinitialize callbacks. The manager initializes upward, deinitializes downward, and reports that
some minimum levels require restart rather than live reload. The class-registration interface is
explicit about classes, methods, properties, signals, constants, and unregister operations.
[G5][G6]

The Nara lesson is not to copy Godot's object model or broad `EditorPlugin` surface. It is to copy
the separation of concerns:

- discoverable addon/package metadata;
- typed editor contributions;
- a distinct native ABI with explicit compatibility and lifecycle if Nara ever needs one;
- enable/disable and recovery behavior owned by the host, not improvised in plugin code.

### Unity: Package as a Multi-Assembly Product Unit

Unity Package Manager reads a root `package.json`. It carries a unique name, version, display
metadata, engine compatibility, and a map of package names to specific dependency versions. The
manager uses it for acquisition, loading, and UI presentation. [U1]

Unity's recommended custom package layout has separate Runtime and Editor directories and assembly
definitions, plus tests, samples, documentation, changelog, license, and third-party notices.
[U2]

Assembly definitions make that split executable rather than merely visual. They declare assembly
references and can include or exclude target platforms, add define constraints, and bind behavior
to dependency versions. Unity explicitly presents assembly boundaries as a way to control
dependencies and reduce unnecessary recompilation. [U3]

Specialized editor contracts remain separate from package identity:

- `ScriptedImporter` registers file extensions, serializes importer settings, and converts source
  files through the Asset Pipeline. [U4]
- `CustomEditor` identifies which object type an editor class can edit. [U5]
- `IPreprocessBuildWithReport` receives a callback before a build starts. [U6]

The Nara lesson is that a good package UX is not achieved by renaming runtime plugins to packages.
A package must be able to carry several separately scoped modules and non-code material, while the
editor can explain what will compile and what will ship.

### Unreal: Plugin Descriptor Above Multiple Typed Modules

Unreal describes a plugin as a collection of code and data that can be enabled per project. A
plugin can add runtime gameplay, engine features, file types, editor menus, commands, and modes.
The engine discovers `.uplugin` JSON descriptors automatically. [E1]

A plugin can contain any number of modules. The descriptor gives each module a name, type, and
loading phase. Runtime, Developer, Editor, and Program module types determine which application
kinds may load the module, and a single plugin may combine several types. Plugins may also contain
content and declare plugin dependencies. [E1]

Each code module has its own `.Build.cs`. Unreal Build Tool uses it to determine public/private
dependencies and compile environment, while the plugin/project descriptor controls module type,
platform compatibility, and loading phase. Module startup/shutdown code is a further lifecycle
layer. [E2][E3]

Reflection and asset tooling are again specialized:

- UHT plus `UCLASS`, `USTRUCT`, `UFUNCTION`, and `UPROPERTY` generate engine/editor-visible
  metadata. [E4]
- `IAssetTools` separately registers and unregisters asset type actions. [E5]
- Interchange defines a customizable, asynchronous import/export framework with ordered pipeline
  stacks, exposed options, translators, and factories. [E6]

The Nara lesson is to put a multi-contribution descriptor above code modules. Unreal also shows why
dependency direction matters: engine-level modules cannot depend on project-level modules, and
editor-only code should not leak into shipping runtime modules. [E1]

## Rust-Specific Constraints on the Nara Model

### 1. Cargo Is a Useful Solver, but Not the Whole Product Model

Cargo already resolves Rust packages, versions, targets, and features. `cargo metadata` emits
structured JSON containing workspace packages, enabled dependencies, targets, and the resolved
dependency graph; consumers are told to request a format version. Nara tooling should consume this
structured output rather than scrape `Cargo.toml` text or build a second Rust dependency solver.
[R2]

However, a Cargo package alone does not express all of the product concepts required here:

- editor-only versus runtime contribution intent in Nara terms;
- stable Nara contribution IDs and contract versions;
- importer formats/options/artifact versions;
- inspector/gizmo/authoring registrations;
- cook/export contribution ordering;
- Nara engine compatibility and capability requirements;
- content-only packages, samples, migrations, disclosure policy, and editor presentation.

Therefore Cargo package identity may initially map one-to-one to a Nara package for code-bearing
extensions, but the concepts should remain distinct. `Cargo.toml`/`Cargo.lock` remain authoritative
for the Rust code graph. A Nara descriptor or generated catalog describes Nara contributions and
must not independently resolve a contradictory Rust version graph.

The exact storage choice can remain open:

- `[package.metadata.nara]` inside `Cargo.toml`;
- a sidecar descriptor included by the Cargo package;
- generated registration metadata checked against a data-only descriptor; or
- a higher-level bundle referencing multiple Cargo packages and content roots.

The invariant matters now: there is one Rust dependency authority and one inspectable Nara
contribution view, with validation that prevents drift.

### 2. A Rust Trait Object Is Not a Stable Plugin ABI

The Rust Reference states that the native `"Rust"` ABI offers no stability guarantees. [R1]
Consequently, loading an independently compiled `Box<dyn Plugin>` across arbitrary compiler,
standard-library, feature, dependency, or engine versions is not a supportable default contract.

Bevy's `bevy_dylib` reinforces the distinction. It dynamically links Bevy to reduce incremental
compile time and warns users not to enable the feature for release builds because extra runtime
libraries would need to ship. It does not define independent plugin compatibility. [B7]

Nara should begin with:

1. source/Cargo integration;
2. a compiled contribution catalog;
3. fresh-process or fresh-runtime reconstruction for structural changes; and
4. optional, explicitly bounded function hot-patching only where compatibility is proven.

A future native extension ABI would be a separate product with C-compatible data layouts, versioned
function tables, ownership rules, panic/unwind rules, allocator rules, threading rules, target
matrices, capability mediation, reload restrictions, and conformance tests. Godot's GDExtension
surface demonstrates the size of that commitment. [G5][G6][G7]

### 3. Third-Party Rust Code Is Trusted Code Unless Isolated

Cargo compiles and executes `build.rs` before building a package, and a build script may perform
arbitrary tasks. The Rust Reference says procedural macros run during compilation with the
compiler's file and process resources and have the same security concerns as Cargo build scripts.
[R3][R4]

This has two consequences:

- A package manifest can disclose requested capabilities, provenance, build scripts, native code,
  and host targets, but it cannot honestly sandbox arbitrary source dependencies once Cargo builds
  them.
- Package preview and trust decisions must occur before dependency build/proc-macro execution.
  Sandboxed script or Wasm packages, if later admitted, need a different execution boundary rather
  than inheriting trust from Rust `Plugin`.

Signing, registry review, allowlists, reproducible source hashes, and isolated builders may improve
supply-chain policy later. They do not turn in-process native Rust into a permission sandbox.

### 4. Compile and Iteration Cost Are Product Requirements

Bevy itself provides optional dynamic linking for faster incremental builds and documents Cargo
feature selection as a way to reduce compile time and binary size. [B7][B10]

The LogLog Games article reports a production-oriented user's experience that proc-macro-heavy
reflection/serialization, incremental compile delays, lack of general hot reload, fragmented GUI
choices, and code-first tooling all reduced gameplay iteration. It argues that even body-only hot
replacement is valuable for UI, visual debugging, and tuning because it avoids reconstructing a
hard-to-reach game state. These are testimony and design pressure, not benchmark facts. [L1]

The appropriate response for Nara is not to promise universal Rust hot reload. It is to make every
extension contribution participate in explicit change classification:

- package content/asset/scene changes use data-domain reload where supported;
- package dependency, contribution, target, or host-scope changes rerun admission and may require
  rebuild and restart;
- importer version or option changes invalidate the affected artifact graph;
- compatible code-body changes may use an optional proven patch path;
- contribution topology, type layout, schema, Cargo feature, or dependency changes rebuild and
  start a fresh isolated runtime;
- editor workspace and validated document/checkpoint state survive only through declared
  restoration contracts.

Package architecture affects iteration directly. Runtime and editor crates/modules should be
separable so an editor-only change does not rebuild every shipping runtime domain, and import tools
should run out of the gameplay frame path.

### 5. Reflection Is Infrastructure, Not the User Experience

Bevy, Godot, Unity, and Unreal all use metadata or class registration to expose types to tools, but
none of them reduce mature editor extension to `reflect every field`:

- inspectors need selection policy, custom controls, multi-property editing, validation, and undo;
- importers need options, presets, dependencies, artifact versions, and concurrency rules;
- packages need stable identity, compatibility, docs, and lifecycle;
- builds need ordered hooks and failure semantics.

Nara's stable schema catalog should be the language between these domains. Rust derives or codegen
may populate it, but package and persistent identities cannot depend only on `TypeId`, Rust type
names, or proc-macro implementation details.

## Alternatives for Nara

| Model | Extensibility | Editor/package UX | Rust honesty | Long-term cost | Verdict |
|---|---:|---:|---:|---:|---|
| Every extension is a runtime `Plugin` | High for in-process ECS/App setup | Low: discovery, import, editor, build, and shipping scope become side effects | Medium | High coupling and hidden mutation | Reject as the universal model |
| Cargo crate equals runtime plugin | Ergonomic for Bevy-like code plugins | Low for content-only or multi-module packages | High for static compilation | Low initially, then forces exceptions | Allow as a simple case, not the conceptual model |
| Package descriptor with typed contributions; Rust plugins compile statically | High across runtime/editor/import/build | High: inspectable before execution and separable by host | High | Moderate catalog and validation work | Recommend |
| Universal dynamic Rust plugin ABI now | Superficially high | Medium if a loader is built | Low: native Rust ABI is unstable | Very high ABI, loader, safety, and compatibility burden | Defer until a concrete use case justifies a separate ABI |
| Script/Wasm package as the universal extension model | Potentially sandboxable and reloadable | Requires language bindings, debugger, editor, package, and host APIs | Medium only after a real sandbox exists | Very high before workflows are proven | Keep optional and adapter-owned |

## Recommended Nara Architecture Shape

### Plane A: Distribution and Admission

A data-only package view should be readable without constructing `App`, `World`, plugin instances,
native handles, editor panels, import workers, or build hooks. It should eventually expose:

- stable package ID and version;
- engine/API compatibility and target support;
- source and resolved provenance;
- license and third-party notices;
- dependencies plus conflicts at the appropriate authority;
- declared contribution IDs, kinds, host scopes, and contract versions;
- required Nara product/service capabilities;
- content, sample, documentation, and migration locations;
- native/build-script/proc-macro/trust disclosures;
- whether an update requires reimport, rebuild, editor restart, runtime restart, or migration.

Discovery and preview must produce immutable data before package code executes. Final Host
admission resolution remains pure but may consume a compiled contribution catalog after trusted
build scripts and proc macros have run; it still completes before code-bearing contributions gain
Host authority. Installation, update, enable/disable, and removal should all have a dry-run/preview
result suitable for CLI and editor presentation.

### Plane B: Typed Contribution Hosts

The resolved package can contribute zero or more entries to host-owned registries:

| Contribution kind | Owning host | Minimum contract |
|---|---|---|
| Runtime plugin factory | Runtime composition | Stable plugin/contribution ID, repeatable factory, requirements/conflicts, lifecycle |
| Editor module | Editor process | Commands/models/panels, host version, enable/disable, state save/restore, cleanup |
| Inspector/property adapter | Editor inspector | Stable schema/type/field predicates, edit commands, undo/redo and validation route |
| Gizmo/viewport tool | Editor viewport | Stable target predicates, draw packet/input command route, selection/undo policy |
| Asset importer/processor | Import host | Source formats, options schema, importer/artifact version, dependencies, budgets, cancellation, diagnostics |
| Schema/type provider | Schema catalog build | Stable IDs, versions, migrations, capabilities, disclosure policy |
| Project settings provider | Project tooling | Namespaced settings schema, defaults, validation, profile behavior |
| Build/cook/export step | Build host | Target predicate, declared inputs/outputs, ordering, cancellation, bounded diagnostics, rollback/cleanup honesty |
| Content/sample/docs | Package/content host | Mount/import policy, stable content identity, license, migration and uninstall references |

These contribution types need not share a base trait. They share package provenance and admission
metadata, while each host owns its own authority, lifecycle, error vocabulary, and test oracle.

### Compile-Time and Runtime Relationship

For the first production path:

1. Cargo resolves and locks code-bearing dependencies.
2. Nara tooling reads Cargo's structured metadata and data-only package declarations.
3. Build-time validation creates or verifies a compiled contribution catalog.
4. The editor/runtime/import/build host selects only contributions valid for its target and
   profile.
5. Runtime product composition resolves plugin factories before constructing a fresh candidate.
6. A plugin may install only the already resolved runtime contribution; it may not secretly add
   package, editor, importer, or build contributions from `build`.

This preserves the current Nara direction that product composition is pure before candidate
mutation and that committed runtime plugins are trusted, fallible in-process modules. It also
preserves Cargo as the Rust graph authority and structural-change reconstruction as the reliable
iteration fallback. It extends those directions instead of turning `Plugin` into a universal
service locator. [N1][N2][N3]

## Ergonomic Scenarios That Should Drive Interface Design

These scenarios are more useful than prematurely freezing exact Rust type names:

| ID | User workflow | Observable success |
|---|---|---|
| PX-01 | Add a runtime-only physics package from CLI or editor | Preview shows Cargo/source change, capabilities, targets, license, rebuild requirement, and resolved runtime plugin before mutation |
| PX-02 | Add a tilemap package containing runtime renderer, editor palette, importer, samples, and docs | One package operation installs several typed contributions; shipping build excludes editor/sample code |
| PX-03 | Disable only a package's editor tool while keeping its runtime format support | Editor contribution stops and cleans up; runtime composition and project data remain valid |
| PX-04 | Build a dedicated server using a package that also has renderer/editor modules | Resolver excludes those host scopes and proves no forbidden dependency/resource is installed |
| PX-05 | Register a custom inspector for a third-party component | Inspector targets stable schema IDs, emits normal edit commands, and participates in validation/undo without arbitrary `World` mutation |
| PX-06 | Change importer options or importer artifact version | Affected artifacts and dependents reimport; unrelated assets and the running game retain last-good values |
| PX-07 | Update a package across an incompatible schema version | Preview names required migrations/reimport/rebuild; failure leaves the old lock/catalog/runtime usable |
| PX-08 | Remove a package still referenced by scenes or settings | Dry run reports stable reference paths and blocks destructive removal unless an explicit migration/removal plan resolves them |
| PX-09 | A package's editor module fails or terminates the editor during enable | Recoverable failure records bounded diagnostics and cleanup; a startup journal lets the next process enter recovery mode without claiming cleanup after an abort |
| PX-10 | A runtime plugin factory metadata differs from its package declaration | Admission fails before native authority or `App` construction; no hidden contribution appears |
| PX-11 | Edit only a compatible gameplay function body | Optional patch path may retain the world at a quiescent boundary; unsupported changes clearly fall back to rebuild/restart |
| PX-12 | Package contains a Cargo build script, proc macro, native library, or network-fetching tool | Install preview discloses trust implications before any package code executes |
| PX-13 | Author a package entirely in Rust without editor UI | A small manifest plus one runtime plugin remains concise; product machinery does not burden the simple path |
| PX-14 | Author a content-only package | It has stable identity, dependency/provenance/license and import/mount behavior without inventing an empty Rust plugin |

The ergonomic target should be one package operation and one coherent preview, not one universal
callback. Advanced authors see separate, typed APIs only when they add those contribution kinds.

## Decisions Worth Recording Now

These are high-cost boundaries with enough cross-engine evidence to record before implementation:

1. A distribution package is not a runtime plugin. A package contains typed contributions, of
   which a runtime plugin is one kind.
2. Package discovery and preview are data-only before package code executes. Final resolution is
   also pure, but may use compiled evidence and completes before mutating a candidate
   runtime/editor/import/build host.
3. Runtime, editor, importer, schema, and build contributions have separate owners and lifecycle
   contracts. Editor-only code is excludable from shipped runtime/server products.
4. Cargo remains authoritative for the Rust source dependency graph and lock. Nara tooling consumes
   structured Cargo metadata and does not create a second contradictory Rust solver.
5. The initial Rust extension path is statically compiled/source integrated. A native dynamic ABI,
   if ever added, is a separate versioned technology and not `dyn Plugin` across libraries.
6. Stable package/contribution/schema IDs and contract versions are independent of Rust `TypeId`,
   type names, crate paths, and native handles.
7. Native Rust packages are trusted code. Capability/permission fields are admission disclosures
   unless execution is actually isolated.
8. Install/update/remove/enable operations produce inspectable plans, structured diagnostics, and
   explicit restart/rebuild/reimport/migration effects.
9. Simple Rust plugin authoring remains a first-class ergonomic path; the product model should add
   depth behind it rather than ceremony in every game.

## Decisions That Should Remain Open

The evidence does not justify freezing these yet:

- the exact package manifest filename or whether initial metadata lives in `Cargo.toml`;
- registry/index protocol, marketplace governance, monetization, ratings, or discovery ranking;
- package signing, transparency log, reproducible builds, and remote builder policy;
- exact semver and compatibility promises before Nara has a public release cadence;
- binary artifact distribution and compiler/standard-library compatibility matrix;
- a native extension ABI, Wasm ABI, universal behavior host, or default scripting VM;
- editor UI toolkit APIs for panels and property controls;
- whether content-only packages use the same resolver as code packages;
- package-level feature selection beyond demonstrated product scenarios;
- cross-package schema migration orchestration beyond the first real upgrade case.

These can be added without changing the foundational package-to-contributions relationship.

## Research Gaps

- Unity and Unreal are documented here from public first-party documentation, not local source
  snapshots. Exact implementation behavior can vary by engine version.
- Unreal's marketplace ingestion, signing, and binary compatibility policy were not investigated;
  they are not needed to answer the package-versus-plugin boundary question.
- Godot Asset Library dependency/version policy was not deeply investigated. The local editor addon
  discovery path is sufficient to establish that `plugin.cfg` and `EditorPlugin` are separate from
  GDExtension, but not to recommend a resolver design.
- Bevy community editor projects were not compared. The question here is what the first-party core
  contracts guarantee, not what an external editor prototype might add.
- Compile-time claims from the LogLog article were not independently benchmarked. Nara should set
  its own P50/P95 edit-to-feedback budgets with the reference game.

## Sources

### Bevy Primary Sources

- **[B1]** `repo-ref/bevy/crates/bevy_app/src/plugin.rs` - `Plugin` purpose and lifecycle.
- **[B2]** `repo-ref/bevy/crates/bevy_app/src/plugin_group.rs` - ordered `PluginGroupBuilder`,
  replacement, enable/disable, and final `App` mutation.
- **[B3]** `repo-ref/bevy/crates/bevy_asset/src/loader.rs` - typed async `AssetLoader` contract.
- **[B4]** `repo-ref/bevy/crates/bevy_asset/src/lib.rs` - `AssetApp` registrations for loaders,
  processors, sources, asset types, and reflection.
- **[B5]** `repo-ref/bevy/crates/bevy_reflect/src/type_registry.rs` - type registrations, type paths,
  dependencies, and type data.
- **[B6]** `repo-ref/bevy/crates/bevy_app/src/app.rs` and
  `repo-ref/bevy/crates/bevy_ecs/src/reflect/` - app/ECS reflection registration.
- **[B7]** `repo-ref/bevy/crates/bevy_dylib/src/lib.rs` and
  `repo-ref/bevy/Cargo.toml` - development dynamic-linking intent and release warning.
- **[B8]** [Bevy Quick Start introduction](https://bevy.org/learn/quick-start/introduction/) -
  first-party maturity warning.
- **[B9]** [Bevy Assets](https://bevy.org/assets/) and `repo-ref/bevy/README.md` - community catalog
  and third-party plugin/resource discovery.
- **[B10]** `repo-ref/bevy/docs/cargo_features.md` - compiled feature selection and compile-size
  motivation.

### Godot Primary Sources

- **[G1]** `repo-ref/godot/editor/plugins/editor_plugin_settings.cpp` - `res://addons` discovery,
  required `plugin.cfg` fields, presentation, and enable/disable flow.
- **[G2]** `repo-ref/godot/editor/plugins/editor_plugin.h` and
  `repo-ref/godot/doc/classes/EditorPlugin.xml` - editor lifecycle and specialized registrations.
- **[G3]** `repo-ref/godot/doc/classes/EditorImportPlugin.xml` and
  `repo-ref/godot/editor/import/editor_import_plugin.h` - importer options, versions, order, and
  threading.
- **[G4]** `repo-ref/godot/doc/classes/EditorInspectorPlugin.xml` and
  `repo-ref/godot/editor/inspector/editor_inspector.h` - custom inspector/property extension.
- **[G5]** `repo-ref/godot/core/extension/gdextension.h` and
  `repo-ref/godot/core/extension/gdextension_interface.json` - native class registration and staged
  initialization interface.
- **[G6]** `repo-ref/godot/core/extension/gdextension_manager.cpp` - staged load/unload/reload and
  restart requirements.
- **[G7]** `repo-ref/godot/core/extension/gdextension_library_loader.cpp` - entry symbol,
  compatibility bounds, platform library selection, dependencies, and reloadability.

### Unity Primary Sources

- **[U1]** [Unity 6 package manifest](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-manifestPkg.html).
- **[U2]** [Unity 6 custom package layout](https://docs.unity3d.com/6000.0/Documentation/Manual/cus-layout.html).
- **[U3]** [Unity 6 assembly definitions](https://docs.unity3d.com/6000.0/Documentation/Manual/assembly-definition-files.html)
  and [assembly definition file format](https://docs.unity3d.com/6000.0/Documentation/Manual/assembly-definition-file-format.html).
- **[U4]** [Unity 6 `ScriptedImporter`](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AssetImporters.ScriptedImporter.html).
- **[U5]** [Unity 6 `CustomEditor`](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/CustomEditor.html).
- **[U6]** [Unity 6 `IPreprocessBuildWithReport`](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/Build.IPreprocessBuildWithReport.html).
- **[U7]** [Unity 6 package dependencies](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-dependencies.html).

### Unreal Primary Sources

- **[E1]** [Plugins in Unreal Engine](https://dev.epicgames.com/documentation/en-us/unreal-engine/plugins-in-unreal-engine) -
  descriptor discovery, content, dependencies, multi-module type, loading, and distribution.
- **[E2]** [Unreal Engine modules](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-modules) -
  module structure, `.Build.cs`, dependencies, and startup/shutdown.
- **[E3]** [Unreal module properties](https://dev.epicgames.com/documentation/en-us/unreal-engine/module-properties-in-unreal-engine) -
  `ModuleRules` and public/private dependency policy.
- **[E4]** [Unreal reflection system](https://dev.epicgames.com/documentation/en-us/unreal-engine/reflection-system-in-unreal-engine)
  and [Unreal Header Tool](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-header-tool-for-unreal-engine).
- **[E5]** [Unreal `IAssetTools`](https://dev.epicgames.com/documentation/en-us/unreal-engine/API/Developer/AssetTools/IAssetTools) -
  asset type action registration and removal.
- **[E6]** [Unreal Interchange import framework](https://dev.epicgames.com/documentation/en-us/unreal-engine/importing-assets-using-interchange-in-unreal-engine) -
  extensible asynchronous import/export pipelines, options, translators, and factories.

### Rust Primary Sources

- **[R1]** [Rust Reference: external block ABI](https://doc.rust-lang.org/reference/items/external-blocks.html#abi) -
  native `"Rust"` ABI stability statement.
- **[R2]** [`cargo metadata`](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html) -
  structured package and resolved dependency graph.
- **[R3]** [Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html) -
  compilation and execution of package build scripts.
- **[R4]** [Rust Reference: procedural macros](https://doc.rust-lang.org/reference/procedural-macros.html) -
  compile-time resources and security concerns.

### Nara Context Sources

- **[N1]** `docs/architecture/runtime-composition-interface-design.md` - pure product resolution,
  repeatable runtime plugin factories, immutable resolved plans, and fresh candidate publication.
- **[N2]** `docs/architecture/adr/0020-project-layout-and-package-format.md` - Cargo manifest and lock
  authority for the Rust source graph; deployment/package publication remains separately deferred.
- **[N3]** `docs/architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md` -
  complete Rust path, explicit edit classification, structural rebuild, optional patching, and no
  universal plugin ABI.

### Experience Report

- **[L1]** [Leaving Rust gamedev after 3 years](https://loglog.games/blog/leaving-rust-gamedev/),
  LogLog Games, 2024-04-26. Used only as clearly labeled practitioner testimony about iteration,
  reflection/proc macros, hot reload, GUI/tooling, and ecosystem maturity.

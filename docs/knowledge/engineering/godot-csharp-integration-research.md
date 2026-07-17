---
type: "Engineering Research"
title: "Godot C# Integration Implications for Nara"
description: "Primary-source review of Godot's .NET product stack and the boundaries it suggests for a future, optional Nara C# gameplay Adapter."
timestamp: 2026-07-17T16:30:00Z
tags: ["nara", "godot", "csharp", "dotnet", "coreclr", "schema", "editor", "export"]
status: "complete"
producer_id: "godot-csharp-primary-source-research"
source_revision: "repo-ref/godot@c939bf3791ce40ff70e0ee29f06486da1ebb6a84"
authority: "non-normative research evidence"
---

# Finding

## Executive conclusion

Godot demonstrates that mature C# support is a complete product stack, not a `hostfxr` call plus an
FFI layer. Its implementation includes CoreCLR hosting, assembly resolution and unload attempts,
native/managed lifetime coordination, source generators and analyzers, a custom MSBuild SDK,
editor build and diagnostic UX, Inspector metadata, debugger handoff, and target-specific publish
and export logic.

The evidence supports Nara's current direction rather than requiring an immediate foundation
redesign:

- stable Schema identity must remain independent from CLR paths, type names, member names, metadata
  tokens, and GC handles;
- a failed C# build or load must not disturb the last-good executable Player runtime;
- a fresh isolated Player runtime generation should be the correctness baseline for C# iteration;
  child-process replacement is the strongest reclamation baseline to compare, not a selected
  Editor/Player topology;
- collectible `AssemblyLoadContext` reload should be a later, optional latency optimization;
- ordinary gameplay assemblies should not need to execute inside the long-lived Editor process for
  Schema or Inspector support;
- managed tasks, delegates, threads, roots, handles, and callbacks require explicit ownership by a
  managed-module and Player-runtime generation;
- managed publish output is a target-specific executable artifact graph, not an asset folder, Rust
  `ExecutableGeneration`, or ADR 0088 runtime-content catalog.

This note is research evidence only. It does not accept an ADR, modify an active plan, authorize a
CoreCLR/Roslyn dependency, select a public C# API, or admit production Adapter work. OQ-007's
existing research and Trial ladder remains the authority.

## Scope and evidence snapshot

The source review used the local Godot tree at
`repo-ref/godot@c939bf3791ce40ff70e0ee29f06486da1ebb6a84`. The `modules/mono` subtree contains 404
files at that revision. The review followed primary source through these product paths:

- native `hostfxr` bootstrap and managed callback registration;
- project `AssemblyLoadContext` loading, unloading, and script-state reconstruction;
- native `Object` to managed `GodotObject` ownership and GC-handle behavior;
- source-generated script discovery, properties, methods, signals, defaults, and reload state;
- editor build, diagnostic, reload-watch, Inspector, Play, and debugger integration;
- `Godot.NET.Sdk`, `dotnet publish`, RID selection, NativeAOT recognition, and platform packaging.

Nara implications were checked against Accepted ADR 0081 and ADR 0093, Accepted Play/service/debug
boundaries in ADR 0034, ADR 0042, and ADR 0076, Proposed ADR 0084/0086/0088, and OQ-007 as present
in the working tree on 2026-07-17. A Proposed ADR is candidate evidence, not authority.

Claims below use three labels:

- **Godot fact**: behavior directly supported by the referenced source.
- **Nara implication**: an inference that preserves current Nara authority.
- **Trial candidate**: a mechanism or question that remains unauthorized until OQ-007 admits it.

## Godot architecture map

Godot's implementation is easier to understand as several lifetimes, not one “C# runtime”:

```text
Godot Editor process
  +-- native Editor and one process-lifetime CoreCLR
  +-- main ALC: GodotPlugins, GodotSharp, GodotSharpEditor, tools
  +-- project ALC: gameplay assembly, collectible only in the Editor
  +-- external dotnet build/publish processes
  +-- generated metadata used by Script/Inspector bridges

Click Play
  +-- launch a separate Godot Player process
      +-- its own CoreCLR/runtime state
      +-- non-collectible project gameplay assembly

Export
  +-- dotnet publish for target RID and architecture
  +-- publish closure: assemblies, runtime files, native libraries, symbols
  +-- platform packaging: desktop, Android, or iOS-specific handling
```

**Godot fact.** The Editor initializes `hostfxr` using `GodotPlugins.runtimeconfig.json`, obtains
`load_assembly_and_get_function_pointer`, and registers an unmanaged entry point from
`GodotPlugins.Main`; the exported Player uses a separate self-contained initialization route
(`repo-ref/godot/modules/mono/mono_gd/gd_mono.cpp:365-490`). Closing the hostfxr context at lines
381 and 419 does not represent unloading CoreCLR.

**Godot fact.** Project assemblies are collectible only when `_editorHint` is true, while the tools
assembly is explicitly loaded non-collectible
(`repo-ref/godot/modules/mono/glue/GodotSharp/GodotPlugins/Main.cs:90-118,139-176`). Player-side
script reload returns immediately outside the Editor
(`repo-ref/godot/modules/mono/csharp_script.cpp:625-635`).

**Godot fact.** Play launches one or more engine child processes with `create_instance`; Stop kills
the tracked process IDs (`repo-ref/godot/editor/run/editor_run.cpp:51-71,161-189,215-230`). A game
view may be presented by the Editor, but its reliable runtime boundary is still a separate process.

**Nara implication.** Nara must keep three identities distinct:

1. **CoreCLR process lifetime**: the process in which CoreCLR and the stable managed bridge live.
2. **Managed module generation**: one validated assembly/dependency/Schema artifact candidate.
3. **Player runtime generation**: one isolated Play instance and its ECS/service state, whether a
   future Host places it in-process or in a child process.

Conflating them would make “reload”, “restart”, “last-good”, and “stopped” impossible to state
accurately.

## Host, load, unload, and reload

### Stable bridge and project load context

**Godot fact.** Godot keeps API assemblies in a main ALC and gives the project ALC an
`AssemblyDependencyResolver`. Shared API assembly names resolve through the main ALC, while project
DLL/PDB files load from streams to avoid locking build output
(`repo-ref/godot/modules/mono/glue/GodotSharp/GodotPlugins/PluginLoadContext.cs:9-23,46-81`).

**Nara implication.** If a future Trial hosts managed code in-process, a small stable bridge
assembly and a replaceable gameplay assembly must not be loaded as independent copies of the same
contract types. The bridge, ABI handshake, and module loader belong to the Adapter, not to ordinary
gameplay `Plugin` APIs.

### ALC unload is cooperative and fallible

**Godot fact.** Avoiding one accidental JIT-held reference requires a non-inlined wrapper and a
tracking `WeakReference` (`repo-ref/godot/modules/mono/glue/GodotSharp/GodotPlugins/Main.cs:15-72`).
Unload then repeatedly forces full GC and waits for finalizers. It warns after 200 ms and returns
failure after 1000 ms, naming strong GC handles and running threads as possible causes
(`Main.cs:232-282`).

**Godot fact.** `CustomGCHandle` implements an apparent strong handle as a weak `GCHandle` plus a
strong-reference table partitioned by ALC. The ALC unloading event clears that table
(`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/CustomGCHandle.cs:12-74`). This is
infrastructure required merely to make unload possible, not gameplay functionality.

**Godot fact.** After repeated unload/load failures, Godot stops retrying the changed assembly and
asks the user to restart the Editor (`repo-ref/godot/modules/mono/mono_gd/gd_mono.cpp:788-831`).

**Nara implication.** An ALC that does not retire is not a successful reload. The owning managed
Host must expose a terminal `RestartRequired` or equivalent result, retain diagnostics, and stop
claiming old roots have been reclaimed. Process replacement is the strongest reclamation proof,
but whether the Editor normally uses a child Player remains an OQ-039 product/placement decision.

### Godot reload is a state-migration procedure

**Godot fact.** Reload first serializes collectible delegates and instance state, releases delegate
handles, removes live scripts and clears script metadata, and only then unloads and loads the project
assembly (`repo-ref/godot/modules/mono/csharp_script.cpp:666-802`). It subsequently reconstructs
script bridges and instances, deserializes delegates and state, and refreshes Inspector and Signal
UI (`repo-ref/godot/modules/mono/csharp_script.cpp:840-1028`).

**Godot fact.** If unload or load fails after old instances have been removed, Godot creates
placeholder instances and restores Variant properties into them
(`repo-ref/godot/modules/mono/csharp_script.cpp:801-830`). That preserves authoring data better than
dropping it, but it is not an atomic last-good runtime publication protocol.

**Nara implication.** Nara should not copy this “dismantle old, then try new” order for Play. A
managed module candidate should be built, hashed, validated, Schema-checked, bridge-compatible, and
started in an unpublished Player candidate before one infallible promotion. Candidate failure must
leave the active Player untouched. This is consistent with ADR 0093 and the candidate/publication
shape under Proposed ADR 0084; it does not accept ADR 0084 by reference.

**Trial candidate.** Compatible in-place managed edits may later be tested at a Nara-owned quiescent
boundary. They must remain an optimization over the fresh-process path, not its correctness basis.

## Native-managed object lifetime and async work

### Godot's object bridge is specific to its Object model

**Godot fact.** Native-to-managed lookup first checks a C# script instance, then an ordinary native
instance binding, and may recreate a collected wrapper
(`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/NativeInterop/InteropUtils.cs:11-47`).
`RefCounted` values use weak handles while other objects use strong handles during binding
(`InteropUtils.cs:50-87`). Native refcount changes can replace weak and strong handles
(`repo-ref/godot/modules/mono/csharp_script.cpp:1251-1325,1847-1906`).

**Godot fact.** `GodotObject` owns a native pointer and its finalizer can enter native disposal,
including defensive checks for a handle replaced by another thread
(`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/GodotObject.base.cs:83-152`).

**Nara implication.** This machinery solves Godot's `Object + ScriptInstance + RefCounted` model;
it is not a template for an ECS engine. A first Nara C# Trial should avoid:

- one managed wrapper or GC root per ECS entity;
- managed object references as entity identity or ownership;
- managed finalizers mutating `World`, despawning entities, or closing native services;
- exposing Rust pointers or Bevy `Entity` values beyond a bounded call.

A better candidate is an Adapter-private Behavior store. It owns Behavior instances for one Player
generation and gives callbacks generation-stamped entity handles, tick-scoped value snapshots,
batched queries, commands, and semantic services. ECS remains the runtime authority.

### Roots, callbacks, exceptions, and tasks are lifecycle participants

**Godot fact.** Godot provides a custom `SynchronizationContext` and `TaskScheduler`; posted work is
queued and activated from the engine frame callback
(`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/GodotSynchronizationContext.cs:8-58`,
`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/GodotTaskScheduler.cs:13-115`, and
`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/Bridge/ScriptManagerBridge.cs:79-89`).
That provides main-thread dispatch but does not encode a Nara Player generation, Stop obligation, or
stale-callback rejection.

**Godot fact.** Managed bridge entry points catch exceptions and forward managed stack information
to Godot's script debugger rather than allowing exceptions to cross the unmanaged boundary
(`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/NativeInterop/ExceptionUtils.cs:50-132`).

**Nara implication.** Every managed task, continuation, delegate, event subscription, thread, GC
root, callback table, and native handle must be attributable to both a managed-module generation and
a Player-runtime generation. Stop must revoke dispatch, cancel tracked work, reject late callbacks,
and report incomplete retirement. Exceptions must become structured callback outcomes with an
explicit policy for already-recorded commands; “log and continue” is not sufficient transaction
semantics.

## Source generation, Schema, and Inspector

### What Godot generates

**Godot fact.** Godot source generators emit script paths, an assembly-level script type list,
cached `StringName` values, property getter/setter bridges, Inspector property lists, default-value
code, method and signal metadata, and state save/restore methods. Representative sources are:

- `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptPathAttributeGenerator.cs:38-71,97-110,152-170`;
- `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptPropertiesGenerator.cs:134-173,181-297`;
- `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptMethodsGenerator.cs:150-224`;
- `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptSignalsGenerator.cs:184-233`;
- `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptSerializationGenerator.cs:113-135,164-235`.

The generated assembly type list is the common discovery path, avoiding an unconditional
`Assembly.GetTypes()` scan (`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/Bridge/ScriptManagerBridge.cs:300-350`).

**Nara implication.** An Adapter-owned source generator/analyzer suite is likely necessary for good
ergonomics and AOT-friendly glue. Its durable product should be explicit Schema/catalog data and
direct binding tables, not a promise that every Rust API or CLR member is automatically exported.
This is a future `Nara.NET.Sdk` role, not a current core requirement.

### Godot identity is unsuitable as Nara persistent identity

**Godot fact.** `ScriptPathAttributeGenerator` derives `res://` identity from the source path and
requires a top-level class whose name matches the file name (`ScriptPathAttributeGenerator.cs:38-58,97-110`).
At load time, Godot builds path-to-CLR-type and script-pointer-to-type maps
(`ScriptManagerBridge.cs:300-311,420-462`). Unpathed reload fallback records assembly simple name
plus CLR full type name, and constructed generics receive `csharp://` virtual paths
(`ScriptManagerBridge.cs:22-42,546-578,647-683`).

**Nara implication.** These values are valid generation-local bindings, not rename-safe persistent
identity. Nara's Accepted ADR 0081 remains the authority: Behavior types, fields, and any future
persistent endpoints require explicit stable IDs, tombstones, versions, and migrations. The mapping
may be `(module generation, stable Behavior ID) -> CLR Type`; the CLR type cannot be the ID.

### Persistence, reload retention, and defaults must stay separate

**Godot fact.** Reload serialization includes all compatible mutable instance fields and properties
except ignored members, not only Inspector-exported state, and keys them by generated member names
(`ScriptSerializationGenerator.cs:113-135,164-235`). It also serializes signal delegates through
`GodotSerializationInfo` (`repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/Bridge/GodotSerializationInfo.cs:33-64`).

**Nara implication.** Nara should preserve three different classes:

- Schema-backed persistent authoring state;
- runtime-private transient managed state;
- explicitly opted-in, stable-ID reload-retained state.

Ordinary CLR field compatibility must not silently opt a value into persistence or reload migration.

**Godot fact.** Some default expressions are emitted into runtime `GetGodotPropertyDefaultValues`
code, while unsupported cases can fall back to CLR default
(`repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptPropertyDefValGenerator.cs:210-224,352-402,431-454`).
Godot scenes can omit a property whose value equals the current default
(`repo-ref/godot/scene/resources/packed_scene.cpp:1053-1068`).

**Nara implication.** A Nara default must be validated data in the Schema artifact, not execution of
a gameplay constructor, static initializer, or arbitrary expression inside the Editor. Persistent
records must distinguish “missing, inherit default” from “present and explicitly authored”, even
when the explicit value currently equals the default.

### Inspector and unavailable modules

**Godot fact.** Generated property metadata drives the language-neutral Inspector property-list
path. The C# editor plugin additionally warns when source modification time is newer than the last
known build (`repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Inspector/InspectorPlugin.cs:28-57`
and `InspectorOutOfSyncWarning.cs:23-28`). On reload failure, Godot placeholders preserve Variant
property values by name.

**Nara implication.** The Editor should consume the last-good, runtime-independent Schema Catalog
without loading or executing ordinary gameplay assemblies in its main process. A Roslyn/source-
generation worker or build artifact can produce the Catalog. Missing or broken assemblies should
enter an explicit Adapter-unavailable state while ADR 0090-style bounded semantic records remain
round-trippable; unknown fields must not be deleted merely because the current assembly cannot
describe them.

Persistent C# signal/callback endpoints should be deferred in the first Trial. If later admitted,
they need stable endpoint IDs, signatures, versions, tombstones, and migration rather than CLR
method or signal names.

## Editor build, debug, and iteration

**Godot fact.** Godot starts external `dotnet build`/`publish` processes, controls restore and
incrementality, installs a custom logger, optionally writes a binlog, and localizes CLI output
(`repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Build/BuildSystem.cs:18-87,150-200,202-300`).
`BuildManager` separately tracks in-progress state, start/finish events, issues, and a last-valid
DLL timestamp (`BuildManager.cs:12-39,74-165`).

**Godot fact.** Play runs a synchronous Debug build unless an external IDE says it already built
the project (`BuildManager.cs:375-384`). A successful manual build asks running games to reload and
then reloads the Editor assembly (`repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Build/MSBuildPanel.cs:74-112`).
The watcher polls every 0.5 seconds and on window focus, using assembly modification state
(`repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/HotReloadAssemblyWatcher.cs:14-55`).

**Nara implication.** Roslyn diagnostics, MSBuild graph evaluation, restore, compilation, artifact
validation, module activation, Player activation, and Inspector freshness are separate states. An
mtime or successful compiler exit cannot represent all of them. Each attempt should carry source
snapshot, toolchain/target fingerprint, attempt ID, structured diagnostics, output manifest, and
activation result.

**Godot fact.** Managed exceptions are bridged into Godot diagnostics, while an external IDE can
request Debug Play and supply debugger-agent connection data
(`repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Ides/MessagingServer.cs:335-379`).

**Nara implication.** Build diagnostics, managed debugger integration, managed profiling/EventPipe,
and Nara's gameplay observation/timeline are four related but distinct product capabilities.
CoreCLR hosting supplies none of them automatically. The first Trial may use standard external .NET
debugger tooling while Nara owns generation identity, process launch, diagnostics, and stable ECS
observation.

## MSBuild SDK, export, and platform artifacts

### A custom SDK is a product boundary

**Godot fact.** Generated projects use a versioned `Godot.NET.Sdk`, target `net8.0` by default,
conditionally target `net9.0` for Android, and enable dynamic loading
(`repo-ref/godot/modules/mono/editor/GodotTools/GodotTools.ProjectEditor/ProjectGenerator.cs:13-41`).
The SDK defines Debug/ExportDebug/ExportRelease, moves outputs under `.godot/mono/temp`, injects
platform constants, and imports platform rules
(`repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.NET.Sdk/Sdk/Sdk.props:4-31,46-107`).
It implicitly references Godot APIs and source generators and rejects incompatible binding families
(`repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.NET.Sdk/Sdk/Sdk.targets:26-52`).

**Trial candidate.** A versioned `Nara.NET.Sdk` would be an appropriate owner for the selected TFM,
first-party managed API package, generator/analyzers, target profiles, output layout, and Host/SDK/
Schema compatibility fingerprint. MSBuild/NuGet should remain the managed dependency authority;
Cargo remains the Rust graph authority. Nara should not invent a merged resolver.

### Managed export is a graph, not one DLL

**Godot fact.** Export maps platform and architecture to a RID, runs `dotnet publish` as
self-contained, and validates either the project assembly or a NativeAOT native library
(`repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Build/BuildSystem.cs:202-263` and
`repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Export/ExportPlugin.cs:249-307`). It walks
the complete publish tree, packages managed/native members, records SHA-512 entries, and performs
Android/iOS-specific handling including `lipo` and XCFramework generation (`ExportPlugin.cs:313-456`).

**Nara implication.** A future immutable managed artifact generation should account for at least:

- gameplay assemblies and generated bootstrap/bridge identity;
- PDB or other symbol artifacts and their disclosure policy;
- `.deps.json`, `.runtimeconfig.json`, resolved NuGet graph, and first-party SDK fingerprint;
- target TFM, RID, architecture, runtime kind, JIT/AOT and trimming policy;
- runtime/workload packs and native libraries;
- generated Schema/catalog fingerprint, member hashes, provenance, and compatibility requirements.

This graph is trusted executable code. It must not gain execution authority by appearing in an asset
mount. It is also not ADR 0086's Rust/Cargo `ExecutableGeneration`, and ADR 0088 currently owns
runtime content rather than a managed-code catalog. A future product package may bind three separate
digests: Rust or stock Host executable, managed module generation, and runtime content catalog.
Exact types and ownership remain Trial decisions.

**Trial candidate.** Start with one desktop, framework/runtime, and debug/release profile. NativeAOT,
mobile workloads, Web, trimming guarantees, and a broad RID matrix should remain deferred. Godot's
platform-specific code demonstrates why “supports CoreCLR” is not equivalent to “exports C# to all
targets”.

## Adopt, adapt, avoid, defer

| Classification | Godot precedent | Nara treatment |
|---|---|---|
| Adopt | Thin ordinary `.csproj` plus versioned engine SDK | Keep MSBuild/NuGet authoritative; evaluate `Nara.NET.Sdk` only in the Trial. |
| Adopt | Build-before-Play and source-aware structured diagnostics | Make build, validation, activation, and freshness separately observable. |
| Adopt | Generated script catalog and analyzer feedback | Generate stable Schema artifacts and direct bindings; avoid unconditional runtime scanning. |
| Adopt | Stable bridge separated from replaceable user code | Keep bridge ABI and loader Adapter-private and explicitly versioned. |
| Adapt | Separate Player process for Play/Stop | Use it as the strongest correctness/reclamation counterfactual; keep ordinary Editor/Player placement open under OQ-039. |
| Adapt | Collectible project ALC | Treat as an optional latency optimization with timeout, poison, and process-restart fallback. |
| Adapt | Placeholder authoring when code is unavailable | Preserve complete bounded stable-ID records under ADR 0090 semantics, not name-only Variants. |
| Adapt | Generated property/default metadata | Use explicit stable IDs and data defaults; never execute gameplay code to discover editor truth. |
| Adapt | Main-thread `Task` dispatch | Add module/runtime generation, cancellation, safe-point, late-result, and finite-Stop semantics. |
| Adapt | Publish tree and per-member hash manifest | Add provenance, Host authorization, compatibility, atomic publication, and last-good activation. |
| Avoid | Script path, CLR full name, or member name as durable identity | Use ADR 0081 stable IDs, aliases, versions, tombstones, and migrations. |
| Avoid | One managed wrapper with native ownership per engine object | Keep ECS entities authoritative; use values, batches, commands, and generation-stamped handles. |
| Avoid | Finalizers directly disposing gameplay/native world state | Finalizers may at most enqueue an idempotent Adapter-owned release notice. |
| Avoid | Dismantling the old runtime before validating the new assembly | Construct and start an unpublished candidate; promote only after all fallible checks. |
| Avoid | Timestamp as build or last-good identity | Use immutable artifact manifests and explicit attempt/generation state. |
| Avoid | Gameplay assembly execution in the long-lived Editor for Inspector | Read generated Catalog data or use a restartable isolated design worker. |
| Defer | C# Editor extensions | Evaluate as a separate higher-trust Tool Host capability, not an implicit gameplay permission. |
| Defer | NativeAOT/mobile/Web/trimming platform matrix | Admit target by target after clean-machine export and runtime evidence. |
| Defer | Persistent managed signals/callbacks | Require stable endpoint identity and migration after a concrete workflow exists. |
| Defer | Universal Behavior Host or managed ECS | Keep the first facade Adapter-local and compare it against the named gameplay workflow. |

## What Nara should change now, and what it should not

### Preserve or clarify now

No code, Accepted ADR, or active-plan change is authorized by this research. The current foundation
work should continue to preserve these language-independent properties:

1. ADR 0081's runtime-independent stable Schema Catalog and binding separation.
2. ADR 0034/0093's isolated Play, fresh reconstruction, explicit restoration, and last-good path.
3. ADR 0042's stable intent plus service-owned native handles and queues.
4. ADR 0076's generation-stamped safe-point control and stable observation identities.
5. Proposed ADR 0084's candidate/publication questions without treating that proposal as Accepted.

At the next permitted OQ-007 documentation review, the research suggests clarifying three candidate
distinctions already implicit in the question:

- a fresh isolated Player runtime generation is the Trial correctness baseline; process replacement
  is the strongest reclamation counterfactual and ALC reload is a reversible optimization;
- ordinary gameplay assemblies do not execute in the main Editor by default;
- managed module artifacts are executable-code artifacts distinct from both Rust executable and
  runtime-content generations.

### Do not change now

Do not add a CoreCLR/Roslyn dependency, C# crate, root feature, project-manifest field, managed ABI,
public `Nara.NET.Sdk`, universal Behavior API, dynamic non-Rust ECS storage, managed package layout,
or platform promise. Do not weaken Rust as the complete production authoring path. Do not reshape
the active reference-game work around a future Adapter before OQ-007's named baselines and separate
Trial admission exist.

## Future Trial questions and stop conditions

The bounded feasibility tracer and later product Trial should answer, rather than pre-decide:

1. Can a stock prebuilt Host and a custom Cargo-built Host activate the same managed module format?
2. Can the Editor inspect and round-trip C# fields from a last-good stable Catalog without loading
   ordinary gameplay code into its process?
3. Which explicit attributes or version-controlled sidecar owns Behavior, field, and attachment IDs?
4. What ABI handshake binds Host build, managed bridge, SDK/generator, Schema Catalog, capabilities,
   module generation, and Player generation?
5. What exact member graph constitutes a build candidate, debug Play candidate, and export result?
6. Can failed build/load/Schema/start attempts leave the running last-good Player fully untouched?
7. Can Stop enumerate and retire every instance, root, delegate, event, task, thread, and callback;
   how is incomplete retirement reported and reclaimed?
8. What state is persistent, runtime-private, or reload-retained, and how are stable-ID migrations
   validated before activation?
9. Can batch/query/command/service APIs make common gameplay direct to write while staying bounded
   and avoiding one interop call per property or one wrapper per entity?
10. What exception policy governs partially recorded commands and runtime faulting?
11. Do external debugger and source diagnostics correlate reliably with module and Player generation?
12. Does a clean machine export one supported desktop target from locked, attributable inputs?

The tracer or Trial should stop, remain deferred, or fall back to the Rust path if any predeclared
condition is met:

- reliable Play requires successful collectible-ALC unload and has no bounded fresh-runtime or
  process-replacement fallback when the old managed generation cannot retire;
- a failed candidate destroys or mutates the last-good Player;
- stale tasks, callbacks, handles, or roots can enter another Player generation;
- Inspector functionality requires arbitrary gameplay execution in the main Editor;
- stable authoring data cannot survive missing/renamed assemblies, types, or fields losslessly;
- acceptable ergonomics require unrestricted mutable `World`, Rust pointers, or per-entity wrappers;
- bridge/GC/frame cost, startup latency, package size, or clean export exceeds the Trial budget;
- the same named gameplay task shows no material workflow improvement over the Rust baseline;
- maintenance or reversibility cost exceeds the precommitted OQ-007 budget.

Passing a hostfxr smoke test is not adoption evidence. Production work still requires OQ-007's
end-to-end authoring comparison, first-playable and Host baselines, clean export, failure injection,
performance measurements, and a separately admitted Trial plan.

## Primary source index

| Concern | Primary source and symbol/lines |
|---|---|
| hostfxr initialization | `repo-ref/godot/modules/mono/mono_gd/gd_mono.cpp:365-490`, `initialize_hostfxr_for_config`, `initialize_hostfxr_self_contained`, `initialize_hostfxr_and_godot_plugins` |
| project ALC load/unload | `repo-ref/godot/modules/mono/glue/GodotSharp/GodotPlugins/Main.cs:15-72,90-176,201-290`, `PluginLoadContextWrapper`, `LoadProjectAssembly`, `UnloadPlugin` |
| dependency resolution | `repo-ref/godot/modules/mono/glue/GodotSharp/GodotPlugins/PluginLoadContext.cs:9-81`, `Load`, `LoadUnmanagedDll` |
| reload failure policy | `repo-ref/godot/modules/mono/mono_gd/gd_mono.cpp:788-831`, `reload_failure`, `reload_project_assemblies` |
| script-state reload | `repo-ref/godot/modules/mono/csharp_script.cpp:625-830,840-1028`, `CSharpLanguage::reload_assemblies` |
| GC roots and ALC | `repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/CustomGCHandle.cs:12-95`, `CustomGCHandle` |
| native/managed lookup | `repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/NativeInterop/InteropUtils.cs:11-87`, `UnmanagedGetManaged`, `TieManagedToUnmanaged` |
| managed disposal | `repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/GodotObject.base.cs:83-152`, `GetPtr`, finalizer, `Dispose` |
| async frame dispatch | `repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/GodotTaskScheduler.cs:13-115`, `Activate`; `ScriptManagerBridge.cs:79-89`, `FrameCallback` |
| script path/type identity | `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptPathAttributeGenerator.cs:38-71,97-110,152-170`; `ScriptManagerBridge.cs:22-42,300-350,420-462,546-578,647-683` |
| generated Inspector data | `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptPropertiesGenerator.cs:134-173,181-297` |
| generated reload state | `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptSerializationGenerator.cs:113-135,164-235` |
| build and publish CLI | `repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Build/BuildSystem.cs:18-87,150-300` |
| Play build callback | `repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Build/BuildManager.cs:375-384`, `EditorBuildCallback` |
| Editor reload watcher | `repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/HotReloadAssemblyWatcher.cs:14-55` |
| Inspector staleness | `repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Inspector/InspectorPlugin.cs:28-57` |
| child Player process | `repo-ref/godot/editor/run/editor_run.cpp:51-71,161-189,215-230`, `EditorRun::run`, `EditorRun::stop` |
| generated C# project | `repo-ref/godot/modules/mono/editor/GodotTools/GodotTools.ProjectEditor/ProjectGenerator.cs:13-41`, `GenGameProject` |
| custom MSBuild SDK | `repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.NET.Sdk/Sdk/Sdk.props:4-107`; `Sdk.targets:26-52` |
| target export graph | `repo-ref/godot/modules/mono/editor/GodotTools/GodotTools/Export/ExportPlugin.cs:170-245,249-456`, `_ExportBeginImpl` |
| engine API binding generation | `repo-ref/godot/modules/mono/editor/bindings_generator.cpp:1747-2132,3886-4255`, `generate_cs_core_project`, `generate_cs_editor_project`, ClassDB traversal |

## Final assessment

Godot validates the product opportunity but also exposes the permanent cost. Nara can plausibly offer
a Godot/Unity-like C# gameplay experience without abandoning its Rust-native engine or stable ECS
foundation, provided it uses Schema as the cross-language semantic authority and treats process,
module, and runtime generations as separate lifecycles.

The most important design choice is therefore not “which hostfxr API should Nara call?” It is that
the first reliable C# path must be **build immutable managed candidate -> validate -> start a fresh
isolated Player runtime generation -> atomically select it**, while the Editor remains healthy and
the previous Player remains last-good. A child process is the strongest reclamation design to
compare, not a topology selected by this note. Godot's ALC machinery is valuable evidence for what
a later optimization costs; it should not define Nara's baseline correctness contract.

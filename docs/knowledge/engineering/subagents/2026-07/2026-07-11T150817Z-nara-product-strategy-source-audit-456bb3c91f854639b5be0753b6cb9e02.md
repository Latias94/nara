---
type: "Subagent Finding"
title: "Nara product strategy source audit"
description: "Cross-checked Godot, Unity, Bevy dynamic schema, optional scripting hosts, market positioning, and Nara's Rust-first modular product direction."
timestamp: 2026-07-11T15:08:17Z
record_id: "456bb3c91f854639b5be0753b6cb9e02"
tags: ["nara", "strategy", "godot", "unity", "bevy", "schema", "scripting"]
status: "complete"
producer_id: "codex-root"
run_id: "session-019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
git_branch: "refactor/engine-foundation-contracts"
verified_by: "seven-read-only-subagents"
---

# Finding

Seven read-only research lanes audited Godot, Unity, Bevy, scripting-host alternatives, Nara's
current source, and the engine market. The evidence supports a Rust-first integrated product built
on `bevy_ecs`, with a coherent editor and delivery workflow, explicit module boundaries, stable
persistent identities, and optional scripting adapters. It does not establish that Nara needs a
second official author language, dynamic non-Rust ECS components, or a universal Behavior Host.

## Status Classification

### Implemented foundations worth preserving

- Stable component type IDs, schema versions, field metadata, value codecs, and migrations exist.
- Scene, prefab, and patch documents are independent from runtime `Entity` values.
- Edit documents and isolated Play state have distinct authority; Stop discards by default and
  Apply Changes emits a patch.
- Scripts are already prohibited by ADR from receiving unrestricted mutable `World` access.
- Backend-neutral intent and service boundaries exist for assets, rendering, tasks, and tooling.

### Current product targets, not current capabilities

- Public Rust APIs cover a complete game production path.
- Editor, scene, asset, data, debug, and export workflows support mixed-discipline teams.
- First-party modules compose into one coherent product while retaining documented reuse and
  replacement boundaries.
- `EngineHost -> ProjectContext -> RuntimeInstance -> App -> World` is the ownership hierarchy.
- A Brotato-like open-source reference game ships downloadable Windows and Linux binaries.
- Optional scripting or hot-patch packages can improve selected workflows without becoming core
  dependencies.

### Direct contract conflicts

1. Public ECS types are appropriate for Rust gameplay, but persistent formats, project documents,
   services, and editor state still need Nara-owned contracts rather than runtime `Entity` or
   backend handles.
2. Rust iteration needs separate asset/data reload, compatible function patch, rebuild/restart, and
   optional script-adapter paths; one `hot reload` promise would hide materially different safety
   contracts.
3. The foundation and ADR 0015 place the visual editor after early runtime work. Product strategy
   still requires editor workflows to grow from reference-game tasks rather than remain a detached
   late-stage shell.
4. Project, tooling, task, and wgpu state currently enter the App World as ECS resources. The
   ownership hierarchy requires explicit migration and cannot be declared implemented merely
   because ADR 0042 describes service boundaries.
5. `ComponentRegistry` still mixes persistent schema identity with process-local Rust/Bevy binding
   metadata; those concerns need separation even in a Rust-only game.

## Godot Source Audit

The local Godot source is `4.8.0-dev`. Godot unifies an object interaction surface:

```text
StringName properties / methods / signals + Variant values + PropertyInfo metadata
```

Native and extension classes register through `ClassDB`; script languages implement
`ScriptInstance`; `Object::get_property_list` combines native, extension, script, and metadata
properties for Inspector and serialization consumers. GDScript `@export` emits property metadata,
and `Resource` supplies the serializable shared-data-object experience.

Godot does **not** provide one language-neutral type authority with stable field IDs, declared
versions, and project-authored migration chains. Script global classes use a registry separate from
`ClassDB`. Text resources instantiate a class and set string-named properties. Script and
GDExtension hot reload preserve stored state primarily by matching property names. Native class
aliases and one-off upgrade tools exist, but general project field rename and semantic migration do
not.

Therefore Nara should copy Godot's coherent Inspector/property experience and extension discipline,
but should not copy string field names as durable identity. Godot's script-instance boundary is
useful prior art for an optional adapter, not a requirement for Nara's core runtime.

## Unity Source Audit

The relevant comparison is Unity's integrated authoring experience combined with DOTS
Authoring -> Baking -> Runtime separation, not a Rust reimplementation of `MonoBehaviour` or a
requirement to adopt C#-style managed hosting.

Nara should copy:

- inspectable Rust components/systems and reusable ScriptableObject-like project data assets;
- a generic schema-driven Inspector integrated with Undo, dirty state, prefab overrides, and
  multi-edit;
- one serialized edit transaction route for Inspector, automation, AI, and Apply Changes;
- explicit authoring-to-runtime projection with dependency tracking and incremental rebuild;
- observable data reload, compatible hot-patch, rebuild/restart, and explicit state-restoration
  modes.

Nara should not copy:

- implicit method-name lifecycle messages on every object;
- managed wrappers and native objects with separate lifetimes and pseudo-null behavior;
- string property paths, serialization that bypasses setters, and `OnValidate` as invariant repair;
- host-local managed reference IDs as durable identity;
- domain/scene reload as the primary correctness mechanism;
- authoring objects that remain the runtime authority.

Unity Entities adds a relevant projection rule: Bakers are stateless, their order is not a contract,
dependencies are recorded, and prior generated output is removed before incremental rebaking.
Nara needs an equivalent Authoring Projection Contract if edit documents are to project into one or
more runtime instances reliably.

## Contingent Dynamic Schema Research on Bevy ECS

Bevy supports dynamic components without a Rust `TypeId`, runtime query access by `ComponentId`,
and scheduler access construction for dynamic queries. IDs are World-local, there is no public
component unregister contract, and raw dynamic insertion requires unsafe layout/drop correctness.

If a concrete scripting adapter or data-authoring workflow later requires non-Rust components with
true ECS presence, the strongest researched implementation candidate is a two-level lowering:

1. Give each stable project `ComponentTypeId` a distinct World-local dynamic `ComponentId`.
2. Use one Nara-private, fixed-layout `RuntimeSchemaRecord` representation for all project-defined
   dynamic components rather than synthesizing a different raw Rust memory layout per Schema.
3. Keep the existing `ComponentValue` family as the persistent and migration boundary; the runtime
   record may use generation-stamped slots or owned storage optimized for repeated access.
4. Keep engine components and measured hotspots as native Rust `Component` types.
5. Allow optional AOT code generation only as an explicit optimization that rebuilds an isolated
   runtime; never maintain dynamic and native copies as simultaneous authorities.

This preserves per-Schema archetype presence, change ticks, scheduler conflicts, and dynamic
filters without creating a second ECS. A single `Map<SchemaId, Value>` component is simpler but
collapses access conflicts and change tracking. A sidecar column store would need to recreate
despawn, querying, conflict detection, and change semantics. Per-Schema custom raw layouts make
schema reload and destructor safety unnecessarily dangerous.

This is a contingent implementation option, not a product requirement. Rust-defined components and
ordinary persistent project data remain the baseline; Nara should not build dynamic ECS lowering
until a real adapter or game workflow proves that a map/resource representation is insufficient.

## Optional Scripting-Host Research

If Nara implements an optional scripting adapter, that adapter must make the following semantics
explicit before Nara extracts a shared Rust trait or host abstraction:

1. program, behavior type, instance, generation, runtime, and stable entity identity;
2. prepare/instantiate/activate/suspend/deactivate/destroy lifecycle and exactly-once rules;
3. fixed, virtual/game, real, and render-interpolation time participation;
4. bounded snapshots or dynamic queries plus atomic validated command batches;
5. persistent, authoritative, VM-private, cached, asynchronous, external, and debug state classes;
6. preflight, quiescence, export, migrate, instantiate, publish, retire, and rollback reload steps;
7. call/instance/program/runtime fault scopes and structured diagnostics;
8. source, memory, instruction, query, command, recursion, callback, and in-flight task budgets;
9. generation-stamped asynchronous requests and declared main-thread integration;
10. source maps, bounded locals, stack data, breakpoints, and distinct statement/tick stepping;
11. host-provided time, RNG, ordering, external outcomes, and explicit determinism limits;
12. versioned package manifests, dependency locks, capabilities, hashes, source maps, and trust
    profiles.

Adapter findings:

- **Luau:** strongest candidate for an optional first-party gameplay adapter, but it should be built
  only for a real project workflow and remains experimental until bindings, diagnostics, debugging,
  reload, GC latency, determinism boundaries, and target packaging are measured.
- **C#/.NET:** a capable trusted-gameplay ecosystem option with substantial unload, GC, FFI,
  dynamic-loading, editor, and Native AOT costs. It is not a sandbox boundary and is not justified
  as a second official language now.
- **Wasm Component:** appropriate for untrusted mods, server extensions, and portable packages; it
  is not an authoring language or a universal plugin ABI.
- **Rhai:** potentially useful for a small embedded workflow or editor automation, but an adapter
  should not be built merely as an abstraction conformance test.

## Market Pressure Test

Nara competes most directly for Rust game developers considering Bevy, Fyrox, or a custom stack,
while its integrated product quality will also be compared with Godot, Unity, and Unreal. It does
not need an exclusive technical category. The useful product hypothesis is:

> Public Rust APIs, visual tools, modular engine domains, headless execution, debugging, and
> delivery work as one production path instead of becoming per-game infrastructure.

That claim remains unproven. The following phrases are not currently evidence-backed product
claims: `Unity-level productivity`, `production-ready`, `general-purpose`, `seamless hot reload`,
`deterministic`, `AI-native`, and `future-facing`.

The reference game can prove one vertical path only if its game layer uses public product APIs,
contains no engine-private escape hatches, measures Rust edit classes and fallback paths, exercises
the editor where relevant, integrates modules through documented contracts, and exports standalone
binaries. Because the engine author will build it, it cannot prove external learnability or
independent team adoption.

# Evidence

- The checkpoint's author/document authority matches ADR 0015 and ADR 0034.
- Current ECS default exposure is visible in `src/lib.rs:378-408` and is explicitly permitted by
  ADR 0002 and ADR 0044.
- Current `App` owns a `World` and schedules, while Play tooling creates a bare `World` rather than
  a complete runtime App.
- `EditorWorkspace`, `ProjectAssetDatabase`, effective project settings, task pools, and wgpu
  backend objects currently have ECS Resource paths.
- Current component registration is Rust-generic and component-only; its persistent schema identity
  still needs separation from process-local Rust/Bevy binding metadata.
- Godot source confirms a common property-list surface and name-based persistence/reload, not a
  stable-ID migration system.
- Unity official documentation confirms ScriptableObject shared data, serialized edit transactions,
  configurable Play reload, and DOTS baking separation.
- Bevy source confirms dynamic descriptors and queries but World-local IDs and unsafe raw layout
  responsibilities.

# Recommendation

1. Prove the complete public Rust path with the Brotato-like reference game before expanding
   language or package-system scope.
2. Keep stable schema identity and migrations focused on concrete persistent consumers. Implement
   dynamic non-Rust ECS lowering only if an adapter or game workflow requires true dynamic
   component presence and scheduling.
3. Add an Authoring Projection Contract before moving project/workspace ownership out of `World`.
4. Build scripting adapters, including a possible `nara_luau`, only from a real workflow; keep VM,
   reload, and private state semantics in the adapter until multiple consumers prove a shared host
   contract.
5. Measure Rust iteration classes and preserve a reliable incremental rebuild, last-good runtime,
   restart, and state-restoration baseline before prototyping native function patching.
6. Keep current productivity, platform, and generality claims hypothesis-labelled until the
   reference game supplies measured evidence.

# Disposition

- This shard records source-backed facts and contingent implementation options.
- Current approved direction lives in `STRATEGY.md`, the product-strategy checkpoint, and the cited
  decisions/ADRs; this research does not independently authorize implementation.
- Dynamic schema and scripting-host sections are retained as reference material for concrete future
  adapters, not as baseline engine commitments.

# Citations

## Nara

- [ADR 0002](../../../../architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md)
- [ADR 0015](../../../../architecture/adr/0015-editor-tooling-and-dogfooding-boundary.md)
- [ADR 0021](../../../../architecture/adr/0021-scripting-and-wasm-boundary.md)
- [ADR 0034](../../../../architecture/adr/0034-editor-play-mode-world-boundary.md)
- [ADR 0042](../../../../architecture/adr/0042-runtime-service-and-backend-boundary.md)
- [ADR 0044](../../../../architecture/adr/0044-root-facade-and-prelude-layering-policy.md)
- [ADR 0045](../../../../architecture/adr/0045-component-schema-capability-metadata.md)
- [`ComponentRegistry`](../../../../../crates/nara_reflect/src/registry.rs)
- [`ComponentSchema`](../../../../../crates/nara_reflect/src/schema.rs)
- [`ComponentValue`](../../../../../crates/nara_reflect/src/value.rs)
- [`ScenePlaySession`](../../../../../crates/nara_tooling/src/play.rs)

## Godot and Bevy source

- [Godot `ClassDB`](../../../../../repo-ref/godot/core/object/class_db.h)
- [Godot object property dispatch](../../../../../repo-ref/godot/core/object/object.cpp)
- [Godot script instance](../../../../../repo-ref/godot/core/object/script_instance.h)
- [Godot GDScript implementation](../../../../../repo-ref/godot/modules/gdscript/gdscript.cpp)
- [Godot text resource format](../../../../../repo-ref/godot/scene/resources/resource_format_text.cpp)
- [Bevy dynamic component descriptors](../../../../../repo-ref/bevy/crates/bevy_ecs/src/component/info.rs)
- [Bevy dynamic query builder](../../../../../repo-ref/bevy/crates/bevy_ecs/src/query/builder.rs)
- [Bevy dynamic system builder](../../../../../repo-ref/bevy/crates/bevy_ecs/src/system/builder.rs)

## External primary sources

- [Unity MonoBehaviour](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/MonoBehaviour.html)
- [Unity ScriptableObject](https://docs.unity3d.com/6000.0/Documentation/Manual/class-ScriptableObject.html)
- [Unity SerializedObject](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SerializedObject.html)
- [Unity configurable Enter Play Mode](https://docs.unity3d.com/6000.0/Documentation/Manual/configurable-enter-play-mode.html)
- [Unity Entities baking overview](https://docs.unity3d.com/Packages/com.unity.entities@1.4/manual/baking-overview.html)
- [Luau type system](https://luau.org/types/)
- [Luau sandbox](https://luau.org/sandbox/)
- [.NET native hosting](https://learn.microsoft.com/dotnet/core/tutorials/netcore-hosting)
- [.NET AssemblyLoadContext unloadability](https://learn.microsoft.com/dotnet/standard/assembly/unloadability)
- [WebAssembly Component Model](https://component-model.bytecodealliance.org/design/why-component-model.html)
- [Fyrox 1.0](https://fyrox.rs/blog/post/fyrox-game-engine-1-0-0/)
- [Bevy 0.19](https://bevy.org/news/bevy-0-19/)

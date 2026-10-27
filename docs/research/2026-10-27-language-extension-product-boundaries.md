# Language And Extension Product Boundaries

**Date:** 2026-10-27. **Status:** Research and recommendations, not a language selection or ADR acceptance.
**Nara basis:** `65402b9206b26555787e4da5b7f7abd1a0c048fa`, branch
`refactor/scene-data-representation`.

## Evidence Basis

Read the supplied local architecture-selection conversation export dated 2026-10-23, lines
4138-4762, in bounded chunks 4138-4437 and 4438-4762. The new user question starts at 4138;
the C#-first answer starts at 4144. Its language ranking, illustrative APIs and milestone are
assistant recommendations, not a subsequent user adoption decision. This local input has no
public permalink. Findings describe the Nara basis, not concurrent drafts. That baseline is
local and unpushed: Nara references use repository-local relative file links, not purported
public GitHub permalinks. Those links resolve the local files; the explicit commit above
identifies the inspected revision rather than making the links immutable snapshots.

| Primary source | Actual pin or observation |
| --- | --- |
| Nara strategy and contracts | [Strategy][N1], Accepted [ADR 0093][N2], Proposed [ADR 0120][N3] and [ADR 0121][N4] at the Nara basis above. |
| Existing behavior experiments | Representation, Schema-driven binding and code-lifetime [reports][T1], [fixture source][T2] and [retained-route source][T3] at that same basis; historical execution is attributed, not rerun. |
| Read-only `repo-ref/godot` | `81ab961f91066a63bcbf3ec340962bb1a1ada558`; [version.py][G0] says **4.8 beta**, not a stable release. |
| Read-only `repo-ref/bevy` | `85503941e488152711a4695bbe248e9ed9f453fc`; [manifest][B0] says **0.20.0-dev**. Nara separately pins published `bevy_ecs = "=0.20.0"`; this checkout is comparison evidence, not an identity claim about that dependency. |
| .NET | Official [native-hosting design][D1], **v10.0.0**, resolved to `60629d14374c56f1cb51819049ad1fa529307f8d`; current Microsoft Learn pages checked online on this date. |
| Trial dependencies | [Manifest][T4]: **mlua 0.12.2**, Lua 5.4 features, **netcorehost 0.22.0**, `net10_0`, defaults disabled. Prior reports record SDK 10.0.400/runtime and hostfxr 10.0.11; no fresh toolchain qualification here. |
| Optional Wasm comparison | Wasmtime **v50.0.0**, resolved to `ebe5e8df1f4e12c5fc2f5954926881d8d61367d8`; inspected [Component Model source][W1]. Not a Nara dependency selection. |

## Findings

### 1. Optional Official C# Is Compatible With Strategy, But Is Not Yet Selected

The update proposes Rust-complete native development, a first official reloadable C# SDK, and
optional Lua/Wasm. **An optional official C# SDK need not contradict Rust-first Strategy**:
Strategy rejects requiring another official author language, not every optional first-party
SDK. The actual decision gap is that ADR 0093 explicitly selects no second official language
or production SDK yet. Formal support would need an explicit product decision and corresponding
contract/support updates; neither the conversation recommendation nor passing trials supplies
that decision. [N1], [N2]

The bigger unresolved change is product breadth: gameplay callbacks become components,
Inspector extensions, docking tools, commands, import/build integrations, diagnostics and
export. Calling C# official before qualifying that workflow would promise capabilities the
existing experiments do not expose. Conversely, requiring C# for any essential Rust production
workflow would conflict with the complete no-VM baseline. Keep *implementation language*,
*execution Host*, *available domain capabilities* and *support tier* separate. [N2], [T1], [T5], [T6]

### 2. Today's Native Tool Flow Is Real, But Not Language-Reloadable

`EditorNativeComposition::register_panel<P: ImguiEditorPanel>` admits a trusted **source-built**
descriptor/factory pair. The panel receives `&dear_imgui_rs::Ui` and a finite context, not a
language-neutral UI ABI. Ordinary project Open does not load executable panel code. The existing
read/prepare/consuming-commit/result ports are useful foundations; they are not a managed panel,
command-discovery or custom-Inspector installation service. [N5], [N6]

Preserve the stronger existing semantics when adding a language adapter: field drafts use
document/entity/component/field identity, while their basis separately includes source revision,
catalog, component/document versions, shape and path. The panel submission returns an exact
result after application drive. A managed object's identity, panel repaint or new global
generation must not replace those source-authority checks. This is a boundary requirement,
not a proposal for UI widgets or layout; those belong to the separate UI investigation. [N6], [N7]

The update's broader native-SDK freedom is also a **target**, not evidence that every native
extension seam already exists. ADR 0121 explicitly identifies the missing independent two-pass
render contribution and remains Proposed. A C# binding cannot create that missing underlying
capability, and a render contribution must not acquire submit/present authority. [N4]

### 3. Schema Identity Does Not Establish Public Runtime Storage Or Native Access

The current language fixture reads an external catalog and registers language callbacks, but
imports a private representation harness and hidden compiler/reference helpers. Its callable
domain is native `Name`; it is not a public dynamic-component owner. The data-only envelope
contains multiple independently addressed Schema records, not one new native Rust type per
record. Native registration still rejects duplicate Rust-type bindings. [T2], [T7], [N8]

Consequently, the update's Rust system accessing C#-defined `EnemyStats` needs a concrete public
contract. It cannot silently mean `Query<&EnemyStats>` for a Rust type absent at compilation.
An owner-provided Schema/field query or bounded data view could provide that cooperation without
duplicating authoritative values. Editing persisted source and scheduled runtime mutation must
remain distinct owner operations; neither should silently synchronize an arbitrary C# object
graph with ECS storage. [T2], [T7], [N3]

The claim that storage must initially sit outside native archetypes is an option, not a Bevy
restriction. Pinned Bevy demonstrates dynamic descriptors, per-ID insertion and queries, with
explicit layout, drop-function and thread-safety obligations. That does not manufacture Rust
generic types or provide persistence/migration automatically. Compare descriptors only if the
same tool/game workflow needs independent per-Schema scheduler access; retain envelope/rows as
alternatives, not simultaneous production families. [B1], [B2], [T1], [N3]

### 4. A Registration Scope Is Necessary Bookkeeping, Not An Unload Proof

`PluginInstanceId + Generation + RegistrationScope` is a useful ownership sketch. Existing
Rust `Plugin` build/finish/shutdown and static composition are not a reversible registration
transaction. A scope must connect to actual domain owners rather than trying to undo arbitrary
`App`/`World` mutations or constructor side effects. [N9], [N10]

ADR 0120 already distinguishes native Runtime, Schema/catalog and execution publications.
Implementing just a plugin-wide counter would lose that distinction. Replacement also needs
the affected dependency/read-set closure: unchanged tools with cached field/query plans can
depend on a changed Schema. The trials check a complete ID/version binding list, but do not
qualify dependent owners, schema migration, running async work or Stop-first replacement. [N3], [T3], [T6]

The update's seven reload steps must distinguish **reversible admission quiesce**, **routing
publication**, and **physical retirement**. Keep old-active only for a valid coexist-and-switch
case; Stop-first needs compatible last-good restart data/recipe and visible interruption.
After publication, cleanup failure retains custody rather than rolling back external effects.
Queued panel results, event delegates, background work, import callbacks, cached plans, UI
factories and native drop glue must either retain their exact code owner or be safely disposed
before reclamation. Rejecting a late write is not evidence that its callback has stopped. [N3], [N6]

### 5. .NET Hosting Supports The Candidate, Not Universal Reload Or Deployment

Official `nethost`/`hostfxr` APIs support native hosting but specify framework-dependent
deployment. The pinned helper's returned component entry pointer is process-lifetime; closing
a host context does not unload that component or the CLR. A permanent bridge plus separately
collectible authored assemblies is therefore the relevant architecture, not repeatedly loading
authored entry DLLs through that helper. [D1], [D2]

`AssemblyLoadContext.Unload()` only starts cooperative unloading; executing stacks and external
strong roots/GCHandles can prevent completion. Pinned Godot shares selected API assemblies,
loads managed files from streams, enables collectible project assemblies under the Editor hint
and observes weak-reference liveness. Its C# language explicitly uses hard reload. These are
engineered state/recreation mechanisms, not proof of atomic replacement or preservation of
arbitrary managed state. Nara's synchronous one-owner trial must not be promoted to those
broader guarantees. [D3], [G1], [G2], [G3], [T6]

Two product obligations remain especially material: one process cannot host multiple CLR
versions, and ALC is not a security boundary. A prebuilt Editor therefore needs a supported
runtime/contract policy across installed tools/projects, explicit trusted-code activation and
an isolation/restart alternative. NativeAOT disallows dynamic assembly loading and runtime code
generation; editor reload and shipped-player code-update support are separate promises. Godot's
currently served stable C# manual still labels mobile support experimental and excludes Godot 4
C# web export; those are Godot-specific limits, not Nara's platform matrix. [D4], [D5], [G4]

### 6. Versioned C ABI Is A Candidate Distribution Seam, Not The Product's Universal API

The Rust ABI has no stability guarantee. A separately distributed native binary needs an
explicit interoperability contract; versioned C function tables, opaque handles and owned
buffers are a credible alternative to ordinary Rust trait objects. But a version number alone
does not specify allocation/destruction, error/unwind policy, thread affinity, callback custody,
capability negotiation or dependency compatibility. Keep normal public native Rust calls direct.
Do not turn this recommendation into an obligatory bridge for ECS, UI or rendering. [R1], [N1], [N2]

Godot provides separate low-level C interface metadata and exposed engine API metadata, plus
extension compatibility/platform-library declarations. Its pinned reload sequence can close
the predecessor before a new load fails; it does not establish general last-good rollback.
The `reloadable` flag also depends on binding support. Even `abi_stable` 0.11.3 explicitly excludes
unloading. Loading, disabling and physically unloading must be separate advertised capabilities.
[G5], [G6], [G7], [R2]

There is no demonstrated requirement here to implement a universal ABI/package kernel before
one concrete managed adapter. A product package may group Rust, managed code and assets, but
must leave source dependency resolution to Cargo/NuGet and execution permission to the Host.
Accepted native plugin composition already distinguishes future installed contributions from
static Rust `Plugin`; it need not be weakened or repurposed as their universal lifecycle. [N1], [N10]

### External Precedents Do Not Select A Language

The update's lifecycle distinctions check out, but do not prove C# is uniquely necessary:

- Godot's official plugin example supplies C# and GDScript, requires building the C# plugin,
  and uses explicit initialization/cleanup hooks. Editor extension breadth comes from its
  `EditorPlugin` APIs, not from the language runtime alone. [G8]
- Unity's versioned manuals require reconstructing non-serializable `VisualElement` trees after
  reload, while loaded native plugins require an Editor restart. Its experience is not universal
  native unload. [U1], [U2]
- Unreal's Live Coding documentation requires repairing retained pointers/caches after object
  reinstancing and excludes consoles/mobile from that workflow. Native-language freedom still
  has lifecycle and target contracts. [E1]
- Pinned Bevy has `HotPatchPlugin`/Dioxus integration as well as source-built plugins. Its
  `dynamic_linking` build facility is a different mechanism; `Plugin::cleanup` runs before
  scheduling, not as a plugin-unload protocol. Rust-first does not imply restart-only iteration,
  and none of these mechanisms supplies Nara's missing installed-tool workflow. [B3], [B4], [B5]

## The Next Discriminating Workflow: One Complete EnemyStats Tool

**Recommendation, not adoption:** compare the same small C# tool against the current native
flow in a prebuilt Editor. Do not grow both trial adapters into permanent SDKs. Hold data,
source/history semantics and expected results constant; select one responsibility-owned path
after this comparison. No `EnemyStats` implementation or proposed registration-scope type was
found in the inspected engine/Editor/fixture sources at this basis. [N5], [N6], [T2]

| User-visible operation | Existing evidence versus the missing public C# closure |
| --- | --- |
| Install/enable a package | Native panels are compiled into composition; private managed loading exists. Missing: an independent package author installs/enables the actual tool without Editor source changes/rebuild, with compatibility/trust refusal and diagnostics. |
| Define/attach `EnemyStats` | External trial catalogs work. Missing: supported C# declaration/generation and public Schema/representation admission, complete creation defaults, stable IDs/version/owner, Attach/Remove and a native Rust reader of the same authoritative data. |
| Inspect/edit/spawn | Native context already supports bounded reads, prepared edits/construction and exact results. Missing: managed panel/custom-Inspector and command activation through those owners, validation, one Undo/Redo entry, and no mutation while painting. |
| Save/Reopen and Prefab reuse | Private source workflows exist. Missing: the installed tool in the ordinary public Editor workflow, actual persisted reopen and repeated Prefab occurrences without private helpers or per-Schema native branches. |
| Edit code and retain declared state | Body-only trial routing exists. Missing: actual tool UI/callback replacement, explicit panel-state reconstruction, separate authoritative document state, failed candidate preserving the valid predecessor, and observable retirement. Schema changes need a separate migration case, not a body-edit success claim. |
| Disable/miss/re-enable | Absent-Schema source retention is already exercised privately. Missing: actual package disable, Save/Reopen with unknown owner/type/version/field data intact, no new execution, and re-enable/migration through ordinary admission. |
| Hold work across replacement | Private retained synchronous callback/instance probes exist. Missing: a real pending tool task and queued completion during reload/disable; stale results cannot edit the successor, while old code remains owned until physical completion. Include final-required-contribution failure and post-publication cleanup failure. |
| Run/export without the Editor | Missing for this managed tool: Runtime/Editor assembly separation, a concrete gameplay consumer, supported runtime deployment and clean-machine target evidence; Rust-only projects still exclude the VM. |

Judge the result by private engine hooks, required author-visible concepts, Editor rebuild/restart,
error recovery and completed user operations. Measure build/reload latency and crossing/allocation
costs on that workload; do not infer a C#/Lua performance ranking from prior unequal fixtures.
The update's 24 million calls/second example is arithmetic, not a measured budget. [N1], [T5], [T6]

## Alternatives And Priority

| Alternative | Appropriate use and cost |
| --- | --- |
| Direct source-built Rust | Preserve as the complete native path and comparison control. Existing native panels/owner APIs are reusable; it does not satisfy install-without-Editor-rebuild by itself. |
| One concrete .NET adapter | Leading experiment if typed reloadable Editor tools are the selected product pressure. Hosting feasibility is established narrowly; binding generation, public domain coverage, lifecycle and deployment remain the work. No automatic official-support designation. |
| Lua adapter | Smaller interpreted embedding candidate if gameplay/mod logic is the priority. mlua 0.12.2 provides conversions/userdata; its optional `send` feature synchronizes VM access, not parallel execution of one VM. Do not build a second full tools SDK merely because the trial already exists. [L1] |
| Wasm / separate tool process / native binary | Wasm WIT/Component Model can define imported/exported capabilities and generated bindings, not native Rust ECS access. A process is an isolation/recovery alternative, not automatically an OS sandbox. A native ABI is justified by an actual independently shipped binary consumer. None should be mandatory for ordinary Rust. [W1], [N2], [R1] |

1. **P0 - Proceed with the authorized optional experiment now.** C# is a concrete experimental
   candidate, not a selected official SDK or mandatory second language. Do not wait for an
   official-support decision to run the already-authorized comparison. Decide formal support
   after end-to-end evidence; preserve the complete Rust/no-VM baseline throughout.
2. **P1 - Close the single EnemyStats workflow.** Use current native flow as control, admit one
   concrete C# owner, and expose only the public representation/editor operations it actually
   consumes. Preserve exact source/catalog identity and native direct access; delegate UI shape
   to its own lane. This is more discriminating than another Name callback or panel-registration demo.
3. **P1 - Make lifecycle and compatibility observable.** Exercise held work, dependency/schema
   changes, rejection of the last required contribution and physical retirement failure. Keep
   restart/process fallback honest; do not advertise unload on generation fencing alone.
4. **P2 - Qualify development and delivery separately.** Check diagnostics/source navigation,
   declared state restoration, packaging, CLR selection and supported native targets. Only then
   decide official support, a shared generated contract and measured data-access optimization.
5. **P3 - Add other execution/distribution paths when demanded.** No universal `LanguageBackend`,
   stable native ABI freeze, new resolver or parallel Lua/Wasm SDK program is required to answer P1.

## Scope Limits

Primary sources only. Official web documentation was checked on 2026-10-27; mutable `/stable/`
and Learn pages are retrieval-time observations, not claims that they match the pinned engine
sources. Source comparisons use the exact commits above; Nara evidence is local, not publicly
retrievable at the unpushed baseline. Old GDExtension tutorial URLs returned
404; the current official `engine_details/engine_api/gdextension/` paths were used instead.

No Cargo, builds, tests, formatting, dependency installation or commits were run. No vendor
contents were read; existing `repo-ref/` checkouts were read-only. Historical trial success is
not independently requalified. This lane neither selects a scene representation, UI toolkit,
widget/state model, official language, platform release scope nor final production SDK. It
changes only this report; concurrent design/plan files and all production/other docs remain untouched.

## Primary Source Links

[N1]: ../../STRATEGY.md
[N2]: ../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md
[N3]: ../architecture/adr/0120-schema-publications-and-execution-representations.md
[N4]: ../architecture/adr/0121-composable-render-programs-and-host-execution.md
[N5]: ../../editor/src/platform/composition.rs
[N6]: ../../editor/src/frontend/imgui/workspace.rs
[N7]: ../../editor/src/frontend/imgui/field_editor.rs
[N8]: ../../crates/nara_reflect/src/registry.rs
[N9]: ../../crates/nara_app/src/plugin.rs
[N10]: ../architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md
[T1]: 2026-10-27-behavior-representation-comparison.md
[T2]: ../../tests/fixtures/behavior-languages/tests/languages.rs
[T3]: ../../tests/fixtures/behavior-languages/tests/support/lifetime.rs
[T4]: ../../tests/fixtures/behavior-languages/Cargo.toml
[T5]: 2026-10-27-schema-driven-behavior-comparison.md
[T6]: 2026-10-27-behavior-code-iteration-and-lifetime.md
[T7]: 2026-10-27-behavior-representation-comparison.md#historical-source-location
[G0]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/version.py
[G1]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/glue/GodotSharp/GodotPlugins/Main.cs#L142
[G2]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/glue/GodotSharp/GodotPlugins/PluginLoadContext.cs#L17
[G3]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/csharp_script.cpp#L625
[G4]: https://docs.godotengine.org/en/stable/tutorials/scripting/c_sharp/c_sharp_basics.html
[G5]: https://docs.godotengine.org/en/stable/engine_details/engine_api/gdextension/gdextension_interface_json_file.html
[G6]: https://docs.godotengine.org/en/stable/engine_details/engine_api/gdextension/gdextension_file.html
[G7]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/extension/gdextension_manager.cpp#L161
[G8]: https://docs.godotengine.org/en/stable/tutorials/plugins/editor/making_plugins.html
[B0]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/Cargo.toml#L1
[B1]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/examples/ecs/dynamic.rs#L111
[B2]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/component/info.rs#L285
[B3]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/src/plugin.rs#L15
[B4]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/src/hotpatch.rs#L19
[B5]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_dylib/src/lib.rs#L7
[D1]: https://github.com/dotnet/runtime/blob/60629d14374c56f1cb51819049ad1fa529307f8d/docs/design/features/native-hosting.md#loading-and-calling-managed-components
[D2]: https://learn.microsoft.com/en-us/dotnet/core/tutorials/netcore-hosting
[D3]: https://learn.microsoft.com/en-us/dotnet/standard/assembly/unloadability
[D4]: https://learn.microsoft.com/en-us/dotnet/api/system.runtime.loader.assemblyloadcontext?view=net-10.0#remarks
[D5]: https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/#limitations-of-native-aot-deployment
[R1]: https://doc.rust-lang.org/reference/items/external-blocks.html#abi
[R2]: https://docs.rs/abi_stable/0.11.3/abi_stable/
[L1]: https://docs.rs/mlua/0.12.2/mlua/#send-and-sync-support
[W1]: https://github.com/bytecodealliance/wasmtime/blob/ebe5e8df1f4e12c5fc2f5954926881d8d61367d8/crates/wasmtime/src/runtime/component/mod.rs#L1
[U1]: https://docs.unity3d.com/6000.0/Documentation/Manual/UIE-HowTo-CreateEditorWindow.html
[U2]: https://docs.unity.com/en-us/engine/6000.5/manual/scripting/compilation-and-code-reload/plug-ins/for-desktop
[E1]: https://dev.epicgames.com/documentation/en-us/unreal-engine/using-live-coding-to-recompile-unreal-engine-applications-at-runtime

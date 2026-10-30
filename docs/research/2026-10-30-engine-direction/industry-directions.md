# Transferable engine directions for Nara

Research cutoff and retrieval date: **2026-10-30**.

**Source fact — Nara scope.** [Strategy](../../../STRATEGY.md) targets a complete Rust-first PC
production workflow, initially proved through a small 2D game. It explicitly values iteration,
programmable authoring, public APIs, reproducible export, modularity, and runtime tail costs. The
[architecture index](../../architecture/README.md) records the intended custom-drawn Editor and
distinguishes accepted contracts from proposed designs and implementation evidence. These documents
were read at Nara `75e1284ed28bc5a96686609f7dae8805d1924788`; this study does not audit Nara implementation.

**Evidence convention.** Every topic separates **source facts**, **inference**, and
**recommendation**. All external links below were checked on **2026-10-30**; versioned documentation
is an architectural baseline, not a claim to be the newest release. Local source citations use
Bevy `ae6152152d6fb8308ad6038314b48ef295e80fbc` and Fyrox
`5495d012b32145674501af2e67d2fd06a06f04dd`, both dated 2026-10-29. Those commits prove source presence,
not release status. Recommendations and validation scenarios below are proposed work, not completed
acceptance or confirmed Nara defects.

**Recommendation — overall priority.** Protect boundaries that are costly to change after games
depend on them: authored identity versus derived data, semantic editing versus presentation,
execution state versus code generations, and asset references versus physical residency. Adopt
advanced algorithms when a consumer or measurement needs them; extensibility does not require
implementing every future subsystem now.

## 1. Asset processing is a build system with recoverable provenance

**Source facts.** Godot separates committed import settings from regenerable imported files and
warns that accessing imported assets as ordinary files can work in the Editor yet fail after export.
See [Godot 4.5 import process](https://docs.godotengine.org/en/4.5/tutorials/assets_pipeline/import_process.html).
Bevy's processor preserves source assets, uses hashes to avoid unnecessary processing, and records
unfinished transactions for recovery. See the
[pinned Bevy processor](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/processor/mod.rs).
Unity's content update workflow requires the original release's content-state file and distinguishes
asset updates from code changes. See
[Addressables 2.7.6 update builds](https://docs.unity3d.com/Packages/com.unity.addressables@2.7/manual/content-update-builds-overview.html).

**Inference.** These systems converge on reproducible transformation and explicit publication,
despite different APIs. A cache, an exported content package, and an editable source document have
different authority and lifetimes. Treating a source path or an in-memory object as all three makes
later packaging, cache sharing, and patch compatibility unnecessarily disruptive.

**Recommendation.** Review Nara's asset boundary for stable source/product identity, explicit
dependencies, importer/version/settings/target inputs, and independently consumable runtime
products. Make a local rebuild from source authoritative; publish completed products coherently
after failure. Validate reimport invalidation and a clean-machine export that never requires
Editor-only source loaders. Record nondeterministic processing explicitly instead of claiming
universal byte determinism. Defer distributed caches, CDN delivery, and patch services until local
build correctness and a real distribution workflow demand them.

## 2. Streaming separates demand, references, residency, and activation

**Source facts.** Unreal World Partition uses streaming sources and cells, while One File Per Actor
supports editing collaboration; referenced actors can be bundled for loading. See
[Unreal 5.6 World Partition](https://dev.epicgames.com/documentation/en-us/unreal-engine/world-partition-in-unreal-engine?application_version=5.6).
Unity documents that releasing an Addressable does not necessarily release its memory until the
containing bundle can unload. See
[Addressables 2.7.6 memory management](https://docs.unity3d.com/Packages/com.unity.addressables@2.7/manual/memory-assets.html).
Fyrox returns shared resource handles with pending, successful, or failed loading states; Godot's
threaded loading getter can still block if called before completion. See the
[pinned Fyrox resource manager](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-resource/src/manager.rs)
and [Godot 4.5 background loading](https://docs.godotengine.org/en/4.5/tutorials/io/background_loading.html).

**Inference.** Asynchronous loading alone is not a world-streaming architecture. Dependency closure,
packaging granularity, activation work, and retained consumers determine stalls and memory. Editing
units need not equal delivery units or runtime entity lifetimes.

**Recommendation.** Preserve distinct authored references, built products, runtime instances, and
residency ownership. Prove the boundary with two levels sharing assets: prefetch the destination,
abandon an obsolete request, activate within an observed frame budget, and explain why each resource
remains resident after departure. Keep demand and activation policy outside the generic loader.
Defer spatial partitioning, HLOD, virtualized geometry, and an open-world scheduler until a measured
world requires them. A tile/chunk consumer should be able to add such policy without replacing
identity or dependency contracts.

## 3. Programmable editors expose semantic operations, not just panels

**Source facts.** Unity's serialized editing API integrates dirty tracking, Undo, and multi-object
editing. Godot assigns editing actions to scene or global histories. Fyrox's command contract has
execute, revert, and finalization, with a separate indication of changes relevant to saving. See
[Unity 6000.0 SerializedObject](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SerializedObject.html),
[Godot 4.5 EditorUndoRedoManager](https://docs.godotengine.org/en/4.5/classes/class_editorundoredomanager.html),
and the [pinned Fyrox command contract](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/editor/src/command/mod.rs).
Unreal's Python integration is Editor-only and explicitly warns that a transaction cannot make
every operation undoable; its selected documentation labels the feature Experimental. See
[Unreal 5.6 Python scripting](https://dev.epicgames.com/documentation/en-us/unreal-engine/scripting-the-unreal-editor-using-python?application_version=5.6).

**Inference.** Tool extensibility becomes durable when domain operations own validation, history,
and persistence independently of the widget that invokes them. Reflection and panel registration
alone do not provide a production authoring API. Neither does wrapping arbitrary operations in an
object named a transaction.

**Recommendation.** Give Nara's actual authoring domains complete public operations: inspect,
prepare/preview, validate against the observed revision, apply with an exact result, undo, and save.
Exercise one independent semantic tool through both UI and a non-UI caller, including stale input
and reopen. Reuse these operations for optional language and external-tool adapters. Introduce
process protocols only where installation or isolation needs them; defer a universal extension
kernel or stable Rust dynamic ABI. This directly supports the intended custom Editor without
making its UI implementation the owner of project semantics.

## 4. Fast iteration needs explicit state transitions as well as faster compilation

**Source facts.** Unreal Live Coding patches running binaries, but constructor default changes do
not update existing instances in every case; structural changes involve object reinstancing and
references must be handled accordingly. See
[Unreal 5.6 Live Coding](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-live-coding-to-recompile-unreal-engine-applications-at-runtime?application_version=5.6).
Unity's option to skip domain reload reduces Play-entry work but leaves static state and event
subscriptions requiring explicit reset. See
[Unity 6000.0 domain reload](https://docs.unity3d.com/6000.0/Documentation/Manual/domain-reloading.html).

**Inference.** Mature engines exchange some automatic state reconstruction for faster iteration;
they do not establish arbitrary code/schema replacement as a free operation. Rust's native data
layouts and resource ownership make the replacement boundary a correctness concern, not merely
a linker optimization.

**Recommendation.** Measure separate paths for data/shader reload, compatible function changes,
and structural changes. Keep restart plus explicit state restoration a complete production path;
add patching only for a defined compatibility envelope. Preserve tool selection and authored
state across runtime restart without treating transient ECS indices or GPU handles as persistent
identities. Report edit-to-first-correct-frame P50/P95, including failure recovery. Defer a second
mandatory gameplay language, universal ABI, and transparent migration of arbitrary Rust memory.
A schema edit during a representative Play session is the useful acceptance scenario.

## 5. Rendering needs resource visibility and shared infrastructure before advanced effects

**Source facts.** Unreal RDG declares resource dependencies and manages resource lifetimes,
synchronization, pass culling, and diagnostics. See
[Unreal 5.6 RDG](https://dev.epicgames.com/documentation/en-us/unreal-engine/render-dependency-graph-in-unreal-engine?application_version=5.6).
Bevy's June 2026 release describes GPU batch preparation, sparse uploads, and GPU visibility work;
its October release replaces the sprite backend with infrastructure shared with 3D and ships
WESL/material extensibility. See [Bevy 0.19, 2026-06-19](https://bevy.org/news/bevy-0-19/)
and [Bevy 0.20, 2026-10-08](https://bevy.org/news/bevy-0-20/).
Godot offers rendering-stage callbacks, but the selected CompositorEffect API is Experimental and
its compositor is limited to Forward+ and Mobile renderers. See
[Godot 4.5 CompositorEffect](https://docs.godotengine.org/en/4.5/classes/class_compositoreffect.html)
and [compositor guide](https://docs.godotengine.org/en/4.5/tutorials/rendering/compositor.html).

**Inference.** The transferable direction is making resources, updates, and extension requirements
visible enough to schedule and diagnose. GPU-driven rendering also depends on economical CPU
preparation; changing the draw-call API cannot fix repeated full-world scanning. The evidence does
not make every engine's graph or extension contract interchangeable.

**Recommendation.** For Nara's 2D game and self-drawn Editor, prove shared textures/buffers, offscreen
views, a custom material/pass, and composition with explicit reads, writes, lifetime, and capability
requirements. A small resource graph is justified when these consumers branch or share outputs;
defer sophisticated scheduling and memory aliasing. Keep extraction, preparation, and submission
costs observable, with incremental uploads and batching driven by measurements. Add GPU culling,
indirect draws, or bindless paths only behind supported capabilities and a working fallback.
Defer Nanite/Lumen equivalents, general ray tracing, and mandatory mesh shaders.

## 6. A custom-drawn tool UI needs semantic and platform foundations

**Source facts.** Godot 4.5 shipped AccessKit integration for Control nodes and custom accessibility
bindings. See the [Godot 4.5 release](https://godotengine.org/releases/4.5/index.html).
Unity models screen-reader navigation using an accessibility hierarchy; custom Slate widgets need
an accessible-widget implementation in Unreal's selected Experimental screen-reader support. See
[Unity 6000.0 AccessibilityHierarchy](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/Accessibility.AccessibilityHierarchy.html)
and [Unreal 5.6 screen readers](https://dev.epicgames.com/documentation/en-us/unreal-engine/supporting-screen-readers-in-unreal-engine?application_version=5.6).
Fyrox implements retained, graphics-API-independent UI with routed messages. See the
[pinned Fyrox UI](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-ui/src/lib.rs).
Bevy 0.20 ships additional editor-oriented widgets; its separate future-work section is not proof
of a completed Editor. See [the October release](https://bevy.org/news/bevy-0-20/).

**Inference.** Custom drawing and accessibility are compatible, but draw commands do not encode
names, roles, actions, focus, or text-editing semantics. Existing toolkit capabilities also do not
prove accessible parity for every Editor or desktop platform.

**Recommendation.** Keep domain commands independent from retained interaction state, layout,
painting, and the platform accessibility adapter. Establish stable semantic widget identities,
keyboard focus/navigation, IME composition, selection/clipboard, text shaping, DPI scaling, and
virtualization in an early real Inspector/browser workflow. Test keyboard-only operation and an
actual screen reader on each claimed desktop platform, including focus after filtering or deleting
items. Share rendering and UI primitives with game UI where useful, while allowing Editor-specific
widgets. Defer a universal declarative language, web compatibility, and an exhaustive widget suite.

## 7. Data-oriented execution does not require one universal object representation

**Source facts.** Unity baking converts flexible authoring GameObjects into runtime entity data;
structural component changes can relocate entities between chunks. See
[Entities 1.3.15 baking](https://docs.unity3d.com/Packages/com.unity.entities@1.3/manual/baking-overview.html)
and [structural changes](https://docs.unity3d.com/Packages/com.unity.entities@1.3/manual/concepts-structural-changes.html).
Unreal Mass uses fragments, archetypes, chunks, and deferred composition changes. See
[Unreal 5.6 MassEntity](https://dev.epicgames.com/documentation/en-us/unreal-engine/overview-of-mass-entity-in-unreal-engine?application_version=5.6).
Bevy explicitly trades faster iteration in table storage against faster insertion/removal in sparse
sets. See the [pinned StorageType contract](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_ecs/src/component/mod.rs).
Godot's scene layer can be bypassed through server APIs; the selected guide carries an outdated-page
warning, so its architectural description is not performance evidence. See
[Godot 4.5 servers](https://docs.godotengine.org/en/4.5/tutorials/performance/using_servers.html).

**Inference.** The common direction is workload-specific representations and batch access, not an
industry-wide replacement of every object with an ECS entity. Editor documents, persistent
references, runtime component storage, and GPU buffers have different optimization criteria.

**Recommendation.** Retain Nara's selected `bevy_ecs` substrate unless actual workloads disprove its
fit. Let typed native components remain the fast Rust path; use explicit conversion/projection for
authoring or dynamic data where needed. Avoid freezing public serialization around archetype rows,
component memory offsets, or renderer layout. Measure iteration throughput, structural churn,
change propagation, and extraction bytes separately on the reference game. Defer a new ECS,
universal reflection-based component access in hot loops, and an automatic requirement that assets,
Editor documents, and UI all share one ECS world.

## 8. AI-assisted creation belongs above semantic tools and asset publication

**Source facts.** Unity's official 2026-05-06 beta article describes an assistant that reads project
context and invokes Editor operations with distinct read-only, script-writing, and broader action
permissions; it also describes undo and generated-asset metadata. This is a dated beta description,
not a claim of general availability or independently verified reliability. See
[Unity AI Assistant modes](https://unity.com/blog/unity-ai-assistant-ask-plan-agent-mode-explained).
Unity separately documents in-editor creation tools and Sentis for running trained models in a
project. See [Unity AI documentation, unversioned snapshot](https://docs.unity.com/en-us/ai).
The semantic editing contracts in topic 3 supply non-AI precedents for this integration boundary.

**Inference.** The valuable engine capability is inspectable project context plus reliable,
bounded authoring operations. Model choice, prompting, and tool transport can change without
altering ECS ownership, rendering, or project persistence. Runtime inference for an actual game
feature is a separate consumer requirement.

**Recommendation.** Make AI an optional client of the same source-edit, scene-edit, import, build,
and diagnostic APIs used by humans and ordinary tools. Return structured observations and exact
results; validate proposed changes against their source revision and supported operations. Keep
generated content editable, record provenance where available, and publish it through normal asset
processing. Validate with one operation such as proposing enemy-stat changes, previewing the diff,
applying, undoing, saving, and rejecting a stale proposal. Defer a mandatory model service, bespoke
agent framework, autonomous runtime authority, or LLM dependencies in core engine crates.

## Integration priorities

**Recommendation.** Use the following order when integrating this research with the implementation
review. The table is a prioritization proposal, not a replacement for Accepted ADRs.

| Priority | Boundary to review now | Evidence needed before expanding |
|---|---|---|
| First | Asset identity, derived products, dependency and publication semantics | Clean export, targeted invalidation, failure recovery |
| First | UI-independent semantic editing and code/state replacement | Independent tool with Undo/save; structural Rust edit and restore |
| First | Custom UI interaction/accessibility and render-resource composition | Real Inspector/browser and multi-view workflow on claimed PC platforms |
| Next, measured | Residency, incremental extraction, batching, ECS layout | Shared-asset travel, activation stalls, frame-tail and memory attribution |
| Optional adapters | AI tools, additional languages, external tooling | One complete consumer using existing public operations |
| Deferred algorithms/services | Open-world partitioning, advanced 3D, remote build infrastructure | A concrete product workload that exceeds the preceding foundation |

**Inference — limit of this study.** These sources identify architectural pressures and useful
counterexamples, not defects in Nara. They support investing early in ownership and representation
boundaries while leaving algorithms replaceable. No benchmark result from another engine predicts
Nara's performance, and no roadmap paragraph establishes release readiness.

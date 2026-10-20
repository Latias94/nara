# Module And Product-Goal Alignment Review

Date: 2026-10-20. Implementation baseline: `727dcbd` on main, after the qualified foundation,
authoring, owned preparation and file-cleanup corrections. This is a source and design review,
not a new release qualification or a claim that every execution path has been tested.

The correction branch was fast-forwarded into main and normally pushed. Remote main was verified
as `727dcbdde5f00bfd63734eb98a420166021f0aee` before this review's documentation work.
The review covers all 35 workspace packages and the root integration owners, with seven bounded
read-only research lanes and independent challenges. No Cargo is run by researchers.

Comparative evidence uses local Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. These are pinned source snapshots, not assertions about
the latest upstream release or equivalent performance. repo-ref remains untracked and read-only.

## Goal And Authority

[STRATEGY](../../STRATEGY.md) selects a Rust-first integrated production path, built outward from
complete games. The current proof is a small general-purpose 2D PC game; modules should remain
independently understandable and usable while the default combination works as one engine.
Artists and designers are part of the intended team, so a successful Rust API alone does not prove
a successful content-authoring workflow.

The user's earlier planning transcript contributes 2D-first, pixel-game, editor, composability and
Pokemon/Stardew-like workflow expectations. Its embedded model responses are recommendations, not
accepted requirements. They do not mandate hecs, a genre-specific core, a second author language,
one-second builds, whole-World serialization, 2.5D, Web support or a universal event framework.
Current Accepted ADRs and Strategy own those choices; the unrelated withdrawn Pele link is excluded.

| Goal | Architecture question | Evidence needed beyond source shape |
| --- | --- | --- |
| Complete Rust production | Can the same game definition create, edit, Check, Play and Export without private game hooks? | A real public author workflow, including a broken-build repair and packaging |
| Coherent editor and runtime | Do document, execution, native, input and resource owners remain explicit during failure and replacement? | Action/result, source-conflict, cancellation, physical close and native interaction trajectories |
| Fast iteration | Which work blocks the owner, and which edits require rebuilding or state reconstruction? | Separate data, body and structural edit-to-result P50/P95 at the current source |
| Integrated modular platform | Can a supported module be consumed or replaced without importing the whole product or duplicating its ordering rules? | Independent source consumers and a complete domain integration |
| Runtime tail budget | Are queue, validation, residency and native costs bounded in the actual game workload? | Per-workload P99, memory and aggregate-worker observations |
| Desktop delivery | Can an external machine run the exported game from an arbitrary working directory? | Current native platform and clean-consumer evidence for each claimed target |

The [reference-game README](../../reference-game/README.md) explicitly calls it a development and
regression fixture, not a finished commercial game. Existing owner and native tests are valuable
acceptance of their recorded slices; they do not supply all of the production evidence above.

## Existing Research Reconciliation

Research records retain their original revision, reference versions and verification limits.
Current behavior is checked against code, owning ADRs and later implementation evidence rather
than copied from an old finding table.

| Research | Current disposition |
| --- | --- |
| [Runtime concepts](../research/2026-10-15-foundation-design/runtime-concepts.md) | Managed execution, unified definition/schema composition and document-revision action admission were adopted under ADR 0117. Ordinary raw-App choreography is obsolete; caller capture-disposal and native affinity remain separate concerns. |
| [Content concepts](../research/2026-10-15-foundation-design/content-concepts.md) | Structured identity, immutable schema composition, published-value/attempt separation and domain reconciliation are adopted within their qualified scopes. Universal save/restore, content packs and arbitrary native replacement are not implemented by those proofs. |
| [Render concepts](../research/2026-10-15-foundation-design/render-concepts.md) | Current ordering, views, material and presentation contracts must be read through ADR 0117 and later native work. Historical primitive-kind and singular-view findings are not current findings by default. |
| [Standalone Editor product model](../research/standalone-editor-product-model.md) | Generic source recovery, executable/schema separation and external Play remain useful product boundaries. Earlier egui and in-process Play descriptions are historical; ADRs 0114/0116/0118 own current behavior. |
| [Editor module seams](../research/2026-10-16-editor-foundation/nara-module-seams.md) | Current Dear ImGui, panel and native-view evidence supersedes the old toolkit migration status. Semantic actions and one native/GPU owner remain the important boundary. |
| [TileSet dependency/loading](../research/2026-10-18-tileset-dependency-and-editor-loading-research.md) | Later R0 and ADR 0117 corrections own strict TileId validation, coherent TileSet/Image publication, revocation and desired-dependency recovery. Brushes, structural tile edits and world residency remain distinct workflows. |
| [Extension ecosystem](../knowledge/engineering/extension-ecosystem-engine-research.md) | The distinction between source packages, runtime plugins and Editor contributions remains sound. A universal package/extension kernel and native dynamic ABI remain unselected; Cargo and source-built contributions are the current baseline. |
| [Subsecond research](../knowledge/engineering/subagents/2026-07/2026-07-12-dioxus-subsecond-rust-hot-patching-research.md) | ADR 0093 keeps compatible patching optional and rebuild/reconstruction reliable. Historical feasibility is not a current supported hot-patch implementation or a latency guarantee. |
| [Positioning research](../knowledge/engineering/subagents/2026-07/2026-07-12-nara-positioning-official-language-research.md) | Strategy separates engine identity from technical mechanisms and product claims. This review uses that distinction without treating dated website wording as current market evidence. |

The [first-playable baseline](../benchmarks/reference-game-first-playable-baseline.md) records a
historical Redirect with missing production metrics. Its old timings are not current regression
measurements. Conversely, newer Open/Save/Reopen phase measurements cannot substitute for current
Rust edit-to-result or whole-game production observations. ADR 0099 keeps each observation tied to
the decision it actually supports, rather than adding a generic evidence or release framework.

## Module Coverage

Goal fit evaluates the current contract, not feature completeness or cross-platform qualification.
Each package has a named research owner; the root boundaries below identify production joins that
cannot be judged from a crate manifest alone.

| Package | Responsibility, goal fit and current limit | Source / consumer |
| --- | --- | --- |
| `nara_app` | One configuration/execution owner, fallible plugins, typed schedule participation and retained close fit headless, desktop and embedding. Built-in ordering stays explicit. | [Runtime](../../crates/nara_app/src/runtime.rs); [ownership tests](../../crates/nara_app/tests/runtime_ownership.rs) |
| `nara_ecs` | Mature Bevy substrate plus narrow lifecycle-free transaction guards; a second ECS is unnecessary. | [Facade](../../crates/nara_ecs/src/lib.rs); [transaction](../../crates/nara_ecs/src/transaction.rs) |
| `nara_ecs_derive` | Delegates to matching upstream macro logic and supports renamed dependencies; keep version parity rather than another derive implementation. | [Derive](../../crates/nara_ecs_derive/src/lib.rs); [renamed facade consumer](../../tests/fixtures/derive-dependencies/renamed-root/Cargo.toml) |
| `nara_core` | Domain-neutral math/color, bounded units and format vocabulary remain a reusable leaf, without service ownership. | [Core](../../crates/nara_core/src/lib.rs); [format tests](../../crates/nara_core/tests/format_contract.rs) |
| `nara_tasks` | Bounded blocking preparation, cancellation and physical retirement fit existing I/O consumers; these pools are neither Futures nor frame jobs. | [Runtime](../../crates/nara_tasks/src/runtime.rs); [Editor preparation](../../src/project_host/runtime/editor/open/preparation.rs) |
| `nara_input` | Retained endpoint observations and semantic-action lifetimes fit fixed-tick input and paused owners; complete controller/rebinding UX remains separate. | [Timeline](../../crates/nara_input/src/timeline.rs); [semantic tests](../../crates/nara_input/tests/semantic_actions.rs) |
| `nara_gameplay` | Bounded tick/source/sequence commands decouple physical input and authoritative gameplay without a universal event bus. | [Queue](../../crates/nara_gameplay/src/queue.rs); [game consumer](../../reference-game/src/input.rs) |
| `nara_diagnostic` | Privacy-classified bounded diagnostics and numeric pressure are sound observations, but not yet a complete runtime debugging product. | [Runtime](../../crates/nara_diagnostic/src/runtime.rs); [watch producer](../../crates/nara_asset_watch/src/observability.rs) |
| `nara_reflect` | Durable schema identity, migration and frozen native bindings fit source recovery and typed Rust execution; runtime type metadata alone cannot replace them. | [Registry](../../crates/nara_reflect/src/registry.rs); [manual consumer](../../tests/fixtures/module-consumer/tests/manual_component_codec.rs) |
| `nara_reflect_derive` | Closed ordinary persistent-component profile reduces registration work; manual schemas/codecs remain the complete public route for unsupported enum/newtype shapes. | [Derive](../../crates/nara_reflect_derive/src/lib.rs); [component guide](../guides/persistent-components.md) |
| `nara_identity` | World-scoped incarnations, document identity and durable runtime identity correctly serve different lifetimes; a save/restore workflow remains separate. | [Domain](../../crates/nara_identity/src/domain.rs); [independent identity consumer](../../tests/fixtures/module-consumer/tests/durable_identity.rs) |
| `nara_hierarchy` | One ordered non-linked relation supports Scene, UI and transforms without making structure own every entity lifetime. | [Writer](../../crates/nara_hierarchy/src/writer.rs); [Scene bridge](../../crates/nara_scene/src/runtime_hierarchy.rs) |
| `nara_transform` | Completed 2D global projection and interpolation fit current consumers; continuous chains and limited KeepWorld/post-affine support need explicit authoring semantics. | [Propagation](../../crates/nara_transform/src/propagation.rs); [presentation](../../crates/nara_transform/src/presentation.rs) |
| `nara_scene` | Bounded documents, unavailable-record preservation, atomic patches and structured Prefab provenance support data-first authoring; native-effect replacement and game-state restoration are separate workflows. | [Authoring](../../crates/nara_scene/src/authoring.rs); [direct module consumer](../../tests/fixtures/module-consumer/tests/scene_spawn.rs) |
| `nara_fs` | Capability-bound reads/publication and explicit cleanup belong below product effects, independently of decoder/GPU policy. | [Filesystem](../../crates/nara_fs/src/lib.rs); [publication](../../crates/nara_fs/src/directory_publication.rs) |
| `nara_asset` | Typed identities, usable publication, attempt state, slots and reload edges support concrete domains without an erased importer executor. | [Storage](../../crates/nara_asset/src/storage.rs); [server](../../crates/nara_asset/src/server.rs) |
| `nara_asset_watch` | Bounded native hints and truthful RescanRequired are a useful adapter; no shipped automatic-rescan workflow is established. | [Adapter](../../crates/nara_asset_watch/src/lib.rs); [queue](../../crates/nara_asset_watch/src/queue.rs) |
| `nara_image` | Bounded owned decode, fresh identity, immutable prepared snapshots and last-good content fit runtime and generic Editor consumers. | [Content](../../crates/nara_image/src/content.rs); [reconciliation consumer](../../tests/fixtures/module-consumer/tests/image_reconciliation.rs) |
| `nara_tilemap` | Separate map schema and concrete TileSet asset execution are appropriate; brushes, stable definition migrations and world streaming remain distinct. A downstream publication gate is corrected in this review. | [Content](../../crates/nara_tilemap/src/content.rs); [membership](../../crates/nara_tilemap/src/admission.rs) |
| `nara_render` | Backend-neutral affine views, targets, presentation mapping and owned packets fit 2D composition; ordinary virtual-canvas presentation remains incomplete. | [Views](../../crates/nara_render/src/view.rs); [mapping](../../crates/nara_render/src/presentation.rs) |
| `nara_material` | A narrow trusted static-WGSL runtime seam is useful; durable material authoring is explicitly absent. | [Material](../../crates/nara_material/src/lib.rs); [palette consumer](../../examples/palette_material.rs) |
| `nara_sprite` | Durable geometry/image/sampler/sort intent stays independent of GPU APIs and fractional world simulation. | [Sprite](../../crates/nara_sprite/src/lib.rs) |
| `nara_sprite_render` | Completed-transform extraction and one ordered Sprite/Tile stream support coherent 2D occlusion; per-view culling still needs a workload. | [Extract](../../crates/nara_sprite_render/src/extract.rs); [queue](../../crates/nara_sprite_render/src/queue.rs) |
| `nara_render_wgpu` | Concrete device/queue authority and exact immutable resources are sound; checked allocation channels were completed in this review. | [Backend](../../crates/nara_render_wgpu/src/backend.rs); [textures](../../crates/nara_render_wgpu/src/texture.rs) |
| `nara_window` | Native-independent configuration/events and surface retirement are appropriate; accepted startup mode must reach its adapter. | [Window](../../crates/nara_window/src/lib.rs); [retirement consumer](../../examples/window_surface_retirement_smoke.rs) |
| `nara_winit` | One concrete event-loop/native-window/input owner is appropriate; ignoring fullscreen settings is corrected during this review. | [Adapter](../../crates/nara_winit/src/lib.rs) |
| `nara_tooling` | Immutable frames, document revision/history, typed actions and exact reports are a reusable semantic boundary, separate from widgets and execution. | [Workspace](../../crates/nara_tooling/src/workspace.rs); [frames](../../crates/nara_tooling/src/frame.rs) |
| `nara_project` | Typed bounded manifest semantics and settings lowering fit the product; Cargo remains the source-package resolver. | [Effective settings](../../crates/nara_project/src/effective.rs); [manifest](../../crates/nara_project/src/manifest.rs) |
| `nara-editor` | Toolkit-free application orchestration plus private ImGui/native adapters supports recovery and extensions; generic structure, multi-document and custom inline tools remain bounded. | [Application](../../editor/src/app.rs); [independent panel](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/imgui_contribution.rs) |
| `nara_text` | Deterministic admitted single-face Unicode layout/raster requests remain independent of ECS, windows and GPU. Fallback/localization and editable text require real consumers. | [Engine](../../crates/nara_text/src/engine.rs); [independent UI consumer](../../tests/fixtures/ui-module-consumer/src/lib.rs) |
| `nara_ui_kernel` | One bounded compiler produces immutable geometry, paint, hit, focus and semantics with logical identity; no World or platform ownership leaks into it. | [Compiler](../../crates/nara_ui_kernel/src/compiler.rs); [root](../../crates/nara_ui_kernel/src/compiler/root.rs) |
| `nara_ui_widgets` | Controlled Button behavior emits typed intents; it neither stores product values nor invokes arbitrary mutable World callbacks. | [Button](../../crates/nara_ui_widgets/src/button.rs); [Reference Game UI](../../reference-game/src/runtime_ui.rs) |
| `nara_ui` | Persistent ECS/Scene projection, surface-local interaction and an atomic action journal connect data authoring to presented behavior. | [Runtime](../../crates/nara_ui/src/runtime.rs); [transaction](../../crates/nara_ui/src/interaction/transaction.rs) |
| `nara_ui_render` | Backend-neutral immutable image/glyph packets and target-local planning preserve ownership; a paint baseline incorrectly uses hit-test bounds and is corrected during this review. | [Planner](../../crates/nara_ui_render/src/planner/mod.rs); [packet tests](../../crates/nara_ui_render/src/packet_tests.rs) |
| Root `nara` | ProductRecipe/schema contributions, headless/desktop Hosts, data-only authoring and compiled-project Check/Export provide real production joins. Root is a product integration owner, not merely re-exports. | [Product](../../src/product.rs); [workflow](../../src/project_workflow/mod.rs); [Reference Game](../../reference-game/README.md) |

All 35 workspace packages are covered above. Root Scene retention/startup, content discovery,
Editor authority, UI presentation, project workflows and executable/delivery adapters were traced
across their production joins. Raw file sizes include tests:
the root src tree is 131 Rust files/71,785 lines, and the Editor 83/44,462 at this baseline. Those
counts describe review scope, not a splitting threshold. Useful extraction follows ownership
changes such as document-keyed lanes, draft/query projections or checked GPU allocation.

## Cross-Module Findings

### Confirmed Defects And Repairs During This Review

| ID | Baseline defect and practical trigger | Owning correction and verification status |
| --- | --- | --- |
| R1 | New/retargeted TileSets gate dependency capture on the latest Image attempt being Loaded. A retained authorized publication cannot be consumed during Loading/Failed. | Corrected in `39a11ab`: use ImageContentSnapshot admission first; only cold unpublished Loading waits. Starting-source test fails; final TileSet 39, Image 85 and public content 17 tests pass, including revocation/raw-slot/identity guards. |
| R2 | Accepted BorderlessFullscreen/Fullscreen configuration reaches Window but native creation ignores its mode, silently creating a windowed game. | Corrected in `d718412`: lower startup mode at Winit creation; exact exclusive selection or explicit failure. Winit 53 ordinary and 4 actual Windows native mode/retirement tests pass. |
| R3 | Quad uniform/instance buffers and material/texture bindings allocate outside checked scopes. Physical allocation failure can escape typed results through WGPU's default panic; create_buffer_init maps before checking error scopes. | Corrected in `09a18b5`: check finite allocation stages before use/mapping; preserve pressure classification, cache validity and reservation refund. All-feature WGPU cohort: 128 pass; ten injected failure stages and actual validation failure recover through production encode/readback. Independent sprite-only and UI-only feature checks pass. |
| R4 | Ordinary runtime and recipe guides teach removed names/raw-App or hidden candidate choreography. | Runtime guide and ADR 0003/0084 synchronized to ADR 0117 at `dfd6730`, independently reviewed. Recipe guide clarifies generic versus trusted compiled placement in `3f8b6c5`. |
| R5 | The primary tutorial's independent Collect Game/plugin workspace is excluded from root checks and absent from ordinary CI. | Added bounded serial sample check/headless tests to the existing consumer job in `3f8b6c5`. Local all-feature/all-target check and 4 gameplay tests pass; hosted results are not claimed. |
| R6 | A visible glyph is discarded when its baseline lies outside the authored extent, before raster ascent and physical clipping are considered. Paint lowering uses a bounded hit-test point mapping. | Corrected in `8582427`: keep input bounds unchanged, affine-map the finite paint baseline and clip the glyph bitmap. Starting-source real-font regression fails; final UI-render cohort: 35 pass, including visible and fully clipped glyphs in direct and fractional letterboxed mappings. |

R1 is a downstream consumer of the previously repaired Image publication contract, not proof that
the earlier Image tests failed. R3 is outside the target-allocation/pipeline stages corrected by
N08/N09. Original qualifications retain their recorded source and scope. None of these repairs
adds Tile tools, a generic importer, a public RHI or a new execution framework.

### Ordinary Generic Authoring Needs A Complete Structural Workflow

Classification: capability limit with high product relevance. Generic authoring can add Empty
entities and understood components, but broader deletion/reparent/reorder is intentionally gated
([frontend](../../editor/src/frontend/imgui/authoring.rs):269,304;
[composition](../../crates/nara_tooling/src/composition.rs):93). Canvas direct manipulation also
requires the trusted compiled path
([mutation](../../src/project_host/runtime/editor/ui_authoring/mutation.rs):58).
These guards protect unavailable/unknown references; merely enabling buttons would be incorrect.

Use one ordinary external project for create -> arrange -> remove -> Undo/Redo -> Save/Reopen ->
strict Check/Play. Admit operations over understood data with explicit dependency protection and
one document transaction. This is more important to the integrated-editor goal than another
generic extension API. Godot combines structure and history
([SceneTreeDock](../../repo-ref/godot/editor/docks/scene_tree_dock.cpp):2513,3010); Bevy exposes
separate spawn/remove/reparent operations
([remote](../../repo-ref/bevy/crates/bevy_remote/src/builtin_methods.rs):51). Neither provides Nara's
unavailable-record guarantees automatically.

### Pixel Presentation Needs An Ordinary Game Consumer

Classification: capability gap central to the user's original pixel-game use case. Affine mapping
and IntegerFit exist, but ordinary frame construction lacks an image-target descriptor and WGPU
runtime execution expects a window target
([frame](../../crates/nara_render/src/frame.rs):108;
[backend](../../crates/nara_render_wgpu/src/backend.rs):920). The private Editor offscreen path does
not establish fixed low-resolution presentation for a shipped game.

Select an optional virtual-canvas product owner: fixed internal extent, nearest/integer scaling,
letterbox, HiDPI and matching pointer inversion, including windows smaller than the virtual canvas.
Keep simulation fractional and reuse DisplayMapping2d rather than adding another coordinate model.
Godot explicitly separates content extent from final window stretch
([Window](../../repo-ref/godot/scene/main/window.cpp):1375), supporting that division of ownership.

### Dynamic Content Requires Acquisition And Delivery Together

Classification: capability boundary shared by asset, Scene and export owners. Copy handles resolve
identity; they do not request loading or retain residency. ProjectContent currently admits a finite
startup closure, and export includes exactly those members
([export](../../src/project_workflow/export.rs):504). A second level or Rust-addressed content group
cannot be assumed to ship merely because it exists under the asset directory.

Use one two-level game to declare additional content roots, compute their bounded closure, acquire/
release/reacquire domains, retain in-flight packets and execute outside the checkout. Preserve the
negative test excluding arbitrary unused content. Godot separates selected export roots and their
dependencies ([export](../../repo-ref/godot/editor/export/editor_export_platform.cpp):1301);
Bevy separates strong retention and loader dependencies
([handle](../../repo-ref/bevy/crates/bevy_asset/src/handle.rs):115;
[loader](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs):377). Neither requires changing Nara's
Copy identity into a universal residency promise. ADR 0037 remains partial; Proposed artifact and
mount/package graphs are not implicitly adopted.

### Native-Domain Successors Need One Complete Integration

Classification: capability and integration boundary, not a new atomic Scene defect. ADR 0117's
external native-state proof establishes a useful direction, while its consumer explicitly retires
hook/RequiredComponents state before strict Scene synchronization
([fixture](../../tests/fixtures/module-consumer/tests/support/domain_reconciliation.rs):396).
[Strict Scene retirement](../../crates/nara_scene/src/runtime_hierarchy.rs):40 still rejects arbitrary
lifecycle effects, as its narrower rollback contract requires. Reference Game Retry is another
finite composition, not a universal lifecycle provider.

The first real physics/audio/effect-owning domain should prove accepted configuration -> native
retirement -> Scene publication -> revision-bound reconciliation, including pending creation,
failure after retirement, Retry, unload and physical close. Keep degraded/unavailable states and
native cleanup with that owner. Copying a general Scene manager or permitting arbitrary hooks in
the protected transaction would bypass the unresolved integration rather than solve it.

Pinned Bevy separates dependency resolution and spawning
([scene](../../repo-ref/bevy/crates/bevy_scene/src/scene.rs):49;
[resolved scene](../../repo-ref/bevy/crates/bevy_scene/src/resolved_scene.rs):47), and Godot separates
resource loading from [SceneState instantiation](../../repo-ref/godot/scene/resources/packed_scene.cpp):222.
Neither establishes universal rollback of native effects. Proposed ADR 0089 remains broader than
the qualified startup/Retry profile.

### Editor Save Is Not Game-State Restoration

Classification: production capability gap. Durable references are implemented, but Scene-capable
component export and the Reference Game observation snapshot are not a versioned save/restore
format ([export](../../crates/nara_scene/src/export.rs):129;
[snapshot](../../reference-game/src/snapshot.rs):68). ADR 0027 is correctly not-started in the ledger.
This is especially relevant to the user's world-exploration and simulation-game expectations.

Use the first game-owned save projection to test process restart, two instances of one source,
dynamic identity, resource state, old-format migration and native reconstruction failure. Project
Scene files must remain authoring truth. A reusable save abstraction needs that consumer; an
ambient World dump or a second identity scheme would undermine the existing boundary.

### Edit And Undo Cost Remain Unmeasured

Classification: source-confirmed work requiring measurement, not a demonstrated latency defect.
[Patch preparation](../../crates/nara_scene/src/patch.rs):267 clones a complete document; data-only
field editing clones and reclassifies its candidate
([authoring decode](../../crates/nara_scene/src/authoring_decode.rs):2180). Ordinary edits and history
invoke these paths synchronously. Owned Open/Save/Reopen do not qualify this interaction.

Measure scalar edit, rejection, Undo/Redo and structural operations on realistic Scene/Prefab
documents, including unavailable records and high-rate interactions. If a declared budget fails,
introduce touched-record candidates or classification-index reuse inside the document owner, with
full rebuild as oracle/fallback. Preserve exact inverse presence and rejection nonmutation; an
async field trait would not remove owner publication or validate incremental equivalence.
Godot's targeted Inspector do/undo operations
([editor_inspector](../../repo-ref/godot/editor/inspector/editor_inspector.cpp):5730) provide a useful
locality comparison, not proof that Nara can skip its document validation.

### Additional Extension And Measurement Triggers

| Boundary | Current fact and next complete consumer |
| --- | --- |
| Same-kind documents and pinned tools | Workspace slots exist, but root has one Scene lane and one Prefab lane; second same-kind Open closes the first. Document-keyed lanes and bounded inactive-document projections must precede tabs/cross-document tools. Godot keeps per-scene selection/history in [EditorData](../../repo-ref/godot/editor/editor_data.h):114,145. |
| Custom Inspector and multi-selection | Supported external contributions are full source-built panels; field projection targets one selected object. An inline/unselected/bulk tool should define narrow owner queries, selection semantics and atomic history before adding hooks. Preserve the proven exact panel results. |
| Controller/rebinding | Physical/semantic endpoint ownership is sound, but ordinary bindings are keyboard/mouse. A real analog/controller/rebind UI must define device values, disconnect, deadzone and held-key lifetime replacement. Godot [InputMap](../../repo-ref/godot/core/input/input_map.h):84 and Bevy [gamepad](../../repo-ref/bevy/crates/bevy_input/src/gamepad.rs):231 provide explicit alternatives. |
| Live source relocation | Cold UUID lookup supports a moved source; an existing runtime binding remains immutable. Browser Move/Rename needs a Host transaction or explicit reconstruction policy, including sidecars, references and stale completion invalidation. Godot updates loaded paths and references explicitly in [filesystem dock](../../repo-ref/godot/editor/docks/filesystem_dock.cpp):1668. |
| Visibility and durable materials | Ordered multiple cameras do not select disjoint renderables; static Rust material instances are not Scene-authored assets. Use a minimap and a saved palette-material workflow to choose each separate contract. A render graph does not solve either missing ownership. |
| ECS parallelism and aggregate workers | The selected bevy_ecs features do not enable multi_threaded; background TaskPools do not parallelize simulation. Measure the combined game/Editor workload and trial existing Bevy executor/scoped capabilities before another job kernel. |
| Shared runtime debugging | The bounded diagnostic/pressure store has limited production producers, while the Editor panel displays operation reports. Prove one real immutable producer-to-view path before a profiler or blanket bus bridge. |
| Browser, index and catalog responsiveness | Work-count and byte limits still perform synchronous filesystem calls and final classification/projection. Measure Browser admission, scan finalization and native frame tails separately from the qualified startup/Save/Reopen phases. |
| Large Tile worlds and multiple views | Extraction visits all tile cells and queue work scales with view count; logical GPU bounds do not qualify CPU culling/upload cost. Measure mostly-offscreen maps before visible-chunk caches, culling or buffer reuse. |
| Rust iteration and module integration | Current body/structural edit-to-result, full external authoring workflow and compile dependency costs need current-source observations. Historical Redirect and newer unrelated phase percentiles cannot settle those choices. |
| Runtime UI text and long-lived caches | Name entry/IME, fallback/localization and richer widgets remain separate from shaped display text. Existing B11 glyph-history capacity can safely reject a small current frame after long size churn; measure a live menu/resize consumer before fresh-incarnation working-set reconstruction. Never mutate old packet pages in place. |
| Runtime UI frame cost | Repeated packet planning of one compiled frame does not measure ECS projection, fresh layout/shaping, input routes, multiple surfaces or GPU preparation. Attribute a changing HUD/inventory before incremental layout or virtualization. |

These are concrete pressure points and owners, not authorization to implement every capability
immediately. They preserve the distinction between a correct current slice and a complete engine
production experience. Large file counts or a mature engine's larger feature set are not sufficient
reasons to create new crates, async traits or framework registries.

## Architecture Verdict And Refactor Order

The architecture is coherent for the selected foundation. Retain the Bevy ECS substrate; one
managed executable owner; typed plugin definitions; frozen persistent schema; document/history
ownership outside World; concrete native adapters; and resource-complete immutable render packets.
Their different lifetimes solve actual recovery, replacement and independent-consumer problems.
Replacing them with a scene-object tree, universal async trait, general importer/package kernel or
another job system would not directly close the observed workflows.

The mature engines expose a more complete author journey, rather than one missing master trait.
Nara's strongest comparison point with Bevy is typed modular composition; its desired difference
is integrated production. Godot is the useful benchmark for coherent document, Inspector,
resource and export workflows. Its Node/Resource representation is an alternative with different
tradeoffs, not proof that Nara's ownership model is wrong. Current source evidence supports the
architecture direction, while complete production maturity remains unproven.

| Order | Complete workflow / decision | First owning changes | Verification before widening scope |
| --- | --- | --- | --- |
| 0 | Close R1-R6 and synchronize main | Concrete TileSet publication, Winit creation, checked GPU stages, glyph paint lowering, current guides and sample CI | Focused regressions, native mode/retirement, GPU readback and relevant consumer checks; final workspace build |
| 1 | Ordinary generic structural authoring and pixel-game presentation | Data-only operation-specific admission/history, plus a finite virtual-canvas presentation owner | Create/arrange/remove/Undo/Save/Reopen/Check/Play in an external project; resize/HiDPI/letterbox input in a game |
| 2 | Measure current authoring and Rust iteration | Decision-local edit/Undo/Browser/catalog/model/compile/activation observations | Separate owner call, total completion and visible result; representative components, data, body and structural changes |
| 3 | One persistent two-level game | Explicit content roots, domain acquisition/unload, native reconciliation and game-owned versioned save projection | Exported second level outside checkout; fresh-process restore; two instances; degraded native reconstruction and retry |
| 4 | Widen editing from demonstrated pressure | Document-keyed lanes, pinned/inline tools, multi-selection, controller/rebinding, durable material fields | Independent real consumer with exact actions/history/close; do not add several unused contribution registries |
| 5 | Optimize the demonstrated bottleneck | Private touched-record/index reuse, visible chunks/culling, buffer reuse, atlas reconstruction or existing ECS parallelism | Rebuild/reference oracle, resource lifetime, bounded refusal and current workload tail/capacity measurements |

Items in orders 1-5 are recommendations for the next selected work, not implemented features or a
commitment to expand this review into every subsystem. Tile tools remain deferred under the user's
instruction. The next architecture checkpoint should follow the complete ordinary authoring and
persistent-game consumers, rather than another round of horizontal abstraction work.

## Verification And Scope

Research covered all 35 packages in seven bounded lanes. Substantive correctness findings were
independently challenged before changes. Existing research was reconciled rather than treated as
an authoritative current backlog. No whole-engine formal proof, exhaustive line-by-line test audit
or latest-upstream benchmark is claimed.

Root executed selected build/test coverage against the fixes and records commands/run IDs in the
[qualification](../knowledge/engineering/verification/2026-10/2026-10-20T044452Z-module-goal-alignment-and-concrete-boundary-repairs-qualification-f7676d2222234fd39b8b565dbcaf996f.md).
Final source is `09a18b5`; locked all-feature/all-target workspace, independent GPU feature checks,
formatting and selected actual Windows Editor regressions pass. Windows native mode tests do not establish Linux/macOS
support. GPU fault injection is deterministic error-path evidence, not genuine physical OOM.
The glyph regression uses actual shaped/rasterized nonzero ink; it is packet correctness evidence,
not native screen-reader or monitor-scanout evidence. Existing latency populations remain frozen.

The [review plan](../plans/2026-10-20-module-goal-alignment-review-plan.md) owns integration status.
R1-R6 are complete within the recorded verification scopes. The six repair/documentation commits
and this report are delivered together on main; the delivery message records the final pushed
object ID. Orders 1-5 remain separately selected capability and measurement work.

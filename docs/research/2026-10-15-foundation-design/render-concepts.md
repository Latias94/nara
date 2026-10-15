# Rendering and Spatial Concepts: Comparative Refactoring Research

Date: 2026-10-15. Nara source baseline: `b91223a`, with concurrent dirty work; current HEAD `757fb6a` adds the correctness audit. No source was changed and no build, test, or probe was run. Paths below are relative to the Nara repository root. References are local development snapshots, **not stable releases**: Bevy `4e6bd141ed3f5a53fab3e5e88e13e89c2d97792c` (`0.20.0-dev`) and Godot `d824ca9fe29137edbcbf682f1bafabb74a435eab` (`4.8dev`). Nara currently selects Bevy ECS `0.19` (`Cargo.toml:94`). Comparative findings do not authorize copying incompatible upstream interfaces.

Here, a **Module** is judged by its **Interface**, including required ordering, errors, configuration, and performance knowledge. A **Seam** identifies where an **Adapter** can vary. Large private **Implementation** is acceptable when it provides **Depth**, **Locality**, and useful **Leverage**. The following are recommendations, separated from observed facts.

## Decision 1: One ordered 2D content stream; private passes

**Evidence.** Nara sorts `kind` before `layer` (`crates/nara_sprite_render/src/queue.rs:184`); `Tilemap` precedes `Sprite` (`types.rs:28`). Domain batching therefore prevents a foreground tile from occluding an actor regardless of layer. Godot builds Z-ordered item lists before its backend receives them (`repo-ref/godot/servers/rendering/renderer_canvas_cull.cpp:79-105`); primitive commands live inside items (`renderer_canvas_render.h:168-200`). Bevy uses a shared transparent 2D phase and stable sorting (`repo-ref/bevy/crates/bevy_core_pipeline/src/core_2d/mod.rs:310-370`). Neither example makes primitive category the user's primary depth order.

**Select:** extract sprites and tilemaps into one engine-owned ordered content stream per view. Define layer, explicit sort group, optional Y anchor, and stable tie order; scope Y sorting deliberately. Keep UI's own painter order inside an explicitly positioned overlay. Batch only adjacent compatible items. Geometry source is metadata, not mandatory order. Preserve fixed private passes for clear/world, optional composition, and presentation.

**Alternative:** domain batches plus a public render graph offers more scheduling authority but does not establish correct primitive interleaving. Reject it for this scope. Conversely, a single stream is not sufficient for a material sampling the current screen: Godot flushes accumulated items before copying its backbuffer (`renderer_rd/renderer_canvas_render_rd.cpp:813-825`). Such an effect requires a deliberate private pass break; a graph is not automatically necessary.

**Migration:** replace `SpriteBatchKind::order` and the comparator in `types.rs`/`queue.rs`; move GPU batch construction behind the renderer. Keep tile dirty/chunk caching private to extraction and preparation rather than adding another public frame taxonomy. **Validation:** translucent sprites interleaved with foreground/background tiles, equal-key deterministic order, adjacent material changes, and one screen-copy effect preserving the established order.

**ADR:** supersede ADR 0105's fixed domain ordering with a successor; retain its private execution and resource-complete transfer principles. Refine 0012, 0017, and 0094 accordingly.

## Decision 2: View, target, and presentation are separate concepts

**Evidence.** Nara's `ExtractedView` stores position rather than an invertible view mapping (`crates/nara_render/src/lib.rs:265`), rejects rotated/scaled camera transforms (`:1149`), and admits exactly one view (`:485`). Godot's Camera2D constructs an affine transform with zoom and optional rotation, then inverts it (`repo-ref/godot/scene/2d/camera_2d.cpp:261-269`); picking uses the same transform chain (`scene/main/canvas_item.cpp:1343-1348`). Bevy separates projection and camera transform (`repo-ref/bevy/crates/bevy_render/src/view/mod.rs:450-451`) and explicitly composes cameras sharing a target (`view/composition.rs:1-9`).

**Select:** a 2D view has a stable identity, projection/camera mapping, viewport, visibility selection, order, and clear/load policy. A target has physical extent and configuration. Presentation maps a completed target into a window or editor panel. Pixel snapping, virtual resolution, letterboxing, and input inversion share one presentation mapping; world simulation remains fractional.

Multiple views on one target are an ordinary 2D extension, not a future 3D requirement. Keep the initial executor serialized, but replace cardinality assumptions with a bounded ordered view list. One target transaction may clear once and load for subsequent views; independent targets can remain explicitly sequenced. Godot topologically orders viewport dependencies (`servers/rendering/renderer_viewport.cpp:98-122`), demonstrating an eventual dependency problem without requiring Nara to implement that general scheduler now.

**Migration:** split `nara_render/src/lib.rs` into view, target, and immutable frame modules; replace `camera_world_position`, update sprite projection and `nara_render_wgpu/src/frame.rs`. **Alternative:** retain a position-only camera and special-case editor/minimap views; rejected because each consumer would duplicate coordinate conversions. **Validation:** two split-screen views, overlay camera without clearing, rotated camera picking, HiDPI resizing, integer-scaled letterbox, and a viewport smaller than its virtual canvas.

**ADR:** refine 0018/0094; supersede the single-view/single-target restriction portions of 0111/0112 through a focused successor, retaining exclusive target ownership.

## Decision 3: Offer a material Seam, not public batching internals

**Evidence.** `Material2dDescriptor` and its hash key have no production consumer (`crates/nara_material/src/lib.rs:73,153`); actual execution uses another material representation (`nara_sprite_render/src/types.rs:201`). Nevertheless extract/queue/sort/batch functions and mutable intermediate collections are publicly exported (`nara_sprite_render/src/lib.rs:65-72`). This is a wide Interface without a coherent extension workflow. Bevy's material extension genuinely connects user shader choices to its adapter (`repo-ref/bevy/crates/bevy_sprite_render/src/sprite_mesh/sprite_extended_material.rs:161-205,322-332`).

**Select:** keep shared sampler/alpha/color semantics; remove the unused descriptor/key model and hide technical intermediate DTOs and orchestration functions. Design a trusted Rust material contribution registered at startup from one concrete palette-swap example: declared shader input contract, parameters, texture references, alpha mode, and supported renderer capabilities. Its Adapter resolves GPU pipelines privately. Script/data mods select allowed material definitions and values; they do not receive ambient Device/Queue access. Shader-code changes take effect on restart in this scope; runtime shader replacement waits for a real editing/hot-reload workflow.

**Alternative:** preserve every currently public function as a future custom-renderer Interface. Reject: users would need to learn extraction order, preparation status, view admission, sorting, and batch invariants. Also reject copying Bevy's entire generic material framework before the first extension. A typed material handle and private implementation descriptor preserve room for growth without claiming universal backend replacement.

**Migration:** delete `Material2dDescriptor`, `Material2dKey`, and hashing helpers; consolidate runtime/extracted material conversion; narrow `sprite_render` re-exports and root preludes. **Validation:** stock and palette materials share ordering, parameters/textures obey the declared contract, unsupported capabilities report a local error, and no gameplay code orchestrates internal stages. Last-good shader pipeline recovery is deferred with runtime shader replacement rather than added to justify this example.

**ADR:** refine 0016/0032/0094 and the 0092 material representation; preserve the latter's explicit color/alpha contract.

## Decision 4: Retain distinct generations, centralize their joins

**Evidence.** Nara compares the same image/registration identity in several host promotion paths (`editor/src/platform/viewport.rs:54-60,678-680,1100-1102`) while WGPU owns another tuple comparison (`crates/nara_render_wgpu/src/editor_execution.rs:125-131`). Device generations are legitimate: Bevy reruns render startup when acquiring a new device (`repo-ref/bevy/crates/bevy_render/src/lib.rs:243-250`); Godot protects reused resource slots with a validator (`repo-ref/godot/core/templates/rid_owner.h:157-163,215-245`). These are not evidence that every caller should reconstruct the identity tuple.

**Select:** retain separate content revision, target revision, device namespace, and presented-frame identity because they invalidate independently. Put their joining and acceptance inside one private presentation Module. Return an opaque accepted/presented token; tools consume that token plus document identity. Revalidate when work crosses an asynchronous interval or mutable owner state changes; remove repeated validation only when ownership proves nothing relevant changed. Do not replace all generations with one global counter.

Publication is also distinct from the latest attempt. Audit F03 (`docs/verification/2026-10-15-foundation-correctness-audit.md`) proves failed reload can hide valid published image data before preparation; this research did not rerun its probe. Preparation must derive from the published immutable snapshot, with attempt failure tracked separately.

**Migration:** consolidate checks in `editor_execution.rs`/`presented.rs`, then simplify `editor/src/platform/viewport.rs`; revise preparation state in `nara_render/src/prepare.rs` and `nara_image/src/prepare.rs`. **Alternative:** remove receipts entirely; rejected because picking and runtime UI need correspondence with the displayed frame. **Validation:** old image after resize, device replacement, delayed composite, and A-published/B-failed before first preparation.

**ADR:** refine 0040/0078/0105/0111/0112; preserve resource-complete packets and exact displayed-frame authority.

## Decision 5: One structural mutation baseline and lifecycle owner

**Evidence.** Nara resolves placement before lifecycle validation flushes external deferred work (`crates/nara_hierarchy/src/writer.rs:119,639`). Audit F01/F02 prove a stale placement proof and missing leaf-root despawn invalidation; not rerun here. Bevy's built-in Children uses `linked_spawn` (`repo-ref/bevy/crates/bevy_ecs/src/hierarchy.rs:148`), but its generic relationship examples omit it, and document cascading as optional (`relationship/mod.rs:62-64,82-83`). Reject the premise that every Bevy relationship inherently owns destruction.

**Select:** retain Nara's ordered writer for atomic structural transactions, not ideological independence. Materialize deferred work before taking the semantic baseline; prevent external callbacks/flushes between preparation and commit. Every participant, including a childless root, needs lifecycle invalidation. Raw ECS mutation is an advanced escape that invalidates completion, never an equivalent second writer. Transform globals remain a derived projection after named semantic completion points.

**Alternative:** adopt a custom non-linked Bevy relationship. It can maintain reverse edges but does not by itself provide Nara's ordered multi-operation atomic publication. Reconsider only if a comparative prototype preserves those semantics with less Interface. Bevy dirty-tree propagation (`repo-ref/bevy/crates/bevy_transform/src/systems.rs:107-145`) is useful implementation evidence, not justification to replace correctness with opportunistic incremental updates.

**Migration:** refactor `nara_hierarchy/src/writer.rs`, membership invalidation in `relation.rs`/`lib.rs`, and Scene retirement callers; keep proof types private. **Validation:** pending despawn, queued sibling mutation, leaf-root removal, rejected batch, Scene replacement, and same-tick transform consumer.

**ADR:** refine 0100/0101 and their 0097 completion slice; preserve hierarchy-versus-lifetime separation.

## Decision 6: Toolkit replacement belongs at the native presentation Adapter

**Evidence.** Neutral `EditorFrameModel` and revision-bound actions already exist (`crates/nara_tooling/src/frame.rs:24,230,380`). Egui view inputs contain toolkit texture IDs without runtime authority (`nara_tooling_egui/src/game_view.rs:17-26`), a useful separation. But core WGPU execution mentions an egui registration generation (`nara_render_wgpu/src/editor_execution.rs:69`), leaking one Adapter's vocabulary. Godot likewise separates scene CanvasItem from render commands (`repo-ref/godot/scene/main/canvas_item.cpp:189`; `servers/rendering/renderer_canvas_render.h:168`), without implying those layers have interchangeable semantics.

**Select:** retain neutral document/actions; localize toolkit texture registration and native painting in the native Editor Adapter. Core execution issues an opaque sampled-image token, independent of egui. Keep game UI and editor UI separate; neither should require the other's widget model. Do not implement Dear ImGui now or create a universal widget trait to promise it.

**Migration:** move registration-specific joins into `editor/src/platform/viewport.rs`; rename toolkit-specific WGPU concepts; keep `nara_tooling_egui` as an optional Adapter. **Alternative:** replace the working editor immediately; rejected because the low-cost reservation is separation, not a second complete Implementation. **Validation:** a small fake presentation Adapter consumes the same neutral frame/actions, stale picks fail, and headless tooling imports no egui/wgpu types.

**ADR:** refine 0107/0111/0112; explicitly retain toolkit choice as open. No decision here introduces a 3D renderer, public graph, second RHI, or universal transaction framework.

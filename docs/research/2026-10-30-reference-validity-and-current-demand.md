---
title: Reference Validity And Current Resource Demand
date: 2026-10-30
input_shape: concept
subject: Pinned Bevy and Godot mechanisms for valid references, load dependencies and current preparation demand
rendered_for: Foundation CP3 design and plan owner
---

# Reference Validity And Current Resource Demand

Status: completed bounded source comparison for the revisions below.
This supplies constraints to the parent design and plan; it adopts no Nara implementation.

## Question, Scope And Provenance

How do scenes, handles, paths and delayed references declare loading and readiness dependencies?
Which boundary prevents startup Scene A from preparing every native texture reachable through
Door targets for future Scenes B and C, while those targets remain valid Product references?

This comparison supports Content CP3 in the existing
[research reconciliation](2026-10-30-engine-direction-reconciliation.md#finding-dispositions) and
[foundation R7](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r7-target-build-and-base-source-free-delivery).
Those sources own the Nara follow-up: distinguish a valid deliverable target from a dependency
needed to materialize the requested Scene, preserve strict Check/Export, and keep the separate
backing/residency investigation separate. R7 already identifies why present `Soft` membership
semantics cannot supply validated delayed destinations. This study does not re-audit those
Nara owners or choose where their replacement contracts belong.

The inspected read-only local checkout HEADs matched these exact commits and had clean worktrees:

- Bevy: `85503941e488152711a4695bbe248e9ed9f453fc`, under `repo-ref/bevy/`.
- Godot: `81ab961f91066a63bcbf3ec340962bb1a1ada558`, under `repo-ref/godot/`.

All source links below are repository-relative file and line anchors at these pins. They describe
these snapshots, rather than moving upstream releases or the revisions in the imported research.
The pass covered dependency declaration, request initiation, load readiness, render preparation
and scene/path resolution. It did not expand the framework comparison, run Cargo or native trials,
or measure startup time, memory, upload cost or backing eviction.

## Result

Avoiding preparation of future Scenes requires deferring the request that enters their resource
graphs. Reference validity, membership in a load-readiness graph, resource lifetime and native
render readiness are distinct facts. Removing a readiness wait does not cancel a load already
requested through another path.

Bevy can retain a path as data until explicit loading, but several path consumers immediately
turn it into a load request. Its asset and render pipelines have separate readiness boundaries.
Godot can retain a future Scene as a string or explicit instance placeholder, but ordinary
external Resource references start loading when the containing resource loads. Merely retaining
a `PackedScene` without instantiating it does not defer its ordinary texture dependencies.

## Bevy: Declared Dependencies And Requested Loads

`Asset` requires `VisitAssetDependencies`. The derive implementation visits only fields marked
`#[dependency]`; the visitor implementation for `Handle<A>` reports the handle's asset ID.
This is explicit membership in the asset dependency calculation, not automatic discovery of
every path-like or reference-like field.
Sources: [Asset and handle visitor, lib.rs:444](../../repo-ref/bevy/crates/bevy_asset/src/lib.rs#L444),
[derive field selection, macros/src/lib.rs:53](../../repo-ref/bevy/crates/bevy_asset/macros/src/lib.rs#L53).

`LoadContext::load` has another route into that dependency set. Its nested loader checks
`should_load_dependencies`: ordinary runtime loading calls `load_with_meta_transform`, while a
context configured not to load dependencies acquires a path handle without starting the load.
Either successful index-handle route records the dependency. `AssetServer` starts a load task
when its path-handle request says loading is necessary. At `LoadContext::finish`, the asset's
visitor adds any remaining declared handle dependencies to the accumulated set.
Sources: [nested load implementation, loader_builders.rs:225](../../repo-ref/bevy/crates/bevy_asset/src/loader_builders.rs#L225),
[task initiation, server/mod.rs:392](../../repo-ref/bevy/crates/bevy_asset/src/server/mod.rs#L392),
[dependency union, loader.rs:576](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L576).

Two counterexamples matter for CP3. Omitting `#[dependency]` does not stop a loader from requesting
the resource, and a handle obtained through `LoadContext::load` is already recorded regardless
of that field attribute. The builder's word "deferred" means the caller receives a handle rather
than the loaded value; the ordinary path still starts loading. The actual conditional above is
the evidence for initiation, not that label.

Bevy also distinguishes normal asset dependencies from loader dependencies whose values are
consumed during loading. Direct nested loading awaits the value and records the path/hash in
`loader_dependencies`; the server retains reverse loader-dependency relationships for watching
and reload. These inputs are not a mechanism for a valid future Scene destination that should
remain unrequested.
Sources: [dependency categories, loader.rs:141](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L141),
[direct load, loader.rs:681](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L681),
[reload tracking, server/info.rs:550](../../repo-ref/bevy/crates/bevy_asset/src/server/info.rs#L550).

### Load Readiness Is About The Registered Graph

`process_asset_load` inserts the asset into the World and computes direct and recursive dependency
states separately. It records pending dependants, propagates failures and emits
`LoadedWithDependencies` when the recursive set is complete. An unknown registered dependency
keeps dependency readiness from reaching Loaded. The convenience query
`is_loaded_with_dependencies` requires `LoadState::Loaded`, `DependencyLoadState::Loaded` and
`RecursiveDependencyLoadState::Loaded` together.
Sources: [state calculation, server/info.rs:474](../../repo-ref/bevy/crates/bevy_asset/src/server/info.rs#L474),
[readiness query, server/mod.rs:1161](../../repo-ref/bevy/crates/bevy_asset/src/server/mod.rs#L1161).

This does not establish the validity of an arbitrary unvisited reference or a complete deliverable
catalogue. It checks the graph supplied by the loader and dependency declarations.

### Neither Weakness Nor UUID Identity Supplies Validated Deferred Membership

This pin has `Handle::Strong` and `Handle::Uuid`, not the older `Handle::Weak` variant. Strong
handles keep assets alive. UUID handles neither guarantee a live asset nor keep one alive.
`LoadContext::finish` explicitly skips UUID dependencies because that loading mechanism cannot
load them; the dependency-query implementation also skips UUID IDs. A UUID that avoids a wait
therefore proves neither existence, required deliverable membership nor eventual loadability.
Sources: [handle variants and lifetime, handle.rs:98](../../repo-ref/bevy/crates/bevy_asset/src/handle.rs#L98),
[UUID exclusion, loader.rs:583](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L583),
[query exclusion, server/mod.rs:1180](../../repo-ref/bevy/crates/bevy_asset/src/server/mod.rs#L1180).

## Bevy: A Path's Consumer Determines Its Demand Boundary

The inspected scene implementation uses `Scene`, `ScenePatch` and templates. Older DynamicScene
assumptions are not evidence for this pin. `Scene::register_dependencies` lists typed asset paths
needed for scene resolution. `ScenePatch::load_with` converts these paths through `LoadFromPath`
into load requests and stores the returned handles in a `#[dependency]` collection. The queued
scene system resolves the patch after `LoadedWithDependencies`, then applies the resolved scene.
Sources: [scene contract, scene.rs:49](../../repo-ref/bevy/crates/bevy_scene/src/scene.rs#L49),
[registration and loads, scene_patch.rs:18](../../repo-ref/bevy/crates/bevy_scene/src/scene_patch.rs#L18),
[resolution gate, spawn.rs:588](../../repo-ref/bevy/crates/bevy_scene/src/spawn.rs#L588),
[queued application, spawn.rs:699](../../repo-ref/bevy/crates/bevy_scene/src/spawn.rs#L699).

`CachedSceneAsset` holds an `AssetPath`, but registers it as a `ScenePatch` dependency and resolves
it into a cached scene inclusion. It expresses composition needed for the requested scene;
representing the target as a path does not make that dependency lazy.
Source: [cached scene registration and resolution, scene.rs:409](../../repo-ref/bevy/crates/bevy_scene/src/scene.rs#L409).

`HandleTemplate::Path` has a different boundary: `build_template` calls `AssetServer::load` when
the component template is built. It defers loading until that build, not until some later gameplay
use of the component. A Door component built at A startup can still request B at A startup if its
target uses this conversion. The application code builds templates and later emits scene `Ready`;
there is no intervening general wait for the assets newly requested by those template builds.
Scene readiness therefore must not be generalized into readiness of every component-held asset.
Sources: [template path conversion, handle.rs:320](../../repo-ref/bevy/crates/bevy_asset/src/handle.rs#L320),
[template application, resolved_scene.rs:722](../../repo-ref/bevy/crates/bevy_scene/src/resolved_scene.rs#L722),
[scene Ready emission, resolved_scene.rs:313](../../repo-ref/bevy/crates/bevy_scene/src/resolved_scene.rs#L313).

Likewise, a serialized `HandleReference::Path` is interpreted by `HandleDeserializeProcessor` as
a call to `LoadFromPath`, not as passive path data. Its ordinary `LoadContext` implementation
uses the same nested loader described above. A plain path field can remain inert, but that alone
does not supply reference validation, dependency enumeration or delivery membership.
Sources: [LoadFromPath adapters, reflect.rs:424](../../repo-ref/bevy/crates/bevy_asset/src/reflect.rs#L424),
[handle deserialization, reflect.rs:477](../../repo-ref/bevy/crates/bevy_asset/src/reflect.rs#L477).

### Render Preparation Is Separate And Can Already Be Requested

Render asset extraction reacts to `Added` and `Modified`, checks `RENDER_WORLD` usage and extracts
the asset. This loop does not select assets by current Scene or visibility; its
`LoadedWithDependencies` branch is still a TODO. `prepare_assets` subsequently populates
`RenderAssets`, or defers work because of the upload budget or `RetryNextUpdate`.
Sources: [extraction trigger, render_asset.rs:318](../../repo-ref/bevy/crates/bevy_render/src/render_asset.rs#L318),
[preparation and retries, render_asset.rs:393](../../repo-ref/bevy/crates/bevy_render/src/render_asset.rs#L393).

`GpuImage::prepare_asset` creates or updates native textures. Sprite preparation skips an image
absent from `RenderAssets<GpuImage>`. Thus loaded dependencies and spawned entities do not by
themselves prove render readiness. Conversely, a future texture already loaded with render-world
usage can enter preparation without its future Scene being instantiated. `MAIN_WORLD`-only usage
can exclude an asset from this render path, but does not solve reference validity, CPU loading or
the future demand transition.
Sources: [native texture creation, gpu_image.rs:103](../../repo-ref/bevy/crates/bevy_render/src/texture/gpu_image.rs#L103),
[missing GPU image handling, render/mod.rs:615](../../repo-ref/bevy/crates/bevy_sprite_render/src/render/mod.rs#L615),
[usage flags and default, render_asset.rs:32](../../repo-ref/bevy/crates/bevy_asset/src/render_asset.rs#L32).

## Godot: External Resources Start Loading With The Containing Resource

The text loader walks the `[ext_resource]` header table before parsing resource bodies. It resolves
a recognized UID with path fallback, normalizes relative paths, applies remaps and calls
`ResourceLoader::_load_start` for every entry. When an actual `ExtResource(...)` value is parsed,
`_parse_ext_resource` calls `_load_complete` and returns a Resource reference. The binary loader
likewise starts its external-resource table entries and completes them when resolving external
object indices. Ordinary external resources are not all lazy.
Sources: [text table and path resolution, resource_format_text.cpp:451](../../repo-ref/godot/scene/resources/resource_format_text.cpp#L451),
[text reference resolution, resource_format_text.cpp:128](../../repo-ref/godot/scene/resources/resource_format_text.cpp#L128),
[binary table, resource_format_binary.cpp:618](../../repo-ref/godot/core/io/resource_format_binary.cpp#L618),
[binary reference resolution, resource_format_binary.cpp:423](../../repo-ref/godot/core/io/resource_format_binary.cpp#L423).

`_load_start` can reuse a cached resource; otherwise it executes the load task on the current
thread or submits it to the worker pool according to load mode. Asynchronous loading changes
execution placement, not whether the resource has been requested. All table entries enter this
start/reuse path. This trace does not additionally assert that a parent explicitly waits for
every unused asynchronous header entry; used external references have the completion path above.
Source: [cache reuse and task initiation, resource_loader.cpp:790](../../repo-ref/godot/core/io/resource_loader.cpp#L790).

### Resource Loading Already Enters Native Texture Preparation

For the ordinary compressed 2D texture path, the format loader calls `CompressedTexture2D::load`,
which reads the image and calls `texture_2d_create`. RenderingServer allocates a RID and either
initializes the texture immediately or queues initialization, according to its thread and backend
capabilities. Deferring PackedScene instantiation therefore does not defer that texture creation
request. The inspected streamed-texture path also creates an initial native texture during load;
streamed quality levels are not a zero-preparation future-Scene mechanism.
Sources: [compressed loader, compressed_texture_resource_format.cpp:35](../../repo-ref/godot/scene/resources/compressed_texture_resource_format.cpp#L35),
[compressed texture load, compressed_texture.cpp:143](../../repo-ref/godot/scene/resources/compressed_texture.cpp#L143),
[creation dispatch, rendering_server_default.h:147](../../repo-ref/godot/servers/rendering/rendering_server_default.h#L147),
[2D dispatch binding, rendering_server_default.h:214](../../repo-ref/godot/servers/rendering/rendering_server_default.h#L214),
[initial streamed texture, streamed_texture.cpp:344](../../repo-ref/godot/scene/resources/streamed_texture.cpp#L344).

`THREAD_LOAD_LOADED` is assigned after the resource load task succeeds. Threaded status reports
that task state, while completion can wait for the task. Main-thread progress polling or retrieval
may also synchronize RenderingServer to advance loading. This differs from Bevy's separate main
and render world preparation, but is not a claim that a frame has been presented or GPU execution
has completed. Node `ready` is a tree lifecycle notification, not a native resource readiness gate.
Sources: [load terminal state, resource_loader.cpp:650](../../repo-ref/godot/core/io/resource_loader.cpp#L650),
[status query, resource_loader.cpp:887](../../repo-ref/godot/core/io/resource_loader.cpp#L887),
[completion wait, resource_loader.cpp:1028](../../repo-ref/godot/core/io/resource_loader.cpp#L1028),
[server progress, resource_loader.cpp:1156](../../repo-ref/godot/core/io/resource_loader.cpp#L1156),
[Node ready, node.cpp:323](../../repo-ref/godot/scene/main/node.cpp#L323).

## Godot: Strings And Explicit Placeholders Can Delay The Request

`SceneTree::change_scene_to_file(path)` loads the PackedScene at that call, then delegates to
PackedScene instantiation. A Door can retain a path until this explicit operation instead of
retaining an already loaded PackedScene. The general loader resolves UID strings and project
paths on request. GDScript `@export_file` declares a String with a file hint; `load` calls the
resource loader when executed. In contrast, `preload` requests a non-script resource during
analysis and retains it as a constant. A path-shaped value is not enough to infer delayed demand.
Sources: [scene change, scene_tree.cpp:1719](../../repo-ref/godot/scene/main/scene_tree.cpp#L1719),
[path normalization, resource_loader.cpp:689](../../repo-ref/godot/core/io/resource_loader.cpp#L689),
[exported file String, gdscript_parser.cpp:155](../../repo-ref/godot/modules/gdscript/gdscript_parser.cpp#L155),
[runtime load, gdscript_utility_functions.cpp:234](../../repo-ref/godot/modules/gdscript/gdscript_utility_functions.cpp#L234),
[preload, gdscript_analyzer.cpp:4777](../../repo-ref/godot/modules/gdscript/gdscript_analyzer.cpp#L4777).

The explicit scene mechanism is `instance_placeholder`. Text parsing stores a path and the
placeholder flag. Scene instantiation normally creates an `InstancePlaceholder` carrying that
path; `create_instance` later loads the PackedScene unless the caller supplies one. If
`disable_placeholders` is enabled, scene instantiation loads the target directly instead.
Sources: [placeholder declaration, resource_format_text.cpp:242](../../repo-ref/godot/scene/resources/resource_format_text.cpp#L242),
[instantiation branches, packed_scene.cpp:310](../../repo-ref/godot/scene/resources/packed_scene.cpp#L310),
[explicit target load, instance_placeholder.cpp:74](../../repo-ref/godot/scene/main/instance_placeholder.cpp#L74).

There are two important limits. First, ordinary Resource references in placeholder override
properties still follow the ordinary external-resource load path; a placeholder does not make
all resources in its containing scene lazy. Second, the text loader's `get_dependencies` sets
`ignore_resource_parsing` and enumerates only the external-resource header table. Its loop does
not discover arbitrary path strings or `instance_placeholder` targets in node records. Deferring
through those representations alone therefore does not prove a complete valid-reference or
delivery inventory. This is a limit of the inspected enumerator, not a claim about every Godot
export policy or plugin.
Source: [dependency enumeration, resource_format_text.cpp:917](../../repo-ref/godot/scene/resources/resource_format_text.cpp#L917).

## Constraints For Nara Schema Declarations And Catalogue Edge Roles

The supported inference is to preserve separate answers to separate questions:

- **Reference validity and delivery membership:** an authored or runtime Product target is
  resolvable, has the expected kind, and belongs to the required finite deliverable closure.
- **Current materialization demand:** this operation needs the target now to produce the
  requested Scene or resource result.
- **Loaded state and retention:** the selected asset graph has completed loading, and an owner
  retains the needed data for its promised lifetime.
- **Native readiness:** the selected render consumer's resources have actually been prepared.

For the conceptual chain `A -> Door destination B -> B textures`, the Door target must remain
discoverable and subject to strict target validation without automatically propagating A's
current preparation through B. A Prefab or included Scene needed to materialize A can require
propagation even when it uses the same reference representation or target kind. The field's
consumer semantics matter; ID shape, target type, handle strength and caching do not determine
the edge's role.

Schema reference declarations need to retain enough semantic information to discover and
validate references, including fields intentionally deferred for later gameplay. Concrete
catalogue edges need to preserve the distinction between valid target reachability and current
materialization requirements. The design owner must decide how those facts are declared,
compiled and consumed together; this comparison selects no new enum, public API or schema format.
Replacing a recognized Product reference with an unrecognized string would lose the very
enumeration and validity information CP3 requires unless another explicit declaration supplies it.

Check/Export traversal may validate and include B and C while A's preparation follows only the
selected materialization demand. An explicit destination request or bounded prefetch may later
introduce B's demand under the owning operation. It is insufficient merely to remove B from A's
readiness wait: no independent loader or template conversion may already request B's heavy graph.
Bevy's Added/Modified extraction and Godot's load-time texture creation are concrete reasons to
audit request initiation as well as readiness membership.

This does not select retained-pack backing, encoded-byte eviction, decoded cache policy, texture
streaming, or source-capture availability. Lazy backing cannot by itself shrink a preparation
closure, and selective preparation cannot by itself release eager encoded storage. Those remain
the separate R7 investigation described by the linked plan.

## Frozen Evidence Boundary

The comparison is **source-ready / frozen** for the two exact pins. It concludes constraints and
counterexamples, not adopted Nara architecture, implemented CP3 behavior, a completed foundation
unit or measured performance improvement. Bevy supplies no equivalent Nara Product-validity
guarantee through UUID handles or load-readiness queries; Godot's deferred strings and placeholders
do not automatically preserve complete reference enumeration. The parent owns the Nara contract
audit, design selection, ADR reconciliation, implementation plan and consumer qualification.

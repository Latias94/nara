# Nara Architecture Compared With Bevy And Godot

Status: source review and recommendations; no architecture decision or implementation change.

Implementation sequencing and acceptance are tracked in the
[foundation correction plan](../plans/2026-10-19-1335-refactor-foundation-correction-plan.md).
The findings below remain source-review evidence until linked implementation verification lands.

## Scope And Verdict

Reviewed Nara `835c5e4dd8d278766be0ea2f99709836db5a3b28`, Bevy
`85503941e488152711a4695bbe248e9ed9f453fc`, and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. Reference paths below are relative to
`repo-ref/bevy/` or `repo-ref/godot/`. These are pinned source comparisons, not claims about the
latest published versions or compatibility with their APIs.

Three read-only research lanes examined runtime/components/plugins, assets/scenes/identity, and
editor/rendering. The synthesis checked the principal findings against source and existing tests.
No new Rust reproduction, benchmark, native qualification, or Cargo run was performed. A confirmed
source-level hazard does not establish that an ordinary first-party workflow currently triggers it.

The architectural direction is sound: retain `bevy_ecs`, typed components/resources, startup
plugins, a frozen product definition, one managed execution owner, versioned document semantics,
and domain-owned native reconciliation. There is no evidence for a replacement ECS or a second
universal engine kernel.

The architecture is not yet mature enough to treat every extension path as settled. The most
important gaps are incomplete execution/admission interfaces and product integrations whose
knowledge still spans several owners. Adding many domains on those interfaces would multiply
future migration cost. Correct those interfaces before expanding their consumers.

The product baseline is [STRATEGY.md](../../STRATEGY.md): a Rust-first integrated general-purpose
engine, initially 2D. The supplied pixel-engine planning conversation contributes desired workflows
such as maps, data authoring, transitions and persistence. Its historical claims about other engines,
mandatory storage models, market size or physics choices are not evidence or accepted requirements.

## Comparison

| Area | Nara today | Bevy source model | Godot source model | Assessment |
| --- | --- | --- | --- | --- |
| Simulation | Typed components/resources/systems over `bevy_ecs` | ECS storage, scheduling and change detection | Node/Object composition with SceneTree processing | Keep Nara's ECS substrate; copying Node inheritance would replace a sound choice. |
| Composition and execution | `App` configures; recipe freezes contributions; `RuntimeInstance` owns execution and close | App/Plugin, runners and SubApps | SceneTree/MainLoop plus module and editor extension paths | Single managed ownership fits embedded Editor and reliable retirement; registered custom execution needs repair. |
| Persistent components | Stable owner/type/field IDs, migrations, pure catalog, native codec binding | Reflection/templates and scene resolution in the inspected snapshot | Property metadata, resource and PackedScene serialization | Nara's separation supports data-only Editor recovery; custom field authoring needs a deeper interface. |
| Scenes and identity | Document-local IDs, instance domains, incarnations, tombstones and structured prefab provenance | Resolved scenes, templates and entity-reference remapping | PackedScene state, scene ownership and resource identity | Preserve these distinctions; bind expanded document and provenance before commit. |
| Asset loading | Typed identity, concrete Image/TileSet owners, strict startup closure, retained snapshots | Registered loaders, dependency tracking and strong handles | ResourceFormatLoader, UID/path indexes and Ref resources | Coherent snapshots are valuable; dependency roles, relocation and residency are still finite in Nara. |
| Editor | Document owner -> read-only models/actions -> Dear ImGui frontend | App/sub-app facilities are runtime precedents, not a complete equivalent Editor workflow | EditorPlugin/docks, per-inspector state and undo histories | Semantic ownership is sound; native contribution composition and instance state are incomplete. |
| Rendering | Exact prepared packets, one serialized WGPU owner, ordered views, bounded fragment material | Render SubApp/extraction, specialized material pipelines | RenderingServer, canvas resources and domain-specific rendering | Current model serves the admitted 2D product. Advanced materials and effects will require focused additions. |
| Scaling | Bounded work/retention, measured hierarchy/transform baseline; whole-document authoring preparation | Specialized ECS/render/asset facilities | Operation histories and dirty/quadrant work in TileMapLayer | Bounds are safety properties, not latency guarantees. Measure real authoring/map workloads before changing algorithms. |

Reference entry points: Bevy `crates/bevy_app/src/app.rs:59`,
`crates/bevy_app/src/plugin.rs:58`, `crates/bevy_scene/src/resolved_scene.rs:20`,
`crates/bevy_asset/src/loader.rs:141`, `crates/bevy_asset/src/handle.rs:115`, and
`crates/bevy_sprite_render/src/mesh2d/material.rs:148`; Godot `scene/main/node.h:54`,
`scene/main/scene_tree.h:92`, `core/io/resource_loader.h:48`,
`scene/resources/packed_scene.cpp:222`, and `editor/plugins/editor_plugin.h:205`.

## Findings And Refactor Order

P1 means a current interface or admission hazard to address before broadening foundation consumers.
P2 means a concrete extension risk to address before its named workflow. These are sequencing
recommendations, not claims that every P2 is a currently failing supported feature.

### A01 — P1: Scene Provenance Is Applied After Commit

**Classification:** confirmed source-level API hazard; high confidence.

`crates/nara_scene/src/spawn.rs:1582` accepts a document and a separately mutable provenance map.
It spawns first, then `apply_scene_provenance` at `1597` resolves each map key and panics at `1607`
if a key is absent. The replacement entry at `1038` also applies provenance after the spawn/replace
transaction returns at `1064`. Identity replacement and old-entity retirement can already have
committed. Earlier entries in the map may already have been applied before a later key fails.

`SceneEntityProvenanceMap` is a public `BTreeMap` (`provenance.rs:7`), and
`PrefabExpansionReport` exposes document and provenance separately (`prefab.rs:457`). Private
constructors on map values do not prevent rekeying an existing value or retaining a map after
editing its associated document. An asynchronous expansion consumer or an external caller can
therefore supply stale, mismatched inputs through safe public APIs.

**Direction:** make a validated expanded-scene candidate carry document and provenance together.
Check key membership, source association and the required coverage before entity, identity or
hierarchy mutation; include provenance installation in the existing prepared commit. Define
intentional partial/empty-map semantics explicitly. Changing the panic to a silent skip would leave
incorrect source identity and would not fix the owning contract.

**Reference:** Bevy `crates/bevy_scene/src/resolved_scene.rs:20` packages a resolved spawn input;
its `spawn` at `49` returns an error and removes its intermediate root on failure. This is useful
precedent for a complete input and explicit failure, not proof of arbitrary native rollback.

**Acceptance:** a stale/rekeyed map rejects before allocation; failed replacement retains the old
instance, identities and provenance; valid nested-prefab source paths remain unchanged.

### A02 — P1: Registered Custom Schedules Have No Managed Execution Interface

**Classification:** confirmed execution capability gap; high confidence.

`crates/nara_app/src/lib.rs:1248` accepts arbitrary labels through `add_systems`; `1285` initializes
custom schedules. After freeze, schedules live in private `RuntimeOwnerState.schedules`
(`runtime.rs:2824`), outside the World. Managed execution drives startup labels, `CoreStage::ALL`
and the private cleanup schedule (`crates/nara_app/src/lib.rs:1811`, `1881`). No public path drives an installed custom
schedule after the App is consumed.

The existing `crates/nara_app/tests/schedule_registry.rs:25` test proves registration and inertness,
not execution. ADR 0003 intentionally keeps automatic frame order closed while describing custom
schedules as explicitly owner-driven. A domain such as an optional pathfinding simulation or
manually advanced combat schedule currently cannot complete that registered-schedule workflow.
Owning an unrelated standalone Bevy Schedule is possible, but does not drive the registered instance.

**Direction:** add a narrow typed execution request at an existing managed safe point, or another
concrete owner-driver interface. Preserve pause/step policy, runtime fault handling, deferred-work
semantics and close authority. Do not expose mutable schedule storage or reopen raw App execution.
Choose whether requests originate from a Host port or a domain system with a real consumer.

**Reference:** Bevy `crates/bevy_ecs/src/world/mod.rs:3981` and `3999` expose explicit schedule
execution for schedules stored in World. Nara needs the equivalent capability at its own owner,
not a blind call into an unrelated World registry.

**Acceptance:** a custom schedule runs exactly when requested through a public consumer; an
unrequested schedule stays inert; missing labels, stopped/faulted owners and invalid graphs report
defined outcomes without bypassing lifecycle checks.

### A03 — P1: Known Schedule Graph Errors Can Escape The Fallible Runtime Path

**Classification:** confirmed source-level admission robustness gap; high confidence.

`validate_public_schedule_compatibility` (`crates/nara_app/src/lib.rs:1550`) initializes only
FixedUpdate and Cleanup. This satisfies the specifically documented anchor-validation inventory,
but leaves other executable startup/frame graphs unbuilt. Their first execution reaches
`run_managed_schedule` at `1837`, which invokes `Schedule::run`.

In the pinned Bevy substrate, `crates/bevy_ecs/src/schedule/schedule.rs:596` calls
`initialize(...).unwrap_or_else(panic)`. A cycle in an Update or startup graph can therefore pass
configuration and produce a panic instead of a structured construction/drive error. This is a
predictable graph configuration error; it is distinct from the broad panic recovery deliberately
excluded by ADR 0084.

**Direction:** fallibly build every graph Nara will execute before startup publication, while
retaining the additional semantic checks for public anchors. Build custom graphs before their
first admitted execution. Retain cleanup ownership on failed admission. Reuse Bevy's graph builder;
do not implement another graph validator or make global panic catching the primary fix.

**Acceptance:** cycles in Startup, Update and a requested custom schedule yield typed diagnostics;
no candidate is published, and no prior runtime is retired merely because a successor graph is bad.

### A04 — P2: Asset References Do Not Yet Express Dependency Roles

**Classification:** confirmed extension limitation; future workflow risk, not a current closure bug.

`ComponentValueShape::AssetRef` (`crates/nara_reflect/src/schema.rs:501`) and
`DeclaredAssetReference` (`asset_reference.rs:20`) carry identity and location without a loading
role. `src/project_content.rs:1551` collects all declared effective references and at `1573`
admits only Image and TileSet source kinds. Existing nested/default traversal and codec kind
validation are strengths; unsupported kinds fail explicitly.

**Trigger:** a real `Portal.target_scene` that must remain addressable without eager activation,
or a third file-backed domain such as Font/Audio. Treating every such reference as one startup hard
edge would either reject valid future content or impose unwanted loading. Adding roles after many
domains ship would affect catalogs, traversal, closure checks, packaging and editor choices.

**Direction:** use that consumer to define the minimum dependency roles and typed source constraints.
Separate discovery, build/read dependencies, residency demand and activation where their semantics
actually differ. Do not add every hypothetical role or a universal importer in advance.

At the same extension point, deepen two already demonstrated interfaces:

- `TileSetAssets::request/reconcile` (`crates/nara_tilemap/src/content.rs:174`, `361`) exposes the
  multi-resource join in both `src/project_host/runtime.rs:1126` and Editor
  `authoring_runtime/tilesets.rs:208`. Hide that concrete join behind one domain-owned interface.
- Cold TileSet loading (`src/project_content.rs:1762`) and Editor source refresh
  (`image_import/transaction.rs:1162`) repeat capability-bound identity/kind/digest/read/decode
  admission. Share private admission facts and a receipt before another domain duplicates them;
  keep strict closure scheduling and incremental Editor scheduling separate.

**Reference:** Bevy `crates/bevy_asset/src/loader.rs:141` distinguishes handle dependencies from
dependencies whose values were read by a loader. It supports distinguishing edge meaning, not
copying Bevy's entire executor or promising whole dependency-bundle atomicity.

**Acceptance:** one genuine soft reference and one additional concrete asset domain work through
Check, Editor and Export without adding independent interpretations of the same admission facts.

### A05 — P2: Live Asset Relocation Needs An Explicit Owner Transaction

**Classification:** missing live-session capability; high confidence.

`AssetServer::reserve_record_id` (`crates/nara_asset/src/server.rs:171`) finds an existing handle
through Stable ID, then `validate_path_binding` at `263` rejects a different path. There is no
explicit relocation operation. This is sound conflict handling for reservation, but insufficient
for moving an asset inside a running Editor session.

The rename test at `src/project_host/runtime/tests.rs:2698` opens a fresh Editor for each path at
`2755`. It proves stable references after reopen, not live relocation. A future browser rename
must also handle active import tasks, retained snapshots, source metadata and derived dependencies.

**Direction:** before adding live move/rename UI, define Host-owned relocation with admitted old/new
directory facts, stable identity preservation, locator updates, stale-work cancellation and renewed
publication. Keep `reserve` rejecting accidental rebinding. Specify the outcome if the filesystem
move succeeds but subsequent runtime admission fails.

**Reference:** Godot `core/io/resource_uid.cpp:169` explicitly updates UID-to-path indexing;
`core/io/resource.cpp:79` updates resource path/cache ownership. This demonstrates controlled
relocation rather than silently weakening identity checks.

**Acceptance:** one session moves a shared Image while Sprite and TileSet consumers exist; old
path completions cannot republish, the new path recovers, and collision/failure preserves clear
ownership and diagnostics.

### A06 — P2: Editor Panel Extensions Lack Owned State And Native Composition

**Classification:** concrete extension limitation; no claim of a current built-in panel failure.

`ImguiEditorPanelPainter` is a function pointer (`editor/src/frontend/imgui/workspace.rs:18`).
Its public context exposes only `model()` and `emit()` at `30` and `34`. Built-in Inspector and
Canvas drafts live in private per-instance `ImguiPanelWidgets`; the independent panel sample is
stateless. Toolkit storage or global maps are possible workarounds, but Nara supplies no extension
owner for ordinary Rust draft state across retargeting, close, reset and graphics reconstruction.

The native path also creates a default application (`editor/src/platform.rs:60`) and default
frontend (`platform/imgui.rs:449`). The custom-application runner at `76` is private. The external
sample's separate workspace/painter installation has not proven a contribution through ordinary
native startup and reconstruction.

**Trigger:** a source-built curve, animation or Tile palette panel with a per-instance draft and
preview. Repeating built-in-only state fields and Host edits for each tool would make the plugin
surface increasingly shallow.

**Direction:** prove one concrete composition carrying panel descriptors and selected-frontend
companions through native startup/rebuild, with an owned per-instance companion/factory and finite
lifecycle. Migrate a built-in stateful panel and one external draft panel through it. Keep document
changes on existing actions; do not put drafts in the runtime World or invent a universal widget
tree, dynamic ABI or new Editor kernel. `nara-editor` remains a private product.

**Reference:** Godot `editor/plugins/editor_plugin.h:205` exposes edit, visibility, state and clear
lifecycle; `editor/plugins/editor_plugin.cpp:135` installs a real owned dock into the Editor.

**Acceptance:** two instances have separate drafts; closing one cleans only its state; retarget,
workspace reset and GPU reconstruction preserve or retire state according to the declared policy.

### A07 — P2: Small Edits Still Prepare Whole Documents

**Classification:** confirmed algorithmic cost; no measured interactive latency regression.

Native patch preparation clones a full document (`crates/nara_scene/src/patch.rs:267`, `437`) and
accounts/validates full candidates per operation (`1276`). The ordinary data-only Editor path is
different: `authoring_decode.rs:2068` clones a Scene once and `2078` classifies its complete
candidate; Prefab preparation at `2139`/`2142`/`2153` also materializes full candidates. It would be
incorrect to attribute every native per-operation validation to every data-only field edit.

UI gesture preview (`src/project_host/runtime/editor/ui_authoring.rs:235`, `277`) invalidates its
stamp-keyed preview, constructs a temporary session, applies the patch and builds another
projection. A compact brush/curve command alone will not reduce that preparation cost. Existing
history stores bounded inverse patches; replacing the undo model is not justified by these facts.

**Trigger:** large saved documents, batch property editing or continuous Canvas/brush previews.
First measure prepare -> validate -> project -> present, allocations and retained history on that
real workflow. The hierarchy/transform benchmark does not measure this path.

**Direction:** if the budget is exceeded, use private touched-record/copy-on-write candidates and
exact revision/catalog baseline reuse. Preserve atomic revision publication, unavailable-record
manifests, inverse bounds and the full validator as an oracle. Do not remove validation simply
because the command payload is small.

**Reference:** Godot `core/object/undo_redo.h:53`, `undo_redo.cpp:346` and
`editor/editor_undo_redo_manager.cpp:246` show operation-based histories and merge policies.
Their effect replay does not provide Nara's failure atomicity, so copying that mechanism wholesale
would exchange guarantees rather than automatically improve the design.

### A08 — P2: Residency, Scene Travel And Saves Need Their Own Workflow Contract

**Classification:** future capability boundary, not a startup leak or broken save implementation.

`ProjectContentSnapshot` retains a full startup closure (`src/project_content.rs:195`). At `1138`
one runtime lease is created, and at `1144` all Images share a retention owner for that lease.
One surviving Image can therefore retain the whole closure's budget charge. This is conservative
accounting; it does not prove all other assets remain physically resident.

`Handle<T>` is a Copy identity (`crates/nara_asset/src/storage.rs:28`), not a residency lease.
Scene export selects authored scene identity (`crates/nara_scene/src/export.rs:194`); it is not a
dynamic-world save format. The existing identity domains, incarnations, tombstones and structured
prefab paths are useful foundations and should remain.

**Trigger:** two regions sharing one Image with independent unload, scene travel with retained
services, or persistence of runtime-created entities and several instances of one authored scene.
If each game/domain invents its own acquisition and restore rules first, later unification will
change gameplay APIs and saved data across those consumers.

**Direction:** prove minimal region acquisition/release using two regions and a shared Image.
Then define a real save workflow with selected components/resources, persistent dynamic identity,
authored-instance bindings, tombstones, compatibility and service reconstruction. Do not persist
raw Entity values or generated prefab expansion locators. ADR 0089 remains Proposed; this audit
does not accept it. Scene travel, residency and save semantics are related but remain separate
owners rather than one giant manager.

**Reference:** Bevy `crates/bevy_asset/src/handle.rs:81`, `89`, `115` has strong ownership and
drop-based lifetime signaling. Godot `scene/resources/packed_scene.cpp:873`, `1147`, `1471`
uses scene ownership while packing. Neither proves a generic whole-World save or automatic native
service restoration.

### A09 — P2: Custom Persistent Field Authoring Has A Manual-Codec Cliff

**Classification:** developer-experience and schema-expression limitation, not data loss.

`PersistentFieldCodec` is sealed (`crates/nara_reflect/src/authoring.rs:26`), and
`ComponentValueShape` has no enum/choice variant (`schema.rs:501`). The documented derive supports
finite scalar/reference/container types; custom newtypes and enums require an explicit component
schema/codec rather than a reusable external field codec. Manual codecs already support external
custom scalar components. They are a working escape, not an unsupported engine capability.

**Trigger:** several game components share a domain ID, bounded scalar or closed enum, and designers
need appropriate generic Inspector choices. String fields alone cannot express a closed option set;
independent manual schemas and whole-component codecs repeat knowledge.

**Direction:** use a real repeated field type to design an explicit reusable field-adapter/derive
interface; define wire shape, validation, creation values and migration ownership together. Add
choice metadata only when its stable identity/version and unknown-value semantics are clear.
Keep data-only Editor catalogs independent from running project Rust. Avoid opening an arbitrary
callback mechanism inside generic data validation.

Module authors also maintain parallel pure catalog and native registration lists
(`crates/nara_reflect/src/provider.rs:442`; `docs/guides/persistent-components.md:48`). Existing
receipt/freeze checks reject disagreement. After the field interface settles, one typed module
declaration could generate both lists while keeping the catalog source pure; this is lower-priority
ergonomic work, not a reason to remove the checks or recreate separate product registration lists.

**Reference:** Bevy `crates/bevy_reflect/src/enums/enum_trait.rs:153` preserves queryable variants;
Godot `core/object/property_info.h:43` carries property hint metadata. Neither supplies Nara's
stable-ID migration rules automatically. Compare the complete author workflow rather than counting
supported field variants.

**Acceptance:** the same external domain field type is reused by two persistent components and
survives derive/manual composition, Inspector edits, Save/Reopen, strict Check and a version change.

## Triggered Extensions To Keep Separate

| Capability | Current limit | Trigger and minimum next proof |
| --- | --- | --- |
| Concurrent document inspection | `nara_tooling/src/workspace.rs:857` builds one complete active-scene model; `editor/src/frontend/imgui/mod.rs:536` asks inactive Source panels to activate | When two pinned inspectors/views are needed, project requested document IDs lazily; do not copy every open document each frame. Multiple panel identity alone is not multi-document content projection. |
| Advanced 2D materials/effects | `nara_material/src/lib.rs:77` admits one fragment input, one parameter vector and one additional texture; materials are transient startup declarations | Reusable authored materials, lighting, normal maps, water or history effects need parameter/binding schemas and explicit target dependencies. Use one real effect to reopen OQ-002/render execution; a render graph or second render World is not required merely because Bevy has one. |
| Semantic tile authoring | Atlas-local TileIndex and accepted-layout freeze prevent silent live reinterpretation; no durable tile definition contract | Before layout editing, tile metadata or Tiled import, choose identity/remapping and coordinate-based transactions. Keep gameplay occupancy/state separate from render cells. Tile tools remain deferred. |
| Physics/audio/animation | Concrete native reconciliation and ownership have a finite fixture; complete production domains are not proved | A first actual plugin must define transform/parameter write authority, scheduling, freshness and cleanup. Missing a domain is not evidence that the ECS or component/plugin split is wrong. |
| Large maps and render culling | Current extraction visits cells and projects per view; existing hierarchy measurements cover a different workload | Measure a saved/reopened map and active views. Borrow Godot's dirty/quadrant approach only after attribution identifies the required cache/culling granularity. |

## Recommended Sequence

1. **Close the current execution and Scene input contracts:** A01-A03, each with one external
   public consumer and focused rejection/retirement regressions. No new engine kernel is needed.
2. **Before another asset domain:** deepen the concrete TileSet/resource join and private source
   admission. Use the first real soft-reference/domain workflow to settle A04; keep A05 relocation
   ahead of live move/rename UI rather than weakening identity reservation.
3. **Before more independent Editor tools:** implement A06 using built-in and external stateful
   native panels. Settle A09 with a repeated game field type, preserving the existing manual path.
4. **Before Tile tools or large authoring documents:** measure A07 and the real map workload.
   Implement only the owning storage/projection changes justified by that measurement.
5. **Before persistent world progression:** prove A08 with scene travel, two region scopes and a
   dynamic save entity. Add effects/physics/animation when a real game supplies their write and
   lifetime contracts.

The first three findings are the immediate correctness/completeness work. The remaining items are
explicit dependency gates for feature work, not an instruction to build all future infrastructure
before another playable game. A successful refactor reduces caller knowledge and affected files;
more traits, crates or ADRs alone do not demonstrate progress.

## Designs To Preserve And Review Limits

- Keep components as data, resources as domain state and plugins as composition. Do not turn an
  entity into a plugin or copy Godot's Node callback inheritance into the ECS.
- Keep one immutable executable composition and one managed Runtime lifecycle. Structural Rust
  changes can reconstruct a candidate; native hot-unload and a stable dynamic ABI are separate work.
- Keep durable schema identity separate from Rust type identity and runtime native bindings.
  Lifecycle-free persistent transactions deliberately reject hooks/RequiredComponents; transient
  native reconciliation is the admitted alternative, not a global ban on Bevy component features.
- Keep exact Image/TileSet snapshots, desired versus published dependency state, explicit revocation,
  bounded retained holders and backend submission lifetime. These solved issues are not reopened by
  the comparison.
- Keep document transactions, bounded inverse history and read-only frontend models/actions.
  Renderer/toolkit types remain in adapters. Optional feature edges are not unconditional leaks.
- Keep the measured hierarchy baseline. The existing 4,096-member measurements do not justify
  rewriting it again or establish a whole-game performance budget.

This is static architecture review at one Nara commit. The listed acceptance cases are proposed
future verification, not tests run during this review. Previous TileSet/native results retain
their original platform and toolchain limits. Existing untracked `myeasylog.log` and `vendor/`
were left untouched. No Accepted ADR was changed.

The standalone HTML report is outside the repository at
`D:\Frankorz\Downloads\nara-bevy-godot-architecture-review-2026-10-19.html`.

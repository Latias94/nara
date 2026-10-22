# Low-Level Contract Mechanisms: Pinned Bevy And Godot Sources

Date: 2026-10-23. Nara baseline: `74cbe3ba5586944bf24c60c11c06f441ae001e6b`.
Bevy: `85503941e488152711a4695bbe248e9ed9f453fc`; Godot: `81ab961f91066a63bcbf3ec340962bb1a1ada558`.
Both read-only local checkout HEADs matched these pins and had clean worktrees.

Supports [review](../audits/2026-10-22-low-level-module-and-interface-review.md) L03/L04/E01/E02/E04/E06 and [Strategy](../../STRATEGY.md).
Recommendations are Nara inferences from fixed-commit mechanisms,
not current-version claims, correctness verdicts or equivalent product guarantees.
Source IDs link to local files; the source index also supplies immutable public links.

## Evidence Matrix

| Boundary | Source-confirmed mechanism | Cost / limit | Nara implication |
|---|---|---|---|
| Bevy global transforms | Ordered propagation in PostStartup/PostUpdate; local/relation changes, added globals and orphaning feed updates. [B1], [B2] | Stored globals depend on propagation timing. | Own invalidation and the consumer window together. |
| Bevy temporal reads | TransformHelper recomputes from current local ancestors. [B3] | Each call traverses ancestors; it does not publish the stored component. | Offer a deliberate fresh-read path only where a consumer needs it. |
| Bevy fallible schedules | A returning handler allows executor completion bookkeeping and deferred application. [B4], [B5] | Ordering does not encode success or undo effects. | Gate domain acknowledgement on accepted results. |
| Bevy typed registration | Generic type-data construction and typed loaders precede erased dispatch. [B6], [B7], [B8] | Erased and custom entrypoints retain runtime obligations. | Preserve type relationships through public preparation. |
| Bevy task custody | Scope awaits tasks/cancellation; executor tests retain completed outputs until handle drop. [B9], [B10] | Scope may block; the output and the running future have different owners. | Specify result publication, execution return and destruction separately. |
| Godot transforms | Setters invalidate descendant caches; getters refresh invalid globals. [G1], [G2] | Setter traversal, ancestor reads and notifications remain distinct work. | Borrow owner-controlled freshness, not an assumed universal dirty flag. |
| Godot nodes | Detach, reparent, serialization owner and queued deletion are distinct operations. [G4], [G5], [G6] | Structural edits can execute lifecycle callbacks; deletion follows child links. | Keep topology, authored membership and retirement policies explicit. |
| Godot resource reload | Load candidate, then copy storage properties into the receiver; external cache mode is separate. [G7], [G8], [G9] | In-place copying is not a general atomic resource-graph replacement. | Specify revision and publication authority independently from caching. |

## 1. Bevy: Change Propagation Has A Time Boundary

TransformPlugin chains `mark_dirty_trees`, `propagate_parent_transforms` and
`sync_simple_transforms` in `TransformSystems::Propagate` in both PostStartup and PostUpdate.
With `bevy-support`, Transform requires GlobalTransform and TransformTreeChanged. [B1], [B11]

The dirty query includes `Changed<Transform>`, `Changed<ChildOf>` and `Added<GlobalTransform>`,
plus removed ChildOf records. The serial dirty pass marks ancestors. The simple-transform pass
updates changed roots without children and handles newly orphaned entities. The serial hierarchy
pass combines ancestor/local/reparent changes; the parallel pass uses subtree dirtiness and
parent-global change detection, with `set_if_neq` when publishing descendants. These are distinct
feature-selected implementations, not one identical algorithm across builds. [B2]

Static optimization is switchable: dirtiness tracking, queue work and equality checks have costs;
serial propagation remains recursive. Neither relative speed nor arbitrary-depth safety was measured. [B2]

Stored GlobalTransform can lag a local write after propagation. TransformHelper instead multiplies
current locals along ancestors, returning errors for missing transforms/entities; it does not
update the cached component. [B3], [B12]

**Borrow:** define a complete consumer interval. For L03/L04, revoke completion on supported
premise-changing mutations, including derived-component replacement, and check it at extraction.
Old generation agreement or a schedule edge cannot prove freshness after subsequent writes.
Bevy's representation does not itself prove repair of external GlobalTransform replacement;
that behavior was not tested. [B12]

## 2. Bevy: Error Routing Does Not Make A Success Chain

`chain()` adds ordering edges and normally inserts ApplyDeferred where a predecessor has deferred
parameters. `chain_ignore_deferred()` removes that automatic synchronization behavior. Neither API
expresses a dependency on a successful domain result. [B4]

In the single-threaded executor, a failed system invokes the error handler; if it returns, the
executor records unapplied work and continues the loop. It can apply deferred work at a sync point
and at schedule end. In the multithreaded executor, a returning handler for a system failure yields
an Ok unwind result; completion marks unapplied work and signals dependents. This is a concrete
continuation mechanism, not an assumption based on the name of the handler. [B5]

A panicking handler follows different unwind paths. Neither path supplies a snapshot/restore
protocol to undo mutations already made. [B5]

Nara's [fault route](../../crates/nara_app/src/runtime/fault_route.rs#L308) records a sticky fault;
[run_managed_schedule](../../crates/nara_app/src/lib.rs#L1901) checks after execution. This supports
E06's timing concern, without reproducing Gameplay acknowledgement failure.

**Borrow:** make success-sensitive Consume/Capture/Acknowledge one fallible operation or use
domain-owned accepted-operation state. That adds transition bookkeeping. Qualify later systems and
deferred effects after an injected Consume failure before selecting an acknowledgement policy.

## 3. Bevy: Retain Types Before Erased Dispatch

TypeRegistry stores TypeRegistrations by TypeId, while `register<T>` obtains registration and
dependencies from T. `register_type_data<T, D>` requires `D: CreateTypeData<T>` and creates the data
for that selected T before inserting it. ReflectComponent's normal construction specializes its
function table for C, including typed component access, insertion, removal and mapping. [B6], [B7]

AssetLoader associates Asset/Settings/Error types. Its ErasedAssetLoader adapter downcasts Settings,
calls the typed loader, then erases the result. The downcast uses `expect`: erased pairing remains
a runtime invariant. Public erased loaders and custom ReflectComponent function tables also exist;
not every registration path is statically constrained. [B7], [B8]

**Borrow:** for Nara E01, make the ordinary preparation result retain its registered component
type and give encoding the already selected `&T`, with read-only context where needed. Erase in
storage/adapters after those relationships are fixed. The current
[advanced codec boundary](../../crates/nara_reflect/src/registry.rs#L1232) accepts an untyped
PreparedComponentCandidate and World/Entity encoder, so callers reconstruct knowledge it owns.

The cost is specialization and adapter code alongside dynamic checks. Types cannot prove codec
semantics, migrations, closure behavior or lifecycle-free application. Retain those checks without
building a closure analyzer or replacing Nara's persistent schema with Bevy reflection.

## 4. Bevy: Completion, Output Ownership And Drop Differ

Bevy reexports dependency-owned `async_task::Task`. Multithreaded spawn permits detaching result
observation; Scope collects results before returning and drains remaining tasks with `cancel().await`
on drop. TaskPool drop closes shutdown and joins workers. These boundaries differ. [B9]

Built-in edge executor tests `drop_executor_and_then_drop_finished_task` and
`drop_finished_task_and_then_drop_executor` return CallOnDrop from a task. Its counter remains zero
after execution and increments on handle drop, even after executor drop. These are alternate-backend
source test oracles, not tests executed in this research. [B10]

This does not establish `is_finished`/capture-disposal ordering for async-task, async-executor,
wasm, non-yielding work or blocking destructors. Dependency-level disposal internals were not
inspected; the scoped cancellation call site alone cannot prove their ordering. [B9], [B10]

**Borrow:** expose an executor-authored per-task return/destruction receipt only for work the
executor actually owns. Nara already documents
[first-terminal publication separately from physical retirement](../../crates/nara_tasks/src/runtime.rs#L896).
Extend that seam for Scene and Browser. Receipt bookkeeping costs work; a blocking join cannot
serve an owner thread that must keep rendering. Transferred results and escaped clones need
separate lifetime accounting beyond executor completion.

## 5. Godot: Setters Own Dirtiness; Reads Own Cache Repair

Node2D::set_position updates/submits the local transform and calls `_notify_transform`.
`set_transform` stores/submits the matrix and dirties decomposed locals before notifying.
That decomposition flag differs from CanvasItem's global-invalid flag. [G1]

CanvasItem::_notify_transform marks the node's global cache invalid and recursively visits canvas
children except top-level children; an already-invalid node short-circuits. Under its read-thread
guard, `get_global_transform` checks invalidity, recursively obtains its parent item's global,
multiplies by the local matrix, caches the result and clears invalidity. Tree entry/exit also
invalidates the global cache. Top-level CanvasItems have no transform parent item. [G2]

Cache invalidation is separate from change notification: notification flags and list membership
control queued delivery. `force_update_transform` removes a queued entry and emits the transform
notification; it is not the getter's matrix-recomputation implementation. Node3D has a related but
different dirty-mask path, and its global getter explicitly requires being inside the tree. Do not
generalize the 2D getter to detached 3D nodes or every engine subsystem. [G2], [G3]

**Borrow:** put invalidation and trustworthy reads behind one owner. Descendant invalidation,
recursive dirty reads and queued effects cost work. Nara's ECS/fixed/extract policy needs measurement;
lazy reads do not prove whole-forest validation, finite matrices or interpolation/completion guarantees.

## 6. Godot: Structure, Lifecycle And Resource Reload Are Not One Protocol

Node::remove_child disconnects tree membership and parent links, sends notifications and retains
the child object. `_set_tree` enters/exits through callback-bearing paths; reparent performs remove
then add and conditionally restores serialization owners. Node2D's keep-global option captures the
global transform and reapplies it after reparenting. This is operation-specific preservation, not
evidence of lifecycle-free mutation or failure-atomic reparenting. [G1], [G4]

Node::owner must be an ancestor and participates in PackedScene's selection of nodes to serialize.
It is distinct from structural parentage and destruction: PREDELETE removes the node from its
parent and deletes children, while queue_free requests SceneTree deletion and a later queue flush
performs memdelete. A detach therefore differs from a delete request and from actual disposal.
Bevy's canonical Children also deliberately uses linked_spawn for descendant despawning; neither
default determines Nara's explicit structural-versus-lifetime policy. [G4], [G5], [G6], [B13]

Resource::reload_from_file loads with CACHE_MODE_IGNORE and returns before copying if loading
returns null. Otherwise copy_from checks matching classes, blocks/coalesces `changed` emission,
calls reset_state, then sets storage properties individually except resource_path. It returns OK
after the loop without checking each set result; reload_from_file does not return that status.
Signal coalescing is not a general rollback or cross-resource atomicity mechanism. [G7]

In the text loader, ordinary IGNORE/REUSE/REPLACE modes use REUSE for external resources; DEEP modes
propagate separate external behavior. Same-class cached subresources in REPLACE mode can be reset
and reused. The general loader also preserves an existing cached instance and calls copy_from
when replacing a different loaded instance. These are identity/cache choices, not proof that every
loader or dependency graph receives an old-or-new publication. [G8], [G9]

**Borrow:** specify preservation, revocation and disposal per operation. In-place reload preserves
references but mutates properties sequentially; immutable publication costs staging/ownership for
stronger admission. L05/E03 still need desired recipe, last-good value, current attempt and captured
store/revision baseline. These Godot paths do not prove those races solved.

## Research Gaps And Acceptance Limits

- Source inspection only: no Cargo, execution, benchmarks or probes; no bug-free, speed or release claim.
- Bevy cfg branches are identified, not exhaustively qualified. Task dependency internals and wasm
  disposal ordering remain outside the inspected pins; a concrete Nara receipt needs its own tests.
- Raw global replacement, hierarchy mutation after projection, zero/multiple fixed steps and actual
  Sprite/camera extraction need Nara regression oracles; source similarity does not close L03/L04/E04.
- Godot reload coverage is Resource plus general/text-loader cache paths. Imported/binary resources,
  editor reimport/watchers, subclass setters and whole-graph failure recovery were not audited.
- L01/L02/L06/L07/L08/L09/L10 receive no new upstream comparative evidence here.
- Recommendations do not change Accepted ADRs. In particular, preserve
  [ADR 0117's separation of persistent transactions and native reconciliation](../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md#persistent-data-and-native-effects-have-different-transaction-semantics).

## Pinned Source Index

Local and public references resolve at the pins above; no moving-branch documentation is evidence.

| Local source | Immutable public source |
|---|---|
| [B1] Transform plugin | [Bevy plugin](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_transform/src/plugins.rs#L22-L47) |
| [B2] Propagation implementations | [Bevy systems](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_transform/src/systems.rs#L38-L734) |
| [B3] Fresh transform helper | [Bevy helper](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_transform/src/helper.rs#L14-L64) |
| [B4] Schedule chains | [Bevy chain](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/schedule/config.rs#L558-L575) |
| [B5] Executor continuation | [Single-threaded](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/schedule/executor/single_threaded.rs#L125-L166), [multithreaded completion](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/schedule/executor/multi_threaded.rs#L732-L760), [returning error handler](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/schedule/executor/multi_threaded.rs#L850-L882) |
| [B6] Typed registry | [Bevy registry](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_reflect/src/type_registry.rs#L202-L377) |
| [B7] Reflected component table | [Bevy component reflection](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/reflect/component.rs#L76-L406) |
| [B8] Typed/erased asset loaders | [Bevy loader](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/loader.rs#L26-L132) |
| [B9] Scope/spawn/drop | [Bevy pool](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/task_pool.rs#L234-L706), [Task reexport](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/lib.rs#L78-L104) |
| [B10] Completed-output drop tests | [Bevy edge executor tests](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/edge_executor.rs#L568-L624), [backend selection](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/executor.rs#L17-L24) |
| [B11] Required transform components | [Bevy Transform](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_transform/src/components/transform.rs#L69-L86) |
| [B12] Stored global timing/representation | [Bevy GlobalTransform](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_transform/src/components/global_transform.rs#L19-L114) |
| [B13] Structural/lifetime default | [Bevy hierarchy](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/hierarchy.rs#L28-L152) |
| [G1] Node2D mutation/reparent | [Godot Node2D](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/2d/node_2d.cpp#L119-L395) |
| [G2] Global cache/notifications | [Godot getter](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/canvas_item.cpp#L201-L245), [tree transitions](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/canvas_item.cpp#L372-L518), [invalidation](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/canvas_item.cpp#L1153-L1193), [forced notification](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/canvas_item.cpp#L1373-L1383), [transform parent](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/canvas_item.cpp#L634-L641) |
| [G3] 3D applicability limit | [Godot Node3D](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/3d/node_3d.cpp#L640-L669), [invalidation](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/3d/node_3d.cpp#L112-L140) |
| [G4] Node structure/lifecycle | [Godot detach](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/node.cpp#L1747-L1802), [reparent](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/node.cpp#L2072-L2116), [tree transition](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/node.cpp#L3411-L3435), [callbacks/deletion](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/node.cpp#L282-L431) |
| [G5] Serialization owner | [Godot owner](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/node.cpp#L2303-L2325), [PackedScene selection](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L873-L903) |
| [G6] Deletion request/flush | [Godot queue_free](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/node.cpp#L3518-L3527), [SceneTree deletion](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/scene_tree.cpp#L1646-L1661) |
| [G7] Resource copying/reload | [Godot copy/reload](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/io/resource.cpp#L230-L275), [changed coalescing](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/io/resource.cpp#L49-L73) |
| [G8] Text-resource cache policy | [Godot subresources](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/resource_format_text.cpp#L550-L615), [external cache modes](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/resource_format_text.cpp#L1406-L1423) |
| [G9] General cached-instance replacement | [Godot loader](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/io/resource_loader.cpp#L559-L603) |

[B1]: ../../repo-ref/bevy/crates/bevy_transform/src/plugins.rs#L22
[B2]: ../../repo-ref/bevy/crates/bevy_transform/src/systems.rs#L38
[B3]: ../../repo-ref/bevy/crates/bevy_transform/src/helper.rs#L14
[B4]: ../../repo-ref/bevy/crates/bevy_ecs/src/schedule/config.rs#L558
[B5]: ../../repo-ref/bevy/crates/bevy_ecs/src/schedule/executor/single_threaded.rs#L125
[B6]: ../../repo-ref/bevy/crates/bevy_reflect/src/type_registry.rs#L32
[B7]: ../../repo-ref/bevy/crates/bevy_ecs/src/reflect/component.rs#L76
[B8]: ../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L26
[B9]: ../../repo-ref/bevy/crates/bevy_tasks/src/task_pool.rs#L234
[B10]: ../../repo-ref/bevy/crates/bevy_tasks/src/edge_executor.rs#L580
[B11]: ../../repo-ref/bevy/crates/bevy_transform/src/components/transform.rs#L69
[B12]: ../../repo-ref/bevy/crates/bevy_transform/src/components/global_transform.rs#L19
[B13]: ../../repo-ref/bevy/crates/bevy_ecs/src/hierarchy.rs#L28
[G1]: ../../repo-ref/godot/scene/2d/node_2d.cpp#L119
[G2]: ../../repo-ref/godot/scene/main/canvas_item.cpp#L201
[G3]: ../../repo-ref/godot/scene/3d/node_3d.cpp#L640
[G4]: ../../repo-ref/godot/scene/main/node.cpp#L1747
[G5]: ../../repo-ref/godot/scene/resources/packed_scene.cpp#L873
[G6]: ../../repo-ref/godot/scene/main/scene_tree.cpp#L1646
[G7]: ../../repo-ref/godot/core/io/resource.cpp#L230
[G8]: ../../repo-ref/godot/scene/resources/resource_format_text.cpp#L1406
[G9]: ../../repo-ref/godot/core/io/resource_loader.cpp#L559

## Design Constraints For Nara

1. Derived-state authority belongs to its producer: every supported local, topology or derived-slot
   mutation revokes freshness, and a read checks the current owner fact within a defined phase window.
2. Preserve component/codec/result type relationships until private erasure; retain runtime checks
   for schema meaning, erased dispatch, lifecycle effects and migration rather than inferring them.
3. Schedule completion and fault reporting do not authorize success-dependent effects. Capture and
   acknowledgement must consume an explicit accepted result or remain inside one fallible operation.
4. A task terminal is not a disposal receipt. Specify executor-owned closure/capture return and late
   result destruction separately from caller-owned outputs, external clones and whole-pool close.
5. Structural parentage, spatial inheritance, authored ownership and destruction are separate
   contracts. Make each operation's preservation set and allowed lifecycle effects explicit.
6. Reload tracks desired configuration, last-good publication and current attempt independently;
   publish against the captured owner/store/revision and keep native rollback claims domain-specific.

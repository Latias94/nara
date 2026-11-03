# Prefab Context And Bounded Traversal

Date: 2026-11-03. Status: supplemental source research, not an adopted contract or verification
receipt. Nara baseline: `88fd7289c0d680481e0f964561650de05f90cbad` on `refactor/foundation`,
with substantial concurrent uncommitted extraction work. The parent lane changed the physical
visitor from recursion to explicit ancestor frames during this study; Nara links below identify
the observed WIP, not a frozen revision. This lane implements none of that change.

Only these local reference snapshots were studied. `git rev-parse HEAD` matched both pins and
`git status --short` was empty in both reference repositories:

- Bevy: `85503941e488152711a4695bbe248e9ed9f453fc`.
- Godot: `81ab961f91066a63bcbf3ec340962bb1a1ada558`.

The pinned links below locate locally inspected source; no network lookup, external model,
engine execution, Cargo command, formatting or product test was used. This supplements
[the extraction study](2026-11-02-prefab-extraction-and-occurrence-continuity.md) with context
selection and traversal evidence, rather than repeating its extraction/publication design.

## Recommendation

Keep the current Scene owner, reference model and the plan's
[Physical Correspondence And Unavailable Contexts](../plans/2026-11-02-prefab-extraction-plan.md#physical-correspondence-and-unavailable-contexts)
subsection. They already distinguish physical custody, occurrence identity and affected contexts.
The evidence supports completing the parent-owned bounded visitor and its focused qualification
now; it does not justify a new public context graph or a general scene interpreter.

One obligation deserves explicit attention during that work: a stack/depth cutoff cannot certify
the dependency closure of the unvisited suffix. Keep depth admission, complete physical coverage,
cycle handling, work admission and unavailable-record preservation as separate obligations.
Neither reference engine supplies Nara's complete dormant-source preservation proof.

## The Distinctions That Matter

These are separate questions even when some answers happen to name the same source:

| Fact | Question it answers | Extraction consequence |
| --- | --- | --- |
| Physical declaration ownership | Which captured document, record and nested declaration slot stores the intent? | Select its exact output declaration and compare unavailable bytes/defaults there. Visual parentage and canonical effective identity cannot substitute for this fact. |
| Target/modified context | In which Prefab base or final routed source can the operation change an instance slot? | An outer declaration can create a dependency of a source other than its physical owner. |
| Lexical reference environment | In which containing-source occurrence was a payload or argument declared? | The override target does not choose the new value's environment. Moving the declaration needs an explicit reference rewrite. |
| Effective occurrence | Which particular instantiation does the compiled entity belong to? | Repeated instances must remain distinct; root binding can merge effective identity without merging contributors' ownership. |
| Reference correspondence | Which old target is represented by which new target, in the relevant environment? | An owner assignment, equal local ID or successful allocation is not this mapping. |
| Affected dependency context | Could this captured source context depend on selected P, including through retained inactive declarations? | A conservative union can exclude unsafe preservation without claiming that every occurrence activates the dependency. |

The existing Nara contract already separates typed source/addition provenance from effective
identity and specifies lexical environments for values and arguments:
[ADR 0038, lines 99-160](../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md#decision).
The plan separately requires exact physical placement coverage, modified-context dependencies
and actual unavailable contributors in its Physical Correspondence subsection, lines 347-387.
These distinctions require operation-local facts, not another durable identity axis.

## New Reference Evidence About Context

**Godot retains a chain of contributing scene states, rather than treating `Node::owner` as the
whole composition context.** `PropertyUtils::get_node_states_stack` walks the live owner chain,
collects instance/inherited states and returns their ordered state/node pairs. Its helper
`_collect_inheritance_chain` follows each `SceneState`'s base state.
[scene/property_utils.cpp, `_collect_inheritance_chain` and `get_node_states_stack`, lines 214-285](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/property_utils.cpp#L214-L285).
`SceneState::_parse_node` separately filters by owner/editable-instance eligibility, obtains this
state stack and uses it when deciding what to pack.
[scene/resources/packed_scene.cpp, `_parse_node`, lines 873-935](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L873-L935).
The transferable distinction is between storage selection and composition contributors. This is
a live-tree/property-state mechanism; it does not enumerate Nara's dormant physical declarations
or prove the meaning of unavailable component payloads.

**Godot resource remapping uses an instance context in addition to source resource identity.**
`SceneState::make_local_resource` chooses a base from the current instance/root or owner, then
looks up the resource under `p_resources_local_to_scenes[base]`. `get_remap_resource` likewise
indexes a nested cache by `p_for_scene` before source resource, with explicit fallback/copy rules.
[scene/resources/packed_scene.cpp, `make_local_resource`, lines 784-811](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L784-L811),
[same file, `get_remap_resource`, lines 157-202](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L157-L202).
Thus the remap key is effectively `(instance context, source resource)`, not just a global source
resource. This supports keeping Nara's occurrence-sensitive rewrites distinct from a source-level
dependency union. It does not prescribe Godot's cache or fallback policy for Nara references.

**Godot's ordering of duplication and rebinding is stronger evidence than changing owners.**
`Node::duplicate_from_editor` completes `_duplicate` before `_duplicate_properties` and signal
duplication. `_duplicate_properties` explicitly relies on the completed tree and remaps internal
Node-valued references using the original-to-copy relative path; references outside the copied
root retain their original value in that branch.
[scene/main/node.cpp, `duplicate_from_editor`, lines 2983-3008](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/node.cpp#L2983-L3008),
[same file, `_duplicate_properties`, lines 3106-3155](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/node.cpp#L3106-L3155).
The additional lesson beyond the prior extraction study is the phase boundary: establish the
actual destination structure before binding references that can point forward. These paths and
live Node values supply no proof for hidden defaults, unknown schemas or unopened dependants.

**Bevy shares a reference table within one root application and creates another for the next
root application.** `ResolvedSceneRoot::apply` creates `SceneEntityReferences`; `apply_with`
registers the current entity's names; `apply_related` passes that same table into related scenes.
[crates/bevy_scene/src/resolved_scene.rs, `ResolvedSceneRoot::apply`, lines 67-75](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_scene/src/resolved_scene.rs#L67-L75),
[same file, `ResolvedScene::apply_with`, lines 224-235](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_scene/src/resolved_scene.rs#L224-L235),
[same file, `apply_related`, lines 352-395](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_scene/src/resolved_scene.rs#L352-L395).
`SceneEntityReferences::get` can allocate an empty entity for a previously unseen reference;
`InnerSceneEntityReference` includes macro location, local name ID and runtime counter.
[crates/bevy_ecs/src/template.rs, `SceneEntityReferences` and `InnerSceneEntityReference`, lines 92-166](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/template.rs#L92-L166).
This is useful evidence for scoped construction and forward references. Allocation success is
not evidence of durable target continuity, and these runtime keys do not replace Nara source IDs.

**Bevy's reverse dependency graph belongs to a particular consumer.** `AssetInfos::loader_dependents`
is recorded for change watching, and `process_asset_load` populates it from loader dependencies
only when that consumer is enabled.
[crates/bevy_asset/src/server/info.rs, `AssetInfos`, lines 121-133](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/server/info.rs#L121-L133),
[same file, `process_asset_load`, lines 550-579](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/server/info.rs#L550-L579).
This supports an operation-specific Nara footprint rather than a universal dependency graph.
A graph of loaded/loader-observed dependencies cannot stand in for an inventory of inactive
authored instance declarations.

## Traversal Evidence And Its Actual Bounds

| Pinned implementation | Observed mechanism | What that mechanism does not establish |
| --- | --- | --- |
| Godot `SceneState::_parse_node` and `_parse_connections` | Recursive calls on child nodes. [packed_scene.cpp:1206-1215](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L1206-L1215), [1407-1415](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L1407-L1415). | These call sites carry no depth/work allowance. Their existence is not evidence that Nara's physical visitor should use native recursion. |
| Godot `SceneState::_instantiate` | A node-array loop still enters nested/inherited scenes recursively; the same routine reserves `sizeof(Node *) * nc` using `alloca`. [packed_scene.cpp:260-304](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L260-L304), [334-338](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L334-L338). | A loop over records does not by itself remove nested call-stack growth; node count can also consume stack storage independently of scene depth. This is a source observation, not a measured failure claim. |
| Godot `ResourceLoader::_dependency_get_progress` | An `in_progress_check` flag breaks active-path revisits, then resets after recursively visiting subtasks. [resource_loader.cpp:856-881](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/io/resource_loader.cpp#L856-L881). | A cycle guard is not a maximum depth, allocation budget or once-per-graph memoization guarantee. |
| Godot `Array::recursive_duplicate` | `MAX_RECURSION` is 100; exceeding it prints an error and returns the newly created array before copying its elements. [typedefs.h:259-260](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/typedefs.h#L259-L260), [array.cpp:552-569](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/variant/array.cpp#L552-L569). | This is a particular container-duplication fallback, not a global scene depth policy or successful lossless extraction. The depth limit does not bound sibling width. |
| Bevy `ResolvedScene::apply_with` / `apply_related` | Applying related scenes re-enters `apply_with`; the parent calls `apply_related` after template writes. [resolved_scene.rs:286-309](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_scene/src/resolved_scene.rs#L286-L309), [352-395](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_scene/src/resolved_scene.rs#L352-L395). | These signatures have no explicit cumulative work or depth limit. Runtime template composition does not qualify Nara's complete source transformation. |
| Bevy `AssetInfos::propagate_loaded_state` / `propagate_failed_state` | Both recursively propagate through reverse waiters and take the waiter sets before descending. [info.rs:627-683](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/server/info.rs#L627-L683). | Consuming dependency bookkeeping does not bound the native stack depth of a long dependency chain. |
| Bevy `DescendantIter` / `DescendantDepthFirstIter` | Breadth-first uses `VecDeque`; depth-first uses `SmallVec<[Entity; 8]>`, popping and extending with reversed children. [relationship_query.rs:146-237](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/relationship/relationship_query.rs#L146-L237). | These methods avoid recursive calls but have no work admission or visited set in the shown implementations. Eight is inline storage, not a traversal cap; a pending-child frontier can grow with width. |

There is also a concrete reason to inspect graph control flow instead of trusting collection
names: Bevy's change-watching `queue_ancestors` inserts into a `HashSet` but recurses regardless of
the insertion result.
[crates/bevy_asset/src/server/mod.rs, `handle_internal_asset_events` local `queue_ancestors`, lines 1950-1960](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/server/mod.rs#L1950-L1960).
That set deduplicates output; this call site does not use it to deduplicate traversal. This study
does not establish which cyclic inputs that subsystem admits or report an engine defect.

The design inference is narrow: explicit traversal storage, semantic depth admission, active-path
cycle detection, completed-state reuse, cumulative work and retained-storage accounting solve
different problems. Mature engines use different subsets for different consumers. Nara must pick
the subset required by extraction's own complete-source contract.

## Comparison With The Current Nara Work

The observed [footprint classifier](../../crates/nara_scene/src/prefab/extraction/footprint.rs)
already has the appropriate private responsibilities:

- `PhysicalScan::inspect_instance` records a dependency both for the declaring source and for an
  independently identified modified context. `PhysicalScan::instance` selects the final routed
  canonical base for a claim's installed context. This accounts for an outer Scene installing P
  into U even when U's own stored slot is empty; it does not transfer the Scene declaration to U.
- `classify_contexts` propagates `Affected`, then `Unproven`, through reverse edges. A known path
  to P takes precedence over incomplete evidence. Each phase enqueues a newly marked source at
  most once, so the propagation portion is bounded by vertices plus edges, including cycles.
  Physical declaration scanning, key comparisons and edge construction have separate costs.
- The parent-owned `PhysicalInstanceFrame`, `instance` and `push_frame` now use ancestor cursors
  and admit storage before growth. This avoids a pending entry for every sibling as well as
  native recursion. This is a textual observation of ongoing work, not a build/test receipt.

The current source-level union intentionally loses occurrence precision: if one captured outer
declaration can install P in a U context, U can be marked affected even when another occurrence
of U is unchanged. This is conservative exclusion from the unavailable-preservation exception;
it is not a claim that all U occurrences execute P. Refining it requires a real blocked workflow,
not simply observing that the union has false positives.

Two existing boundaries must remain intact. First, `Preserved` still runs understood reference
rewriting and permits allocation reuse only when that rewrite reports no change:
[`corpus::prepare`, observed lines 397-439](../../crates/nara_scene/src/prefab/extraction/corpus.rs).
Second, unavailable observations need the original physical contributor, mapped effective entity,
unchanged readiness reason and complete record/default preservation:
[`preservation::verify_unavailable_records`, observed lines 168-214](../../crates/nara_scene/src/prefab/extraction/preservation.rs)
and the plan's Physical Correspondence subsection. Equal runtime ownership or equal omission
counts cannot replace these checks.

## Now Versus Later

**Now:** complete the bounded visitor in the parent lane, retain the conservative source-level
union and verify its real edge directions. Use the existing focused witnesses for dormant
installations, final-hop canonical contexts, missing associations, cycles, cumulative capacity,
deep declarations and wide sibling cohorts. This lane neither adds a duplicate implementation
nor repeats those tests.

**Now:** qualify what happens at the existing depth boundary. The observed visitor still uses
`PrefabDeclarationLimits::MAX_DEPTH`; that public limit is 64 and is documented for declaration
observation/Revert, not as proof that every captured extraction input has already been fully
enumerated.
[`authoring/prefab_declarations.rs`, lines 16-43](../../crates/nara_scene/src/authoring/prefab_declarations.rs),
[`PhysicalScan::instance`, observed lines 300-308](../../crates/nara_scene/src/prefab/extraction/footprint.rs).
An unvisited nested claim could name another canonical modified context whose dependency edge
has not been emitted. Marking only the current frame's known owner/context `Unproven` therefore
does not, by itself, establish independence for every other context. Either establish complete
coverage from actual input admission, refuse the incomplete scan, or conservatively exclude all
contexts that the unseen suffix could affect. This is a proof obligation, not a reproduced bug
or a recommendation to increase the depth number. Capacity exhaustion must not return a partial
footprint as successful complete evidence.

**Now:** keep the present plan structure. Its requirement to build the footprint from all
captured physical declarations and preserve actual contributors already owns the right contract.
If the parent clarifies it, one sentence distinguishing depth admission from complete scan
coverage is sufficient. No durable owner/API redesign or new ADR follows from these references.

**Later, on demonstrated need:** refine the union for a concrete unchanged-occurrence workflow,
or admit additional parameter environments through the existing compiler's real semantics.
Keep unsupported environments `Unproven` until that evidence exists. Do not add a second overlay
interpreter to turn conservative classification into apparent semantic precision.

**Later, on repeated equivalent consumers:** consider sharing a small traversal/accounting
mechanism. The inspected code does not justify a universal graph library, public contributor graph,
new runtime identity axis or custom Rust/source analyzer for this operation.

## Limits

This is a focused local source study, not an audit of either engine. No crash, memory maximum,
performance result, serialization round trip or end-to-end extraction success was measured.
Claims about absent guards are limited to the cited implementations, not every upstream caller.
The changing Nara WIP was inspected without freezing or editing it; final qualification belongs
to the parent lane. In particular, the depth-boundary observation needs admission-path evidence
before it can be called a confirmed preservation defect.

Godot's owner/state chains, live Node remapping, missing-resource fallbacks and per-instance
resource caches do not prove complete durable unavailable semantics. Bevy's runtime template
references and loader-observed graphs do not prove dormant declaration continuity or project-wide
capture/publication/recovery. Preserve those limits when applying the useful context and traversal
patterns above.

## Parent Disposition

The parent retained the owner/context distinctions and the plan structure. The extraction WIP
now uses one operation-local `MAX_PHYSICAL_DEPTH`, independent of public observation/Revert and
compiler occurrence limits. Physical scanning refuses an unvisited suffix instead of returning
a partial footprint; its new control hides a claim modifying a different U context beyond that
boundary. Declaration mutation and before/after topology checks also use admitted ancestor
frames and complete-or-refuse traversal. The focused extraction cohort passed 75/75 under
Windows/MSVC Rust1.95.0, default thread stacks, jobs1, root target; nextest
`46ce9a91-7716-439a-a534-f16169172dca`. This is bounded private-owner evidence, not public
project extraction, file publication/recovery or native qualification.

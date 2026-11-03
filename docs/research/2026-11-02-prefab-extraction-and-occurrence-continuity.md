# Prefab Extraction And Complete Occurrence Continuity

Date: 2026-11-02. Inspected baseline: `ed21b23051342587574be75da017da888270b597`
with the in-progress authored Schema upgrade. Status: source study for the separate CP2 successor
in [foundation R5](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r5-durable-addressing-prefab-overlays-and-projection-lifetime).
This is not an adopted extraction contract, implementation evidence or an additional CP1 gate.

## Finding

Extracting part of Prefab P into new Prefab Q changes declaration ownership and occurrence paths.
It needs a Scene-owned extraction operation that proves both correspondences over the complete
authored tree. Copying the current projection, replacing every P key with Q, or submitting a no-op
Schema upgrade cannot establish that proof. Keep CP1's value/selector and Schema succession
contracts intact while completing its current qualification.

The Project Host can reuse finite corpus capture, actual file publication, Pending fencing and
recovery mechanisms. Their Schema-specific receipts must not become topology authority. Workspace
continues to own complete source/history/saved-checkpoint installation.

## Existing Owners And Limits

| Owner | Available mechanism | Extraction limit |
| --- | --- | --- |
| [Complete instance conversion](../../crates/nara_scene/src/authoring/prefab_conversion/localize.rs) | Actual owning source, selected surviving projection, provenance-to-local correspondence and dependency fences | Its selected effective result does not inventory dormant declarations. |
| [Native Prefab copying](../../crates/nara_scene/src/prefab_copy/rewrite.rs) | Walks additions, claims, properties, placements, bindings and suppressions | Replacing source keys preserves hop count and local origins; partial extraction changes them while P remains valid. |
| [Authoring classification](../../crates/nara_scene/src/authoring_decode.rs) | Lossless complete overlay storage and understood component/property facts | The unavailable-target manifest is not a complete index of structural addresses and bindings. |
| [Authored field-upgrade proof](../../crates/nara_scene/src/authoring_decode/schema_upgrade.rs) | Independently validates the permitted complete-tree difference | It deliberately preserves source keys, targets, roots, hierarchy and instance topology. Relaxing these equalities would remove CP1's proof. |

Two different correspondences are necessary:

- Declaration ownership: which complete records and declarations move from P to Q, which stay in
  P or an outer overlay, and which fragments are unchanged.
- Occurrence continuity: old and new canonical containing sources, source anchors, all typed hops
  and expected bases, and terminal origins. Repeated instances remain distinct and the mapping
  cannot merge different occurrences.

[PrefabEntityTarget and PrefabInstanceHop](../../crates/nara_scene/src/prefab/overlay.rs) provide
much of the existing address vocabulary. A declaration array position remains an exact
snapshot/revision-bound [PrefabInstancePath](../../crates/nara_scene/src/prefab/declaration.rs),
not a durable entity identity.

For an anchor-preserving candidate, an outer overlay based on P would change one target from
`through=[], SourceEntity(leaf)` to
`through=[{anchor: SourceEntity(k), expected_base: Q}], SourceEntity(leaf)`.
Any later nested hops remain after that new prefix. Preserve SourceEntity versus InstanceAddition
at every level. References that still mean P must continue to mean P.

The complete rewrite includes component, property, instance and suppression targets; placement
targets, parents and Before/After references; payload references and binding targets/expected IDs;
nested source declarations, overlay bases and all expected bases. Include dormant declarations
and empty Properties batches. A topology change alone does not authorize changing Schema binding
slots. Keep the existing [exact binding-consumption checks](../../crates/nara_scene/src/prefab/overlay/references.rs).

## Representation Gates Before Selecting Production

An anchor-preserving experiment can keep local anchor k in P, move its ordered source-owned
children to Q, and instantiate Q at k. Components and placement on k remain local. An outer
overlay addition placed visually beneath that subtree remains owned by its original declaration
layer. Effective visual ancestry does not transfer declaration ownership.

This experiment does not settle the ordinary branch-to-Prefab workflow. Current compilation
places Prefab roots beneath an anchor rather than merging root and anchor, and distinguishes
projected-root ordering from ordinary children in
[staged layout](../../crates/nara_scene/src/prefab/overlay/compile/staged.rs) and
[overlay layout](../../crates/nara_scene/src/prefab/overlay/layout.rs). Moving the selected root
itself needs an explicit root/instance identity decision. Adding a wrapper is not a proof that
hierarchy, transforms, ordering, references and author-visible identity remain equivalent.

Ordinary [SceneLocal references](../../crates/nara_identity/src/types.rs) contain a local ID, not
an occurrence route. Cross-cut references may therefore become unrepresentable. Before adopting
a production extraction contract, compare the complete expected workflow with a deliberate
instance-root and reference representation change. Do not turn a common author workflow into an
arbitrary refusal merely to preserve the current data structures. Any new reference model must
also settle its module ownership without a Scene/Identity dependency cycle.

Unknown semantics remain a different case: moving opaque bytes to another source can change
their namespace meaning even when the bytes are equal. Preserve only fragments proven unrelated;
refuse affected semantics that cannot be understood. Also account for explicit None instance
pins, placement ordering domains, Absent/default meaning and dormant expected-base mismatches.
Neither successful current expansion nor silently adding pins establishes continuity.

## Reuse Without Fabricating Schema Authority

| Mechanism | CP2 use |
| --- | --- |
| [FilePublicationHost](../../src/project_host/file_publication.rs) | Reuse Existing/Missing bindings, real old/absent observations, temporary writes, exact outcomes and native cleanup custody. New Q and identity sidecar require their own actual missing-target observations. |
| [Project Ready/Pending](../../crates/nara_project/src/authoring.rs) | Reuse the project fence and bounded recovery reference; identify and admit the concrete extraction recovery package. |
| [Complete inventory](../../src/project_host/authoring/source_inventory.rs) | Reuse private scans, profile union, deduplication, negative observations, retained leaves/parents and re-enumeration. Prove old membership to explicitly new membership; CP1's unchanged-set proof is insufficient. |
| [Workspace binding](../../crates/nara_tooling/src/workspace/authored_schema_upgrade.rs) | Reuse private checkpoint/saved-binding mechanisms and bind-all-before-install ordering. Keep a concrete extraction proof at its public boundary. |
| [Finite source publication](../../src/project_host/authoring/source_upgrade_publication.rs) | Reuse deterministic writes and actual outcome observation. Already-new members retain their leaves and receive no invented replacement witness. |
| [Bounded source work](../../crates/nara_scene/src/authoring/schema_preparation.rs) | Reuse private work/storage arithmetic where justified. The extraction owner supplies its real map, traversal, validation and encoding costs. |

Keep `AuthoredFieldUpgrade`, catalog predecessor/generation admission, selector/value succession
and [PreparedSourceUpgradeSchema](../../src/project_host/authoring/source_upgrade_ready.rs)
specific to CP1. A topology-only live operation uses its actual unchanged nominal Schema basis.
Do not manufacture a field upgrade or an empty catalog successor to obtain those tokens.

The current [recovery payload](../../src/project_host/authoring/source_upgrade_recovery.rs)
requires existing old source bytes, observed metadata and a field-upgrade description. Extraction
needs concrete modified-existing and create-new roles, including the original Absent meaning for
Q/sidecar. During recovery, not-yet-created cannot be confused with deletion of an original member.
Share the exclusive final Ready mechanism only after each concrete operation prepares its own
valid future project receipt; arbitrary publication success cannot fill a CP1 receipt.

## Construction Alternatives

| Alternative | Benefit | Cost |
| --- | --- | --- |
| Scene-owned construction from an explicit cut | One finite transformation constructs outputs, ownership and correspondence together; the proof has a smaller surface. | Must settle root, reference and ID semantics first. Another extraction form needs an explicit extension. |
| Project-supplied finite extraction proposal and correspondence | Allows a demonstrated project-specific naming/ID policy. | Scene must still independently prove the concrete cut and complete rewrite; producer footprint is not authority. More retained data and validation surface. |

No project-specific extraction algorithm has been demonstrated. Prefer Scene-owned construction
after the representation gates are settled. Neither option requires a universal transaction,
graph, migration-callback or semantic-equivalence engine.

## Pinned Reference Evidence

The read-only reference study inspected these unchanged local source snapshots, not current
releases:

- Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558`:
  [Save Branch as Scene](../../repo-ref/godot/editor/docks/scene_tree_dock.cpp) duplicates nodes,
  keeps bidirectional correspondence, adjusts owners, packs/saves, then replaces the editor tree.
  [SceneState::pack](../../repo-ref/godot/scene/resources/packed_scene.cpp) selects owned/editable
  instance content and constructs node maps/owner-chain paths.
  [Node duplication](../../repo-ref/godot/scene/main/node.cpp) rebinds internal node references.
  Ownership and explicit correspondence are useful precedents. Live-tree packing, name/ID fallback
  during instance restoration and local editor Undo do not prove Nara's dormant declaration or
  complete-project recovery contracts.
- Bevy `85503941e488152711a4695bbe248e9ed9f453fc`:
  [ResolvedSceneRoot](../../repo-ref/bevy/crates/bevy_scene/src/resolved_scene.rs) and
  [template entity references](../../repo-ref/bevy/crates/bevy_ecs/src/template.rs) use
  generation-scoped reference tables with macro/name/runtime context. Explicit mapping and scope
  isolation are useful precedents. The separate
  [SceneEntityMapper](../../repo-ref/bevy/crates/bevy_ecs/src/entity/map_entities.rs) can generate a
  dead reference for an unmapped entity; that is not successful durable rewriting. This pin's
  [Scene implementation](../../repo-ref/bevy/crates/bevy_scene/src/lib.rs) does not yet support
  `.bsn` files, so it supplies no disk extraction/recovery qualification.

## Successor Oracles

Use two Scenes, repeated/nested instances of P, an inactive dirty Scene/Prefab, unopened dependants
and dormant declarations. The actual Schema basis is unchanged. Verify:

1. Complete authored coverage and injective old/new occurrence correspondence, with exact
   preservation of unrelated declarations and distinct repeated instances.
2. Concrete mapped hierarchy, order, values and reference targets; Revert/branch switching must
   preserve previously dormant intent. Prove this extraction, not arbitrary semantic equivalence.
3. An independent Session review/confirm consumer, JSON/RON Save/Reopen, subsequent edit/Undo and
   Check/Play through public APIs, with explicit history-checkpoint effects.
4. Late-source, Q/sidecar collision, source/metadata change, basis and capacity refusals before
   publication, preserving the original source/history.
5. Actual process interruption around Q creation, sidecar publication, dependent replacements and
   Ready-before-ack. Reuse fixed IDs/output bytes, distinguish third content and preserve actual
   publication evidence without interpreting old topology inverses under the new one.

The study ran no extraction, consumer or native tests. CP2 remains unimplemented. Complete CP1,
then settle the root/reference contracts before implementing Scene correspondence, new-member
publication/recovery and complete Workspace/Session integration.

# Unity And Godot: Prefab Editing And Source Ownership

**Date:** 2026-10-26. **Status:** Research and bounded Nara recommendations, not an Accepted
decision, production implementation report, or native Editor qualification.

## Baselines And Method

| Subject | Inspected baseline | Evidence type |
| --- | --- | --- |
| Unity | Unity 6.0, explicitly versioned `6000.0/Documentation/` English Manual and Scripting API, accessed 2026-10-26 | Official documented behavior; no Unity binary or native implementation inspected |
| Godot | `81ab961f91066a63bcbf3ec340962bb1a1ada558`, local `repo-ref/godot/`; `version.py` identifies `4.8.0-beta` | Read-only owning-source observations, not executed behavior |
| Nara | `90dcc43403ae7ab24848bb5dc78b76a1f41e27fe`, Accepted ADRs 0026/0038 and the Proposed overlay design | Current contracts versus design recommendations; concurrent private prototype changes are not qualification evidence here |

Unity URLs use the selected version rather than `current` or an unversioned redirect. These are
maintained documentation branches, not immutable patch-release snapshots: the retrieved English
pages display a documentation build date of 2026-10-26, not a verified Editor binary version.
The search service successfully retrieved the cited pages. The older
`PrefabOverridesMultiLevel.html` URL was inaccessible through that service and is not used as
evidence. No Python HTTP fallback was necessary.

This study ran no Cargo, formatter, SCons, Editor or benchmark, did not modify `repo-ref/`, and
created no commit. Local links below name the pinned Godot owning files; the version pin, not a
future moving checkout, defines their meaning.

The distinction throughout is deliberate:

- **Documentation observation:** a Unity page describes a public workflow or API.
- **Source observation:** the pinned Godot code implements a particular mechanism.
- **Nara inference:** a proposed consequence for Nara, not a guarantee attributed to either engine.

## 1. Unity: Editing An Instance Is Not Editing Its Asset

### Overrides And Editing Context

**Documentation observation:** instance property values, component additions/removals, and child
GameObject additions/removals are override categories. An overridden property takes precedence
over subsequent changes to the asset. Inspector and Hierarchy expose override indicators; the
Overrides UI supports individual, selected and all changes. These are visible authoring concepts,
not just an internal diff format.
Source: [Override prefab instances](https://docs.unity3d.com/6000.0/Documentation/Manual/PrefabInstanceOverrides.html).

**Documentation observation:** nested instances retain their own asset links while belonging to
an outer prefab. Variants inherit from a base, can themselves be bases, and give their overrides
precedence. Composition placement and inheritance therefore introduce more than one meaningful
source context.
Sources: [Nested prefabs](https://docs.unity3d.com/6000.0/Documentation/Manual/NestedPrefabs.html),
[Prefab variants](https://docs.unity3d.com/6000.0/Documentation/Manual/PrefabVariants.html).

**Documentation observation:** Prefab Mode edits the asset, either in isolation or with the
surrounding scene visible but locked. Breadcrumbs identify the edited asset and provide navigation
back. Seeing an instance in its scene does not imply that the scene is the current mutation owner.
Source: [Edit prefab assets](https://docs.unity3d.com/6000.0/Documentation/Manual/EditingInPrefabMode.html).

**Nara inference:** the Inspector needs an explicit editing context, source link and occurrence
breadcrumb. Selection, visible context, source document, instance overlay and runtime projection
are different facts. Nara should retain its document/provenance model, not acquire mutable Unity
objects merely to provide familiar source navigation.

### Apply And Revert Have Different Owners

**Documentation observation:** `ApplyPrefabInstance` promotes the instance's changes to its
asset and removes their override status. For nested prefabs and variants, this all-overrides API
targets the outermost prefab. Root transform/default overrides have exceptions; its behavior is
not a universal copy-every-property operation.
Source: [ApplyPrefabInstance](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PrefabUtility.ApplyPrefabInstance.html).

**Documentation observation:** `ApplyPropertyOverride` instead requires an explicit asset path,
because either an inner nested asset or an outer asset can be a valid destination. When an array
element does not exist in the shorter destination array, this API applies the entire array; in
`UserAction` mode it offers proceed/cancel. Granularity can widen and must be visible to the author.
Source: [ApplyPropertyOverride](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PrefabUtility.ApplyPropertyOverride.html).

**Documentation observation:** `RevertPropertyOverride` removes the single property's override
status so the asset value is reflected again. `RevertObjectOverride` is narrower than a subtree
operation: for a GameObject it does not revert its components or children, and it excludes default
overrides such as root position/rotation.
Sources: [RevertPropertyOverride](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PrefabUtility.RevertPropertyOverride.html),
[RevertObjectOverride](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PrefabUtility.RevertObjectOverride.html).

**Nara inference:** expose these meanings separately, even if the eventual UI uses shorter labels:

| Author action | Intended source effect in a Nara design |
| --- | --- |
| Edit an instance | Change intent owned by the containing source |
| Reset an override / resume inheritance | Remove that intent, not write a copy of the current base value |
| Edit the source asset | Change the explicitly selected source document |
| Apply an override to a source | Promote selected intent to an explicitly selected ancestor source and reconcile the affected containing intent |
| Convert to local | Transfer effective data and declared references to local source ownership |

This is not authority to add a multi-file transaction kernel. Apply may affect two source owners;
its publication, persistence and Undo boundary require a concrete workflow decision. A batch of
ordinary actions is not proof of atomic promotion. Until that boundary is qualified, a limited or
unavailable Apply command is more honest than silently choosing an asset or claiming cross-document
rollback. An inner-source edit may affect other occurrences; that consequence belongs in preview.

### Unresolved Data Is Not Automatically Disposable

**Documentation observation:** invalid override targets or unknown property paths can leave unused
override data. Unity retains it in the scene file, can reapply it if a script/field returns, and
provides explicit cleanup with confirmation. The documentation also points to
`FormerlySerializedAsAttribute` for field renames.
Source: [Removing unused override data](https://docs.unity3d.com/6000.0/Documentation/Manual/UnusedOverrides.html).

**Nara inference:** retaining unresolved author intent is a useful product precedent for the
Proposed overlay design. It does not establish Unity's handling of every schema change, nor prove
Nara's proposed conflict format. Nara should distinguish Save/Reopen of preserved intent from
admission of an effective projection, and make deletion/remapping an explicit source edit. Unity
field-name migration is not a reason to replace Nara's admitted stable property identities.

## 2. Unity: Inspector Convenience Comes From A Shared Editing Path

### Serialized Editing, Synchronization And Validation

**Documentation observation:** `SerializedObject` provides a serialized editing stream over one
or more objects, including common fields across different types. Independent streams over the same
targets require synchronization. Serialized field application bypasses associated property setters;
the page calls out validation separately. Multi-target value setters affect all targets, whereas
ordinary value getters return the first target's value.
Source: [SerializedObject](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SerializedObject.html).

**Documentation observation:** `Update` refreshes from the targets and discards unapplied local
property changes. `ApplyModifiedProperties` records Undo information before loading the pending
serialized content back into targets; its boolean indicates whether changes were applied.
Sources: [Update](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SerializedObject.Update.html),
[ApplyModifiedProperties](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SerializedObject.ApplyModifiedProperties.html).

**Nara inference:** borrow the complete editing path, not Unity's synchronization semantics.
Nara already has exact source/revision preparation and consuming commit; retain that stronger
stale-edit rejection instead of overwriting or refreshing away a proposal. Schema/domain validation
must remain below the widget layer. A custom Inspector should not separately implement mutation,
dirty state, Undo, override recording and projection refresh.

### Direct Mutation Has Additional Obligations

**Documentation observation:** `Undo.RecordObject` snapshots state and records property differences
at frame end, using binary comparison. No differences means no Undo entry. Reparenting, component
addition and destruction need dedicated APIs. Prefab modification recording is an additional
requirement when this direct-object route is used.
Source: [Undo.RecordObject](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/Undo.RecordObject.html).

**Documentation observation:** Unity recommends `SerializedObject`/`SerializedProperty` for
Prefab edits. For a direct mutation, its example records Undo before the change and calls
`RecordPrefabInstancePropertyModifications` afterward to preserve the modification on save/reopen.
Source: [RecordPrefabInstancePropertyModifications](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PrefabUtility.RecordPrefabInstancePropertyModifications.html).

**Nara inference:** avoid exposing a normal editor path requiring callers to remember this list
of follow-up calls. The existing source owner should produce inverse history and publish source
state only after admission. Unity's frame-end snapshot diff is neither a durable override grammar
nor evidence that Nara should replace exact transactions. Equally, exact root-field coalescing for
Undo is not evidence that its output preserves durable leaf intent.

### Multi-Object Editing Is More Than A Selection Vector

**Documentation observation:** custom Editors opt in with `CanEditMultipleObjects`; unsupported
ones report the limitation. `SerializedProperty` is the shared property-editing interface for
Undo, multi-object edits and Prefab overrides, including a mixed-value observation.
Sources: [CanEditMultipleObjects](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/CanEditMultipleObjects.html),
[SerializedProperty](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SerializedProperty.html).

**Documentation observation:** `EditorGUI.BeginProperty`/`EndProperty` supplies custom controls
with override styling, a Prefab revert context menu and mixed-value display. Its example only
writes back after a user change; repainting a control must not assign the first displayed value
to all selected targets.
Source: [EditorGUI.BeginProperty](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/EditorGUI.BeginProperty.html).

**Nara inference:** the useful seam is an admitted semantic property read/prepare/commit path
available to first-party and independent tools. Mixed values, mixed inheritance state,
non-editability and unavailable targets need truthful observations. They cannot be collapsed to
the first entity's value. First qualify multiple targets in one document and one owner-created
history entry; cross-document selection does not automatically acquire the same guarantee.

## 3. Godot: Concrete Owners Behind Similar Workflows

All observations in this section are from the pinned local source, not Unity documentation or
a running Godot Editor. Godot's live `Node`/`Resource` editing is not Nara's document-first model.

### Scene Composition And Saved Differences Are Separate From Undo

**Source observation:** `SceneState::NodeData` stores parent, owner, instance, index and property
records. `SceneState::_instantiate` creates inherited/nested bases before applying local property
values. `_parse_node` filters ownership/editable-instance boundaries and obtains a state stack
for comparing inherited values. None of this representation is an Editor Undo log.
Sources: [SceneState storage](../../repo-ref/godot/scene/resources/packed_scene.h#L39),
[base instantiation](../../repo-ref/godot/scene/resources/packed_scene.cpp#L300),
[property application](../../repo-ref/godot/scene/resources/packed_scene.cpp#L567),
[ownership and state comparison](../../repo-ref/godot/scene/resources/packed_scene.cpp#L873).

**Source observation:** `_parse_node` normally omits values equal to their resolved defaults,
with resource-sharing exceptions. Pinned properties bypass that equality-based omission.
`EditorInspector::_property_pinned` records pin/unpin itself as an Undo action. Intent can therefore
change without changing the displayed scalar value.
Sources: [pinned-property save rule](../../repo-ref/godot/scene/resources/packed_scene.cpp#L1061),
[pin history](../../repo-ref/godot/editor/inspector/editor_inspector.cpp#L6011).

**Nara inference:** preserve the distinction between inheritance and an explicit equal-value pin.
Do not infer override existence by comparing effective values, and do not mistake a successful
Undo log for a persistent inheritance representation. The Godot rule is evidence for that
distinction, not a specification for Nara's keyed selectors or component-version migration.

### Revert Uses Contextual Inheritance, Not Merely Type Defaults

**Source observation:** `PropertyUtils::get_property_default_value` resolves instance/inheritance
state first, then script defaults, then native-class defaults. `get_node_states_stack` follows
owner/instance contexts. `EditorPropertyRevert` allows a custom revert value first and otherwise
uses this utility; the Inspector emits the resulting value through its normal edit signal.
Sources: [default resolution](../../repo-ref/godot/scene/property_utils.cpp#L80),
[owner-chain collection](../../repo-ref/godot/scene/property_utils.cpp#L239),
[revert observation](../../repo-ref/godot/editor/inspector/editor_inspector.cpp#L928),
[revert edit signal](../../repo-ref/godot/editor/inspector/editor_inspector.cpp#L1293).

**Nara inference:** distinguish a Schema default, an inherited effective value, explicit absence,
explicit null and resume-inheritance. Godot's live value-setting route is not proof that assigning
a default encodes all of these meanings in Nara. Resolve through existing source/Schema owners;
do not make a property widget chase source files or reconstruct an owner chain from display IDs.

### Inspector Plugins Feed An Existing Mutation/History Path

**Source observation:** `EditorInspector::_connect_property_editor_signals` wires contributed
property editors to common edit, pin and multi-property handlers. `_edit_set` records do/undo
properties, linked-property state, hooks and refresh operations for normal local objects. It also
has explicit branches for multi-node edits, remote objects and objects opting out of Undo.
Sources: [shared signal wiring](../../repo-ref/godot/editor/inspector/editor_inspector.cpp#L4273),
[edit dispatcher](../../repo-ref/godot/editor/inspector/editor_inspector.cpp#L5730),
[grouped property edits](../../repo-ref/godot/editor/inspector/editor_inspector.cpp#L5896).

**Nara inference:** shared semantics for custom controls is the useful ergonomic precedent.
Copying Godot's runtime type switches, arbitrary property setters or live callbacks would weaken
Nara's admitted candidate boundary. These call paths do not demonstrate Nara's final-candidate
validation, bounded inverse retention or last-operation failure atomicity.

### History Routing And Multi-Node Editing Have Explicit Limits

**Source observation:** `EditorUndoRedoManager` owns histories. It chooses scene histories for
nodes in the edited scene and certain built-in resources, with pending/global fallback; a normal
action reports a mismatch when its objects resolve to different histories. A supplied context or
explicit history can assign the action, and forced routing is a separate mechanism.
Sources: [history selection](../../repo-ref/godot/editor/editor_undo_redo_manager.cpp#L63),
[mismatch and explicit routing](../../repo-ref/godot/editor/editor_undo_redo_manager.cpp#L107).

**Source observation:** `MultiNodeEdit::_get_property_list` intersects compatible `PropertyInfo`
declarations. `_set_impl` groups changes in one action, can update one field while preserving each
node's other fields, and makes NodePath values relative to each node. It skips missing nodes;
`_get` returns the first found value. This is not a cross-document all-targets-or-reject proof.
Sources: [multi-node writes](../../repo-ref/godot/editor/inspector/multi_node_edit.cpp#L44),
[first-found read](../../repo-ref/godot/editor/inspector/multi_node_edit.cpp#L124),
[property intersection](../../repo-ref/godot/editor/inspector/multi_node_edit.cpp#L153).

**Nara inference:** capture document/revision and per-target addresses explicitly instead of
inferring authority from the currently visible scene. Shared editing needs declaration and
capability compatibility as well as per-target reference lowering. Do not copy missing-target
skipping into an action advertised as atomic. A merged Undo action alone does not prove atomic
source admission or cancellation of a continuous gesture.

### Instantiation And Editable Children Preserve Scene Ownership

**Source observation:** `SceneTreeDock::_perform_instantiate_scenes` prepares instances before
creating the action, uses the current scene's explicit history, adds the child, sets its owner to
the edited scene and retains the object for Undo. Editable children are a separate command on
the containing scene, with history, rather than changing the nested asset merely by selecting it.
Sources: [scene instantiation](../../repo-ref/godot/editor/docks/scene_tree_dock.cpp#L366),
[editable-child command](../../repo-ref/godot/editor/docks/scene_tree_dock.cpp#L2834).

**Nara inference:** local additions need explicit containing-source ownership and occurrence
provenance even when visually placed under an inherited parent. They must not masquerade as base
Prefab entities or acquire identity by parsing an expanded-ID prefix. This source path demonstrates
ownership-aware composition, not Unity-style arbitrary promotion into a selected ancestor asset.

## 4. Bounded Recommendations For Nara

The current [ADR 0038](../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
still requires `ScenePatchDocument` durable overrides relative to source Prefab IDs. The current
[ADR 0026](../architecture/adr/0026-editor-command-patch-and-undo-model.md) owns exact transactions,
prepared admission and Undo; its stable-property coalescing explicitly adds no durable override
semantics, while its implementation notes still describe patch reuse for Prefab overrides.
The [declarative overlay design](../architecture/prefab-overlay-design.md) is Proposed. Its private
comparisons do not replace those contracts or establish production Apply/Revert support.

1. **Keep the document owner; replace only the wrong representation.** Continue using exact
   prepared patches/inverses for a particular source revision. Preserve original durable intent
   separately from root lowering, including explicit pins. Reconcile the patch-as-durable-overlay
   clauses in 0038 and 0026 together only when the replacement covers existing structure and
   references. Neither upstream justifies a second Undo owner or a universal editing object model.
2. **Make context and intent observable through the existing tooling seam.** A future Inspector
   needs source/occurrence provenance, editable state, effective/inherited value and explicit
   override/conflict status. A local addition must identify its real containing owner. Keep these
   observations distinct from runtime incarnation and preview epoch; an unresolved override remains
   repairable source data, not a guessed target or silently dropped projection item.
3. **Define reset before broad Apply.** Reset removes selected intent with one source transaction
   and Undo; it must survive a later compatible base change as inheritance. Apply requires an
   explicit admitted destination, original source basis and truthful promotion scope. Defer broad
   multi-source Apply rather than deriving it from a UI batch or introducing speculative global
   transactions. Never widen leaf intent to a container without explicit author consent.
4. **Make a complete custom control the ergonomics test.** An independent control should read
   state, prepare/preview, retain/commit, receive the exact result, Undo/Redo and Save/Reopen without
   direct ECS mutation or hand-maintained dirty/override calls. Painting or refreshing it must not
   create a pin. Existing owner checks remain the authority, not the field's visual widget.
5. **Bound multi-selection to a real owner first.** Prove one-document, multiple-occurrence edits
   with mixed values and mixed override states, one history entry, exact per-target references and
   no mutation when the last target is stale/missing/invalid. Preserve untouched leaves per target.
   State cross-document limitations explicitly; neither cited implementation establishes Nara's
   stronger atomicity or grants authority to infer it.

These are design and acceptance recommendations, not a new execution roadmap. The immediate
overlay replacement still needs complete structural/reference semantics, durable source editing,
conflict persistence, admitted base evidence and source/Runtime/UI provenance before a production
format change. Familiar Inspector behavior should test those owners rather than conceal missing
semantics with UI fallbacks.

## Limits

- Unity documentation supplies public intent and API ergonomics, not a verified internal format,
  memory bound, conflict algorithm, stable-property identity system or cross-document transaction.
- Godot source supplies concrete owner and history mechanisms at one fixed development snapshot.
  It was not executed here. Do not generalize the inspected paths to every plugin/resource/reimport
  case, or interpret absence from this study as whole-engine feature absence.
- No claim is made that either engine provides Nara's proposed unresolved-overlay admission,
  alias-canonical source equality, prepared activation fence or complete persistent UI key contract.
- Root-transform default overrides, mutable Unity objects and Godot's live Node/Resource ownership
  are engine-specific policies. Learn from their workflows without importing those object models
  or making Rust gameplay depend on a second reflection-owned mutation universe.

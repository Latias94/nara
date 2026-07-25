---
type: Research
title: Unity and Godot missing-type and inspector primary-source research
description: Primary-source evidence for unavailable types, retained authoring data, and inspector extension boundaries relevant to Nara schema and package design.
timestamp: 2026-07-22T06:15:00Z
record_id: d2790ff6945b481a89b8218e370dd2dc
producer_id: codex-research-missing-schema-engines
run_id: research-missing-schema-engines-20260722
tags:
  - unity
  - godot
  - schema
  - inspector
  - unavailable-type
  - package
---

# Scope

This record examines only first-party Unity documentation and Godot's checked-in
source/documentation. It distinguishes unavailable *type* behavior from generic
asset identity and from inspector extension behavior. It does not define a Nara
runtime API or change any ADR.

# Unity Findings

1. `SerializedObject` and `SerializedProperty` expose a generic editor-side view
   over serialized fields. Unity documents that this path handles dirty state,
   Undo, multi-object editing, and Prefab overrides; a long-lived stream must be
   explicitly refreshed and its modifications applied. This is an edit-data
   transaction model, not an invitation for inspector controls to mutate target
   objects through arbitrary runtime APIs.

2. Unity's `SerializeReference` has a narrowly specified unavailable-type path.
   The persisted namespace, class, and assembly name are used during
   deserialization. When that type is unavailable, the managed field is `null`,
   but the serialized state remains in the asset and is included on re-save; the
   state can recover if the type later becomes available. This guarantee is
   specifically for managed references, not a general statement about every
   missing Unity component.

3. Unity separately exposes an editor API that counts `MonoBehaviour` instances
   with a missing script and links to an explicit removal operation. The reviewed
   public API documents detection/removal, but does not give the same payload
   preservation contract as `SerializeReference`; those two cases must not be
   conflated.

4. A Unity UPM removal removes a direct project dependency. When nothing else
   requires it, the package's Editor/runtime functionality is unavailable; asset
   packages imported into a project have a separate removal workflow. This is
   concrete evidence that package dependency removal and authored-content
   ownership are distinct product operations.

# Godot Findings

1. Godot's `ResourceUID` documentation says UIDs keep resource references intact
   when files are renamed or moved within a project. UID-to-path registration is
   an identity/location facility; it is not a promise that the resource's class
   is currently present.

2. Godot has an internal `MissingResource` editor class for a resource whose type
   is unrecognized, commonly because an extension is no longer loaded. Its
   implementation records the original class and a property map, and returns the
   original class for saving. The text and binary resource loaders create this
   placeholder only when the class-unavailable missing-resource mode is enabled.

3. Godot also has `MissingNode`. `PackedScene` creates it when a stored node class
   cannot be instantiated and records properties while loading. Its own
   configuration warning differentiates two cases: an unavailable original class
   retains data as a re-saveable placeholder, while a missing instantiated scene
   warns that saving discards the instance and its properties. Therefore even a
   mature editor's apparent "missing type" behavior is case-specific, rather
   than one universal lossless contract.

4. `EditorInspectorPlugin` selects supported `Object` values and can replace or
   supplement individual property controls. Its custom controls must derive from
   `EditorProperty`, which emits a named-property/value change and receives
   refresh/read-only state. This is a powerful toolkit-specific object/property
   adapter, not a standalone persistent schema or migration authority.

# Cross-Engine Comparison

| Concern | Unity evidence | Godot evidence | Design consequence to examine for Nara |
| --- | --- | --- | --- |
| Unavailable type | `SerializeReference` retains serialized state while exposing `null`; missing MonoBehaviours have a separate detection/removal API | `MissingResource`/`MissingNode` retain original type and recorded properties under a controlled loader mode | Classify absence precisely; never label all unavailable cases as equivalently editable or recoverable. |
| Lossless re-save | Explicit for a missing managed-reference type | Explicit for unknown node class, but explicitly not for a missing instantiated scene | Nara needs per-state preservation and re-save rules, not a Boolean `missing`. |
| Stable asset identity | Not established by the reviewed Unity sources | UID preserves references across in-project rename/move | Stable asset identity and schema/provider availability must remain independent axes. |
| Inspector extension | Serialized streams provide undo/prefab/multi-edit semantics | Object/property parser plus toolkit-specific `EditorProperty` controls | Editor presentation should consume an owned schema/value projection and issue validated edit transactions; it should not become the persistence authority. |
| Package removal | Removing UPM dependency can remove available editor/runtime functionality; imported asset packages are separate | Missing extension classes are a concrete cause of placeholders | Package disable/uninstall needs an impact report and a source/data ownership policy before mutation. |

# Implications And Questions For Nara

The evidence reinforces, but does not by itself decide, the direction explored by
Proposed ADR 0090:

- A durable record needs separate facts for stable schema identity/version,
  payload grammar readability, available migration, and available runtime binding.
  A type's absence must never be represented merely by a synthetic Rust component.
- A known-unbound record can be retained without granting arbitrary field edits.
  Godot deliberately permits property mutation on missing placeholders; Unity's
  documented managed-reference recovery instead leaves the object field `null`.
  Nara must choose this authoring policy explicitly rather than inherit either
  behavior accidentally.
- Inspector extensions should be an Adapter over typed observation and patch
  transactions. They require explicit refresh, read-only, validation, undo, and
  provenance semantics, but should not require a UI toolkit or a live ECS object
  as their persistence model.
- Package removal, disabling a schema provider, and deleting imported project
  content are different operations. A future package UX tracer should prove its
  preflight report, lossless preservation boundaries, reactivation path, and
  explicit destructive action before a package-manager UI claims safe uninstall.

Open questions for a future tracer:

1. Which unavailable states allow structure-only inspection, exact raw payload
   export, or no editing at all?
2. What registered schema-only projection is sufficient for an Inspector when a
   native/runtime provider is absent?
3. Which records block scene spawn/cook versus merely block runtime binding?
4. What user-confirmed operation may discard a retained unavailable record after
   a package/provider is disabled or uninstalled?

# Sources

## Unity official documentation

- [SerializedObject](https://docs.unity3d.com/ScriptReference/SerializedObject.html): generic serialized editing, synchronization, `ApplyModifiedProperties`, Undo, and Prefab behavior.
- [SerializedProperty](https://docs.unity3d.com/ScriptReference/SerializedProperty.html): generic property traversal and multi-object editing.
- [SerializationUtility.HasManagedReferencesWithMissingTypes](https://docs.unity3d.com/ScriptReference/SerializationUtility.HasManagedReferencesWithMissingTypes.html): persisted type identity, `null` field, retained serialized state, and recovery after the type returns.
- [GameObjectUtility.GetMonoBehavioursWithMissingScriptCount](https://docs.unity3d.com/ScriptReference/GameObjectUtility.GetMonoBehavioursWithMissingScriptCount.html): missing-script detection and linked explicit removal API.
- [CustomEditor](https://docs.unity3d.com/ScriptReference/CustomEditor.html) and [PropertyDrawer](https://docs.unity3d.com/ScriptReference/PropertyDrawer.html): type-bound custom editor and serialized-property drawer extension points.
- [Remove a UPM package from a project](https://docs.unity3d.com/Manual/upm-ui-remove.html): direct dependency removal, remaining transitive dependencies, and distinction from asset-package removal.

## Godot checked-in primary sources

- `repo-ref/godot/doc/classes/ResourceUID.xml`: UID identity and rename/move behavior.
- `repo-ref/godot/core/io/missing_resource.h` and `repo-ref/godot/core/io/missing_resource.cpp`: original-class/property retention and save-class behavior.
- `repo-ref/godot/doc/classes/MissingResource.xml`: documented editor-only unknown-resource purpose.
- `repo-ref/godot/scene/main/missing_node.h` and `repo-ref/godot/scene/main/missing_node.cpp`: original type/scene retention and distinct losslessness warnings.
- `repo-ref/godot/doc/classes/MissingNode.xml` and `repo-ref/godot/scene/resources/packed_scene.cpp`: unknown-class/scene placeholder construction, property capture, and re-save handling.
- `repo-ref/godot/doc/classes/EditorInspectorPlugin.xml` and `repo-ref/godot/doc/classes/EditorProperty.xml`: inspector parsing, custom-property controls, updates, read-only state, and property changes.

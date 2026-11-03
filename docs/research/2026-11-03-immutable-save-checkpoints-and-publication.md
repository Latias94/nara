# Immutable Save Checkpoints And Publication

**Scope:** The source snapshot, actual encoded output, filesystem result and saved/history
acknowledgement needed by ordinary Save and reviewed Prefab extraction. This source comparison
does not qualify Nara's complete multi-file publication or recovery protocol.

**References:** Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`, inspected through the local pinned `repo-ref/` sources.
The [complete extraction plan](../plans/2026-11-02-prefab-extraction-plan.md) owns implementation
and acceptance. [ADR0047](../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
owns the Workspace and saved-state contract.

## Concrete Pressure

An extraction changes P and some working dependants. A different dirty document can require
an explicit SaveCurrent, retaining its working source and Undo/Redo. An unrelated dirty source
must remain a guard. The three destinations cannot all be implemented as a reopened source or
as an ordinary fallible Save after Ready.

The incumbent publication constructor consumed a real replacement receipt but accepted a
checkpoint independently. It checked the receipt's identities, durability and verified digest;
it did not itself prove that those published bytes were the checkpoint's actual codec output.
The trusted ordinary Host call chain supplied that association procedurally. Making this seam
reusable requires an output owner that closes the association before filesystem effects.

## Pinned Source Findings

### Bevy

`DynamicWorldBuilder::extract_entities` clones reflected components into owned storage;
`build` consumes that storage. This separates the captured data from the original World.
`DynamicWorld` exposes mutable collections, so the ownership separation is not a sealed immutable
checkpoint or a nominal capture identity.
Sources: [builder](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_world_serialization/src/dynamic_world_builder.rs#L309-L338),
[dynamic world](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_world_serialization/src/dynamic_world.rs#L25-L29).

The example `save_world_system` serializes to an owned String before moving that output into
the asynchronous IO task. Its single `File::write` call does not check the returned byte count;
the example does not provide a publication receipt or Editor saved/history acknowledgement.
The reusable `save_using_saver` awaits the saver, flushes and writes metadata, but returns a
procedural result rather than evidence associated with one Editor checkpoint.
Sources: [world serialization example](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/examples/scene/world_serialization.rs#L192-L214),
[asset saver](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/saver.rs#L544-L571).

### Godot

`EditorNode::_save_scene` recreates the cached PackedScene state before packing, allowing
existing references to retain the previous SceneState. Packing collects node-property Variant
values. Ordinary Array copies share underlying storage; this is not evidence of recursively
immutable data suitable for an asynchronous capture.
Sources: [scene save](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/editor_node.cpp#L2551-L2567),
[SceneState packing](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L1459-L1464),
[Array sharing](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/variant/array.cpp#L56-L72).

After the synchronous `ResourceSaver::save` returns OK, `_save_scene` marks the scene saved.
`EditorUndoRedoManager::set_history_as_saved` records the current history version without
clearing history. It receives no captured-version token. The `scene_saved` signal precedes
the save error check; that signal alone is not a successful-publication witness.
Sources: [save acknowledgement](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/editor_node.cpp#L2580-L2598),
[history acknowledgement](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/editor_undo_redo_manager.cpp#L385-L388).

The text saver checks stream error before local file destruction. Windows `FileAccessWindows::_close`
performs temporary-file replacement and reports replacement failure separately. The saver's Error
is therefore not a typed witness of both final name publication and verified output contents.
Sources: [text saver](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/resource_format_text.cpp#L2145-L2149),
[Windows close](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/drivers/windows/file_access_windows.cpp#L237-L277).

## Selected Nara Design

Retain actual immutable Scene/Prefab and classification allocations through Arc. The source
owner's replacement semantics keep prior captures stable through Edit, Undo and Reopen.
Readmission shares the actual classification; validating a save does not create another Session,
source incarnation or history owner merely to compare preservation expectations.

The checkpoint's codec issues one immutable encoded output and its digest. Callers cannot attach
arbitrary bytes or a supplied digest. Both written and verified receipt digests must match that
issued output. Refusal returns the same capture/output and physical evidence. Already-new
reconciliation verifies the fixed output through an actual file instead of fabricating a receipt;
the concrete Host separately proves its configured current parent/name.

ReplaceSource targets the fixed prepared future incarnation/revision/content. SaveCurrent targets
the exact captured incumbent and retains its history. Guard changes no saved or source state.
The saved stage retains nominal capture and actual output allocations so equal values or an
ordinary recapture cannot replace its issued input. Every source, saved field and Session is
bound before installation; the enclosing owner must also bind persistence, lane and future Project
destinations before Ready.

Use the same canonical codecs for ordinary Save and extraction. Output growth is admitted before
reserve or copy. The enclosing operation admits canonical copies, serializer input and actual
decode/classification work; every publication or verification retry uses that original cumulative
allowance. The shared leaf adoption mechanism has no Schema-specific meaning, while CP1 and
extraction retain their distinct source and publication policies.

Once the actual checkpoint codec has issued and sealed an extraction output, consuming a file
result for that same output does not require another decode or classification. The concrete
publication owner validates the configured current path and retained ancestors, then transfers
the original encoding with its actual receipt or observed leaf. Receipt refusal retains both
inputs; nominal acceptance refusal retains the resulting commit. Retry continues that state
without writing the file again. Successful conversion consumes the encoding and raw receipt;
the actual leaf, digest and Editor receipt projection remain beside the stage-owned commit.
The immutable package continues retaining its fixed bytes. An observed leaf has no historical
receipt, and its additional bounded digest read spends the original IO allowance before running.
The separate complete-installation guard still owns the final saved-state update.

Actual reclassification must carry that admission through selected defaults and effective-value
validation. Sparse source can omit a large scalar default that is absent from both raw bytes and
retained classification facts. A fixed multiple of those two representations does not bound the
work of interpreting Schema. Source-copy admission and per-classification admission have separate
responsibilities; a retry cannot reuse an earlier default-materialization allowance.

The references support separating captured data, actual output and saved acknowledgement. They
do not establish Nara's multi-file crash safety, Pending/Ready protocol, fresh-process recovery or
infallible final installation. Those obligations require Nara's actual independent consumers.

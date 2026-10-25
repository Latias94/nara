---
title: "R2 Format Admission: Pinned Bevy and Godot Boundaries"
date: 2026-10-25
status: "Pinned source comparison; alternatives unqualified"
---

# Scope And Pins

Read-only source study, not another prepared-panel review. No repository writes, Cargo,
tests, child agents, network research, or optimization implementation. All source paths below are repository-relative.

- Bevy: `85503941e488152711a4695bbe248e9ed9f453fc`; reference checkout reported clean.
- Godot: `81ab961f91066a63bcbf3ec340962bb1a1ada558`; reference checkout reported clean.
- Nara base: `763840994a7114716b54e17bf307d50caad49dc3`, with pre-existing concurrent edits.
  Nara document citations refer to the observed working tree, not immutable HEAD content.
- Observed `docs/benchmarks/2026-10-25-prepared-panel.md` SHA256:
  `0F65541AC55854BB042C906C710C7D8CD70C99FDA4956ADF7BB7800561B4367C`.

The comparison selects Bevy's built-in **local Inspector** and Godot's ordinary local
property Inspector, including a built-in Node2D rotation setter. It does not generalize
remote editing, arbitrary plugins, or script callbacks into the same guarantees.

## Main Finding

Neither inspected ordinary scalar-edit path serializes a whole persistent document as
its generic edit-admission step. This is **not evidence that they provide Nara's same
guarantee more cheaply**. Bevy edits live reflected ECS values without an inverse/history
transaction in that path. Godot records object/property/value operations and invokes
setters; scene packing and persistent encoding are separate Save operations. Their
invariants do not imply that every accepted edit and inverse fits exact whole-document
JSON and RON byte/shape ceilings.

# Bevy: Widget To Live ECS; Serialization Separately

1. **Trigger and ownership.** `emit_field_edit` records the displayed value and emits
   `FieldEdit { entity, component, path, value }`; its observer queues `write_field_edit`
   with exclusive `&mut World` access. Sources:
   `repo-ref/bevy/crates/bevy_inspector/src/details_panel.rs:439`,
   `repo-ref/bevy/crates/bevy_inspector/src/details_panel.rs:689`.
2. **Current object/type validity.** At execution, `write_field_edit` requires Local mode,
   the registry entry and `ReflectComponent`, a still-existing Entity, a mutable component,
   the component still on that Entity, and a resolvable reflected field path. Ordinary
   `ReflectComponent` access returns the actual component borrow, not a document clone.
   Sources: `repo-ref/bevy/crates/bevy_inspector/src/details_panel.rs:698`,
   `repo-ref/bevy/crates/bevy_ecs/src/reflect/component.rs:375`.
3. **Scalar admission.** `write_field_value` downcasts to supported scalar types;
   `write_number_field` rejects non-finite inputs, non-finite float narrowing, fractional
   integer input, and integer range overflow. It assigns the selected value directly.
   Only `Written` marks the component changed; rejection can refresh the widget.
   Sources: `repo-ref/bevy/crates/bevy_inspector/src/details_panel.rs:734`,
   `repo-ref/bevy/crates/bevy_inspector/src/details_panel.rs:813`,
   `repo-ref/bevy/crates/bevy_inspector/src/details_panel.rs:864`.
4. **Undo and stale basis.** This observer/write path creates no inverse or history entry
   and has no document revision precondition. A source test explicitly expects an edit to
   overwrite a value the game replaced after the widget was built. That is not Nara's
   exact-basis rejection contract. Sources:
   `repo-ref/bevy/crates/bevy_inspector/src/lib.rs:115`,
   `repo-ref/bevy/crates/bevy_inspector/src/details_panel.rs:4308`.
   This is a scoped absence in the inspected implementation, not a claim that no Bevy
   application can supply Undo or persistence.
5. **Actual save/serialization boundary.** This pin uses `DynamicWorld`, not the older
   `DynamicScene` API. The separate world-serialization example extracts a world, calls
   `dynamic_world.serialize(&type_registry)`, then writes the resulting bytes. Extraction
   visits entities/resources; component extraction depends on filters and registered
   reflection support. Missing extraction prerequisites skip that component rather than
   retaining a Nara-style unknown-record manifest. Sources:
   `repo-ref/bevy/examples/scene/world_serialization.rs:166`,
   `repo-ref/bevy/crates/bevy_world_serialization/src/dynamic_world.rs:72`,
   `repo-ref/bevy/crates/bevy_world_serialization/src/dynamic_world_builder.rs:293`.
6. **What gets encoded.** `DynamicWorld::serialize` invokes RON pretty serialization.
   Its serializer visits the extracted resources and entities, sorts component/resource
   entries by type path, and uses `TypedReflectSerializer`. This is whole extracted-world
   serialization **when called**, not on each Inspector scalar edit. The inspected edit
   path does not invoke it or pre-admit its final encoded size/shape. Sources:
   `repo-ref/bevy/crates/bevy_world_serialization/src/dynamic_world.rs:229`,
   `repo-ref/bevy/crates/bevy_world_serialization/src/dynamic_world.rs:236`,
   `repo-ref/bevy/crates/bevy_world_serialization/src/serde.rs:79`,
   `repo-ref/bevy/crates/bevy_world_serialization/src/serde.rs:111`,
   `repo-ref/bevy/crates/bevy_world_serialization/src/serde.rs:170`.

The example is evidence of a separate serialization API, **not a built-in Inspector Save
workflow**. Serializer/library limits and errors are not evidence of Nara-equivalent
edit-time dual-encoding admission.

# Godot: Property Undo To Object Setter; Pack And Encode At Save

1. **Trigger.** Integer/float widgets emit property changes; float input can convert
   displayed degrees to radians. `EditorProperty::emit_changed` emits `property_changed`,
   connected to `EditorInspector::_property_changed`, which calls `_edit_set`.
   Sources: `repo-ref/godot/editor/inspector/editor_properties.cpp:1601`,
   `repo-ref/godot/editor/inspector/editor_properties.cpp:1757`,
   `repo-ref/godot/editor/inspector/editor_inspector.cpp:322`,
   `repo-ref/godot/editor/inspector/editor_inspector.cpp:4274`,
   `repo-ref/godot/editor/inspector/editor_inspector.cpp:5878`.
2. **Object/type boundary.** The Inspector stores `edited_object_id` and resolves it through
   ObjectDB; node removal clears its edited object. `_edit_set` itself assumes a usable
   resolved object rather than performing Nara-style document-incarnation/revision
   admission. `Object::set` dispatches to script/extension/native/custom setters; the native
   path calls a bound setter, with an optional call-validity result. For the concrete
   built-in scalar, Node2D exposes a FLOAT rotation property; `set_rotation` updates rotation
   and the rendering transform, not persistent document bytes. Sources:
   `repo-ref/godot/editor/inspector/editor_inspector.cpp:5473`,
   `repo-ref/godot/editor/inspector/editor_inspector.cpp:6059`,
   `repo-ref/godot/editor/inspector/editor_inspector.cpp:5739`,
   `repo-ref/godot/core/object/object.cpp:198`,
   `repo-ref/godot/core/object/object.cpp:349`,
   `repo-ref/godot/scene/2d/node_2d.cpp:505`,
   `repo-ref/godot/scene/2d/node_2d.cpp:168`,
   `repo-ref/godot/scene/2d/node_2d.cpp:139`.
3. **Undo boundary.** The ordinary branch creates a `MERGE_ENDS` action, stores a do-property
   operation, reads the current value for the undo-property operation, captures linked
   properties/hooks where applicable, and commits. The manager selects scene/built-in
   resource/global history and delegates to UndoRedo. Operations retain ObjectID, property
   name and Variant value, plus a reference for RefCounted targets. Commit executes redo;
   replay resolves ObjectID, skips a deleted target, and calls `obj->set`. It does not
   pre-serialize forward/inverse documents or pass a validity-result pointer to that set.
   Sources: `repo-ref/godot/editor/inspector/editor_inspector.cpp:5774`,
   `repo-ref/godot/editor/inspector/editor_inspector.cpp:5797`,
   `repo-ref/godot/editor/inspector/editor_inspector.cpp:5868`,
   `repo-ref/godot/editor/editor_undo_redo_manager.cpp:63`,
   `repo-ref/godot/editor/editor_undo_redo_manager.cpp:246`,
   `repo-ref/godot/core/object/undo_redo.cpp:202`,
   `repo-ref/godot/core/object/undo_redo.cpp:311`,
   `repo-ref/godot/core/object/undo_redo.cpp:346`.
   Setter-specific behavior and hooks matter; this is not an atomic unpublished source
   candidate with Nara's guaranteed admitted inverse. Explicit no-Undo, multi-edit and
   debugger branches also exist at `editor_inspector.cpp:5755`.
4. **Save owner.** `EditorNode::_save_scene` checks the scene root and cyclic instance
   inclusion, applies editor changes, obtains/recreates PackedScene state, calls
   `sdata->pack(scene)`, then `ResourceSaver::save`. Pack failure and save failure are
   handled here; saved state is marked on success. Sources:
   `repo-ref/godot/editor/editor_node.cpp:2528`,
   `repo-ref/godot/editor/editor_node.cpp:2567`,
   `repo-ref/godot/editor/editor_node.cpp:2580`,
   `repo-ref/godot/editor/editor_node.cpp:2595`.
5. **Whole scene, with ownership/default filtering.** PackedScene delegates to
   `SceneState::pack`, which clears and reconstructs packed state, parses the root and
   descendants plus connections, and selects storage properties. It excludes out-of-owner
   nodes and omits eligible defaults; some node references become NodePaths. Thus Save
   reconstructs the applicable persistent scene, not all runtime memory and not merely
   the edited scalar. Sources: `repo-ref/godot/scene/resources/packed_scene.cpp:2548`,
   `repo-ref/godot/scene/resources/packed_scene.cpp:1418`,
   `repo-ref/godot/scene/resources/packed_scene.cpp:873`,
   `repo-ref/godot/scene/resources/packed_scene.cpp:947`,
   `repo-ref/godot/scene/resources/packed_scene.cpp:1061`,
   `repo-ref/godot/scene/resources/packed_scene.cpp:1210`.
6. **Persistent text encoding and size/shape behavior.** ResourceSaver selects a matching
   saver. The text saver opens the file for writing, discovers resources, writes selected
   resource/node properties through VariantWriter, and checks file errors. The Editor's
   large-text-resource check is a warning after a successful Save, not an edit-admission
   ceiling. VariantWriter's dictionary/array recursion cutoff logs and writes `{}`/`[]`;
   that is explicitly not Nara's reject-before-publication shape guarantee. Sources:
   `repo-ref/godot/core/io/resource_saver.cpp:102`,
   `repo-ref/godot/scene/resources/resource_format_text.cpp:2186`,
   `repo-ref/godot/scene/resources/resource_format_text.cpp:1868`,
   `repo-ref/godot/scene/resources/resource_format_text.cpp:1614`,
   `repo-ref/godot/scene/resources/resource_format_text.cpp:1802`,
   `repo-ref/godot/scene/resources/resource_format_text.cpp:2077`,
   `repo-ref/godot/scene/resources/resource_format_text.cpp:2141`,
   `repo-ref/godot/editor/editor_node.cpp:2600`,
   `repo-ref/godot/core/variant/variant_parser.cpp:2127`,
   `repo-ref/godot/core/variant/variant_parser.cpp:2172`.

These findings describe the inspected stock paths, not every resource codec. Arbitrary
tool scripts, getters, Inspector Undo hooks, or notifications can perform additional work;
there is no universal claim that custom scalar setters never serialize.

# Nara Baseline And Non-Comparability

The supplied attribution is taken as established, not retraced. The current report records
about 30 ms debug preparation for 257 entities with **257 actual Link components, 257
unknown records, and real SceneLocal references**, on both direct and panel interfaces.
The field fast path is already reached; complete canonical-envelope/encoding work,
forward/inverse admission and classification-state copying remain. References make the
current shape certificate decline; even certificate success still performs both complete
encodings. Sources: `docs/benchmarks/2026-10-25-prepared-panel.md:49`,
`docs/benchmarks/2026-10-25-prepared-panel.md:64`,
`docs/benchmarks/2026-10-25-prepared-panel.md:83`.

This total does **not** attribute milliseconds to encoding, parsing, copying or
classification, and the reference engines supply no matched timing/control here.

Material differences prevent a direct performance or policy comparison:

- Live reflected ECS values versus Nara's persistent source document and frozen Schema basis.
- No built-in inverse in the inspected Bevy path; setter-operation Undo with merge/deleted-target
  behavior in Godot versus Nara's admitted two-way replay/history and atomic publication.
- One later RON world serialization or selected Godot scene/resource codec versus Nara's current
  exact JSON **and** RON byte/shape admission before an edit is accepted.
- Filtered reflected extraction or storage/default-based scene packing versus Nara's canonical
  unavailable-record preservation. Godot has missing-resource handling, but this study does not
  establish equality with Nara's unavailable-manifest contract.

Relevant current owners are ADR 0026 (`:34`, `:47`, `:54`) for basis/candidates/two-way
history, ADR 0049 (`:35`, `:52`) for bounded input/publication, ADR 0045 (`:132`) for
aggregate materialization, and ADR 0090 (`:92`, `:152`) for complete candidate validation
and unavailable preservation. These refer respectively to
`docs/architecture/adr/0026-editor-command-patch-and-undo-model.md`,
`docs/architecture/adr/0049-untrusted-project-input-and-parse-budget-policy.md`,
`docs/architecture/adr/0045-component-schema-capability-metadata.md`, and
`docs/architecture/adr/0090-unavailable-schema-and-lossless-authoring.md`.
The ledger explicitly records unchanged exact JSON/RON limits under the earlier private
optimization: `docs/architecture/adr/implementation-status.md:261`.

# Bounded Format-Owner Alternatives

These are design candidates, not implemented algorithms or measured speedups.

## A. Exact Compositional Admission Summaries

Keep private, format-owned facts for the admitted canonical document. Recompute the
affected canonical subtree and its aggregate path, then admit forward and inverse against
the same limits. Facts must describe **actual JSON/RON wire and parser accounting**, not
merely logical ComponentValue counts: encoded UTF-8/escaping/number lengths, indentation
and separators, omission/wrapper/tag rules, per-container and aggregate shape/string
charges, depth, and affected envelope metadata. Extrema need removal-safe aggregation;
subtracting one scalar's nominal size is not sufficient. Reference forms must be explicitly
covered rather than presumed safe because they are typed.

The format owner produces and validates the facts from its canonical grammar/encoder;
callers do not mint certificates. Bind them to document incarnation/content, format policy
and limits, and jointly publish/invalidate them with the owned candidate. Import, structural
change, unsupported representation or stale evidence uses the unchanged full oracle.
Unchanged unknown siblings may reuse exact admitted facts only when canonical representation
and envelope contributions are proven unchanged. Semantic/reference closure, unavailable
preservation and history budgets remain separate obligations.

**ADR impact:** no durable contract revision for a private exact-equivalent proof with
complete fallback and unchanged canonical bytes/acceptance. Update design/evidence. A new
authority exposed to callers or weaker acceptance would be a different decision.

## B. Format-Owned Admitted Fragments / Owned Canonical Representation

An alternative to independently modeling byte lengths is to retain format-owned canonical
fragments produced by the real encoders, with exact parse-shape facts and context-aware
composition. Replace the touched fragment, recompute applicable aggregates and wrappers,
and retain the inverse's admitted evidence. Save can consume that same proven representation
instead of granting a second owner permission to reinterpret it. Untouched fragments must
remain immutable and bounded; this trades retained memory and representation complexity
for possible avoided repeated work.

Composition must prove exact output for both formats, including enclosing indentation,
canonical ordering and metadata. A generic Serde-event count is not automatically identical
to the JSON/RON preflight parser's accounting. Context-dependent fragments require an exact
rule or full fallback, not concatenation by assumption. Keep this a finite persistent-format
implementation, not a reflection/compiler/graph framework or second document authority.

**ADR impact:** no durable revision if the representation is private, existing ownership,
atomicity, canonical bytes, limits and Save results remain equivalent. Changing the envelope,
canonical output or supported wire policy requires reconciling ADR 0051; a real document
version/compatibility change also reaches ADR 0043. Merely storing an internal tree does not.

## C. Smaller Certificate Extension, Explicitly Not Full Encoding Locality

Prove the finite persistent reference forms in the current accept-only shape certificate,
keeping exact full JSON/RON encoders and the original fallback. This could avoid parser
prechecks for the real Link fixture without weakening admission. It cannot by itself remove
the two complete encodings or classification-state copying, and no timing benefit is
established here. **ADR impact:** none if the proof is private and sound with full fallback.

## D. Deliberately Change The Editing/Save Contract

Admit structurally/semantically valid working state and property-level Undo, but enforce
full encoded size/shape only at Save/Export; alternatively guarantee only one selected
encoding during editing. This borrows a separation visible in the references, **not their
guarantees**. It may leave an accepted dirty document unsavable, or an inverse outside
current replay admission. Keeping logical/history limits does not prove exact encoded
limits. Late Save rejection must preserve dirty state, recovery evidence and last-good
source; the product needs an explicit repair path. It cannot be described as preserving
the current contract.

**ADR impact: required before adoption.** Revise/reconcile ADRs 0026/0047/0118 at the
accepted-edit, candidate/history and persistence boundary; ADR 0049 must explicitly locate
the retained safety limits and delayed enforcement, while ADR 0090 must retain or redefine
degraded-document preservation. ADR 0049's loader rules alone do not mandate today's
two-serialization implementation. Amend ADRs 0043/0051 only if document compatibility,
format support or canonical encoding also changes. Do not silently relax hostile-input
admission or accepted inverse replay as a performance patch.

# Recommended Next Experiment

This is a first experiment under the current contract, not a decision that edit-time dual-format
admission is the best long-term product boundary. The fearless rebaseline may compare D against
exact-locality alternatives on the same complete editing/history/Save/repair workflow and revise
the owning ADRs. Existing acceptance does not settle that comparison, and reference-engine policy
does not prove it. Phase measurements should expose the cost of the current boundary before such
a choice is frozen.

Prefer **A as the first locality hypothesis**, using B only if exact fragment ownership is
simpler than maintaining equivalent contextual summaries. First follow the report's pending
phase-attribution experiment: instrument actual preparation with per-call total, canonical
envelope, encoding/shape admission, classification/manifest and disposal-inclusive phase
costs, plus actual certificate/fallback evidence. Keep the existing 257-Link/257-unknown
Scene and Prefab fixtures and real references. Do not infer phase cost by subtracting
unrelated quantiles or replace the workload with reference-free entities.

Then, if format work warrants it, a bounded future proof experiment should compare one
reference-bearing scalar-replacement summary against the unchanged complete encoders and
parsers for **forward and inverse**. Use one consolidated differential corpus: digit/sign
and escaping/UTF-8 changes, omission/default and reference representations, unchanged unknown
siblings, and byte/depth/container/aggregate-string limit boundaries. Require the same
admission outcomes and exact Save bytes; verify failed preparation changes no document or
history and an accepted edit remains Undo/Redo/Save/Reopen-capable. Preserve fallback for
unproven cases. Count retained memory and invalidation cost as well as preparation time.

This experiment would test exactness and locality; it would not by itself qualify all
structural edits, native latency, installable tools, or R2 completion. Extending only C is
a bounded option if measurements favor parser work, not a substitute for attribution.

# Remaining Uncertainty

No checks were executed beyond read-only source/status inspection. The reference call
graphs establish ownership separation, not equivalent safety, causal timing, asymptotic
complexity, or the cost of Nara phases. Exact summary feasibility, canonical-envelope
invalidation breadth, inverse evidence reuse and retained-memory tradeoffs remain unproven.
No conclusion here supersedes the supplied Nara trace or authorizes a save-policy change.

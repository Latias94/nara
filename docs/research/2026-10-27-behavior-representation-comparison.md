# Runtime-Defined Behavior Representation Comparison

**Date:** 2026-10-27. **Status:** Executed private comparison, not a production representation,
language SDK or adoption of ADR 0120.

## Scope And Baselines

The [comparison plan](../plans/2026-10-27-behavior-representation-comparison-plan.md) varies
execution storage while holding authored records, Schema, source history, Prefab compilation,
reference traversal and native Name materialization constant. Nara starts at
`66989bec2cffa2b5f9eaf5426a61f9c66cf5a611`; production code is unchanged by this experiment.
The full [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
remains active.

The independently compiled
[fixture](../../tests/fixtures/module-consumer/tests/behavior_representation.rs) uses the existing
hidden authoring compiler and adapter receipts. A bounded Rust feasibility adapter executes
runtime-defined Mover and Sensor Schemas without a Rust component type per Behavior. This is
evidence about representation, not a complete public Runtime consumer. Both alternatives prepare
an unpublished fresh World and check the exact Definition publication before activation.
They do not implement in-place rollback, code replacement or module unload.

| Alternative | Actual storage | Membership authority |
| --- | --- | --- |
| Envelope | ECS `Attachments(BTreeMap<ComponentTypeId, SceneComponentRecord>)` | Entity owns its complete prepared member group |
| Rows | Resource `SchemaRows(BTreeMap<ComponentTypeId, BTreeMap<Entity, SceneComponentRecord>>)` | Owner resource owns per-Schema rows, keyed by full entity generation |

Both retain the same independently addressed authored component records. The envelope is not
a new serialized blob or a replacement for semantic Schema dispatch. Installing each Schema
by replacing the whole carrier would lose siblings; the fixture installs one complete group
per entity. The native stable-ID/Rust-type binding bijection is unchanged.

## Executed Workflows

- A source property edit crosses prepare/commit, Undo/Redo and actual temporary-file JSON/RON
  Save/Reopen. Reopening retains the records and starts a new history.
- Mover and Sensor coexist with native Name. Mover reads a numeric field and changes Name;
  Sensor uses the existing declared-reference plan to signal a referenced native entity.
  Runtime mutation of Mover preserves Sensor; removing Mover leaves Sensor readable/exportable.
  Runtime reference replacement rejects missing and already-destroyed local targets before
  mutating either record, using the existing declared-reference traversal.
- A Prefab leaf override uses the existing declarative compiler. Nested optional/list local
  references are rewritten to the materialized occurrence, and the effective asset inventory
  includes Sensor's nested image reference.
- Two instances of that Prefab signal only their respective friend entities. Native export's
  actual remap rewrites the Behavior references; a fresh Runtime built from the exported
  document preserves that separation. Distinct native Names identify each original pair
  independently of the exported Sensor payload, so swapping the two references fails the oracle.
- Missing execution binding and an invalid last record reject fresh preparation while an
  existing Runtime remains executable. Absent Schema publication preserves source data through
  actual serialization/admission; restoring the publication makes the original data usable.
- A deliberately constructed version-two candidate renames Mover's field and leaves Sensor
  exactly unchanged. Old/foreign Definition tokens reject. This is an explicit transformed-data
  candidate, not an implemented migration service, catalog successor transaction or migration Undo.
- Export of a deliberately corrupted runtime record reports EncodeFailure. A retirement refusal
  preserves native state and both Behavior records.

Fresh World construction isolates candidate failure by discarding unpublished state. An unchanged
active predecessor is a separate Runtime; it is not evidence of atomic reconciliation inside a
live native/VM instance.

## Counts And Lifetime Counterexamples

The query workload contains one Mover+Sensor actor, 32 Sensor-only entities, its native friend,
and 256 additional native-only entities. Envelope enumeration filters actual archetypes by the
carrier ComponentId before inspecting carriers. It does not scan all entities.

| Observation | Envelope | Rows |
| --- | --- | --- |
| Mover results / inspected records | 1 / 33 carriers | 1 / 1 target-Schema row |
| Sensor results / inspected records | 33 / 33 carriers | 33 / 33 target-Schema rows |
| Physically retained members before coordinated retirement | 34 | 34 |
| Physically retained members after coordinated retirement | 0 | 0 |
| Additional native-only entities surviving coordinated retirement | 256 | 256 |
| Direct despawn of one of two two-member entities: live Mover results / inspected | 1 / 1 | 1 / 2, including the orphan |
| Same direct despawn: physically retained members | 2 | 4 |
| After explicitly cleaning only the dead entity's members | 2 | 2 |
| Ordinary strict Scene retirement of the original two-member actor: retained members | 0 | 2 |

These are deterministic operation counts, not measured frame time, throughput, allocation,
resident memory or a scalability threshold. Archetype metadata checks, map lookup costs and
native owner work are excluded. The counts show a real query tradeoff without ranking total cost.

Adding a Rows membership component with an `on_remove` hook cleans its rows on direct despawn.
That same hook makes ordinary strict Scene retirement ineligible. Both ordinary retirement and
the fixture's coordinated retirement then reject with two members and native state intact.
Explicit removal of the hook component performs real row cleanup, after which ordinary Scene
retirement succeeds. It does not demonstrate automatic rollback after owner cleanup.

The fixture's initial coordinated retirement removed members before the fallible Scene operation.
Reviewing this ordering exposed data loss on refusal. The corrected path first completes strict
Scene retirement, then performs infallible exact row cleanup under exclusive World access.
It applies to this data-only, lifecycle-free carrier; future VM callbacks require their own owner
retirement admission rather than treating this tail as a general transaction.

## Source Comparison

Read-only source observations use Bevy
`85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. Neither engine was built or benchmarked here.

- Bevy [dynamic insertion](../../repo-ref/bevy/crates/bevy_ecs/src/world/entity_access/world_mut.rs)
  replaces an existing value of the same ComponentId. Merely permitting two native Schema IDs
  for one carrier cannot establish independent membership or encode behavior.
- Bevy [component descriptors](../../repo-ref/bevy/crates/bevy_ecs/src/component/info.rs) can
  have no Rust TypeId. Their caller owns layout, drop and thread-safety validity.
  [Dynamic queries](../../repo-ref/bevy/crates/bevy_ecs/src/query/builder.rs) admit per-ID
  presence, read and write access. First-class per-Behavior Bevy QueryState and independent
  scheduler access are therefore a named reason to compare descriptors. This experiment
  neither implements nor rejects that alternative.
- Godot's [GDScriptInstance](../../repo-ref/godot/modules/gdscript/gdscript.h) uses one instance
  class with a script-specific member vector. An [Object](../../repo-ref/godot/core/object/object.cpp)
  has one direct ScriptInstance, and changing its script replaces that instance. This supports
  separating semantic Schema from compiled native type, but does not prove Nara's multiple
  independently attached Behaviors or its exact persistence/retirement contract.

The earlier [Unity/Godot editor study](2026-10-26-unity-godot-prefab-editor-ownership.md) continues
to guide explicit source contexts, Inspector history and override ownership. Familiar Editor
interaction does not require either engine's runtime object model.

## Revised Candidate And Remaining Proof

Use the data-only entity envelope as the initial candidate for the next bounded language
workflow comparison. The previous preference for Rows is not supported as an unconditional
architecture choice: an independent semantic Schema does not require independent physical
storage. Both alternatives support independent records; only one inherits entity data lifetime
without a second cleanup path in the executed fixtures.

Keep the Rows query result as a counterpressure. A real selected-language dispatch workload,
frequent Schema-wide queries, safe thread-local VM custody or required scheduler access may
justify an owner index, Rows or descriptors. Do not add such an index speculatively or advertise
the envelope as faster. Preserve these private comparison fixtures until the concrete language
owner replaces them; do not ship two production representation families for optionality.

The envelope currently contains data values only. It has no VM handles, callback pointers or
language destructors. Their cleanup, safe-point behavior and old-code leases are unproved for
both alternatives. Full native coexistence through a public representation contract, actual
asset loading, author-driven attach/remove, owner-catalog migration, cross-owner compatibility,
same-type multiplicity, hot replacement and unload remain open. Exact Arc identity in this fixture
is a publication fence, not an implementation of those broader contracts.

The fixture exports one complete admitted instance. Its cached initial entity roster does not
support export after a member entity has independently retired, and unrelated native entities
are not integrated into its export remap. A production owner must derive the current eligible
membership and export scope rather than promoting this private roster helper.

[ADR 0120](../architecture/adr/0120-schema-publications-and-execution-representations.md)
remains Proposed. Its next step is the same gameplay task through bounded C# and Lua adapters,
with actual toolchain and instance-lifetime evidence before selecting a production owner.

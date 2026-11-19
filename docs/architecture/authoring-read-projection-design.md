---
title: Selection-Independent Authoring Read Projections
date: 2026-10-25
status: Bounded direct-source recursive child discovery and independent panel workflow qualified; full R2 remains open
---

# Selection-Independent Authoring Read Projections

## Consumer And Missing Contract

R2 needs an independently compiled tool to enumerate a non-active Scene or Prefab, discover an
actual component and root field, read its current value, compute a replacement, and use the
existing prepare/commit/history/persistence path. Supplying a value already known to the test
does not prove this workflow. Entity enumeration, root discovery/reading and the bounded direct
Record-member owner path are now qualified; the complete nested panel workflow is qualified
separately below.

The [read-query qualification](../knowledge/engineering/verification/2026-10/2026-10-25T002933Z-bounded-authoring-query-complete-review-and-aggregate-budget-qualification-893b5a3ffa104ec8bc7c7413c12e0c5d.md)
records owner/public/native/static evidence, complete nine-lens review and the independently
rechecked multi-row budget test correction. The [direct-member and panel query qualification](../knowledge/engineering/verification/2026-10/2026-10-26T004155Z-bounded-direct-member-discovery-and-panel-query-workflow-7003450c49204d8eba92ba90db765dc0.md)
adds budget-aware one-level member discovery, selection-independent Session/panel query
facades and an internal headless panel workflow. Its independent-compilation claim is corrected
by the [public Exit successor](../knowledge/engineering/verification/2026-10/2026-10-26T022556Z-public-exit-drivers-retain-maintenance-after-workspace-retirement-2c1f097eeed04d499d81adc33d3232dd.md).
The [independent direct-member successor](../knowledge/engineering/verification/2026-10/2026-10-26T024129Z-pre-budget-query-ownership-and-independent-direct-member-panel-workflow-1acbda3fb8cc47218dd9b54bcd59f367.md)
removes a pre-budget projection clone and adds the actually separate non-active Scene panel
discover/read/prepare/retain/commit, Undo/Redo and Save/Reopen consumer. These records do not
qualify full R2. The [selected-root baseline](../benchmarks/2026-10-25-authoring-root-read.md) separates
operation-local diagnostic timing from normal Application/frame work.

Source interpretation belongs to Scene/Prefab. Tooling owns explicit workspace/document routing,
exact captured basis, bounded copied observations and action outcomes. Neither a Runtime World
nor the selected Inspector is an alternative authoring authority.

## Alternatives And Selection

These alternatives record the original root-read baseline. The selector-directed successor below
keeps its inventory/borrowed-value split rather than adding another authority or a query language.

| Interface | Benefit | Cost or failure mode |
| --- | --- | --- |
| Build the Inspector and truncate | Reuses a working presentation model | Already performs unbounded unwanted work and copies unrelated values; rejected |
| Flat component/field/value stream | One traversal can populate a complete inspector | A huge early field either blocks discovery or needs partial-value semantics; discovery unnecessarily reads values |
| Actual inventory followed by root reads | Discovery remains cheap; callers read only values they need | Adds one explicit read operation, but no retained read session or query language |

Select actual inventory plus finite property reads for the experiment. Discovery pages have an
explicit projection: actual components of one entity, or declared root fields of one actual
Available component. They do not enumerate catalog-only creation capabilities. A separate root
read returns Present, Defaulted or Missing, with explicit Null represented as a Present value.
An oversized requested value rejects that read rather than silently becoming Missing, truncating
its tree, or preventing metadata discovery from progressing. This is not arbitrary nested-value
pagination or a general query DSL.

## Source-Owned Borrowed Views

### Selector-Directed Successor

Replace the root-only read operation with one property read carrying the existing
`ComponentPropertyPath`. Empty steps read the root; Member, keyed ListItem and MapEntry steps
select an existing descendant. Root discovery remains a separate metadata operation. Do not
retain unreleased root-only aliases or introduce a string query language.

Scene/Prefab first require their current Available record and admitted Schema. They borrow the
explicit root or its declared default, then ask Reflect's shared selector walker to resolve only
that root. Defaulted provenance propagates to the selected descendant. A missing declared root
without a default is Missing only for an empty selector; descending from that absence is a typed
unavailable-value error. OptionalAbsent, unknown/retired members, wrong key types and missing map
or list keys remain resolution failures, not Missing. Explicit Null remains explicit data.

The bounded root walker is an operation over an already admitted field/root, not arbitrary input
validation, native decoding or mutation authority. Share its member-lineage and keyed-item rules
with the existing complete component resolver. Do not call complete effective-value materialization
or validate/copy every component sibling just to read one descendant.

Separate finite scan entries/logical bytes from copied metadata and output nodes/logical bytes.
Charge examined declaration members, tombstone paths, optional unwrapping and keyed-list items,
including compared key bytes, before the corresponding work. Normal root/storage lookups retain
their admitted bounded-path/index responsibilities. A small scalar output is not an O(depth),
elapsed-time or allocator bound; keyed lookup can scan its list and must refuse exhausted work.
The full selector contributes to metadata accounting before any owned observation is constructed.

### Source-owned recursive child discovery successor

The selected successor adds `Properties { component, property }` for one direct child page below
an existing durable selector. Scene/Prefab owns interpretation of Record declarations,
root-relative member tombstones, Map keys and KeyedList identities. Tooling routes exact source
bases and accounts for output; it must not reconstruct these rules from a copied Schema or value.
Ordinary unkeyed lists refuse with the existing typed selector error rather than invent positional
identity. Prefab expansion and overlays remain outside this direct-source operation.

Record declarations may be discovered below an absent optional parent. `Available` describes a
declaration or collection identity, not value presence. The separate selected read retains
Present/Defaulted/Missing and explicit Null, including OptionalAbsent refusal when descending into
Null. Map and KeyedList enumeration requires the actual selected explicit/defaulted collection;
absent or Null collections create no identities. Available source admission already validates
key uniqueness and declaration compatibility; enumeration must not revalidate all siblings per
page.

The source view borrows child IDs and continuation anchors. Root lookup uses admitted sorted
declarations; nested lookup, optional wrappers, examined declarations/tombstones and collection
keys share one finite scan allowance. Report actual scan entries and logical bytes. Exhaustion
can return a continuation only after advancing the child stream, even when anonymous declarations
or nonmatching tombstones produce no row. Refuse an allowance that cannot make progress. A parent
at the selector-depth limit must never emit an over-depth child.

Preflight caller selector metadata before Source work. Admit the aggregate target, full child
paths, repeated parent IDs and successor target/anchor before cloning any returned identity.
Use saturating length accounting so oversized untrusted selectors refuse rather than overflow.
Keep the existing direct-member consumer as a regression. Qualify the successor through a
separate independently compiled panel workflow: discover a non-active nested Record/Map/KeyedList
leaf, read its bounded scalar, prepare/cancel/retain competing edits, dispatch exact results,
Undo/Redo, Save/Reopen and physically close the application. The bounded operation now has the
qualification below; full R2, locality/cost comparisons and value pagination remain open.

### November 16 implementation and qualification

Production source and owning regressions are committed at `7f45d879c49cd2fbaf00d27d68fad23f31d2b0b0`.
The independent public Host workflow, CI targets and necessary lock closure are committed at
`0737909f8dda08df2873ea46b81130943e6d1a13`.

The implementation keeps one borrowed Source child page, the original Reflect selector walker
and the existing Tooling cursor/basis. A new schema-in-indexed-root operation avoids scanning
unrelated root declarations. The shared walker now compares borrowed selector prefixes to
tombstones instead of constructing an owned declaration trace on every successful lookup.
Retired-path diagnostics still admit their owned storage before copying; writes retain the
original storage-path and candidate admission.

Review corrected source interpretation in Tooling, child identities copied before aggregate
page admission, temporary successful-lookup declaration copies, unchecked metadata addition,
unbudgeted tombstone filtering and a possible nonprogress continuation after parent lookup.
The first complete child-query run `9547b59b-d570-457d-bedf-0049f80eef00` passed 27/28, with one
fixture assuming construction order rather than the admitted Schema's normalized wire-name
order. The expectation now follows the existing Schema owner. That run predates the final shared
walker correction and is intermediate evidence. The first shared run
`4a787fb1-a5ec-4a60-b6d9-2b408204edb0` passed 1838/1839; its sole failure was a storage-path
test still charging the removed temporary declaration. The corrected test keeps exact root,
growth, wire-copy and one-byte-short refusal assertions. Shared Reflect/Scene/Tooling
all-target release run `c7d70d50-e322-451b-8dc4-7791f21782d6` passes **1839/1839**, with four
existing skips. Independently locked public semantic Host run
`87c45405-c46d-41ea-925f-08ccbbefd87c` passes **8/8**. Its new recursive workflow discovers
Record/Map/KeyedList selectors, reads the one-node/eight-byte leaf beside 48 KiB untouched data,
computes `+2`, cancels one candidate and retains competing edits. It checks exact accepted/stale
submission IDs, Undo/Redo, Save, a different in-memory edit before Reopen, complete restored bytes,
unknown records, unchanged navigation, stale observations and original Application close.
These are semantic CPU tests; UI compilation is not GPU presentation. The original direct-member
consumer remains unchanged except for the exhaustive enum migration in its surrounding test target.
Its independently locked prepared-authoring target passes **10/10**, with one existing skip,
in run `e39e814e-d0eb-44a5-811c-bd4e13e0d0f6`. This retains the actual direct-member panel,
selected/defaulted reads, complete basis refusal, source retirement and persistence regressions.
Final query controls additionally check that anonymous declarations can produce an empty page
while advancing an exact cursor, and that the finite stream still terminates. Parent lookup
exhaustion remains a refusal rather than a nonprogress cursor. After this positive control and a
local `ok_or` cleanup for the unchanged keyed-item error, final query run
`d478653e-3bff-4adf-b556-38ad923b62a8` passes **29/29**, with one existing skip. This final
source run covers the two query targets, including all 18 recursive-child cases. The broader
shared and public workflows above precede only that error-construction style cleanup and the
additional test; their implementation scope is retained rather than rerun for documentation.

Owning all-target/all-feature correctness Clippy and both independently locked selected consumer
Clippy checks pass. Root and both independent workspace format checks pass; the owning check and
root format check pass again after the style cleanup. Local Markdown links/fragments, workflow
YAML, the necessary independent lockfile update and whitespace checks pass. The lock update adds
only the existing Two-Level Game notify dependency closure; it upgrades no dependency version.
CI now includes the public semantic Host targets, but no remote CI execution or publication is
claimed. Scoped implementation and independent review used two agents, both closed.

This is Windows 11/Rust 1.95 source-semantic qualification. It does not measure allocations,
elapsed-time bounds, populated-index maintenance or preparation locality. Schema/value lookup
can revisit the selected path, but every visit shares the actual scan allowance. Expanded/overlay
property discovery, value pagination, unopened-source discovery and full R2 remain open.

### Bounded direct-member discovery

The qualified successor adds a `Members { component, root }` projection for exactly one direct
Record level. Scene/Prefab borrows the admitted root's immutable declaration and tombstone
slices; Tooling advances through them with independent scan and row limits without building a
sorted inventory for the whole Record. Current members without durable IDs consume scan work but
produce no selector row. Direct tombstones produce retired rows. Nested members, collection
items, arbitrary traversal and Prefab expansion remain outside this projection.

Each returned row carries the durable `root + Member(id)` `ComponentPropertyPath`, the current
member kind or retired state, and the root's declared edit capability. A retired row has no
current kind and is never editable. Member order is the admitted Schema declaration order
followed by retained tombstones; this is deterministic and does not imply value presence.

`EditorWorkspace::document_query_basis`, `EditorProjectSession` query forwarding and the
`ImguiEditorPanelContext` read-only query facade now expose the same exact basis without exposing
mutable Session access. A registered Workspace-target panel can query an explicitly named
non-active Scene, discover a direct member, perform a bounded read, prepare the existing
source-owned candidate, retain it across a paint, and commit it through the existing ordered
submission path. This is a consumer qualification, not a second query service or transaction
authority.

Direct Workspace, Session and Application action/result routes carry the same observation.
Source-built panels may use a synchronous read-only Session borrow during paint, then retain or
submit the ordinary source-owned candidate. No mutable document access, extra query session,
candidate registry or second history is introduced. Read failure publishes nothing and cannot
change navigation. Fencing remains exact document/source incarnation/revision/Schema admission.

The concrete proof is a non-active Scene/Prefab keyed-row score: whole-root copying exceeds a
one-node/eight-byte output budget, selected scalar reading fits, and the observed value drives a
real prepared edit through cancellation, stale refusal, history and Save/Reopen. Test scan and
copy budgets separately, together with default/null/absence/retired-selector behavior. This is
qualified by the [scoped successor evidence](../knowledge/engineering/verification/2026-10/2026-10-25T051125Z-selector-directed-property-reads-and-indexed-root-retirement-qualification-bd267ec01e4d44d3ad5f2a2d4543514d.md),
not qualification of recursive/collection discovery, value pagination or full R2. Root field/tombstone lookups
reuse the admitted catalog's sorted slices; they do not hide full retired-root scans. Storage-path
lookup still has its admitted bound and may build diagnostic prefixes; this is not allocation-free.
Reading never creates source. The source-mutation successor under ADR 0026 may initialize only
the target missing root in its private candidate, using a declared default for nested writes and
the supplied value for an explicit root write. It does not merge explicit/partial roots, create
storage ancestors or materialize unrelated defaults. The existing root patch supplies the exact
absence-restoring inverse; read permission alone still grants no mutation authority.

Add a borrowed semantic view of one admitted direct source entity. The owning session retains
its document and classification together. Reuse the canonical sorted entity sequence for binary
lookup; a private index locates direct component facts without a document/fact scan per request.
Empty entities remain discoverable without additional per-empty-entity index allocation.
Indexes store classification positions, not a second document, editable values,
history, Runtime bindings or executable leases. Reclassification/replacement rebuilds them with
the source candidate; preparation publishes none of them early.

Actual component views expose their retained classification fact. Only Available/current direct
records expose root Schema metadata or semantic values. Availability permits interpretation, not
editing every root: the admitted component/field capabilities and ordinary mutation admission
still govern edits. Unknown, future, unmigratable and known-unbound records still have actual
headers but do not gain editable payload access.
An admitted empty entity, a missing entity and a session without authoring admission are distinct.

Root discovery borrows the admitted, ID-sorted field sequence. Root reads use binary lookup in
that sequence without another per-field map, then inspect only the selected declared wire path.
Component headers and root metadata do not look up source payload records; the view resolves the
actual record only for a value read. They borrow an explicit
source subtree or that field's default, without materializing a whole effective component or
validating an already admitted sibling tree again. Unexpected path incompatibility is an error,
not Missing. Missing applies only to a declared root with no source value and no supplied default;
an absent component or unknown root is a distinct error. Defaulted Null remains distinct from
Present Null. Borrowed views cannot survive a mutable session borrow; owned Tooling observations
have their own historical basis and do not certify current edit authority.

### Stored Prefab property claims

A tool reviewing a stored instance override needs to find an exact claim and inspect its
payload even if the base is missing or the effective composition is rejected. The existing
whole-declaration observation remains useful for reviewing structural intent. Property
inspection uses the same non-active document query and continuation through a dedicated
projection of one instance declaration. Its entity argument is the direct anchor. Optional
nested steps follow stored addition or instance-claim slots in that same document.

Discover one page of actual claim headers in declaration order. Each row retains an opaque
claim address tied to its original workspace, document basis, instance path and slot. The
header gives its typed target route, component, version, and durable property selector.
Discovery visits headers only. Reading a selected slot copies its admitted raw
intent and distinguishes explicit absence from `Set(Null)`. This includes unavailable source
claims. Interpretation of inheritance, defaults, and effective targets stays with Scene
composition. Source repair can prepare Revert using the retained claim address. Repeated
instances with equal declaration indices cannot redirect that address.

Charge each examined component or property claim, then admit aggregate row metadata, the
page target and the shared claim/continuation path before copying. Stop measuring a header at
the first identity segment that would exceed the remaining scan allowance. When a header
exceeds the durable selector depth or cannot fit on a single-row page at the hard cap, return
`OversizedPrefabPropertyClaim` with the exact address and advance past it. Both the page target
and retained path must fit the metadata limit. A page with no rows can continue after advancing
past stored non-property intent. Point reading uses indexed lookup and the existing selected-value
budget. Qualification covers a named non-active source, an unavailable sibling, cancellation,
a retained competing Revert, exact Undo/Redo and real Save/Reopen.

At the Godot pin below, `PropertyUtils::get_node_states_stack` samples the editing source
chain separately from reading current values. Scene serialization also retains explicit
equal-value pins. At the Bevy pin, a patch can inherit fields from a cached template, while
the Inspector reads components from the current World. Effective composition and stored
intent therefore need distinct observations. The instance query here keeps source intent
available when there is no effective composition to inspect.

- [Godot source-state traversal](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/property_utils.cpp#L239) and [property serialization](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L1061).
- [Bevy cached template patching](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_scene/src/resolved_scene.rs#L472) and [Inspector component discovery](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_inspector/src/details_panel.rs#L1103).

#### November 20 stored-claim qualification

The production path and independent consumer are committed together at `c44a398`.
Windows x64 release nextest `28fdea69-d2a7-4619-b79f-3d24ae8b1922` passes 322/322 with one
test skipped. The Identity and Tooling libraries, `authoring_queries`, and `property_children`
cover unchanged direct reads and the new selection-independent Scene/Prefab claim path.
The new regressions cover exact target and Schema basis, routed targets, nested declaration
slots, repeatable and foreign cursor handling, selected payload cost, and continuation past
unavailable claims with large values or oversized selectors. The large-selector fixture
exercises a 22 KiB anchor, a 70 KiB key, and an over-depth sequence of empty keys.

Review corrected three custody and capacity errors before that run. A discovered slot now
retains its own instance path and workspace. Header measurement stops at the allotted bytes
or selector depth. Rows and their cursor share the path so paging remains possible for the
long anchor above. Both the header copy and subsequent Revert continue to use their own
existing budgets. Discovery avoids the former whole-instance copy for property review.

The independently locked `prepared_authoring` target passes 12/12 in Windows release
nextest `596991ac-38b7-4671-b4ec-6e8de6fa5fe1`. The two new trajectories use the registered
semantic panel in an ordinary project. Each discovers two anchors with equal slot indices,
selects a claim from its read value, cancels a candidate, and retains two competing candidates
across panel drives. Exactly one publishes. Undo/Redo and attempt-matched Save/Reopen verify the
complete containing Scene or Prefab, including preserved unavailable intent and unchanged base
file bytes. The reopened source rejects both retained addresses and their old cursor.
The same build's hidden-window Windows/DX12 embedded Host passes 1/1 in
`76ae8d1e-bc39-4bbb-b0a9-f58753bf4364`, covering native startup, painting and close while the
claim workflow itself runs without a native window. The tests use Rust 1.95.0. Scoped library
correctness Clippy, root and independent formatting, and changed documentation links pass.
The earlier broad Scene unit-test build was superseded before test execution.

#### Effective instance fields

The next consumer needs the effective value and the provenance of its inheritance separately
from a stored claim. Use the Scene compiler's actual selected instance and the Host's demanded
source receipts. Include two mounts of one base, a root-bound alias, an equal-value pin, a
later base change, and an independently loaded missing or stale dependency. Compare the
visible value after Revert with the selected underlying declaration. A retained effective
observation must identify its source and dependency versions through preparation and publication.

## Workspace Contract

Use a common document-query basis for entity and authoring projections: explicit document,
source incarnation/revision and exact retained Schema admission. Rename the entity-specific basis
when the shared family is implemented; do not retain an unreleased compatibility alias.

Owner-created continuation captures workspace affinity, document kind, exact basis, entity,
projection (including the concrete component, not just the Roots category) and root/component
position. It cannot be serialized or retargeted. Replaying a current
continuation with the same limits returns the same page. Navigation, Save and edits in unrelated
documents do not stale it. Target mutation, Undo/Redo, Reopen, Close, Schema replacement and
foreign workspace do. Finite root reads validate the same basis before interpreting source values.

Separate limits bound scanned entries, returned rows, copied metadata bytes and copied value
nodes/logical bytes. Discovery copies IDs and compact field facts, not complete Schemas, recursive
shapes, default trees or catalog inventories. Metadata bytes count UTF-8 ID payloads; fixed-size
facts and Schema handles are independently bounded by the row/type shape, not charged as an
invented wire format or allocator size. Charge the aggregate observation, including copied
page/continuation identity, before cloning; `AuthoringRecordFact` itself is not a compact header
because it also retains declared asset references. Reads use `cost_up_to` before cloning the selected
value; failure returns no partially copied observation. These are operation-local logical limits,
not allocator isolation, elapsed-time guarantees or a global Editor budget. Schema/path lookup
and index maintenance costs must remain explicit; a point lookup must not hide a full scan.

Start the experiment with at most 256 discovered entries and 64 KiB copied metadata per operation,
and at most 4,096 nodes / 256 KiB logical payload per root read. These independent read ceilings
serve a bounded scalar/structured tool, not mutation's eight-write ceiling. Callers choose smaller
positive limits. Measure owner lookup/reading on a large document with a large unrequested sibling;
do not call a chosen cap measured memory or latency evidence.

Direct Workspace, project Session and Application paths must carry the same observation. Existing
action/outcome transport supplies registered consumers; no active-tab switch or inferred result is
needed. Read-only observation during retained close does not reopen mutation admission. Prepared
panel submission has its own scoped qualification; installable-tool placement/lifecycle remains
separate.

## Acceptance And Experiment Stops

- Source Scene/Prefab tests distinguish actual versus catalog-only components, all retained
  readiness states, empty/missing entities, current defaults, omitted optional roots and explicit
  Null. A requested root must not clone or traverse an unrelated large subtree.
- Public discovery progresses inside components/roots with small limits and can discover fields
  even when a value exceeds the read ceiling. Exact limits reject before copying; no unprogressing
  empty continuation is allowed. Cursor/basis fencing preserves navigation, selection, history,
  saved identity and dirty state on success and refusal.
- An independent Application consumer performs discover -> read -> computed edit on non-active
  Scene and Prefab documents, then one Undo/Redo and Save/Reopen. Retained receipts remain historical.
- Compare author concepts and engine-private changes with current explicit-value editing. Record
  index/storage cost and lookup/read timing; investigate any linear full-source-scan point-read
  slope or copying of an unrequested component. Entity/component/root lookup remains indexed;
  keyed-item scans have their explicit separate budget, not a logarithmic lookup promise. Do not
  freeze thresholds from fabricated timings.
- Reconcile ADRs 0047/0118 before shipping changed public contracts. Update the implementation
  ledger only after actual owner/public evidence. Selector-directed reads and one-level direct
  member discovery do not close durable Prefab overlay, recursive/collection discovery, value
  pagination, unopened source queries, full R2 or the Goal.

If the borrowed view requires reimplementing recursive default materialization, a second
classification service or a generic query planner, stop and deepen the existing source/Reflect seam instead. If inventory
plus reads makes the real tool's choreography worse, compare a focused combined projection on the
same consumer rather than adding every alternative to production.

## Pinned Reference Evidence

Read-only comparison used Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. These are mechanism references, not evidence that Nara
has already qualified the proposed transport.

- Bevy's `World::inspect_entity` enumerates the entity's actual archetype components, while
  reflected access separately resolves existing storage and reflection metadata. `Struct::field`
  borrows one field; `to_dynamic_struct` converts every field. Reuse the actual-inventory/borrowed-
  root split, not complete dynamic conversion. See
  `repo-ref/bevy/crates/bevy_ecs/src/world/mod.rs:891`,
  `repo-ref/bevy/crates/bevy_ecs/src/world/reflect.rs:69` and
  `repo-ref/bevy/crates/bevy_reflect/src/structs.rs:50` at the pin above.
- Godot composes an instance property list and separately calls `Object::get(name, &valid)`.
  A returned NIL alone cannot distinguish absence from a valid null. Revert/default lookup is a
  separate operation. Its Inspector builds a full property list, and callbacks may participate;
  neither mechanism provides Nara's scan/copy budgets or exact revision-bound continuation. See
  `repo-ref/godot/core/object/object.cpp:271`, `repo-ref/godot/core/object/object.cpp:558`,
  `repo-ref/godot/editor/inspector/editor_inspector.cpp:4504` and
  `repo-ref/godot/scene/property_utils.cpp:80` at the pin above.
- Godot placeholders retain some missing-script data but can remove absent/default-equal values
  on update. This is not Nara's unknown-record preservation rule. See
  `repo-ref/godot/core/object/script_language.cpp:723` at the pin above.

Both references support separating actual presence, interpretation and targeted access. Neither
provides Present/Defaulted source provenance, immutable admitted Schema identity and bounded
owned pages as a complete contract. Those remain explicit Nara responsibilities. In particular,
inventory plus reads must include root-key discovery; point reads of already known keys alone
do not satisfy R2.

## References

- [Foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [R2 seam investigation](../knowledge/engineering/subagents/2026-10/2026-10-24T215152Z-r2-selection-independent-field-reading-seam-389b97a2cf8f4275abbae7eeaa84bc6d.md)
- [ADR 0047](adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Source classification](../../crates/nara_scene/src/authoring_decode.rs)
- [Current entity query](../../crates/nara_tooling/src/entity_query.rs)
- [Effective defaults](../../crates/nara_reflect/src/effective.rs)

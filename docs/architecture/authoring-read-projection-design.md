---
title: Selection-Independent Authoring Read Projections
date: 2026-10-25
status: Direct source discovery, selected Prefab field inspection and property value pages
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
Charge examined declaration members, tombstone paths, optional unwrapping and key lookups
before the corresponding work. Collection enumeration also charges each visited identity. The
full selector contributes to metadata accounting before any owned observation is constructed.
The counts describe admitted work and output. Index comparisons, elapsed time and allocator
usage have their own costs.

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
the supplied value for an explicit root write. The default-backed edit retains its selector,
replacement and exact default witness. Undo checks the materialized result before restoring
omission. Storage parents must already exist. The draft still requires the current typed mutation
authority and publication limits.

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

The Editor inspects one directly owned anchor of a named open Scene or Prefab. The existing
selected-instance compiler supplies surviving targets, merged component records and applied
property claims. A read reports the effective field and the declaration that contributed to it.
This includes explicit Set claims equal to the inherited value and Absent claims that expose a
default or leave an optional root missing. Target routes keep their authored source/addition
facets and canonical base pins. Root-bound aliases share the same effective record. Copied
entity references use authored routes from the containing document and retain persistent IDs.

The Host captures the containing source, its nominal Schema basis and the file or working copy
that supplies each actually demanded Prefab. Open working-copy inputs retain their immutable
documents. The loader acquires a dependency's physical file observation only when selecting that
working copy. It releases decoded dependencies after the observation is ready and keeps their
source validation receipts. Borrowed discovery can query a retained snapshot without recompiling.
Selector traversal has a separate work allowance, and copying a selected value has explicit node,
byte and depth limits. The operation retains one bounded selected composition and uses the
compiler's existing work allowance. A panel can borrow the snapshot once per update and read
multiple fields from it. The borrow checks retained working-copy and document participation.
Individual field reads then use the snapshot's index.

An exact controlling property claim in the containing source can initiate Revert. An inherited
claim or a claim covering a broader selector does not authorize removing that declaration from
this address. Preparing and committing a Revert each check the live source and working-copy
identity and revalidate physical source evidence in the same Session drive that consumes it.
`PrepareRevert` passes the caller's `PrefabDeclarationLimits` to the source declaration owner.
When that allowance is exhausted, the inspection reports `BudgetExceeded` with the source
diagnostic. The existing prepared edit supplies the review and original transaction. Cancel and Session
Close retain pending inspection work until physical return. Revert changes the containing source
and creates one normal history entry. Read a new observation to see the value revealed by Revert.

The acceptance workflow uses two mounts of one base, a root-bound alias, an equal-value pin,
and a later base change. It checks stale and missing demanded sources, unrelated invalid sources,
the newly inherited value, exact Undo/Redo, and Save/Reopen of both containing source kinds.
The retained value represents the bytes and working copies originally acquired. A read of that
snapshot performs no disk access. Revert's explicit validation may read the admitted sources and
stable lookup scope on the owner thread. File observations provide bounded revalidation under the
existing filesystem contract. The operation composes source records without resource hydration
or Native preview admission.

##### Effective field verification

The implementation and independent consumer land together at `d175fa3`.
On Windows x64 with Rust 1.95.0, release nextest `e268fc9e-e8a1-48dc-8527-571a849f753a`
passes 68/68 selected Scene tests. The new cases cover merged root aliases, ordered claim
origins, working defaults and their reference demand, and three typed references under one
4,096-byte Map key. The latter copies the complete result at exactly eight nodes, 4,099
logical bytes and depth three. Each limit one unit lower refuses the same copy. The other
selected tests cover existing conversion, selected compilation and stored declaration Revert.

The first independent panel run `75312a87-026f-438d-be04-49c8f04b83d6` passes 14/14
on Windows in release. It includes both new registered semantic panel workflows alongside the
existing authoring cases. The new workflows refuse same-length base edits before preparation
and after review. Each then reads the inherited value after an exact Revert and verifies the
complete source through Undo, Redo, Save and Reopen.

Review moved the physical Revert fence into the consuming Session drive and put inspection
work into the public maintenance signal. Dependency file acquisition now waits for the existing
persistence owner only on actual compiler demand. The containing Prefab's shared document is
charged once for inspection. The validation receipt accounts for the surviving source facts,
working-copy selection and file capabilities after the compiler inputs are released.

Windows nextest `0adc5d06-0be7-4292-a0d1-b263bdd9781c` passes 19/19 selected tests. The
Host cases exercise deferred working-copy file acquisition, release of the worker's budget,
same-length disk drift between dispatch and the consuming drive, and physical Close after
Cancel. The existing native Windows/DX12 conversion still completes its Scene and Prefab
Review, Commit, Undo/Redo, Save and Reopen trajectory. That native case qualifies the shared
acquisition route. The effective-field workflow has its own semantic panel consumer.

The final independently locked Windows release run `88c2e869-8a09-4643-ba8f-507a2b8aebb0`
passes 15/15 with the optional cost diagnostic outside the selection. The fourteen ordinary
panel tests include the two complete effective-field workflows. The DX12 embedded Host verifies
hidden-window startup, painting and normal close with the same build. No latency or allocation
target was selected for this unit.

Affected library correctness Clippy passes for Scene, Tooling, Nara and Editor. Scene also
builds with default features disabled. Root and independent panel formatting checks and local
documentation links pass.

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

### Inspection Of An Unopened Project Source

A registered tool can select an indexed Scene or Prefab for inspection while continuing to work
in the active document. The Project Browser's existing document preparation lane admits one
named source on its I/O pool. Inspection returns a retained immutable source observation with
its original read capability. Workspace documents, history, selection and preview remain in
their current owners throughout the observation.
The Scene owner lends direct entity IDs, actual component records, interpreted fields and
Present, Missing and Defaulted values through the same classified source view as open documents.
Enumeration can resume after an ID. Tools bound their own iteration and use the existing selector
work allowance for property reads. A retained inspection reflects the admitted bytes and Schema.

An explicit `OpenInspectedSource` uses the original observation's locator and identity. The
normal Open path binds the actual file to its persistence owner, with the original body digest,
leaf and parent identities, admitted sidecar and Schema/index lineage. The I/O task checks the
current body against the original digest. The Session checks the current index, location,
file identity, aliases and source metadata before creating the Workspace document. Writes to
the same file after the digest check are subject to the ordinary Save conflict check. Mutation
preparation uses the newly opened document's query basis. Cleanup and cancellation remain with
the Browser request and its original task pool.

The consumer checks real Scene and Prefab files, including inspection that preserves unrelated
selection and history, deliberate Open, a value-derived prepared edit, Undo/Redo and Save/Reopen.
It also checks source or sidecar drift, late cancellation, and reuse of a stale result. The
selected workflow covers direct authored records.

Bevy's `DynamicWorld` loader exposes decoded source entities before any selected write maps
them into a target World. Godot's translation extractor reads `PackedScene::get_state()` by
asset path. Scene activation separately registers and switches the editor root. These designs
support reading saved records without constructing an editing or simulation instance. Nara's
explicit Open retains its nominal Schema, file and publication requirements. The selected
promotion checks the source digest on the preparation worker. Subsequent individual field
reads use the already admitted observation.
[Bevy loader at `85503941e488`](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_world_serialization/src/world_asset_loader.rs#L49-L75)
· [Godot extractor at `81ab961f9106`](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/translations/packed_scene_translation_parser_plugin.cpp#L42-L76)
· [Godot scene activation at `81ab961f9106`](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/editor_node.cpp#L5043-L5069)

#### November 20 unopened-source verification

The initial independently locked panel target passes 16 tests in
`e4184d05-7193-49c1-9386-d4ea9b71c143`, including the new Scene and Prefab workflows. This run
precedes the final review corrections. Both new consumers use the registered panel and real
stable-metadata project sources. They discover a stored value and compute the replacement.
Explicit Open, prepared Commit, Undo, Redo, Save and Reopen preserve the complete authored
payload and the unrelated dirty source's selection and history.

Independent review moves the promotion's final body check to its I/O task and checks the
current canonical file at publication. The Session drains completed preparation results during
physical Close. Cancelling an unrelated dirty document's Close preserves the inspected source.
The inspection checks existing file ownership, including a renamed file retained by an open
document. Six Host controls exercise those corrections, independent Save during pending
inspection, and rejection of a same-byte replacement after worker verification. The first
combined Host/Drawn run `a564c15d-e288-4461-9e87-2e57f3d061da` passes 39/40. The remaining
case exposes changed diagnostic precedence for an alias introduced after preparation. The
existing alias check now runs before comparison with the admitted object's identities. The
corrected run `80e5d9c4-96b3-40c1-9ab2-dd0883c31777` passes all 40 selected tests.
The final independently locked release run `5ffa7048-4c4f-41c6-b240-b736301004d1` passes all
16 selected panel tests, with the separate embedded Host case outside this run.

The complete native Drawn source workspace passes in `addb1517-1fdc-46e4-a4a6-4cabc21b08cd`
in 70.581 seconds with hardware DX12 selected and ImGui disabled. The original hidden Winit
window presents the exact inspection acknowledgement control and its status. The fixture
borrows the closed Prefab, verifies Workspace, preview and file continuity, acknowledges the
result through the presented Shell and continues its original regular Open and authoring
trajectory. Explicit promotion is exercised by the independent semantic panel consumers.

The selected evidence uses Windows x64 and Rust 1.95.0. Correctness Clippy passes for the
affected libraries. Rust formatting checks pass for the root workspace and the independently
locked panel workspace. Value pagination, all-source discovery and preparation locality retain
their own acceptance.
[Independent panel workflow](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/unopened_authoring.rs)
· [Host origin and close controls](../../src/project_host/runtime/tests/browser_preparation.rs)

### Selected Property Value Pages

A tool that examines a long description or a large collection needs to read it without
increasing the complete-value copy ceiling. It starts with the same named document, entity,
component and durable property selector as a complete read. Scene/Prefab resolves the admitted
value and its Present, Defaulted or Missing origin through the original selector walker. The
page inspects the borrowed payload at that location. A tool may select a wire key or list index
within the value using a relative field path. A position is local to this observation. Prepared
editing continues to require a durable property selector and its exact document basis.

Lists report their length and contiguous item positions. Maps report their length and keys in
canonical order; continuation seeks after the last admitted key. Immediate children report a
container length, text byte length or complete scalar. Their payload can be opened in another
page without copying unrequested descendants. Text pages contain complete UTF-8 scalars and
report byte offsets for concatenation. The result preserves explicit Null, empty containers,
empty text and Missing. A path into Missing or an absent or incompatible child refuses.

Each continuation retains the original workspace, kind, document basis, selected property and
relative location. After checking these facts, it borrows the same root and follows the wire
position already resolved by Reflect. Resuming thus avoids repeating the selector's declaration
and keyed-list search. Save, selection and edits in another document preserve the basis. Target
edit, history navigation, Reopen, Schema adoption and project replacement follow the current
basis rules. The Session refuses new page reads while Close is pending. A page is an owned
historical observation that can outlive the workspace.

Use the existing read limits. The selected container or text costs one output node. Returned
container and text headers cost one node each. Copied scalars retain their canonical value cost,
including reference routes. Copy work charges text chunks, Map keys and scalar data. A Map
continuation shares the returned key, and the page and continuation share their durable target
metadata. Relative path keys draw from the scan allowance, so a key returned by a Map page can
open its child even above the durable selector metadata ceiling. Selector lookup, relative path
probes and newly examined child identities share that allowance. A Map continuation seeks past
its admitted key without charging it as a new child. These counts bound logical work. Native
allocation, index comparison time and Editor frame work are separate.
An insufficient allowance either returns actual progress with a continuation or refuses. A
continuation means there is more data. Its next read can refuse under its supplied limits.
An indivisible key, scalar or UTF-8 code point that cannot fit on an empty page returns the
typed value-budget refusal. Complete-property reads keep their complete-value contract.

Godot's array Inspector addresses each visible page through its array slots, and Bevy exposes
borrowing access to individual list elements and map values. These references support paging
the selected structure. Nara's page additionally enforces exact working-copy and Schema
identity and keeps display positions separate from edit authority.
[Godot array at `b070e63b161a`](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/inspector/editor_properties_array_dict.cpp#L510-L542)
· [Bevy List at `779770720e6a`](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_reflect/src/list.rs#L59-L112)
· [Bevy Map at `779770720e6a`](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_reflect/src/map.rs#L52-L78)

Qualification uses a registered independently locked panel with a non-active source. A
complete read refuses under the selected allowance. The panel reads actual pages, derives a
durable prepared edit and uses ordinary Undo, Redo, Save and Reopen. Owner tests exercise
replay, terminal and exact budget boundaries, source-basis refusals, and repeated text or
collection pages without reading unrelated payloads.

#### November 20 property page verification

The first focused Windows release run `0ce0a949-eca5-4728-9b6a-1141bcefa21c` passed 11/11 before
review corrections. A broader fresh run `481f0f3b-2f22-4f42-a149-bfdd51a497d0` passed all seven
page cases and 220/221 overall. An existing combined oversized and over-depth selector exposed
changed diagnostic priority. The shared read path now admits metadata before returning the
depth diagnostic, matching existing complete reads and discovery. The corrected Tooling
release run `0c870eac-0d8c-4feb-b76d-c7c5e932b69a` passes 221/221 with one diagnostic skipped.

Review distinguishes returned Map keys from durable selector metadata. A long key now opens
its text through the relative scan allowance. It also found repeated keyed-list searches and
a Map anchor charge that could block progress after the preceding row fit. The cursor now
reuses the resolved wire position and seeks past the admitted Map key. Exact source fences
remain at the Workspace. The Session owns synchronous and routed close admission. A copied
entity reference uses the identity owner's bounded cost check before cloning.

Windows nextest run `d0471e8c-d255-47f7-a9f8-fbbfecfc2c2d` passes 2/2. It checks the same-batch
Host Close gate and reads the committed `Observed 属性 café` text in pages after a native DX12
Inspector edit. Actual Undo restores the earlier value and invalidates the prior continuation.
The Identity and Reflect libraries pass 392/392 in release run
`015cb0d2-02cc-4dd7-8cf6-d303952e48d9`, shared with the separate spatial regression selection.
The independently locked `prepared_authoring` target passes 19/19 in release run
`e60bdfae-6ca6-4053-980a-57856ec12505`. The registered Scene and Prefab tests each page 4,097
entries in a non-active document, compute a durable edit and verify the complete result through
Undo, Redo, Save and Reopen. The same run includes separate Cook and camera work.

### Prepared Edit Property Preview

The [prepared-source discovery successor](#discovery-in-a-prepared-source) replaces the original
property-only interface. This section records the earlier value-read qualification.

A semantic tool can read the proposed value of any direct property from a retained
`PreparedEditorDocumentEdit`. It passes the original `EditorPropertyRead` or starts an
`EditorPropertyPageRead` with the preparation's exact document, revision and Schema. Scene and
Prefab lend the same indexed property view used for live reads, over the candidate's document
and classification. The result uses the ordinary selector, presence, scan and copy limits.
The caller can use a live read from the original basis for the before value while that basis
is current. The candidate supplies the after value, including authored presence, defaults
and missing fields. The forward and inverse patches remain available for structural review.

A preview has the lifetime of its prepared edit. Its owned pages are historical observations.
Continuations bind to the exact candidate, including when two candidates start at the same
basis. The live workspace and another candidate refuse those continuations. A candidate can
still be reviewed after its base changes. Preview borrows the candidate for inspection.
Publication requires successful commit against the current source.

Acceptance uses an independently registered panel in non-active Scene and Prefab documents.
It compares live and proposed values, crosses a value page boundary, and checks cursor ownership.
The panel consumes the reviewed candidate through normal submission, Undo, Redo, Save and Reopen.

The pinned Godot Inspector records do/undo property values for an editor action. The pinned Bevy
reflector borrows a component at its actual entity storage location. Together these give concrete
precedent for reviewing property-sized data. Nara additionally retains an unpublished source
candidate with the classification and revision its commit will check.
[Godot Inspector](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/inspector/editor_inspector.cpp#L2981-L2992)
· [Bevy ReflectComponent](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/reflect/component.rs#L205-L216)

#### November 20 prepared preview verification

The independently locked release run `5b17f2c8-5adc-4ebf-a467-882ce32bea75` passes 19/19,
including 18 ordinary consumer cases and the preparation diagnostic. Each of the registered
Scene and Prefab consumers reads the proposed score alongside the current score, checks value
limits and cursor ownership, and commits the original reviewed edit. Undo leaves the retained
second candidate available for review and its stale submission is refused. Redo, Save and
Reopen produce the exact reviewed value under the original document's persistence identity.
[Panel consumer](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages.rs)

An independent source review corrected a diagnostic that mentioned a cursor for a complete read
with a mismatched document. Tooling release run `dfec11e6-51e7-479d-9d0f-286ec0423724` passes
230/230 with one pre-existing diagnostic skipped. The owner cases cover candidate presence,
value limits, structural removals and nominal readmission of the same Schema catalog without
advancing the live revision. The first two fixture attempts were refused by the existing
read-only component and placement rules. The final fixture uses a fully editable record and
removes its placement in the same prepared entity deletion.

Windows release run `3cc801ad-76d7-461a-8fd8-8a2de1600c70` passes both the Session Close gate
and the original DX12 Drawn Inspector workflow after the shared property reader change. The
native path reads presented, committed values. Candidate reads are exercised by the independent
panel and Tooling targets. Expanded Prefab values use the separate effective-field workflow.

### Discovery In A Prepared Source

A semantic tool reviewing a construction needs to discover the entities and fields that exist in
the candidate, including a newly resolved local reference. The prepared edit and the prepared
construction each lend an `EditorPreparedDocumentPreview`. It exposes the original `basis()` and
uses the existing entity, authoring and property requests. The preview answers direct inventory,
property reads and value pages. Stored Prefab claims remain observable through the same raw
declaration operation, including in a prepared Revert. The view borrows the exact source candidate
and the classification that commit will consume.

Each preparation has its own query owner. Every continuation and stored claim identifies that
owner. A preview can outlive a change to the live document as long as the tool retains the
preparation. Its continuations and claims are valid only on that same candidate. Live reads and
Revert preparation refuse preview claims. The original basis is used to describe preview facts and
does not make them live facts. Commit still uses its original admission and history. Expanded or
effective Prefab observation continues to use the separate admitted dependency workflow.

Use the existing pagers and budgets for live and prepared sources. The candidate replaces the
property-only preview methods with the shared view. The tool keeps one candidate for review and
commit. It needs no mutable document handle, patch interpreter or separate preview capture. Bevy
borrows reflected data from the entity's actual storage. Godot's section paste records the old
and new values on the Undo action for those exact edited objects. These patterns support
observing the owning data and retaining the same operation through review. Nara additionally
keeps the admitted candidate and its revision-bound history before any publication.
[Bevy ReflectComponent](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/reflect/component.rs#L203-L216)
· [Godot section paste](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/inspector/editor_inspector.cpp#L2976-L2992)

The registered panel uses `preview()` on the same preparation that it queues for commit. It
enumerates allocated entities, discovers components and fields through bounded pages, and reads the
resolved reference between two new entities. The Scene and Prefab workflows compare those values
after commit, Undo, Redo, Save and Reopen while another document stays active. The Revert workflows
also keep a historical candidate through live Undo and verify that the removed claim is absent from
its inventory and that the remaining declaration retains its authored Absent intent.

The independently locked Windows x64 release test target passes 24/24
(`88e926f7-0f1d-4a97-99ab-1bca508d3462`). It verifies replay on the same preparation, refusal when
passing an entity, authoring or value continuation to a different read source, and refusal of a
prepared claim at the live Revert boundary. The production diff passed an independent correctness
review. The preview covers direct source records and stored declarations. Whole-document edit
preparation cost and effective Prefab dependency observations retain their own qualification.

The Windows DX12 Drawn workflow passes 1/1 (`a4ec23d5-c2ce-4c6e-80b2-af39843af0c0`). Before
the presented Inspector Apply, a separately retained preparation reads the multilingual draft
while the working copy still has the earlier value. After Apply it still reads its original
candidate. The existing native trajectory checks live pagination, Undo, Save, Reopen and close.
[Drawn qualification procedure](../verification/native-editor-qualification.md#independent-drawn-source-workspace)

The final focused Scene and Tooling owner cohort passes 254/254
(`52036872-f4af-43ca-81dd-8769b81169f9`). It covers construction, stored claims, and the existing
live and prepared property readers, including nominal Schema changes and value presence.

The independently locked public consumers pass 12/12
(`83f2b0c4-2d9e-48ec-8425-a3f1f0b00082`) across the direct property authoring and registered panel
targets. The independent read surface remains window-free and does not add a document store.

### Selective Edits In Authored Collections

A tool can page an authored collection and edit an existing member using its durable Schema
selector. For an explicit root, the source patch records the selected replacement and the
inverse records the previous selected value. Changing the key of a keyed-list item updates the
inverse selector to the resulting key. Later writes in the same transaction can use that key.
An explicit whole-root write uses root-field history. The first nested write into an omitted
defaulted root uses the local materialization operation, whose inverse restores omission.

Qualification uses non-active Scene and Prefab documents with 4,097 stored keyed records.
It prepares and previews selected edits through the registered panel, verifies the record after
an ordered key change, and commits the retained candidate. Undo and Redo check the complete
source together with Save and Reopen. A failed final selector preserves source, selection and
history. The existing bounds apply to actual replacement payloads, selectors, and both
directions of history. Complete source classification and publication admission still apply.
Private staging allows the admitted source and one bounded field patch. This lets an exact-limit
document shrink one field and grow another in the same transaction. Publication and history replay
check the final source against its file limits and retained destination encoding.

The seventh workflow review reproduced the original fixed threshold at 86 two-field records.
Godot pairs a property write with the previous property value for Undo, and Bevy Reflect offers
mutable access to an individual list element. Nara's durable keyed selector applies the
individual replacement to the admitted source and gives inverse replay the appropriate key.
[Reproduction](../audits/2026-11-20-camera-recovery-and-authoring-review.md#p2-architecture--a-keyed-list-can-be-displayed-after-its-row-selector-stops-working)
· [Godot Inspector](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/inspector/editor_inspector.cpp#L5773-L5788)
· [Bevy Reflect List](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_reflect/src/list.rs#L59-L66)

The independently locked Windows run `3e770e3e-5922-4d54-a289-a11bf36a5af1` passes 29 tests
across the Scene and registered-panel consumers. The exact file-limit cohort passes 9/9 in
`008af713-218e-4685-8741-df91765d4c43`. A balanced mixed structural/property candidate reaches
the original source allowance, while one more byte refuses without history or source changes.
Both history sides survive nominal Schema readmission. The Host's same-batch pending Close
guard passes in `facba9af-c907-4383-b5c4-fc6b5f9c5907`. The original Windows/DX12 Drawn Shell
structure, history and persistence trajectory passes in `f8ad5d60-dea0-46bc-a7d0-b25a603b9a16`.
This edit qualification kept the original first-lookup scan, including the uniqueness check
across the list. The follow-up below addresses the separate 32,767-row refusal.

### Indexed Reads In Admitted Keyed Collections

In `06ce6a0`, direct Scene and Prefab records retain positions from the same uniqueness validation that admits
their stored and contextual default values. Each entry is relative to the admitted root and the
exact containing list. Canonical defaults share one index owned by the admitted Schema. The read
selects an index together with its exact root, so an explicit value hiding a default uses its own
positions. Nested lists share common path segments within that root. Read budgets charge
declarations, paths and one lookup per indexed collection step. Child pages still
visit the enumerated rows. Map keys and nested list positions are interpreted by the same Reflect
walker that reads a property in arbitrary current-version values.

Classification owns the index alongside the exact document, including retained prepared edits.
Scoped admission charges its modeled allocation. History replay reconstructs positions from the
replayed value. Each ordered write resolves its selector against the evolving candidate and checks
for key conflicts before publication. The source owner retains all index positions.

The separate write refusal came from the existing persistence check. Scene and Prefab publication
validate canonical JSON and RON. Two-space RON indentation in `617b920` keeps the reviewed two-field
workload inside the original 16 MiB format limit. The same grammar and shape checks apply.

Bevy's `DynamicStruct` retains names and slot positions in the same container. Godot's `SceneState`
owns the node-path index it uses for edit-time lookup. The Scene and Prefab source owners use the
same ownership rule for Schema-defined row IDs while preserving authored list order.
[Bevy `DynamicStruct`](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_reflect/src/structs.rs#L306-L381)
· [Godot `SceneState`](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L1570-L1630)

The independently locked Windows release run `a62d78c6-1985-44eb-863d-b255ccc53f72` passed
20/20. The registered panel exercises non-active Scene and Prefab documents with 32,768 keyed
records each. Its first and last property reads use a 128-entry, 4 KiB lookup allowance.
It previews and commits a scalar beside the list, changes the last row's key and value in order,
and rejects a missing or duplicate key in the final operation. Undo, Redo, Save and Reopen compare
complete JSON documents. The test also checks the active document and selection, cursor refusal
after commit, and the new source incarnation on Reopen.
[Independent panel regression](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages.rs)

The Windows release owner cohort `546bba23-4246-4ad4-aaa2-5cb79aa185f1` passed 1,382/1,382
with one ignored diagnostic. A Tooling read starts the 32,768-row numeric list's last value page
with five scan entries. Reflect verifies separate nested row positions and one retained copy of a
common long Map key. Scene proves the distinct canonical-default and explicit positions and
round-trips both large document kinds through RON. Migration tests still reject a specific
RON-only byte overflow and retain the incumbent document. These runs measure scan counts and
encoded sizes. Preparation latency and resident memory have no measured claim in this slice.

Correctness Clippy and formatting passed. The original native Drawn source workspace completed
its structure, history and persistence run on Windows/DX12 in
`80e0981f-6db6-4f36-b9c3-2886cbf06357`. The first linker attempt failed while writing its PDB
with 0.27 GiB free on the build drive. The same command passed after moving the partial PDB
off the drive. The large-list acceptance ran through the registered-panel interface.

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
  index/storage cost and lookup/read timing. Entity, component, root and admitted keyed lookups
  use the owning index. Collection enumeration and unindexed values have their own scan costs.
- Reconcile ADRs 0047/0118 before shipping changed public contracts. Update the implementation
  ledger only after actual owner/public evidence. Full field query coverage, durable Prefab overlay,
  and the remaining R2 preparation and cost work retain their owning acceptance requirements.

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

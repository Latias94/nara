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

Preflight caller selector metadata before Source work. Admit the retained target, child identities
and any separate successor identity before cloning output. Recursive rows share the immutable
parent selector with the target. A Map cursor reuses the last emitted key.
Use saturating length accounting so oversized untrusted selectors refuse before copies.
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

### Metadata-Limited Property Discovery

Scene/Prefab receives the remaining child identity allowance after Tooling admits the target.
It returns the fitting prefix and a borrowed successor position. Metadata exhaustion with zero
returned rows refuses the page. When a later child would exceed the allowance, the next page
starts with that child. Examination charges scan work before output admission. Empty pages
can still advance through anonymous Record declarations and unrelated tombstones.

`EditorAuthoringPage::target` is shared with its direct cursor. `Properties` takes a shared
parent path, and its row selector exposes `child_step()` and materializes the complete durable
path with `to_path()` for a selected read or edit. A detached row retains only its parent path
and step. A Map cursor retains that step and resumes through the original ordered range in the
exact live or prepared source.

Bevy's `Map::iter` borrows keys and values from the reflected map. Godot's Inspector constructs
dictionary rows for the selected page and retrieves the keys by index within that dictionary.
Nara also borrows keys for discovery, with a private replayable continuation to keep the page
bound to its source revision and preparation. [Bevy `Map`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/map.rs) ·
[Godot dictionary Inspector](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/inspector/editor_properties_array_dict.cpp) ·
[Audit reproduction](../audits/2026-11-22-starter-input-authoring-and-runtime-review.md#p2--a-legal-large-map-key-can-make-the-property-catalog-unpageable)

On Windows x64 and Rust 1.95.0, the scoped Tooling query targets passed 38/38
(`5f95dc99-2017-4cc4-8615-59028617a7e1`). The first run passed 37/38 and exposed a Root query
test still expecting a duplicated target charge. The corrected exact-budget and one-byte-short
controls use the shared target cost. Selected Scene tests passed 83/83
(`2cd20b9c-48d4-43b5-ba4c-0e2597278354`), including borrowed Prefab inspection, the examined
but unreturned row charge, and an invalid Map anchor.

The independent semantic Host passed 10/10 (`ba56e311-26ce-4de3-a50c-e2bb5dfe7d9f`). Its live
and prepared views page two 32,768-byte keys with a common UTF-8 prefix and a short tail under a
16 KiB parent. The maximum 256-row request fits the first key in 64 KiB, and a continuation can
choose an exact smaller allowance. Both non-active document kinds reject stale and sibling
candidate cursors. The selected long key supports Prepare, Commit, Undo/Redo and byte-exact
Save/Reopen while the active tab and its selection stay intact.

The independently locked prepared-authoring consumer passed 33/33 with nine existing skips
(`55cd0c9c-7ebd-4763-bece-03cf04b7b902`). Scene and Tooling all-target/all-feature correctness
Clippy passed. Workspace, semantic Host and registered panel package formatting passed. The
first aggregate module-consumer formatting invocation reached the Windows command-length limit,
and the affected panel package passed its own check. Independent review checked cursor progress,
shared ownership and metadata accounting. These results cover the Windows semantic workflow.

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
composition. Source repair can prepare Revert using the retained live claim address. A
candidate claim handle remains attached to its candidate. Repeated instances with equal
declaration indices cannot redirect that address. A consumer with a full stored observation
selects a Revert from the original live `PrefabInstanceObservation`. The selected action retains
the observation's exact revision, Schema and nested path. Scene checks those facts against the
current source before accessing the selected slot. A direct borrowed property Revert requires the
stored declaration from that exact live revision and path. Preview and detached overlay borrows
remain available for display. Equal values in sibling declarations cannot establish live identity.

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

#### November 22 exact declaration Revert

The Windows x64 / Rust 1.95.0 owner run `fd1250a9-e00f-40c7-b02c-7c97f05ad33e`
passes 106 of 107 selected tests. The Scene tests refuse property coordinates and component
coordinates without their declarations, and also refuse same-slot candidate declarations with
a different field or floating-point zero sign. The built-in Inspector tests select a property
row, retain its exact candidate across tab navigation, then cancel or commit it and replay one
Undo. The fixture drives one fresh paint before clicking Refresh after externally dispatched
Undo. A separate conversion work-budget test fails at `classification-snapshot`; the same
failure reproduced in the older Scene test binary from before the Revert change.

Independently locked release run `45f9f2c1-4e0e-497f-9854-59df4b2bfec2` passes 11/11 selected
panel tests. The non-active Scene and Prefab workflows reject both the opaque candidate handle
and the candidate's different observed declaration in a live Revert. They check the equivalent
live declaration's exact forward and reverse patches, successful history replay and persistence.
The separate Windows/DX12 conversion trajectory passes 1/1 in
`6787a34e-ed1a-4aeb-94bd-087b769a7df5`, covering the common selected-source and document
publication paths. Independently locked release run `3263284d-a39f-4d14-a880-49c917ae0ac9`
also passes the public extraction and inheritance trajectory after its Revert migration.
After review migrated the independent component-intent consumer, release run
`7a4a3095-50f1-4233-9836-73822e925429` passes its 1/1 case. Both source kinds check Cancel,
outdated preparation, preservation of an unavailable sibling, Undo/Redo and Save/Reopen.
[Independent panel regressions](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_property_claims.rs)

#### November 23 nested Revert identity

The source owner rejects a generic Revert if the chosen property was borrowed from a preview or
another instance. A live borrow carries its document revision and must match the original slot
at the selected path. The retained `PrefabInstanceObservation` can select a property, whole
component or overlay while preserving the original revision, Schema and complete path. The
built-in declaration panel prepares from that observation.

Windows x64 release nextest passes 54/54 selected Host, Scene and Tooling tests
(`fb1b6419-6379-40e9-877a-5056bdca4e60`) and 34/34 tests in the independent consumer
(`6f275ff1-6758-4a3b-b670-71b761555e05`). Scene checks same-value siblings and a foreign session
that shares the declaration storage. The independent Scene and Prefab consumers remove the
first nested instance in a preview. They confirm that using the second instance's equal
declaration at the old live index changes neither source nor history. The exact live selection
produces one inverse and survives Undo, Redo, Save and Reopen. The independent run also covers
existing effective property Set, Absent, Apply, complete component edit and conversion workflows.
The built-in Prefab Intent and Windows DX12 conversion selection passes 11/11 in
`be0baaf3-bee8-4889-baa4-8aab4a38cc37`. The first Editor test build exposed four remaining direct
reads of the observation fields in the fixtures. The passing run uses the observation accessors
with the same assertions. Scene also builds without default features. Focused correctness
Clippy and format checks pass.
[Audit reproduction](../audits/2026-11-23-prefab-authoring-player-and-input-review.md#p2--generic-revert-can-remove-an-identical-declaration-from-the-wrong-parent)
· [Independent nested Revert](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_property_claims/nested_revert.rs)

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

##### Explicit scalar overrides

`PreparePropertyWrite` with `Set(value)` writes a scalar selected from the effective inspection
into the direct instance overlay of the containing Scene or Prefab. The compiler supplies the
canonical base and the selected layer's existing component group even when the read used
another root-binding alias.
The source owner writes one `Set` claim and preserves the existing target spelling of that group.
An explicit Set equal to an inherited value pins it. The remaining fields keep their existing
inheritance. The selected component and field must admit editing. A complete component owned
by the containing anchor or selected overlay has its own authoring location. Strict ancestor or
descendant overlap within one property group refuses the write. Repeating an identical stored Set
returns `AlreadyWritten` and creates no history entry. Specialized UI Prefab fields retain their own
domain admission, and unrelated UI records can coexist with a scalar Set in the same source.

The original declaration and its proposed replacement enter the same prepared edit and history
owners as Revert. Before review, the Host uses the selected compiler on the exact unpublished
candidate with its original files and working-copy selections. It checks that the original
address now observes the requested scalar under an exact Set from the selected containing layer.
This also refuses writes that remove the selector's meaning, such as changing the key used by a
selected keyed item. The recompilation runs on the existing inspection task lane. A read-only
inspection releases decoded dependency inputs once Ready. The write path reacquires the demanded
inputs only for the unpublished candidate and compares their file, lookup and working-copy facts
with the original observation. The verification uses the candidate at every actual demand for
the containing Prefab itself. Files read as content keep their canonical source and exact digest.
Open working copies also keep their original file object and live revision. Commit revalidates
those original facts and consumes the exact reviewed edit. `prefab_inspection_review` borrows the
same prepared candidate view, patch and inverse for writes and Revert. Cancel and Close retain
pending work until physical task return.

The Set operation covers editable scalar leaves available through their current selector.
Collection identity changes, writes to an owning complete record and applying an override into a
different source have separate contracts.

Godot retains a pinned property even when the current value equals the inherited default and
stores pinning in its normal Inspector Undo. Bevy copies a cached component template on the
first local write, preserving the cached template for other consumers. Nara keeps the same
authored distinction between local and inherited values using its existing stable selectors
and exact source transaction.
[Godot packed scene](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.cpp#L1061-L1085) ·
[Godot Inspector pinning](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/inspector/editor_inspector.cpp#L6022-L6023) ·
[Bevy resolved templates](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_scene/src/resolved_scene.rs#L463-L503)

The acceptance workflow uses two mounts of one base, a root-bound alias, an equal-value pin,
and a later base change. It checks stale and missing demanded sources, unrelated invalid sources,
the newly inherited value, exact Undo/Redo, and Save/Reopen of both containing source kinds.
The retained value represents the bytes and working copies originally acquired. A read of that
snapshot performs no disk access. Revert's explicit validation may read the admitted sources and
stable lookup scope on the owner thread. File observations provide bounded revalidation under the
existing filesystem contract. The operation composes source records without resource hydration
or Native preview admission.

##### Authored root absence

`PreparePropertyWrite` with `Absent` uses the same selected instance layer and exact prepared
transaction. It accepts a declared editable root whose schema makes the field optional or
supplies a default, and whose storage path ends in a map field. The root can already read as
Missing. The candidate must report the exact Absent claim in the selected layer and an effective
value of Missing or Defaulted. No replacement value is retained in the claim. The same compiler
resolves defaults in their original occurrence scope.

Clearing the last retained reference can remove a source demand. Revealing a reference default
can introduce one. The Host compares file facts, lookups and working copies shared by the
original and candidate captures. It retains every original observation and admits the
candidate's additional inputs through the same source budget. A finite self-reference can add
the containing Prefab as a working-copy participant while using an existing file observation.
Commit revalidates the original inspection, including its removed demands, together with the
newly demanded sources. A changed file, lookup or working copy refuses the exact candidate
before publication.

`Set(Null)` pins a value where the declared shape permits Null. An Absent claim clears inherited
storage and can expose a default. An exact Revert removes either claim and reveals the next
inherited state. Repeating a directly stored Absent returns `AlreadyWritten`. An inherited
absence can still be pinned in the current layer. These operations share the existing document
history, unavailable-record preservation, publication format and Save/Reopen owner.

##### Effective entity-reference writes

The selected property write accepts an `EntityReference` at a declared editable reference leaf.
A missing optional reference root can acquire its first Set when its storage parent exists.
A nullable reference can pin explicit Null. The containing instance layer stores the exact authored
reference. An inline route starts in the lexical scope of the containing occurrence and can target
an ordinary local record, another instance or a root-bound alias. The existing reference compiler
checks the stated base at each hop and resolves the route before Review. Persistent references retain
their durable identity and follow the existing runtime lookup contract.

The candidate must compose the selected exact Set. The inspection uses the controlling claim's
slot to check the authored operand in the prepared source and takes the effective endpoint from
the original compiler. Root-binding aliases and declared input arguments can resolve to another
spelling of the same target. The Host applies one freshness rule to property writes. It compares
the shared observations, retains the complete original capture and adds the candidate's new file,
lookup and working-copy evidence under the existing allowance. Final Commit revalidates both
sets, including a dependency the original value used and the candidate dropped.

Godot's NodePath picker derives the stored path in the edited node's context. Bevy's entity mapping
gives a clone its own entity destination map. These inform the separation between a written
reference and its instance-specific resolution.
[Godot NodePath picker](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/inspector/editor_properties.cpp#L3036-L3066) ·
[Bevy entity mapping](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_ecs/src/entity/map_entities.rs#L19-L55)

##### Structured property overrides

`PreparePropertyWrite` sets a complete Record, Map, list or keyed-list item in an inherited
component. Scene resolves the shape through the admitted Schema. The selected compiler validates
the component, the enclosing keyed item's identity and nested references. Omitted fields,
explicit Null, collection keys and authored reference operands keep their source semantics.
The selected property keeps one exact Revert and the containing document's ordinary history.

The declaration owner checks the complete instance patch cost, including the original and
replacement declarations and the new value's depth, before the request waits for a drive.
The pending request holds a compact copy of the admitted payload. Repeated writes and same-layer
ancestor or descendant conflicts keep their original diagnostics and carry no proposed value.
Lowering uses that admission against the original working allocation, revision and Schema.
An existing declaration group keeps its stored target spelling even when an equivalent alias
selected it. Candidate verification compares the exact stored payload and target at the
compiler's controlling claim. The compiled value can resolve its own endpoints without changing
the authored operands.

During `Review`, `prefab_inspection_review` borrows the candidate patch and inverse and
`prefab_effective_observation` borrows the already admitted compilation of the same candidate.
Commit retains the original and newly demanded source observations. The registered panel can
compare stored values with resolved endpoints before changing either the document or its history.
Cross-source promotion of a structured claim still needs a portability check for each nested
reference and its binding chain.

Godot's dictionary control sends the complete edited Dictionary to the property owner. Bevy's
`MapEntities` visits nested ordered maps and vectors in the instance's mapping context. Nara
similarly edits the selected complete value and lets the compiler resolve its reference leaves
for the occurrence shown in Review.
[Godot dictionary edits](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/inspector/editor_properties_array_dict.cpp#L1066-L1077) ·
[Bevy collection entity remapping](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_ecs/src/entity/map_entities.rs#L139-L178)

##### Whole-component overrides

The same inspection can edit a complete component in the directly owned instance layer of a
named Scene or Prefab. An effective target exposes its present records and a separate borrowed
inventory of component declarations in that layer. The second inventory includes Suppress,
which has no effective record, and typed root aliases share the same declaration. The compiler
retains the declaration kind and position and the ownership of the underlying record. That last
fact still identifies a local record under a stored Suppress or Replace. The source owner reads
the complete declaration from the working allocation captured by the inspection. The allocation
and revision are both checked before lowering, without retaining a copy of the document.

`PrepareComponentEdit` takes the selected target, component ID, intent and declaration limits.
Add requires an absent lower-layer record. Replace and Suppress require a lower-layer record,
including one already hidden by a local Suppress. Set supplies the complete authored value, so
omitted members and explicit values keep their ordinary Schema semantics. The ordinary record
constructor captures all contextual defaults. The immutable Schema admission records the field
positions once. Construction, patch budgeting and candidate verification borrow them from the
same admission. The patch budget includes the captured default values. Their references resolve
in the containing source layer, and an explicit field can shadow a default without removing it
from the declaration. Revert removes the exact local declaration and exposes the lower layer
again. A complete replacement or suppression
can supersede a selected property group. Review shows that change and the inverse retains the
original group. Components declared directly on the containing anchor or on an addition's own
record remain edits of their respective storage locations.

Scene and Prefab prepare the existing `SetPrefabInstance` transaction. The Host compiles the
exact candidate and checks the stored intent and its effective consequence at the selected
address. A complete Set can introduce authored references. The compiler resolves them and the
Host retains both the original source evidence and the evidence acquired at new candidate
demands. The bounded complete value is copied into compact storage before the task retains it.
The same inspection ID binds Review, Commit and Cancel. Commit reports the accepted component
edit kind and revision, and the original document history supplies Undo and Redo.
The separate stored-declaration Revert remains available for repair when composition is rejected.

Godot's resource picker distinguishes assigning an empty value from making a local copy of a
shared resource. Bevy's resolved Scene applies a prepared template to its destination, and its
dynamic World update retires the previous instance before materializing the current contents.
Nara likewise keeps authored complete-value assignment, explicit component suppression and
runtime materialization as separate decisions, with per-occurrence source history.
[Godot resource picker](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/inspector/editor_resource_picker.cpp#L494-L510) ·
[Bevy resolved Scene patch](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_scene/src/scene_patch.rs#L69-L88) ·
[Bevy dynamic World update](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_world_serialization/src/world_asset_spawner.rs#L397-L429)

##### Apply an instance property to its source

Apply identifies an exact controlling claim in the containing instance and the explicitly named
open Prefab that owns the contributing source record. The source owners prepare a property edit
for the destination and a Revert of the containing declaration. One selected compilation uses
both candidates. It verifies that the selected occurrence inherits its previous effective value
from the edited record after removing the claim. Review exposes both source candidates. The
destination change can affect other occurrences that inherit the same record.

Root-bound aliases can select the same component. The compiler identifies the record that
supplies it and retains the exact stored claim for Revert. An authored entity reference can
move into the destination only if the traversed instance declarations and endpoint belong to
its source subtree. An Input reference also requires every argument in its binding chain to
belong to that subtree. The original resolver checks those facts while resolving the reference.
Apply removes the destination prefix from the original operand and keeps the Input selector.
The candidate must resolve to the same endpoint in the selected occurrence.

Godot records node paths relative to their property owner, then resolves them after inherited
nodes have been instantiated. Bevy's named entity references resolve in their own invocation
scope and can be passed into another scene. A nested Nara Input retains its authored parameter
name so each instance can resolve it in its own scope. [Godot path storage](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/resources/packed_scene.cpp#L968-L977)
· [Godot deferred resolution](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/resources/packed_scene.cpp#L673-L693)
· [Bevy reference scope](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_scene/src/lib.rs#L219-L236)

Use the exact saved containing source for promotion. Any working copies supplying other selected
inputs must also match their confirmed checkpoints. Pending persistence blocks its own
participating document. The destination may have changes of its own. Ordinary Save publishes the
whole destination working revision after Commit. While Save is pending or refused, the containing
source retains the selected override. `FinishApply` verifies a Save of the exact revision for
modified content and the original confirmed checkpoint when it reuses saved content. It
verifies the file in the same source slot and revalidates the other original and candidate inputs
before consuming the containing Revert. The containing source then has its own ordinary Save.
Each document owns its own Undo and Redo entry.

`DestinationSaveRequired` reports the destination checkpoint. The final source check uses the
retained file observations when it consumes Finish. An independent destination Save can succeed
if another file changed on disk in the meantime. In that case Finish stops and preserves the
containing override.

The recoverable intermediate state is the destination publication with the containing pin still
in place. If this state survives process exit, a new inspection can promote the same claim when
the saved destination already carries the intended value. The existing file-publishing Hosts own
refusals and retries. A restart begins a fresh review. Multi-file recovery of the original review,
coupled Undo and promotion into an intermediate overlay layer retain separate design scope.

Unity's property Apply supplies an explicit source destination. Godot associates Undo with the
edited scene and marks its saved history only after writing the scene. Bevy's scene patch applies
an asset to a particular ECS instance. These references motivate explicit source identity and
document-scoped editing and persistence.
[Prefab workflow study](../research/2026-10-26-unity-godot-prefab-editor-ownership.md) ·
[Godot scene Undo](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/editor_undo_redo_manager.cpp#L63-L127) ·
[Godot scene Save](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/editor_node.cpp#L2540-L2610) ·
[Bevy scene application](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_scene/src/scene_patch.rs#L69-L110)

###### Structured property promotion

An exact Set claim can promote a complete admitted Record, Map, List or keyed-list item through
the same Apply operation. The selected compiler joins all scoped reference results for the exact
claim. Every route and argument chain must belong to the destination. A single failure rejects
the claim even when it currently resolves to the same entity. The existing value copy visits
references in the admitted value without building a storage path per leaf. It preserves wire
members, collection keys, Null, and all non-reference values. It removes the destination prefix
from each authored reference and keeps the remaining route and Input selector. The ordinary
property editor still admits the destination's stored value and exact inverse.

For a claim with scoped references, admission charges the raw and projected values before
retaining copies. The joint candidate compiler checks the complete effective value with exact
float comparison and normalized reference endpoints. Remaining ancestor or descendant pins
prevent Apply, including pins that happen to produce the same value. The destination retains its
ordinary Save fence. The selected occurrence is the observed scope; other occurrences can
inherit the new value or report an authored conflict on their next compilation.

Assess the workflow through a registered panel on non-active Scene and Prefab documents. Review
the exact destination patch and inverse, retain the containing pin until the destination is saved,
and verify separate Undo/Redo and Save/Reopen. Compare two sibling occurrences with destination-local
references. Exercise a mixed-ownership claim whose two leaves resolve successfully and a retry
from the saved destination checkpoint. Godot resolves deferred node references in typed Arrays
and Dictionaries relative to the owning node. Bevy applies one entity mapper to every container
element. Nara uses the same path-free value copy for projected reads and destination operands.
[Godot collection resolution](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/resources/packed_scene.cpp#L680-L722) ·
[Bevy collection remapping](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_ecs/src/entity/map_entities.rs#L139-L178)

On Windows x64 with Rust 1.95.0, nextest `17e7ba89-e9b2-45b6-80a3-e1d5a813ae88` passes
51/51 selected Scene cases. The structured controls compare the original combined value
budget, independently changed reference endpoints, signed zero and an equal value still pinned
by a descendant. Independent review identified a repeated Map-key charge in the temporary
Apply reference plan. The shared value copy admits a 16 KiB key with eight references under a
36 KiB declaration allowance and verifies the resulting candidate. A 32 KiB key with eight
references in the same fixture reaches the existing `reference-work-bytes` limit during selected
compilation.
[Scene Apply controls](../../crates/nara_scene/src/prefab/inspection/property_apply/tests.rs)

The selected Host and Tooling suite passes 30/30 in `f5a258e7-a383-4ed2-86f4-248e677159fe`.
The independently locked release panel passes 33/33 in `4f244fc8-d999-4d42-938a-7c0217c000e4`.
The new public cases compare both reviewed patches and their inverses, confirm the saved
destination before consuming the pin, replay both histories, and read each sibling's normalized
references after reopening. The Prefab case cancels at the saved prefix and retries without
a destination edit or history entry. The refused Scene case resolves two valid endpoints,
including one owned by a sibling. Apply preserves the original claim and both files.
[Registered structured Apply workflows](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/apply/structured.rs)

The shared native conversion trajectory passes 1/1 in Windows/DX12 release nextest
`2e63bdc0-21d1-4a55-bf33-14e6afa4d16b`. It covers Inspector input, reviewed conversion, history
and disk round trips in both source kinds. The independent panel cases qualify structured Apply
through the shared authoring and persistence APIs. Scene correctness Clippy and the
no-default-features library also pass. The root workspace and independent panel pass formatting.

##### Open design for applying complete component declarations

The existing compiler requires an absent lower-layer record for Add and a present record for
Replace and Suppress. Saving an Add to the destination while its containing declaration is
still on disk makes the original Add conflict. Removing the destination record while its
Suppress remains has the same problem. Saving the containing Revert first temporarily loses
the effect of the selected override. Other instances can also carry declarations that conflict
with the new presence. The property Apply checkpoint order therefore needs a different
publication contract for operations that change presence.
[Presence checks](../../crates/nara_scene/src/prefab/overlay/compile/staged.rs)

`AuthoringPrefabTargetView::component_override` now returns the selected declaration and the
physical origin of its lower record, including when Suppress makes that record absent from
the effective view. The compiler retains that location at the point where it applies the
selected layer. A root binding can merge two facets into one visual entity. Selection of either
facet reports the same winning lower record, so the caller can distinguish the visible entity
from the Prefab declaration that would be edited. The view borrows the location from the
admitted snapshot and preserves the existing bounded query.
[Selected declaration observation](../../crates/nara_scene/src/prefab/inspection.rs)

Use the same presence rules for the planned component Apply. Scene prepares a pair against the
original working copies and the common Schema. For Replace and Suppress, locate the physical
lower record. For Add, require an explicit writable destination facet, and prove both the
merged lower program and the selected storage slot are empty. An overlay origin identifies its
instance and slot. A direct record write requires a direct source record.

For Add and Replace, move the complete stored record, including its owner, version, omissions,
explicit Null and contextual defaults. Use a component-specific request in the selected
compiler to prove the effective and dormant reference routes. Each moved reference retains
its destination-relative route and the original Input selector. The destination must own the
traversed declarations, every binding in the argument chain and the endpoint before alias
merging. Ordinary inspection continues to follow surviving reference demand. Validate the joint
candidate against the original selected occurrence. Require equal effective values and
endpoints, the exact new physical contributor, and inheritance of the promoted record without
a property pin. Suppress requires the selected entity to survive and the component to be absent.
Removing the correct record can expose a second record through a root binding.
[Reference demand](../../crates/nara_scene/src/prefab/overlay/compile/staged/reference_program.rs)

Keep this as a concrete Project Host operation using the existing project Pending/Ready
boundary. Review binds the exact containing declaration and destination, selects their two
existing files for replacement, and validates the affected saved composition using the captured
disk sources for the other members. The selected independent consumer includes a sibling that
inherits the change and a closed dependent whose conflicting declaration prevents confirmation.
Use `SourceInventory` for physical membership and environment evidence, `RecoveryStorageHost`
for the authenticated Pending package, and `FilePublicationHost` for each fixed output. After
both outputs are accepted and the final saved generation is verified, bind both live
destinations before completing the original project's Ready transition.
[Source inventory](../../src/project_host/authoring/source_inventory.rs) ·
[Recovery storage](../../src/project_host/authoring/recovery_storage.rs) ·
[Extraction publication](../plans/2026-11-02-prefab-extraction-plan.md)

Review explicitly selects a new document-history checkpoint for the replaced sources. Guard
documents retain their existing history. Independent Undo entries can restore a conflicting
declaration on only one side. A future paired Undo requires the same joint validation and
publication guarantees. The new checkpoint uses the existing Scene source-history owner.

The complete acceptance runs Add, Replace and Suppress from preparation through Review and
reopen. It includes exact declared storage and contextual defaults, a reference with an
outer-owned argument that happens to resolve to the same endpoint, and an incorrect source
facet at a bound root. Interrupt actual publication after the first output. Fresh Check and
Play must refuse the Pending generation. A fresh process uses the authenticated package to
retain the already-published member's identity and exact bytes, publish the other member,
and approve Ready. Also exercise a third file state and dependent drift. Ordinary Reopen,
Check and a later edit with Undo verify the completed generation and history checkpoint.

###### Lower record and mixed-generation qualification

Windows x64 nextest `93040e7f-2fd5-446e-bb66-db37e14bab10` passes 53/53 selected Scene tests
with Rust 1.95.0. At a bound root, both aliases expose the same lower direct source or
addition while the selected declaration suppresses it. A second control removes the exact
lower direct record and its Suppress from the two candidates. The compiler then exposes the
nested Prefab's record. This makes effective absence a separate acceptance condition from
identifying the right storage slot.

The original compiler accepts the paired candidate for each intent in a simple direct source.
The destination-only Add and Suppress candidates fail component composition when the original
declaration is still present. Destination-only Replace keeps its pin. Publishing only the
containing edit changes the effective value or presence for all three intents.
[Selected Scene controls](../../crates/nara_scene/src/prefab/overlay/compile/selected_tests/component_edits.rs)

The independent `prepared_authoring` selection passes 33/33 in
`44ee4514-0a33-4f12-bac5-281f98d24340`. Registered panels query the declaration and its lower
record in both non-active Scene and Prefab documents, including nested sources. The cases retain
their Review, Revert, Undo/Redo, Save and Reopen assertions. Scene correctness Clippy, minimal
library compilation and root and panel format checks pass.
[Registered component workflows](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/component_edit.rs)

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

##### November 22 scalar Set qualification

Windows x64 / Rust 1.95.0 nextest run `f1025ab1-93d8-4b25-895b-414f42d381f3` passes
63/63 selected tests. That includes the original Scene, Tooling, selected source acquisition
and operation lifetime controls and the actual Windows/DX12 Prefab conversion trajectory.
The Host tests confirm physical task return on cancellation and rejection of changed content.
They also replace an unopened dependency with identical bytes separately before Prepare and
before Commit and verify that both admit the same content.

The independently locked release run `20429830-ebd6-48fd-8192-dcd23ef52451` passes 30/30
selected tests, including 29 panel tests and the hidden-window Windows/DX12 embedded Host.
The registered panel discovers a scalar in a non-active Scene or Prefab, derives the new
value, queries the exact prepared candidate, then commits it. The fixture checks Cancel,
one history entry per Set, Undo/Redo, Save/Reopen and Revert. It observes an equal-value pin
after a change to the shared base and confirms that the other instance inherits the new base.
Other controls exercise the nested storage route behind a direct addition alias, an exact
already-stored Set, read-only and overlapping claims, budget refusal, a change to a
participating open Prefab and a candidate that would break a finite recursive Prefab.
The recursive test requires a composition diagnostic before any review or source edit.
[Registered panel cases](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/set.rs) ·
[Recursive regression](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/recursive.rs)

The independent panel cases exercise the semantic API. Native qualification covers the shared
source transaction and selected acquisition used by existing authoring controls.
Correctness Clippy passes for the affected libraries, Scene checks without default features,
and the affected packages pass formatting and local documentation link checks.

##### November 22 root absence qualification

Windows x64 nextest `d94cfb13-47fe-4dd7-bdc6-089ee1574c9a` passes all 125 non-native
cases with Rust 1.95.0. They cover exact declaration limits, duplicate Absent, a claim on an
already Missing field, pinning an inherited Absent in an outer instance, and worker custody
through Cancel and Close. The Host cases remove a file demand, introduce one through a default,
and first demand the current unsaved containing Prefab through a finite self-reference. The
file cases reject a source change after Review with the document and history intact, then
commit exactly once after a fresh inspection. The containing Prefab case resolves the new
default to an entity available only in the working copy.
[Source-demand regressions](../../src/project_host/runtime/editor/prefab_conversion/tests/effective_absent_reference.rs)

The Windows/DX12 Prefab conversion trajectory reports a missing painted mapping row in that
combined run. The same trajectory passes in isolation in nextest
`df333067-3c61-4251-928e-02f48e7e78bd`, including review, commit, history and persistence
through the shared selected-source owner.

Independently locked release nextest `f8971938-da43-4eaf-8c32-dedea6bab327` passes 31/31
ordinary prepared-authoring cases. The new registered panel cases pin an already Missing
field and replace a stored Null on a required default-bearing field with Absent. They check
budget and address refusal, the exact prepared claim and inverse, cancellation, repeat
refusal, history replay, and the complete saved document after Reopen. After a change to the
shared base, the sibling inherits both changes.
The selected instance keeps both Absent claims until the panel Reverts the exact default-bearing
claim. The active document, selection and unrelated source files stay outside the edit.
[Registered panel cases](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/absent.rs)

Correctness Clippy passes for the affected libraries. Scene checks without default features.
Root and independent panel package formatting checks and local documentation links pass.

##### November 23 reference write qualification

With Rust 1.95.0 on Windows x64, release nextest run `b1f34e61-044e-4c81-aac3-2d4846bd4c22`
passes 65/65 selected Scene, Tooling and Host cases. The compiler control compares two valid
reference claims of the same shape and verifies only the requested stored operand. The Host
changes the reference between two sibling Prefabs. It refuses changes to either dependency after
Review in separate disk and working-copy runs. A clean retry composes the new endpoint and
preserves unrelated edits.
[Compiler control](../../crates/nara_scene/src/prefab/overlay/compile/selected_tests.rs) ·
[Host source regressions](../../src/project_host/runtime/editor/prefab_conversion/tests/effective_reference_write.rs)

Independently locked release nextest `444ee6cb-2563-402e-a251-3d0c6fe8b088` passes 13/13.
The first run declared a required nullable field in the fixture, so its initial Absent claim
was correctly rejected. The final fixture registers the same explicit optional storage Schema
with its authoring bundle and native codec. The registered panel writes a missing reference to
a sibling's root-bound alias and then to a containing local record in both source kinds. It checks
the suppressed endpoint refusal, exact review and inverse, repeat refusal, Undo/Redo and Save/Reopen.
A fresh compile of the saved files verifies the materialized Scene-local endpoints in both
instances.
[Registered reference workflows](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/reference.rs)

The Windows/DX12 complete Prefab conversion regression passes 1/1 in
`9c68d646-e487-4a0a-88bf-6539da4ca288` through the shared source acquisition and presentation
path. Reference editing itself is qualified through the registered semantic panel. Correctness
Clippy for the affected libraries, Scene without default features, workspace and independent
panel formatting, and changed-document links pass.

##### November 23 structured property qualification

The Windows x64 release Scene run `f4fcc570-9b3f-4800-ae5b-12f7014ee04d` passes 49/49
selected cases with Rust 1.95.0. The compact structured-value copy preserves the original
admission order and exact declaration, including authored reference operands. The selected
compiler compares the retained claim and its effective endpoints. The cohort also includes
the existing complete conversion budget and selected dependency controls.

Host and Tooling release nextest `dd630d0b-314e-4bb7-adc6-52513094a0be` passes 30/30.
The group covers the prepared source and history routes, rejected file and working-copy changes,
and the complete component edit that first requires another Prefab source during preparation.
It also covers the closing and cancellation behavior of the original inspection workers.

The independently locked Windows x64 release run `e69b0953-9503-449e-8b60-43eb30dc37a7`
passes 30/30 selected cases with Rust 1.95.0. The new Scene and Prefab workflows replace a
keyed record containing a map of nullable references. Review exposes the authored alias and
the compiled endpoints from the same candidate. The workflows exercise the exact 6/7 depth
boundary, preserve the original document on invalid shape, key change and unresolved route, and
leave the active preview and sibling occurrence alone. They verify the exact inverse, one
history entry, Undo/Redo, Save/Reopen, and Revert to the later inherited value.
[Independent registered panel](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/structured.rs)

The shared Windows/DX12 complete Prefab conversion trajectory passes 1/1 in release run
`74adce32-71f7-4632-aaf2-52781bf6ee12`. It exercises original native review, history,
persistence and preview integration. Structured editing is exercised through the registered
panel above. The affected Scene, Tooling, Host and Editor libraries pass correctness Clippy.
Scene builds without default features. Workspace and independent panel package formatting and
added local links pass.

##### November 23 selected property Apply qualification

On Windows x64 with Rust 1.95.0, owner nextest `f722ec2f-885a-4097-9695-952c3711b8a8` passes
112 of 113 selected cases. All new Apply cases pass. The Scene controls cover alias selection,
reference route ownership and endpoint equality, scalar and optional storage, signed zero and
logical limits. The Host drives real Save workers through both publication fences. It refuses
mixed working and disk checkpoints, confirms reuse after the earlier Save completes, and permits
an unrelated Save to continue. It also reports a stale working revision before prompting for a
destination Save and checks the destination's ordinary domain policy before Review. Physical
source controls replace the exact destination file and retain the original and newly demanded
dependency, sidecar and stable lookup checks. The existing conversion copy-stage assertion fails
at `classification-snapshot`, matching the earlier pre-Revert control.
[Scene tests](../../crates/nara_scene/src/prefab/inspection/property_apply/tests.rs) ·
[Host timing tests](../../src/project_host/runtime/editor/prefab_conversion/tests/apply.rs) ·
[Physical source control](../../src/project_host/authoring/project/capture/tests/host_retention.rs)

Independently locked release nextest `27c4664a-57e2-41e3-959b-2ebd41e0d6ff` passes 17/17
effective inspection and stored-claim cases. Four Apply cases exercise both non-active source
kinds, the exact destination and Revert patches, sibling inheritance, separate Undo/Redo and
Save/Reopen, and reuse after cancellation beyond the destination Save. The file-conflict case
refuses an external write before Save and stops Finish after a second same-length external
write. Both refusals preserve the containing pin and its history.
[Registered Apply workflows](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/apply.rs)

Windows/DX12 release nextest `8b2be38c-3f46-4bea-b11d-a7903dbde876` passes the shared native
Prefab conversion trajectory. Apply itself runs through the independent registered panel above.
Recovery covers a fresh inspection using the saved destination and the surviving override.
Continuation of the original Review in another process remains open.

Correctness Clippy passes for the affected libraries. Scene checks without default features.
Root and independent panel formatting and changed-document links pass.

##### November 23 whole-component qualification

The Windows x64 Scene cohort passes 76/76 with Rust 1.95.0 in nextest
`13b5493f-f9cc-45e4-ab2d-d8ae69dec4e4`. The final Scene library run
`33276396-3ca1-438a-972c-34a4dd196c19` passes 851/851 without skips. It covers Add and Replace
admission, bounded compact operands, exact source identity, signed-zero history and dormant
declarations under root aliases. A stale edit plan is refused even after Undo restores the
original declaration.
The review exposed a suppressed local record whose ownership disappeared from the final program.
The compiler now observes the lower record before the selected declaration applies. Both aliases
refuse a complete rewrite of that local record and still permit exact Revert. The ordinary patch
control compares signed zero in Set, Properties, additions and nested instance claims, and checks
the actual source bytes after the inverse and replay.
[Scene component edit cases](../../crates/nara_scene/src/prefab/overlay/compile/selected_tests/component_edits.rs) ·
[Exact instance patch case](../../crates/nara_scene/src/patch/prefab_instance_tests.rs)

Independently locked release nextest `f0d44358-43b4-464a-bac7-02e25d245330` passes 42/42 ordinary
`prepared_authoring` cases, with nine ignored cases. Three new registered-panel cases drive
non-active Scene and Prefab workspaces through invalid Add, review and cancel, Suppress, saved
Reopen, Revert, complete Replace and Add, Undo/Redo and further Save/Reopen. The sibling's
effective value and declarations survive. A complete record can introduce a dependency that
the original inspection did not load. An external same-length write to that dependency after
Review makes Commit refuse and preserves the original document and history.
[Registered component edit workflows](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/component_edit.rs)

An isolated run of the existing `last_loader_restriction_governs_complete_root_and_classification_copies`
case reproduced a stale assertion about the 7 MiB copy limit. The known JSON source now reaches
`classification-snapshot` after the single JSON publication check. The unbound source checks
both encodings and stops at `publication-buffer`. The revised test exercises both at the same
allowance while preserving the tighter Scene limits, Prefab cases, and shallow certificate
control. The original loader limit still gates each owning copy.
[Encoding-specific conversion controls](../../crates/nara_scene/src/authoring/prefab_conversion/localize/tests.rs)

The resumed release test execution uses the completed Host, Tooling and Editor binaries from
the interrupted joint build. Host passes 21/21, including rejection of a newly demanded source
changed after Commit dispatch and successful retry after restoring it. Tooling passes 9/9
declaration and prepared-edit cases. The shared Windows/DX12 native conversion case passes 1/1,
driving review, commit, Undo/Redo and disk round trips for both containing sources. The new
component-edit action itself is exercised by the registered panels above.
[Host dependency control](../../src/project_host/runtime/editor/prefab_conversion/tests/effective_reference_write.rs) ·
[Shared native conversion](../../editor/src/platform/imgui/authoring_workflow_tests/prefab_conversion.rs)

The affected Scene, Tooling and Host libraries pass correctness Clippy, and Scene compiles with
default features disabled. Root workspace and independent panel package formatting pass. The
all-package independent formatter invocation hits Windows error 206, so the changed independent
package is checked explicitly. Local document links and diff whitespace pass.

##### November 23 contextual defaults and nested input Apply

The [workflow review](../audits/2026-11-23-prefab-authoring-player-and-input-review.md)
found that complete Add and Replace could omit a Schema default that referred to a local
entity. It also found that Apply could reject a nested Input whose entire binding chain already
belonged to the destination. Both corrections use facts from the original owners. The Schema
admission identifies contextual default fields for ordinary construction and complete Set.
The existing reference resolver observes lexical ownership as it resolves a selected property.
The selected original route must enter the actual destination occurrence. All later instance
declarations, Input arguments and the terminal record must belong to its subtree.

Windows x64 Scene nextest `7cc7dbd3-732e-4d92-a064-6c750b253b1f` passes 46/46 selected cases
on Rust 1.95.0. It covers ordinary source construction, complete Set and exact history, and
the inclusive node and byte allowances for the retained declaration. A candidate with an
altered shadowed default still compiles but fails exact declaration verification. The reference
control compares two chains that resolve to the same endpoint. The chain that leaves the
destination's lexical scope remains ineligible for Apply even after returning to that endpoint.
[Expected declaration and budgets](../../crates/nara_scene/src/prefab/overlay/compile/selected_tests/component_edits.rs) ·
[Binding chain controls](../../crates/nara_scene/src/prefab/inspection/property_apply/tests.rs)

Independently locked Windows release nextest `91cc22cd-eefe-4c9f-9ab4-acbb4acde4af` passes
28/28 selected registered-panel cases. The component cases exercise Add and Replace with the
reference omitted and explicitly stored in non-active Scene and Prefab tabs. They compare
the exact forward and inverse instance patches, Undo/Redo, Save/Reopen and the sibling's
inherited reference. The Apply cases Set and Save the containing claim, review both edits,
Save the destination before Finish and then save the containing Revert. Reloading the saved
sources resolves the same nested Input to the corresponding entity in each instance. The
two outer-binding controls refuse before either document changes.
[Component panel cases](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/component_edit/contextual_defaults.rs) ·
[Input Apply panel cases](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection/apply/nested_input.rs)

Root release nextest `bd067383-66bb-47d5-b9a6-16d9c5f78ae9` passes 34/34 selected cases.
Host passes 21, Tooling passes 12 and the existing Windows/DX12 complete Prefab conversion
passes one. The Host cohort includes rejection of a change to a source first read by a complete
Set candidate between Commit dispatch and publication. The shared native case covers the
conversion path. The registered panel cases above cover the new component and Input actions.
[Host source revalidation](../../src/project_host/runtime/editor/prefab_conversion/tests/effective_reference_write.rs) ·
[Native conversion case](../../editor/src/platform/imgui/authoring_workflow_tests/prefab_conversion.rs)

Correctness Clippy passes for Scene, Tooling and the affected Host library. Scene compiles
with default features disabled. Root workspace and independent panel package formatting pass.

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

### Resolving a directory parent once

The property directory borrows the admitted root and resolves the Schema shape and wire value in
the same traversal. The selected root's keyed-list index remains available to the walk. Map and
keyed-list pages charge the actual child entries and their retained metadata after the parent.
The borrowed location belongs to the original live or prepared read. The page cursor still
requires the same source and Schema.

`EditorAuthoringQueryLimits::default()` supplies the engine's existing bounds. A panel can request
the first page or resume with its cursor without computing a separate parent allowance. Explicit
custom limits remain available for smaller pages or tighter work budgets. A first child above the
metadata allowance returns the structured refusal. An admitted prefix can continue on another page.

The design applies the reuse rule in [ADR 0049](adr/0049-untrusted-project-input-and-parse-budget-policy.md).
Bevy Reflect exposes named Struct and keyed Map access, and Godot presents variable-length
dictionaries in pages. Their APIs separate finding a field from materializing the displayed
children. [Bevy Struct](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_reflect/src/structs.rs#L50-L55)
· [Bevy Map](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_reflect/src/map.rs#L53-L58)
· [Godot dictionary pages](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/inspector/editor_properties_array_dict.cpp#L1589-L1620)

Windows x64 release nextest passes 286/286 in the Reflect library
(`ebfc5b90-687a-44fe-a0fd-06aefd55108e`) and 5/5 in the independently locked panel consumer
(`6c375883-9420-471e-94bc-da931aa7bfca`). Both non-active source kinds expand the 130th Map in
live and prepared views with 132 observed entries. The same run retains the large-key,
keyed-list and first-child metadata controls. A schema-only directory can now borrow the root
value during resolution. An unindexed keyed-list lookup can reach the scan limit before a
terminal unkeyed-list refusal. This evidence measures the selected scan counts.
[Panel regression](../../tests/fixtures/editor-ui-host-consumer/tests/property_children/late_map.rs)

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

#### Confirming an unchanged property edit

A prepared field transaction whose final stored values equal its live basis keeps a readable
preview and exposes empty forward and inverse patches. Scene compares only the stored roots
touched by the ordered edit, including missing values and exact floating-point bits. First
writing a contextual or Schema default explicitly changes storage, even when the resulting
effective value is identical.

Commit still checks the original document, revision and Schema. An unchanged commit reports
`ScenePatchReport::is_unchanged()` through the ordinary panel completion. The existing document,
saved state, dirty flag, Undo and Redo remain current. A stale candidate remains a refusal.
Field-only Source Tool proposals use the same accepted unchanged result and retain the
existing history when confirmed.

On Windows x64 with Rust 1.95.0, the actual Source Tool process trajectory passes 1/1
in `6d40631d-0e7e-44dc-88d4-ee8951d2fa1f`. It applies a new value, repeats the exact
proposal, receives an accepted unchanged receipt and keeps the original Undo entry.
The existing Windows/DX12 field edit, reset, remove and persistence trajectory also
passes on the rebuilt Editor test binary.
[Source Tool process case](../../editor/src/app/persistence_tests.rs) ·
[Independent panel cases](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages/unchanged.rs)

Independently locked release nextest `20d45d59-e2c1-49d4-b03c-3ee903730eaa` passes 13/13
selected cases. The new panel cases create a Redo entry, review a changed value and then the
original value in distinct update cycles, and receive the unchanged receipt through one normal
submission. Their inactive Scene and Prefab keep the complete tab and saved state. Redo then
restores the earlier change. The remaining cases cover stored and effective Prefab Revert.

Windows Scene integration run `73aea0ff-a64f-414a-9db9-664d59fa6caa` passes 41/41
with two cost diagnostics skipped. It verifies ordered keyed writes that return to their
original storage, stale confirmation after Undo returns to equal values, identical effective
defaults becoming explicitly stored, exact signed-zero replay, and unchanged field-only
construction in both source kinds.
Correctness Clippy passes for Scene, Tooling, Nara and Editor. Root and independent fixture
formatting and changed local documentation links pass.
[Scene integration cases](../../crates/nara_scene/tests/default_property_writes.rs)

The [round twelve Inspector correction](../audits/2026-11-23-prefab-authoring-player-and-input-review.md#p3-findings)
reduces both direct Inspector and prepared-edit completion to the original Workspace patch
receipt. The draft closes and the Host clears matching field feedback after an accepted unchanged
edit. An independent review identified a remaining Host rejection branch after the first draft
fix. Both consumers now use `EditorWorkspaceCommandReport::is_unchanged()`.

The final Windows x64 Rust 1.95.0 runs pass 11/11 Editor interaction tests
(`09dc423f-004d-40d7-8e96-a9dde36d8b84`), 2/2 independent panel tests
(`616fe00e-36c7-4e74-8af9-49c74603dd3d`) and 2/2 native DX12 tests
(`dd33092c-83b1-40a6-ab6a-8f303fda7f9b`). The direct cases submit an equal integer using
different text after Undo in Scene and Prefab, clear the draft with no rejection feedback,
preserve the document tab and then execute the original Redo. The native Scene case also
completes Save and Reopen through the existing Inspector workflow. Editor correctness Clippy,
root formatting and the changed independent panel package's formatting pass. The complete
independent workspace formatting command exceeds the Windows command-line length, so the
format check is scoped to the changed package.

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

### Ordinary Prepared Edit Interactions

A registered source tool can capture one query basis for an interaction that emits several
proposals. Each new proposal prepares an immutable candidate with that basis. The tool reads
the exact candidate it shows and replaces its previous candidate. The panel instance owns the
retained proposal. Cancellation before enqueue drops it, and closing the instance retires it
with that instance. Confirmation moves the candidate into the existing typed submission, where
the document owner checks the basis and records one history entry on success.

Godot's light gizmo separates drag feedback, cancellation and release-time Undo. Bevy's editor
gizmo routes continuous edits through live component values. The Nara fixture exercises
isolated source proposals. It reads the live working source and the candidate at
the same address and checks that preview and cancellation preserve navigation and history.
[Godot light gizmo](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/scene/3d/gizmos/light_3d_gizmo_plugin.cpp#L105-L240) ·
[Bevy transform gizmo](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_gizmos/src/transform_gizmo.rs#L408-L659)

The separately presented Canvas continues to own its gesture and preview contract. The
[keyed collection diagnostic](../benchmarks/2026-10-25-prepared-panel.md#november-22-keyed-collection-workload)
captures preparation and publication cost for ordinary source tools.

The independent Windows x64 release run `f87e0c1f-f693-456b-98b2-514483a8cf2d` passes 38/38
with Rust 1.95.0. Its two ordinary interaction cases use 32,768-row non-active sources.
They cancel two successive proposals before enqueue, close another retained proposal with its
original panel, and reopen the panel with empty state. They then submit the last reviewed value
and make a separate keyed-row edit. Undo and Redo of both edits check exact values and history
depths. Each saved checkpoint compares the complete source, including its unknown record.
Reopen checks its new source incarnation, exact content and unchanged active document.
[Registered panel consumer](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages.rs)

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

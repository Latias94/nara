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
can instead supply the borrowed property declaration and current document basis. Scene checks
the complete declaration in its original slot, including its target route, component and
version, selector and original Set or Absent intent, before preparing the ordinary transaction.
The declaration browser uses the same rule when reverting a whole component intent, retaining
every claim in a property group as part of the observed declaration.

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

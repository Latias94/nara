---
title: Durable Property Addressing
date: 2026-10-24
status: R5a Schema-relative property selector and owner-edit slice qualified; source identity remains open
---

# Durable Property Addressing

## Pressure And Ownership

R5a precedes durable Prefab overlays and Behavior representation. The current root
`ComponentFieldId` already differs from its storage `ComponentFieldPath`, but nested Record
members have only wire names and Lists have only positions. Reusing those coordinates for a
durable target would silently select another value after rename or reorder. A runtime entity or
preview identity epoch cannot repair that ambiguity.

R5a's first implementation boundary is deliberately smaller than a complete persistent target:
it defines a Schema-relative property selector and a resolver. The source document, source entity,
and Prefab occurrence remain outer address axes owned by the document/provenance layer. Do not
publish a supposedly complete durable target until that owner supplies the exact source identity.

The complete [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
remains authoritative for scope. This unit must deliver an actual schema-owned property consumer,
not merely another serializable DTO or a second patch validator.

## Distinct Coordinates

- **Source document:** an authored source reference owned by the document/asset admission
  authority. A project database may resolve a Path reference to its admitted stable Asset ID; only
  that owner can prove the two references identify the same asset. Hand-authored Path references
  remain valid but are not rename-proof; source moves require explicit reference migration, not
  path normalization or equality with an unrelated Stable ID. A session ID, document incarnation,
  or workspace slot is not durable source identity.
- **Source object:** source document plus authored entity. `SceneEntityId` is document-relative,
  not globally unique; a bare entity ID is not this address. Reusing an authored ID asserts the
  same identity. The current document owner does not retain arbitrary deletion history, so no
  stronger cross-reopen non-reuse guarantee is implied.
- **Prefab occurrence:** the Scene source document, Scene-local anchor and exact outer-to-inner
  occurrence chain. Each anchor is relative to its containing source document and each hop names
  the referenced Prefab document and source entity. Two instances of one Prefab's entity have
  different addresses. Source Path/Stable-ID spelling and structured occurrence are distinct
  facts.
- **Durable property:** component Schema identity, root `ComponentFieldId`, explicitly identified
  nested members, owner-declared list item identities and map entries. Reflect owns only this
  Schema-relative resolution.
- **Storage path:** current field names and List indices in one admitted value/Schema. The existing
  `ComponentFieldPath` remains useful here but is not made durable by serialization.
- **Operation basis:** exact document incarnation/revision, component schema version, and retained
  Schema admission. Scene owns edit preparation/history; resolution does not bypass that basis or
  authorize mutation.
- **Runtime locator / presentation token:** observation of one execution or displayed geometry.
  Neither becomes a source or property address.

The eventual composite target is `source object or Prefab occurrence + ComponentTypeId +
property selector`. Revision, schema admission, and component version are operation evidence, not
identity. R5a implements only the final property selector/resolver; it neither creates a document
registry nor infers source context from a Rust type, current Workspace slot or active tab.

## Alternatives

1. Persist names and indices, then guess rename/reorder intent. Reject: a syntactically valid path
   can select the wrong object, and Shape equality cannot prove semantic continuity.
2. Assign UUIDs to every scalar and collection slot. Reject as a default: it adds identity owners,
   persistence work and author concepts even when a root property is replaced as one value.
3. Make every nested edit replace the complete root forever. Retain root-field replacement as the
   existing transaction/storage boundary, not as the only authoring selector for tools and future
   declarative overlays.
4. Explicit member IDs and owner-declared item identities, resolved through admitted Schema/value
   facts. This is the implementation candidate. Root field IDs are component-scoped; nested IDs
   are Record-scoped and resolved through the complete stable ancestor chain. The same identified
   Vec2 shape can be reused under `position` and `velocity` without reallocating x/y IDs. A
   component-wide nested namespace was rejected in review: it made the existing tombstone storage
   mechanism dictate authoring ergonomics while the selector still needed its ancestor chain.

## Selected Contract

A property selector has one required root field ID plus tagged nested segments:

```text
ComponentPropertyPath {
    root: ComponentFieldId,
    steps: [Member(ComponentFieldId) | ListItem { by: ComponentFieldId, key: ComponentPropertyKey }
            | MapEntry(String)]
}

ComponentPropertyKey = String(String) | I64(i64) | U64(u64)
```

The tags distinguish a Record member from a map entry even though both use string keys in the
current value representation. `ComponentFieldPath` remains a temporary current-value locator
with names and indices; serializing it in current Schema metadata does not make it a durable
property selector.

Root fields retain their current component-scoped stable IDs. A nested Record member may opt into
stable addressing with a `ComponentFieldId` unique among its sibling members; unaddressable members
remain usable through root replacement and revision-bound authoring, but cannot be traversed by a
durable property selector. Wire names and display aliases are not identity. Reusing x/y IDs under
different stable parents is legal; omitting an unidentified ancestor to reach its identified leaf
is not.

Each root `ComponentFieldSchema` retains `member_tombstones`: root-relative declaration paths with
tagged `Member(id)`, `ListItem`, and `MapEntry` schema segments. Collection segments select a schema
scope, not a particular item key; Optional is transparent. Paths end at a member. A removed member
needs a tombstone at its complete declaration path; that prefix also protects its descendants.
Root-field deletion uses the existing component-level field tombstone. The root retains nested
tombstones even if its shape changes from Record to scalar, so a later Record cannot silently
reactivate old member paths. Active paths cannot overlap tombstoned prefixes, and successors retain
all non-redundant retired scopes. This is owner-local lineage metadata, not a global shape registry
or per-item value history.

A keyed List declares which identified, required Record member carries each item's logical
identity. Keys use exact String/I64/U64 values, never floats, positions, runtime entities or a
generated hash. Shape admission proves the key member exists and has a supported non-optional
scalar shape; value admission rejects missing, malformed and duplicate keys. A key value denotes
the item's identity: changing it changes identity, and reusing a historical key asserts continuity
with that identity. The resolver does not retain arbitrary value history to detect a dishonest or
accidental reuse. If future overlays need to distinguish a removed item incarnation from an
intentionally restored identity, their owner must retain lineage or require an explicit remap.
Reordering preserves the key-selected target. Unkeyed List positions remain revision-local and
have no durable selector.

`ListItem.by` must equal the current stable identity member declared by that List. Changing the
identity policy from `id` to `slug`, even after a successful component migration, rejects a selector
captured under `id`. A version bump and current-value admission do not migrate old selectors. A
later overlay owner must explicitly remap them or retain an unresolved identity-policy conflict.

Map entries use exact String keys and the same explicit identity convention: rename means remove
one entry and add another; reusing a key asserts the same logical entry. Optional containers are
transparent only while present; absence is reported rather than manufacturing an intermediate
object.

The path contains no display name, raw List index, Rust path, runtime entity, `TypeId`, Bevy
component ID or codec pointer. Depth/size use existing bounded schema/value admission rather than
an unbounded recursive parser.

The author declaration must make stability opt-in and legible without requiring a second Schema
registry. Keep simple `record` shapes concise for root-only users; offer an explicit member
constructor/ID builder and a keyed-list builder that names the required identity member. Catalog
freeze should report the component, member wire path and colliding/retired ID. Derive support is
not a prerequisite: manual semantic Schemas already work for independent providers, and the
selected consumer should establish whether a reusable derive adds real value.

Reflect resolves a selector only against the exact admitted current Schema and migrated value. It
returns a value/shape observation or performs one replacement while resolving against that same
candidate; it does not hand callers a durable or reusable storage path. Unknown/retired IDs,
unavailable values, unidentified members, unkeyed collections, missing keys and ambiguous keys
are explicit refusals. No fallback to member name, first match, row zero or fuzzy coercion. The
current-schema check also refuses old, future or otherwise unmigrated component records: stable
identity does not make their value semantics current.

## Schema Evolution And Files

Member IDs, the keyed-item declaration and root-relative member tombstones participate in Shape
validation, canonical fingerprints, file budgets, catalog successor checks and authoring bundle
admission. They are data-only metadata. Introducing them must not hide a Shape-semantic change
behind the same component version. Data migrations still own value/wire-layout changes; address
resolution is not a migration engine and never guesses a replacement for a removed target.

The persistent selector does not include `ComponentSchemaVersion`; the version is a value
interpretation basis. A resolver accepts only a record already migrated and admitted at the
current version. A storage-path migration may preserve the selector when the semantic member IDs
remain the same. A split, merge, semantic replacement, or retired ID needs an explicit address
remap owned by a later overlay/rebase contract, or remains an unresolved conflict. A tombstone
proves ID retirement only; it does not preserve removed value payload.

An existing root-only Schema may remain root-only. The initial production tracer uses an explicit
named nested record and a keyed collection rather than claiming that every built-in codec or
derive already supplies stable nested identity. Unknown/unavailable component records remain
losslessly preserved by their current Scene owner; unknown nested members in a known closed Record
are not silently preserved or dropped and require an explicit migration. Native codecs remain
responsible for complete business invariants on the final candidate.

Adding semantic member IDs or list identity metadata changes catalog meaning and canonical
fingerprints. Catalog and authoring-bundle envelopes move to canonical v3; v1/v2 reject explicitly.
The fingerprint domain also moves to v3. Repository fixtures retain their component histories and
recompute predecessor links under that encoding; root-only metadata does not silently acquire
stable member identities. Scene/Prefab/patch value envelopes are unchanged. Nara is unreleased:
there is no compatibility alias/reader or new migration guide. This explicit metadata reset does
not authorize silently dropping unavailable component payloads or interpreting an old nested
field write using new value semantics.

## Upstream Comparison

The local Bevy snapshot at `85503941e488152711a4695bbe248e9ed9f453fc` exposes reflected paths for
current struct fields, tuple/field indices, lists and map keys. Its `GetPath` documentation
explicitly warns that field-index paths are fragile and discourages storing them in persistent
assets (`repo-ref/bevy/crates/bevy_reflect/src/path/mod.rs`); the access implementation operates on
the current reflected value (`repo-ref/bevy/crates/bevy_reflect/src/path/access.rs`). That is a
useful runtime traversal API, not stable semantic identity, component-version migration, or
owner-scoped tombstone history. Nara should keep Bevy reflection as an adapter and not persist a
Bevy `ParsedPath` as its own address format.

The local Godot snapshot at `81ab961f91066a63bcbf3ec340962bb1a1ada558` separates node identity and
property access: `SceneState` stores per-node unique IDs
(`repo-ref/godot/scene/resources/packed_scene.cpp`), while `Object::get_indexed`/`set_indexed`
traverse the current `NodePath` property subnames
(`repo-ref/godot/core/object/object.cpp`). Inspector property paths are assembled from current
property names (`repo-ref/godot/editor/inspector/editor_inspector.cpp`). Its node IDs are useful
precedent for the outer source-object axis, but do not make nested Record members or list rows
stable property identities. Nara's proposal combines neither whole engine's model: it keeps an
explicit semantic document owner and adds only the missing property-level ID/key layer.

The comparison therefore supports the separation between a convenient reflection locator and a
persistent property target; it does not justify adopting a universal property graph, a new global
registry, or arbitrary IDs for every scalar.

## Actual Consumer And Ordered Work

1. Reconcile ADR 0011's current root-only identity contract and ADR 0081's stable catalog
   semantics. Add Schema metadata, bounded resolution and hostile schema/value/lineage tests in
   Reflect. Test sibling-ID collisions, reusable identified shapes, retired parent/descendant
   paths, Record-to-scalar-to-Record history, and rejection of reactivation. Test key type mismatch,
   identity-policy change, duplicate current keys, and the documented deletion/reuse semantics.
2. Route one Scene/Prefab property edit through the existing owner-owned prepared edit. Resolve the
   stable property against the evolving private candidate, coalesce nested writes under the
   existing root-field operation, then reuse ordinary final-candidate validation and the existing
   inverse/history budget. Each operation resolves in order against the candidate produced by
   preceding operations; failure at any operation publishes no document, revision, history or
   dirty-state change. Do not create a second dry-run, Undo stack or direct World edit.
3. Compile an independent consumer: capture an address, rename a wire member without changing its
   ID, reorder keyed items, run an explicit component migration, then resolve the same selector
   against the migrated current-version value. Reject unknown/unidentified/retired members,
   duplicate keys, unmigrated/unsupported versions and an unkeyed indexed substitute. Cover one
   admitted prepare/commit, Undo/Redo and Save/Reopen; fail the final nested operation without
   changing document, revision, history or dirty state.
   Include two writes under one root without a lost update, and a key rename followed by a write
   through the new key and a final failure. Coalescing must remain Scene-owned, not tool-owned.
4. Reconcile ADRs 0011/0026/0043/0081 and the owning implementation ledger for the selected scope.
   The containing source-document/entity/occurrence contract and declarative Prefab overlay remain
   distinct dependencies under their own ADRs, including 0038/0102, not silently implemented by a
   relative component-property path.

## Adjacent Source-Identity Finding

The independent review traced a likely cross-document key collision: `nara_ui` derives persistent
UI root/local keys from `source_entity_id` and an optional Prefab occurrence, while
`SceneEntitySource` does not carry the containing Scene source. Two independently resident Scenes
with the same authored local root ID can therefore produce equal keys; the current projector should
reject duplicate roots closed rather than route actions ambiguously. The same risk applies to equal
anchors and Prefab chains. This is an independent source-provenance/UI owner change, not part of the
Reflect property selector. Add a public same-World/two-Scene reproduction before calling it a
qualified runtime finding. If confirmed, carry admitted source identity from the document owner;
never substitute `SceneInstanceId`, preview epochs or process-local Workspace IDs. ADR 0102's
frozen conformance gate requires a focused amendment, updated independent consumer and refreeze
before changing shared UI key semantics.

## Acceptance And Limits

Same-name members in different scopes resolve by their complete stable parent/member paths, never
by alias. Rename and keyed reorder preserve the intended target. Retired member paths cannot be
reused. An
item's deletion or key change makes its old selector unresolved while that key is absent; using
the same historical item/map key asserts continuity and cannot be distinguished from intentional
restoration without owner-retained lineage. Default/optional fields, numeric domains,
typed references, preservation of unavailable component records and final native invariants retain
their existing rules. Unknown nested fields require explicit migration. Stale revisions, stale
Schema admissions and unmigrated values reject. The selector alone grants no mutation or file
authority: query/edit owners still require their exact source, revision, current Schema and
source-local reference admission. Durable Prefab overlays must retain base evidence and explicit
migration/conflict ownership rather than infer identity from a key that the owner cannot prove.

No cross-document atomic edit, universal property graph, dynamic Behavior binding, public tool
installation, automatic Prefab rebase or migration guide follows from this unit. Implementation
and qualification claims require source/tests/consumer evidence; this document alone closes none.

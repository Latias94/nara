---
type: "Work Progress"
title: "Prefab extraction representation selected after root ordering and reference value counterexamples"
description: "CP2 selects explicit roots, source placements, inline references and flat compiled output; pure identity ownership is being replaced before product qualification."
timestamp: 2026-11-01T21:26:48Z
record_id: "044888fa29f7401b91a5904e8a230cb1"
producer_id: "codex-foundation-main"
run_id: "prefab-extraction-representation-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "8b25d838963e604c2baaf914e676a0bed9efeb30"
---

# Summary

Full foundation Goal remains active. CP1/S2 is committed and locally merged at8b25d838;
the next complete CP2 unit now has a selected representation and an actual pure-identity
ownership replacement in progress. This record is a development checkpoint, not extraction
qualification. Source/overlay layout, inline references, contextual defaults, sealed flat output,
complete extraction and recovery remain later work in that same unit.

# Details

The root-only proposal failed a concrete ordinary authoring case: P owns b under k, k instances
Q, and b must appear between Q:d1 and Q:d2. P's local-ID children list cannot name the inherited
siblings; Q's overlay cannot name P:b. A containing Scene's overlay cannot stand in for editing P.
Mencius withdrew the root-flag-only recommendation and supported source-owned typed placements
replacing authored roots/children, with separate compiled flat hierarchy. The actual two-root
Reference UI is evidence for retaining explicit forest mounts alongside root-bound instances.
Canonical effective root identity stays with the containing anchor, while source-root declaration
facets and instance slots remain distinct. An old None pin on a root's original R slot must move
to the corresponding Q-root facet, not detach the newly created Q link.

Reference comparison found that moving coordinate bindings into a complete component record fixes
atomic storage but not value meaning. Equal local socket placeholders can carry different targets;
swapping the bare list values cannot express swapping those targets. Explicit Null can hide an
outward reference default, leaving no current slot in an exactly consumed binding table. Source
defaults therefore remain explicit version/field-bound contextual declarations through extraction,
Reset/Absent and Schema succession. Unknown affected semantics still refuse specifically.

Selected direction uses one inline reference/value language and separate source/flat compiled
owners. The generic reference-parameter value-tree alternative was considered and independently
recommended by one reviewer, but parent chose neutral pure identities plus inline references to
avoid provider/default/migration/opaque-domain lifts. A Rust1.95 metadata probe in ignored root
target/architecture-probes confirms unconstrained Value<R=...>::Null failsE0282 and a concrete role
alias fixes inference. That narrow language result proves neither a Reflect implementation nor
Native safety. Both candidate designs still need actual declaration-context/default semantics.

Canonical UUID/path/source-key representations move to Identity without an Asset/App dependency.
Asset keeps aliases, explicit scopes, metadata/catalogues and actual record compatibility; Scene
keeps admission, Prefab/occurrence interpretation and compilation. The Asset and Scene facades
export the same canonical values. Existing definitions/readers are removed rather than wrapped.
The actual metadata comparison moves from SourceDocumentKey::validate_asset_source to
AssetRecord::validate_source_document_key; it grants no file admission.

Root/source study Mencius01a198de-8b6b-72f2-9e6f-2ba49804090c, reference study
Lovelace01a198de-a79b-7941-9771-5236cbe717a2 and architecture comparison
Dalton01a198ea-0c97-74e2-b38a-14d77112adcb are closed. Maxwell
01a198fb-d9f2-79c2-b486-e09dcc662673 independently reviewed the selected complete CP2 plan,
found no assigned P1/P2 and is closed. These were static studies, not production qualification.

Cicero01a198f8-93ec-7cd3-9e02-6ef11294d667 and
Godel01a198f8-ca34-7903-9fc2-116f8fcec69d completed the disjoint Identity/Asset writers and are
closed. Parent changed Scene/Root callers, removed old Scene source.rs/wire.rs and preserved one
lexical validator. Source key modeled bytes retain len+2*sizeof(usize) per string plus sizeof(Self);
Asset path retained capacity keeps its own existing model. joined() was corrected after the first
test run to preallocate the whole result as the old scope implementation did, avoiding intermediate
String growth. Scope suffix conversion now asserts its proven lexical invariant rather than
hiding an impossible parse failure as out-of-scope.

Owner all-feature library check60989 passed. First owner run
169efa4e-ef63-4172-a343-94a8693dc76d passed772/772, zero skipped, before the narrow joined allocation
correction. Popper01a19908-419b-71c1-b8c7-c96c13f7965f independently found three residual Root calls
to the deleted method in capture/scenes, source_upgrade_inventory and source_upgrade_recovery.
Parent fixed them with identical error mapping and requested rereview; Root all-feature library
check19031 is currently running. No Root result or final-source owner rerun is claimed here.

All19 tracked Identity-consumer lockfiles were resolved serially offline with the root target.
Existing package versions/checksums did not change. A stale schedule fixture additionally needed
serde_json/itoa/zmij records; unchanged nara_project already requested Asset serde at8b25d83.
Initial offline resolution selected newer cached serde_json/zmij, then parent explicitly pinned
those newly added records to the original root1.0.150/1.0.21 versions. Verify final package facts
before committing. The temporary baseline is TEMP/nara-source-identity-lock-baselines.json.

# Next Action

Poll existing Cargo19031 (TEMP/nara-source-identity-root-check.log); do not start another Cargo
until terminal. Receive Popper rereview. Run final focused owning/public/Root capture-upgrade
tests, minimal and docs/portable checks appropriate to the relocated values. Reuse root target,
jobs1, serial Cargo. Check and record the19 lockfile facts, final formatting, current document
links and memory. Commit the complete pure-identity prerequisite precisely and locally ff main
when qualified; never call it complete CP2 or full foundation. No push/PR. Vendor remains untouched.

Then implement the selected source model under the reconciled ADR0006/0038/0101 contracts and
complete plan. Do not fall back to child-only extraction, keep duplicate source/compiled hierarchy
authorities, restore persistent payload slot sidecars, or let authored-only refs through Native
defaults/codecs. Remaining ADR0043/default/upgrade details must track the later actual grammar.

# Citations

- [Complete CP2 unit](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Pinned source study](../../../../research/2026-11-02-prefab-extraction-and-occurrence-continuity.md)
- [Prior complete CP1/S2 qualification](../../verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md)
- [Identity contract](../../../../architecture/adr/0058-stable-runtime-identity-and-entity-references.md)
- [Source/compiled contract](../../../../architecture/adr/0006-scene-and-prefab-data-model.md)
- [Placement contract](../../../../architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md)

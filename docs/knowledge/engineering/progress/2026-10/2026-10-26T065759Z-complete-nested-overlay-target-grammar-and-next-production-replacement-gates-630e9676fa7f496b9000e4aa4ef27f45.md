---
type: "Work Progress"
title: "Complete nested overlay target grammar and next production replacement gates"
description: "Read-only owner analysis exposes mixed-origin target omissions and bounds the next durable grammar, source codec and source-history replacement without adopting a public format."
timestamp: 2026-10-26T06:57:59Z
record_id: "630e9676fa7f496b9000e4aa4ef27f45"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
base_commit: "59c10a8"
---

# Summary

Read-only sidecar `01a176ee-12d9-7d73-93bd-fc30efee8d29` analyzed the next durable overlay
grammar against actual source/property/provenance owners and Accepted contracts. It made no
changes, ran no Cargo and is closed. The Proposed overlay design now records its bounded
Complete Nested Target Grammar. These are design recommendations, not a production format,
Accepted API promise or new execution qualification.

# Details

The current private Target distinguishes a source-only chain from a root Added ID and drops
any production provenance path containing an addition step. Actual fixtures already produce
source-anchor/addition, added-anchor/source and added-anchor/addition paths. The next target
comparison must express every anchor and terminal origin, including repeated outer instances.
Expected root/nested base keys are rebase conditions, not new Runtime identity axes or owners
of local additions. Addition-only survival cannot bypass base capture or source replacement.

The recommended direction uses an origin-tagged relative anchor chain, an expected canonical
base per entered instance, and a terminal origin. Lookup stays in `PrefabInstance.source`.
Reuse Reflect property paths, values and exact payload reference coordinates. Distinguish
Add/Replace/Suppress/property claims rather than inherit the private comparison's upsert.
Define explicit absence narrowly through existing removable-root semantics, separately from
Null and Revert; require complete keyed-container replacement for identity-changing edits.
The design records actual alternatives and executable refusal/acceptance gates.

Scene's source-key semantic owner is `crates/nara_scene/src/source.rs`; persistence belongs
to private Scene format wire structs and existing constructors. UI's canonical opaque key
encoder is precedent, not a shared wire owner or permission to persist occurrence. The real
Host still admits physical metadata/location and captures/revalidates actual immutable bases.
Reuse encoded-byte/shape and domain/value counters; private JSON roundtrips and fixed 64-item
limits do not qualify hostile admission or full pre-copy accounting.

Before production replacement, reconcile ADRs 0006/0038/0026/0090/0043 for durable intent versus
exact editor operations, source repair/history and canonical v1. Preserve 0058/0102's existing
origin/runtime/UI separation and 0101's one ordered closure. Remove embedded operation-path,
patch-version and creation-operation origin inference in the same coherent source/wire/compiler
switch; keep exact editor patches and owner-created inverse history. Do not add a legacy reader.

# Next Action

Implement bounded mixed-origin target comparisons, then settle and exercise complete property,
component/reference and conflict encoding through the real source owner before switching
`PrefabInstance.overrides`. Include nested/repeated additions and added anchors, root/nested
base replacement versus alias, equal-value pins and uncovered siblings, Absent/Null/Revert,
same-spelled cross-origin default/scalar references, final invalid candidate and unchanged source
history. Actual source edit/Undo/Redo, conflict Save/Reopen and convert-to-local remain required.
The full foundation Goal stays active; neither the complete-value extraction nor Editor progress
ergonomics closes the durable overlay work.

# Citations

- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [Complete-value checkpoint](../../verification/2026-10/2026-10-26T061353Z-complete-value-projection-comparison-without-source-history-or-templates-047bc67b5dec41c2a940091b39ceedba.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- `crates/nara_scene/src/provenance.rs`, `crates/nara_scene/src/source.rs`, `crates/nara_scene/src/format.rs`
- `crates/nara_scene/tests/prefab_sources.rs`, `crates/nara_scene/src/prefab/overlay_structure_comparison.rs`
- `crates/nara_reflect/src/property.rs`, `crates/nara_reflect/src/schema.rs`
- `crates/nara_ui/src/persistent_key.rs`, `src/project_content.rs`

---
type: "Work Progress"
title: "Final reference demand compiler replacement in progress after production overwrite regression"
description: "A production property repair fails eager reference lookup; compiler normalization now preserves transient per-leaf declaration origins before final demand."
timestamp: 2026-11-01T22:17:29Z
record_id: "a31a71bcf62c4deb8a38131a3a44907e"
producer_id: "codex-foundation-main"
run_id: "prefab-final-reference-demand-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "df01801"
supersedes: "044888fa29f7401b91a5904e8a230cb1"
---

# Summary

Full foundation Goal active. The pure identity prerequisite is committed and locally ff main at
df01801. CP2 remains incomplete. The next actual production compiler correction is WIP and must
not be committed/qualified until integration, review and relevant tests pass. Current changes
are parent/authorized workers, not user pollution. No push/PR, no vendor changes.

# Details

Public expansion test later_property_pin_replaces_an_unresolved_inherited_reference_before_target_lookup
actually fails under run0da9be78-6b4a-46b3-ba2c-f8005d25f508. A source host.target names missing;
an outer root property pin fully replaces it with socket. Source remapping throws
scene.prefab-entity-reference-rewrite-failed/missing-source-entity before applying the pin.
Complete component Set selection did not cover surviving component programs with overwritten
reference leaves. Program-wide binding demand also reads discarded nested targets too early.

Private mechanism selected: retain raw normalized values plus transient ComponentFieldPath-to-
ScopedReference origin maps. Normalize selected record/defaults, validate exact binding grammar,
apply real Reflect properties and update only actual replaced subtrees. Only final leaves drive
target/source demand. Opening a new occurrence ends the current demand batch and resumes the
existing structural/program closure. Final lower admits actual replacement IDs/costs, without
fake symbol strings, second value/selector evaluator or persistent coordinate table. Temporary
origin copies remain separately preadmitted work storage. Source/claim/Absent defaults keep
their actual declaring scopes; uncorrected missing references must still refuse.

Faraday01a19925-801e-7aa0-b909-523c0ca7045a changed from read-only advisor to active core writer.
Owns staged.rs, staged/values.rs, staged/output.rs and new staged/reference_program.rs. No Cargo,
fmt or commit permission. Parent must not overwrite those files before source-ready. Core removes
the early remap/value_ids bridge and program_bindings scan, reuses final normalized origin maps,
and preserves exact value/component/generated-ID limits plus established diagnostic slots.

Heisenberg01a19929-01fe-70f1-a781-ae6065886c58 implemented and froze Reflect's internal
resolve_component_property_storage_path_with_limits(schema,version,value,path,limits)->path.
It calls existing resolve_location, returns snapshot-relative actual storage and changes neither
the public observation struct nor its Copy contract. Four focused tests pass in run
7b0ef93f-a9c8-4a2a-9e0b-98368eab66f6,161 excluded. The agent is closed; Cargo28368 is terminal0.

Lorentz01a1992a-aba4-7ed0-967c-32cdab9cd1f8 froze references.rs and references/tests.rs, then
closed. New plan_bindings/plan_property_bindings return BTreeMap<ComponentFieldPath,
PrefabEntityTarget>, validate exact grammar with the SAME Reflect planner, but never look up
targets. The old bind_references/bind_property_references/binding_growth helpers are removed.
Its13 new syntax controls are not yet executed. Caller owns preallocation admission for path,
target and planner metadata; no default cap change or hidden loose traversal allowance.

Parent properties.rs is source-ready: prepare_claims returns PreparedPropertyClaims{value,
replacements}; each PropertyReferenceReplacement{path,references} contains a full actual component
storage prefix and payload-relative typed target map. Shapes/selectors/key preservation remain;
no target lookup. Absent materializes its raw default and records claim-scope replacement intent.
A small cfg(test) apply_claims facade calls this production preparation then lowers its one-layer
plans for existing selector/value conformance tests; it is not the old binder or another compiler.

Parent added four public compiler controls in component_program_tests.rs: the actual failing
inherited-reference repair with an uncorrected refusal, overwritten nested missing-source demand
with exactly zero missing reads on repair, a one-byte final local target exact7/one-short6-byte
budget, and mixed same-spelled target/untouched refs where outer Absent uses outer.socket but
untouched reference keeps base.socket. The last three have not run; core source is transitional.

Copernicus01a19931-20c5-7433-8243-c1be7f19d13a is an active test-migration writer. Owns only old
mod reference_regressions in overlay/regression_tests.rs and optionally a new public compiler
test submodule. Do not overwrite its block. Old binder helper calls must not be restored;
redundant grammar assertions move to the13 tests and unique target/scope behavior uses actual
production expansion. Parent owns all other test file changes.

# Next Action

Receive source-ready from Faraday and Copernicus; review resulting complete diff before fmt.
No Cargo known active at this capture. Do not build Scene against their transitional edits.
Then fmt and serial focused new reference/program/property/budget tests, fix real failures,
run owning full Scene/Reflect and relevant public/Root capture workflows. Selected/authoring
Native paths share this compiler, so include their existing original-scope/omission/budget cases.
Keep exact limits; do not loosen expected provenance/value counts or resurrect eager binders.
Independently review allocation admission, fixed-point invalidation, final survival and diagnostic
origins before qualification/commit. Overall CP2 source graph/inline refs/defaults/extraction and
full foundation remain active even if this mechanism correction passes.

# Citations

- [Complete CP2 unit](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Staged compiler correction](../../../../architecture/prefab-overlay-design.md#selected-staged-replacement)
- [Qualified identity prerequisite](../../verification/2026-11/2026-11-01T215838Z-canonical-source-identity-ownership-prerequisite-25574e28909847799bbcd2c2b1f1d469.md)
- TEMP logs: nara-final-reference-demand-reproduction.log,
  nara-final-reference-demand-storage-tests.log.

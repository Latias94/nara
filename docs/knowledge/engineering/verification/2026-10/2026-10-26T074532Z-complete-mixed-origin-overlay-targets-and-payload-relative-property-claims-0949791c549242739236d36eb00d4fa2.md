---
type: "Verification Evidence"
title: "Complete mixed-origin overlay targets and payload-relative property claims"
description: "Complete private mixed-origin target indexing and bounded property claims with shared Reflect payload reference traversal; production overlay storage remains unchanged."
timestamp: 2026-10-26T07:45:32Z
record_id: "0949791c549242739236d36eb00d4fa2"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/complete-overlay-targets"
base_commit: "20926a6"
---

# Verification

Successor to the history-free full-value comparison and complete nested-target proposal. The
private compiler now combines structure, component modes, Schema-relative property intent and
reference binding. It still does not replace `PrefabInstance.overrides: ScenePatchDocument`.

# Result

- `Target` contains typed source/addition origin at every instance anchor and terminal, with a
  canonical expected base for each entered instance. Root base identity remains on the overlay.
  Actual production expansion supplies a test-only resolved-base trace; no production retained
  dependency inventory or Runtime/UI identity axis is introduced.
- Real nested/repeated expansion has nine targets where the old source-only index retained four.
  Source-to-addition, addition-to-source and addition-to-addition paths retain separate references
  and complete provenance. Alias canonical equality succeeds; root/nested base replacement,
  including addition-only instances, conflicts until explicit remap.
- Explicit Add/Replace/Suppress/property-set modes replace implicit component upsert. Property
  selectors resolve against one immutable admitted base, preserve explicit equal pins, reject
  duplicate/ancestor overlap and keyed-item identity changes, and allow explicit whole-container
  replacement. Revert removes intent; Absent removes only a declared removable root's storage.
- Reflect's existing declared-reference walker now also accepts a selected shape and owning root
  field with payload-relative paths. A scalar reference occupies the empty path. Capabilities,
  undeclared/invalid references, assets and traversal limits share the existing interpretation;
  this metadata helper does not grant edit/native/source authority or verify shape affinity.
- Replacement property payloads and defaults exposed by Absent bind local references explicitly.
  Existing inherited references remain untouched. Derived effective candidates can materialize
  lawful default ancestors through Reflect's existing default plan; original storage and retained
  Absent intent remain unchanged. Final survivor admission rejects newly dangling references.
- Raw property declarations have inclusive 64-claim, 4,096 replacement-node and 64-KiB selector/
  payload bounds before cloning/binding. Complete comparison values have 16,384-node/1-MiB
  limits. These are private fixture policies, not measured production workload or complete
  aggregate pre-copy admission guarantees.

# Evidence

Serial Rust 1.95.0, `CARGO_BUILD_JOBS=1`, existing root `target`:

- RED `d0a0fc6e-4088-435e-b31a-de5af821c124`: real mixed-origin index expected nine entries;
  the old source-only map retained four. After target replacement the focused structural cohort
  `194354b5-e620-4835-9ec0-110d94f254ee` passed 23 tests.
- Reflect helper's first test compile failed because `SceneEntityId` has no `FromStr`; corrected
  the fixture to use its actual `new` constructor. Initial two tests passed in
  `581adb3b-33ed-4445-b523-c9f956d11130`. Full all-feature Reflect
  `64ebdba0-0d68-4623-8ed2-0daff128da67` passed 266 tests, including trybuild, before the
  test-only coverage expansion. Final focused `3cf5c1c0-b62a-45d6-a7eb-ae7a47cc2846` passed
  all four payload tests, including exact 2-node/11-byte budgets, mixed asset/entity payloads,
  relative-path rewrite and hidden opaque-reference rejection.
- Initial integrated private comparison `03f7e992-40f3-4273-b629-e3a7694f2857` passed 39 tests.
  Full all-feature Scene `41084ba8-2f6e-4d89-a8a2-a2c32dd77452` passed 397 tests, 3 skipped.
- Independent review found P2: Absent plus a default at `settings.wire_value` accepted missing
  ancestors in property normalization but rejected them during parent default binding. The new
  regression initially used nonexistent `ComponentValueShape::U64`; after correcting it to the
  actual scalar constructor, RED `0e993a3e-a0bf-4d2a-9ebb-e2822678df59` reproduced
  `Conflict::Component` at that boundary.
- The fix reuses `effective_component_value_owned` for lawful default ancestors before slot
  rewrite, and removes an unnecessary pre-binding payload clone. The regression crosses missing
  versus empty ancestor with scalar versus local-reference default, checks missing-binding
  rejection despite a same-spelled Scene target, retained Absent intent and unchanged source,
  revision and history. Additional integration checks reject Set/default references whose target
  is later suppressed. Final private cohort `ca5d53b4-8121-4c7b-8a99-c014a4bb60db` passed 40.
- Final full all-feature Scene `9364afd1-e8f5-4ac0-a523-0baa1367afe0` passed 398 tests,
  3 skipped across 20 binaries in 24.378 seconds after a 1.07-second no-change build.
- Workspace all-feature/all-target correctness Clippy passed with existing warnings before the
  final private P2 correction. Final affected Reflect/Scene all-feature/all-target correctness
  Clippy passed after it; `cargo fmt --all -- --check` passed. Four new test-only clone warnings
  were then removed with `slice::from_ref`; the affected 13 property tests passed in
  `2268e28a-9368-4272-83f5-57d2047bf053`, and final Scene all-feature/all-target correctness
  Clippy passed with no warning in the added comparison/property files. These equivalent test
  input changes do not alter the production or compiler logic covered by the full Scene run.
- Local links in the four changed architecture/evidence documents pass. Engineering-memory
  validation and derived rendering checks pass with existing historical warnings; diff whitespace
  checks pass. No product native matrix was rerun for this test-private Scene/compiler unit.

## Independent Review

- Zeno `01a17726-97f1-7400-853e-d236c6d2ce82` reviewed mixed target indexing and the test-only
  trace without confirmed findings. The 32-hop private limit covers current default expansion
  depth; deeper custom expansion remains outside this evidence. The later property integration
  was not part of this review.
- Banach `01a1772d-627f-74a0-9aaf-688dedd2764c` found no confirmed Reflect logic defect and
  requested budget/mixed-asset coverage. Parent added both; the focused follow-up confirmed those
  assertions and explicit opaque-reference path checking. It ran no tests and is closed.
- Erdos `01a17733-9246-7a00-bba7-981dabadfbe1` reviewed the property module and integration,
  found the P2 default-ancestor mismatch above and suggested removing the extra payload clone.
  Parent owns the executable reproduction and correction. Its completed follow-up confirms
  source-level closure with no new findings and
  verifies the borrowed Set and default-ancestor corrections. It ran no tests and is closed;
  no source-only review is reported as execution evidence.
- Poincare `01a1773b-3252-7ac2-80c0-8c20cd92a2e9` inventoried concrete production consumers
  read-only. It identified the distinction between addressing an already-expanded added instance
  and declaring a complete new added record containing an instance, plus the absence of an exact
  source operation for mutating an existing `entity.prefab`. The design now maps the required
  decoder/classification/history/compiler/two-Host/persistence/conversion replacement boundaries.
  It ran no tests, made no changes and is closed.

# Follow-up

Production storage, bounded canonical `SourceDocumentKey`/conflict codec, aggregate pre-copy
admission, actual captured base-closure activation, source edit/Undo/repair/conflict Save/Reopen
and convert-to-local remain open. Private `SourceIdentity` is a Prefab-only comparison codec,
not the selected all-kind source persistence owner. Before production replacement reconcile
0006/0038/0026/0090/0043 together and replace their real consumers without a legacy reader.
The private new-addition grammar remains IDs plus separate component declarations; it does not
yet express a complete newly declared nested Prefab record. Existing mixed-origin fixtures use
real current production expansion for those anchors. Generic authoring preview still explicitly
omits nested Prefab expansion, and Play Apply Changes retains its ordinary-entity-only scope.

This unit changes no Editor/native production behavior; it does not rerun or inherit a native
qualification claim. Unity/Godot remain editor workflow references, especially explicit source
context, equal pins, Revert and shared custom-control/history routing. The full foundation Goal
remains active; private semantic completeness is not durable production implementation.

# Citations

- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [Unity/Godot workflow study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
- [Full-value predecessor](2026-10-26T061353Z-complete-value-projection-comparison-without-source-history-or-templates-047bc67b5dec41c2a940091b39ceedba.md)
- `crates/nara_reflect/src/entity_reference.rs`, `crates/nara_reflect/tests/property_reference_payload.rs`
- `crates/nara_scene/src/prefab.rs`, `prefab/overlay_structure_comparison.rs`
- `prefab/overlay_structure_comparison/properties.rs`, `properties/tests.rs`, `references.rs`, `tests.rs`

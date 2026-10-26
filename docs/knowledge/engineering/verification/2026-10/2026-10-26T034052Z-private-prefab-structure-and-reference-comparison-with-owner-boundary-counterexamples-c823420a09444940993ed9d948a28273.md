---
type: "Verification Evidence"
title: "Private Prefab structure and reference comparison with owner-boundary counterexamples"
description: "Executed nested structure and explicit reference binding comparison, corrected default namespace and mixed-owner failures, and rejected lossy full-component lowering without changing production Prefab storage."
timestamp: 2026-10-26T03:40:52Z
record_id: "c823420a09444940993ed9d948a28273"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "design/prefab-structure-comparison"
git_commit: "90dcc43"
---

# Verification

Test-private structural/reference comparison after `90dcc43`. This is a successor to the
[six scalar cases](2026-10-26T025005Z-private-prefab-property-intent-comparison-and-next-owner-replacement-70ca642be09045ab909751b87ed5164d.md),
not a production `PrefabInstance.overrides` replacement. Accepted ADR 0038 remains unchanged.

# Result

- Real Prefab expansion supplies canonical source/entity chains for direct and repeated nested
  targets. The comparison does not parse generated locators or treat lookup aliases as identity.
- Existing construction and Scene placement owners allocate added entities and lower desired
  structure. Slot cohorts use declaration order, not allocation or old operation replay order.
  A reordered base that rejects the real exact `MoveEntity` override accepts desired placement.
  Parent rotation, suppression with a moved survivor, missing/cyclic/duplicate relations and
  occurrence isolation have executable evidence.
- Complete `ComponentValue` plus explicit version-bound payload-slot bindings is the private
  reference choice. The existing Schema reference planner performs traversal/rewrite; duplicate,
  missing, stray, wrong-expected and wrong-target bindings reject. Typed assets and Persistent
  references keep their meaning. Same-spelled containing, inherited, added and repeated nested
  entities are not interchangeable.
- A controlled JSON round trip retains the private intent/conflict, and reprepare can accept a
  restored missing target through an alias. This is not hostile file admission, real source
  Save/Reopen or Host activation qualification.
- The production direction is still declarative durable intent, separate exact source history
  and a derived projection candidate. Newly demonstrated limits rule out using generic
  creation-template plus root edits as the complete production component compiler.

# Evidence

Rust 1.95.0, serial commands, `CARGO_BUILD_JOBS=1`, root checkout `target`:

- Reference helper first passed its five cases. The integrated pre-review comparison passed
  15 cases, run `6cd077b4-4a3a-4f75-8344-d48d6c619f8c`.
- Default-reference RED, run `4f0e1814-bfb1-406f-a5c4-ef24e504150f`: omitting a reference field
  accepted the Schema-introduced `socket` default in the unrelated containing Scene namespace.
  Materializing defaults before exact binding corrects that error; adding an explicit binding
  then accepts the inherited target instead. Earlier run `eda7e99e-e564-4239-8930-f7fd64920e0a`
  selected the wrong sorted Schema fixture field and is not a product/compiler counterexample.
- Pre-review Scene/Tooling run `56d3eb3b-44f3-4e7d-8558-5da83c218e53`: 442 passed, 4 skipped.
- Reviewer-counterexample RED, run `52f872ba-854e-42fb-a208-286b662f547e`: all three new cases
  failed for their intended reasons. An empty overlay and a reorder both rejected legal
  containing-owner children. A lawful `{}` whole component silently became `{ group: {} }`.
- Focused post-fix GREEN, run `b422f5e3-b351-442b-82fd-73e762564bb1`: all 19 structural/reference
  cases passed. The component case now rejects the lossy input and separately accepts the exact
  empty-container value; it does not claim full-value support for the rejected valid input.
- Post-review Scene/Tooling cohort, run `075e77f6-5d9c-40d9-9539-c5f8b3645be2`: 445 passed, 4 skipped.
  Command: `cargo +1.95.0 nextest run -p nara_scene -p nara_tooling --all-features --locked
  --no-fail-fast --test-threads=2 --status-level fail --final-status-level fail`.
- Strict all-target Clippy with `-D warnings` stopped in unchanged `nara_app`: 16 existing
  large-error, type-complexity and collapsible-if warnings became errors. That attempt did not
  qualify the downstream files; no unrelated API redesign or lint suppression was added.
- Ordinary all-feature/all-target Scene/Tooling Clippy passed. Its one new comparison warning
  was a needless cloned one-element test slice; using `slice::from_ref` removed it. Final serial
  nextest run `2c96f263-13b9-493d-958c-bb6430f71886` again passed 445 tests, 4 skipped, followed
  by passing ordinary all-target Clippy with no warnings in the new comparison files. Existing
  warnings in other code remain; this is not zero-warning workspace qualification.
- `cargo +1.95.0 fmt --all` completed. No production behavior, native Editor, GPU, benchmark or
  whole-foundation acceptance was run for this private comparison.

## Independent Challenge

Reviewer `01a17639-2827-7820-beba-18b31395e0ec` supplied the two concrete P2 owner-boundary
counterexamples above. The containing-child correction treats them as a foreign ordered
subsequence and moves only the projection's own entities through existing Scene operations.
The component correction compares every final admitted Set value against its materialized,
bound full input; mismatch retains intent and returns a component conflict rather than quietly
claiming success. This is an honest experiment limit, not a replacement full-value codec.
The reviewer's scoped follow-up found no remaining concrete findings in the corrected
core/reference rules or their design description. It was static and did not independently rerun
tests or verify upstream sources; the parent's final executions above supply that evidence.

The reference worker `01a17624-aec2-7073-8bb6-e32b18191ed9` owned only `references.rs` and is
closed. Research worker `01a17645-1f9e-7960-8eab-a5bd7efe6415` owned only the Unity/Godot study,
ran no Cargo and is closed. The parent owns integration and serial verification.

Explorer `01a1764e-8b1e-70a1-9ec8-fca7ab133f10` traced the next production obligation without
editing or Cargo: `expand_prefab_instance` patches the base copy before assigning provenance,
so it tags additions with that base's source key. `candidate::validate_scene_provenance` and
`spawn::publication` preserve the resulting tuple; `nara_ui::persistent_key` correctly encodes
all currently represented placement axes but cannot recover discarded origin. This is a missing
overlay-local ownership axis, not absence of complete keys or evidence of an active ID collision.
The next address must carry the true owning-source placement and explicit origin at both terminal
entities and added instance anchors. Existing source admission, occurrence, activation and UI
opaque-key facilities stay. The Proposed design records that constrained direction.

# Follow-up

- Reconcile owning ADRs before the production switch: durable versus exact semantics in
  0006/0026/0038, repair/source authority in 0090, value/source encoding in 0043 and actual
  provenance/key representation changes in 0038/0058/0102. Amend 0101 only if placement rules
  change. Do not promote Proposed 0083 as a circular prerequisite.
- Reuse full candidate classification for derived projection values. A projection does not need
  to manufacture source history through the generic creation-template path. Keep actual overlay
  editing/inverses at the source owner rather than adding a parallel Undo system.
- Give overlay-local additions their real containing-owner chain and explicit local-origin kind
  across Scene, Runtime and UI keys. Current private TargetMap and generated entity IDs do not
  establish production provenance, especially for an overlay inside a repeated nested source.
- Complete bounded durable source/conflict encoding and original property grammar, exact nested
  base-closure retention/publication, source edit/repair/history, conversion and the independent
  public/native workflow. Remove obsolete patch-as-durable-overlay only with that replacement.
- Use the Unity/Godot findings for explicit edit context, restore-inheritance versus pin,
  destination-aware Apply, unused-intent retention and shared Inspector semantics. The study is
  documentation/source evidence, not a claim that either engine implements Nara's guarantees.
- No speed/allocation win, new format/API, compatibility layer, migration guide or full Goal
  completion is claimed. Do not keep adding leaf-only private tests instead of replacing owners.

# Citations

- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [Unity/Godot study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0038](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- `crates/nara_scene/src/prefab/overlay_structure_comparison.rs` and its `tests.rs`/`references.rs`

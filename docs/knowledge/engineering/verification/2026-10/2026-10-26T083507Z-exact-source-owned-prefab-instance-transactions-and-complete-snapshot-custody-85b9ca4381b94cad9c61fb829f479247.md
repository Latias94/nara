---
type: "Verification Evidence"
title: "Exact source-owned Prefab instance transactions and complete snapshot custody"
description: "Source instance declaration replacement through existing preparation/history and real Host consumers; durable declarative overlay storage remains separate."
timestamp: 2026-10-26T08:35:07Z
record_id: "85b9ca4381b94cad9c61fb829f479247"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/source-instance-transactions"
base_commit: "4d2bd20"
---

# Verification

Production prerequisite discovered while mapping the complete declarative-overlay replacement:
the source transaction vocabulary could not mutate an existing entity's instance declaration.
This unit adds that responsibility to the existing exact patch/preparation/history owner. Durable
overlay storage still remains `ScenePatchDocument`; no new compatibility layer is introduced.

# Result

- `SetPrefabInstance { entity, expected, value }` compares the complete prior optional declaration
  before replacing it and creates an inverse by exchanging snapshots. Attach, replace and detach
  preserve source-local records, children and placement. Detachment is not convert-to-local.
- Both Scene and Prefab source sessions use existing move-only prepare/commit and one Undo/Redo
  entry, with exact incarnation/revision/Schema and forward/reverse replay/history checks.
  Borrowed forward input cost is checked before cloning snapshots; cheap format/field-count
  failures retain their prior rejection ordering.
- Bounded replay cost now admits semantic declaration/value depth before recursive equality,
  cloning or serialization. The current 64-level policy is no stricter than the default canonical
  wire depth. Native Scene/Prefab and owned private entry points share this borrowed admission.
  Value checking stops within the limit; it does not recurse down a rejected 10,000-level tree.
  History cost includes every retained component-owner string in both snapshots and entity records.
- Canonical file counting and version traversal recurse through both expected and value snapshots.
  Native canonicalization validates exact snapshots against current Schema without migration or
  native callbacks. It does not rewrite defaults, owners or payloads independently of their basis.
- Embedded classification paths append an explicit side (0 expected, 1 value) before descending
  into nested operations. Complete unavailable records and unavailable field-operation payloads
  participate in the same lossless manifest; equal target metadata cannot conceal value drift.
- Both real Host Prefab scanners and retained-byte owners recurse through both snapshots.
  Editor UI-domain guards also inspect both branches, including nested field operations. Source
  declaration preparation does not resolve or activate the lookup, and generic nested preview
  remains unsupported. Runtime activation and source preservation keep their separate owners.

# Evidence

Serial Rust 1.95.0, `CARGO_BUILD_JOBS=1`, root `target`; independent consumer explicitly sets the
same `CARGO_TARGET_DIR`:

- Initial new Scene tests `b72b4f78-6579-4271-a2d3-bde3f39a7ba0`: 12 passed, 1 failed.
  The invalid numeric value correctly rejected with `nara.reflect.numeric-range`; the fixture
  expected the enclosing context code instead. Corrected the assertion to the actual structured
  diagnostic, retaining rejection and zero migration/native callback requirements.
- New plus prepared-owner cohort `d135b812-cbc9-4c0a-9ec9-ab4b352c1d61`: 32 passed,
  1 failed, 2 skipped. New early history-cost checking changed the old invalid field-count error
  precedence. The borrowed admission now yields cheap over-count input back to the original owner
  before payload accounting, using the same finite field-operation predicate; no test was weakened.
- Root actual-scanner/retention/UI guard cohort `ffab4017-10fa-4985-88e5-13f80b1b8aff`:
  5 passed, including real Authoring source/asset closure from complete records, Runtime stable-ID
  discovery and repeated-edge charge, and separate expected/value retained allocation.
- Full Scene before review corrections `df18d01c-af7d-40c5-b884-48641f012514`: 411 passed,
  3 skipped across 22 binaries. This proves the cheap rejection-order correction, not subsequent
  payload/depth/owner-cost fixes.
- Independently compiled public `property_authoring` consumer
  `3484bd88-8d54-453a-9398-663bd0bca6de`: 7 passed. Two new real window-free Editor trajectories
  cover non-active Scene/Prefab attach/replace/detach, prepare/drop/retained commit, exact inverse,
  one Undo/Redo, complete source Save/Reopen, local component/children preservation, exact mismatch
  and unavailable complete-record barriers. This run precedes later classification/depth fixes;
  final affected verification is recorded separately after its terminal result.
- RED `712f66cd-0eff-4dd6-a6d4-8a8f76b144ae`: public readmission incorrectly accepted a changed
  unavailable SetField payload in the expected snapshot. The new regression also crosses both
  snapshot sides, SetField/SetAssetRefField and Scene/Prefab readmission plus actual source edits.
  The fix retains an optional complete canonical payload in `UnavailablePatchTarget`, distinguishing
  removal from explicit Null; cloning occurs only after unavailable classification.
- Payload-fix cohort `17ab6486-54a7-47c9-8a91-590382cfa685`: 2 passed. Final new file/source
  and bounded-kernel cohort `61cb7926-2f69-4db6-aa25-023873686c5e`: 21 passed, including both
  snapshot sides and entity/RestoreEntity paths, nested Map/List value depth, the inclusive
  semantic-depth boundary, pre-validation native/owned refusal, unchanged authoring state and
  owner-string history admission at exactly 53 bytes versus refusal at 52.
- Final full all-feature Scene `08401598-13b2-4aec-9802-76563285732b`: 418 passed,
  3 skipped across 22 binaries, 26.578 seconds after a 1m06s build. This run includes final
  unavailable-payload, depth and owner-cost corrections.
- Final root and Tooling all-feature library cohort `ab91888b-351e-49e1-93cd-9e1d7afcdeae`:
  525 passed, 3 skipped across 2 binaries in 164.324 seconds after a 1m22s build. This includes
  the actual Host dependency/retention/UI guards and existing Editor source/history/persistence.
- Final independent public `property_authoring` consumer
  `6d81e442-75c8-45cf-94e1-93984a1f504d`: 7 passed in 1.141 seconds after a 36.48-second
  build against final source, including the later unavailable-payload/depth/owner corrections.
- Selected actual Windows/DX12 navigation group `cb1016fd-7853-46e0-bcd7-9028ec5f70ff`:
  2 passed, 246 excluded by the explicit filter. `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`
  and `native_ui_canvas_priority_survives_closing_retained_scene` executed against dirty source
  based on `4d2bd20` (the source paths in this unit), with no subsequent semantic source changes.
  This protects existing retained Scene/Prefab navigation and Canvas priority; it does not qualify
  a native instance editor, nested Prefab preview or the complete three-trajectory platform gate.
- Final workspace all-feature/all-target correctness Clippy passed in 2m49s with existing
  warnings; the new instance file/authoring fixtures produce none. Scene
  `cargo check -p nara_scene --no-default-features --locked` passed in 18.76 seconds with
  existing dead-code warnings. Final workspace formatting, whitespace, local document links,
  engineering-memory validation and derived-view checks pass.

## Independent Review

- Laplace `01a1774a-6261-7eb0-839d-245aea44d454` implemented file counting, exact snapshot
  canonicalization, classification and eight public file tests in its disjoint write set.
- Kant `01a1774a-916e-7891-97d6-e7cb7c36fbda` implemented Host scanners/retention/UI guards
  and five focused tests; Confucius `01a1774e-aab1-7f53-be26-7aca4a83cebe` implemented the
  independent public Editor consumer. Workers ran no Cargo/fmt and made no commit; all are closed.
- Franklin `01a17754-84e3-7e41-83cb-13a829bd9d7b` independently found the unavailable field
  payload bypass and programmatic snapshot depth gap. It confirmed the parent-identified omitted
  owner strings in Scene history cost. Parent owns the executable payload reproduction; Franklin
  implemented the bounded-depth correction in its assigned patch-kernel scope. Parent froze the
  source, reviewed it and ran the 21-case cohort; Franklin was closed while its handoff remained
  pending, so no completed post-implementation review receipt is attributed to it.
- Pasteur `01a17758-1314-7fc1-8234-3eb476e2ebb8` independently reviewed file/classification
  and actual Host consumers. It confirmed the same payload issue through public Save/Reopen
  readmission and found no other confirmed issue in that scope. Source-only review is not runtime
  evidence. Its focused follow-up confirms payload preservation and readmission/source-edit
  coverage with no new findings; it ran no tests and is closed.
- Ramanujan `01a17769-a429-79a0-a9eb-dc4e760a843a` separately reviews the final bounded-depth
  correction and owner-cost boundary. Its completed source review confirms closure with no new
  finding, including native/owned/published entry order, snapshot/entity/descendant/value coverage,
  the literal 53-byte history boundary and O(depth) rejection. It ran no tests and is closed.
  Arbitrary Rust construction/destruction safety and larger custom file-depth policy are not
  qualified by this default-boundary review.

# Follow-up

The full foundation Goal remains active. This source transaction is a production prerequisite,
not adoption of the private declarative compiler or completion of the source overlay workflow.
The next integrated replacement still needs complete new added records (including nested
instances), bounded canonical source/overlay/conflict storage, aggregate admission, actual captured
base-closure activation, explicit overlay repair and conversion, public/native workflow and
removal of patch-as-durable-intent storage together with its operation-path dependence.

Known adjacent gap from the Host inventory: Authoring asset discovery currently consumes complete
component record facts; partial SetField/SetAssetRefField payloads do not publish equivalent
declared-asset dependency facts. Runtime scans those fields through the shared Reflect helper.
This unit does not qualify complete asset closure for arbitrary field overrides. Do not add a
Host-owned second Schema walker to hide that missing source-owned fact when replacing overlays.

Unity/Godot continue to inform explicit source context, Apply/Revert and shared history routing;
this source declaration change neither installs new Inspector controls nor qualifies nested
preview, instance conversion or broad cross-document Apply.
The independent instance consumer captures expected declarations from its known authored source
fixture and probes exact state through preparation. It does not qualify a complete bounded
instance/overlay discovery query for Inspector authors. That observation path must join the real
source/occurrence query owner during the durable overlay workflow, without widget-owned file I/O.

# Citations

- [Overlay replacement boundaries](../../../../architecture/prefab-overlay-design.md#concrete-production-replacement-boundaries)
- [Exact source transaction contract](../../../../architecture/adr/0026-editor-command-patch-and-undo-model.md)
- [Source/provenance owner](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- [Snapshot migration policy](../../../../architecture/adr/0043-scene-prefab-and-patch-document-migration-policy.md)
- [Lossless preservation](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- `crates/nara_scene/src/patch.rs`, `authoring.rs`, `authoring/prepared.rs`, `authoring_decode/structure.rs`
- `crates/nara_scene/tests/prefab_instance_authoring.rs`, `prefab_instance_format.rs`
- `tests/fixtures/module-consumer/tests/property_authoring.rs`

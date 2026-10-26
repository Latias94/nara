---
type: "Work Progress"
title: "Production declarative Prefab overlay replacement integration in progress"
description: "Breaking production replacement is in progress; do not commit or claim qualification until the complete owner/Host integration passes."
timestamp: 2026-10-26T11:44:09Z
record_id: "b0111148a8964507912918f46ac2dace"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/declarative-prefab-overlays"
---

# Summary

Base main `47a2000`, branch `refactor/declarative-prefab-overlays`. Large authorized uncommitted
replacement across Scene, Reflect, Runtime/Authoring Hosts and consumers. Existing untracked
vendor remains untouched. No commit/merge/native qualification for this replacement yet.

# Details

- `PrefabInstance.overrides` is now `Option<PrefabOverlay>`. New complete typed declarations in
  `prefab/overlay.rs`; no old reader/adapter. Exact Scene patches remain source transactions.
- Production compiler selects nested instance pins before recursion, then applies components,
  properties and final layout. Current source compiled all-feature and no-default libraries once;
  this does not qualify the subsequent active corrections or the complete workspace.
- Removed old test-private overlay compiler and patch no-inverse expansion machinery; assertions
  moved into production regression helpers/public expansion. Structured authoring locations and
  complete per-anchor manifests replace fake operation-index paths. Full overlay retention is
  shared by both root/Authoring accounting.
- Reflect schema-only selector claims and selected payload-relative asset traversal now serve
  nested property inventory. Run `1b12fd9d-7ee3-4836-a42c-03afa8c9e7b8`: 10 new Reflect tests pass.
- Public expansion first run `87bc939b-9553-4049-b305-b515a476e82e`: 8 passed. No-default
  provenance run `7fc4e2db-1cd2-49f4-96ca-3d8e384997c1`: 5 passed, 4 failed due exact old
  provenance index formulas/diagnostic count. These are not final acceptance.
- Initial lib test build exposed missing `.unwrap()` in new fixtures; fixed. Lib run
  `a1a3ab66-240b-421d-963f-08c0df46d71b`: 257 passed, 18 failed out of 275. Duplicate entity-id
  diagnostic field caused several panics and is fixed in authoring location decoration. Focused
  expansion successor `fdfdd920-5c73-4da2-b17d-f44b0d2e8a55` passes. Other failures include
  old test-only validator budgets, migration test registry binding, payload error categorization,
  exact cost formulas and lost structured source/cycle/depth fields. Fixes are in flight.
- A later Scene test build sampled an actively edited core and failed on transitional types;
  do not rerun until the core writer freezes. No Cargo command is live from these records.

Active lanes at this capture (coordinate runtime state rather than assuming files are reserved):

- Helmholtz `01a17803-dd12-7ff2-b50c-ebc96f8690a7`: `prefab.rs`, `overlay/compile.rs`, new
  compiler regressions. Confirmed P1s: finite same-source reentry falsely considered cyclic;
  rescued descendants lose ancestor base facts after suppression; layered component Add leaks
  materialized component budget. Also correcting ancestry-aware suppression demand and derived
  scratch admission. Parent freezes those files until source-ready.
- Avicenna `01a177f1-986d-7e62-9ccd-e9f7c18d916f`: `src/project_content.rs`. Actual lazy
  selected source callback, concrete errors, retained receipts, effective assets then native gate.
  Source decoding overlaps full expansion work reservation (~770 MiB vs default512 MiB);
  coordinating compiler-owned working-set observations to resize unused worst-case capacity
  during a suspended loader call without releasing live copies. Do not simply raise limits.
- Pauli `01a177e1-57b8-7b52-923a-86a4c3776be9`: production components/properties/references,
  aggregate residual defaults/binding growth, direct addition normalization and typed shape error.
- Kierkegaard `01a177f5-72d7-7f21-ba6a-bc4b768dd2b8`: removing obsolete test-only whole-doc
  validator in complete_value.rs and migrating its assertions into production regression tests.
- Hume `01a17814-7e78-75d1-9f35-555d8575c018`: UI source/provenance tests, module-consumer
  fixtures and example storage migration. No Cargo/fmt by workers.

Closed lanes: Descartes format/migration/cost; Mill source classifier; Planck layout; Hubble Scene
integration tests; Boole old patch path removal; Anscombe Reflect payload assets; Kuhn Authoring
inventory/retention tests. Their code is integrated in the working tree, not independently verified
as a complete replacement. All parent Cargo is serial with jobs1 and root target.

# Next Action

Complete the active core/Host fixes before broad reruns; parent owns diagnosis and precise test
updates. Format only after workers freeze and inspect diff. Remaining product work includes
Authoring effective source loading (current inventory can still open discarded raw sources),
actual nested preview, complete external fixtures/native qualification and source-owned
convert-to-local. Full foundation Goal stays active; no completion or percentage claim.

Root `src/project_content/catalogue/measurement_tests.rs` still calls removed raw scanner and
queue-limit fixture expectations need meaningful synchronous loader successors. Editor/root
runtime and independent consumer literals still need full migration. Root all-feature library
check previously stopped at known old Authoring Host arguments/format variant; no success claim.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Replacement design](../../../../architecture/prefab-overlay-design.md)
- [Verified predecessor](../../verification/2026-10/2026-10-26T103859Z-nested-instance-repair-baseline-and-effective-source-demand-replacement-design-a6dbd3843c104c40b6ba817e428478f7.md)

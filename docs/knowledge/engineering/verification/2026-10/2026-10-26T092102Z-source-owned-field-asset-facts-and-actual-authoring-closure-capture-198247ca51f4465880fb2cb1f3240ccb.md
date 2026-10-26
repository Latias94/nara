---
type: "Verification Evidence"
title: "Source-owned field asset facts and actual Authoring closure capture"
description: "Current Schema field/default references join complete record facts for bounded source dependency publication and real Host capture."
timestamp: 2026-10-26T09:21:02Z
record_id: "198247ca51f4465880fb2cb1f3240ccb"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/authoring-field-asset-facts"
base_commit: "5851523"
---

# Verification

Production correction of the partial-field asset dependency gap recorded by the exact instance
transaction inventory. Schema interpretation stays in Reflect and the Scene source classifier;
the actual Authoring Host consumes immutable declared facts through its existing closure owner.

# Result

- Published Scene/Prefab authoring documents lend `schema_declared_asset_references()`, combining
  existing complete-record facts and retained field-declaration facts without another component
  record or Schema walker. Complete records retain their existing individual observations.
- Current Schema field Set, explicit SetAssetRefField and RemoveField-exposed defaults use the
  shared Reflect declaration visitor, including recursive collections and both nested instance
  snapshots. Optional Null produces no asset. Current KnownUnbound Schema still describes
  dependencies; unknown or incompatible versions preserve original data without guessed assets.
- Reflect adds an explicit-limit variant of its existing selected-field collector. The default
  helper delegates unchanged. Limits apply before owning the resulting paths and asset references.
- A document-wide projection allowance shares the selected file policy's node/logical-byte
  ceilings. Remaining allowance is supplied before each reference-bearing traversal/default
  materialization; retained field IDs, paths and asset locators are charged. The raw source
  payload budget remains separate. Complete records without an asset shape retain their old
  reference traversal policy rather than consuming an unrelated asset-fact allowance.
- Field-only classification uses the complete source classifier when retained facts exist or a
  touched Schema can introduce them. It does not reset the aggregate allowance for one edited
  subtree. The scalar no-asset fast path remains; reference-bearing preparation locality and
  latency are not claimed unchanged or improved by this correctness correction.
- Authoring closure capture consumes the document projection through its existing catalogue,
  dependency budgets, actual source bytes and receipts. Retained projection metadata and duplicate
  storage are charged separately; a partial field is not presented as a complete component fact.

# Evidence

Rust 1.95.0, serial Cargo, `CARGO_BUILD_JOBS=1`, existing root target:

- Reflect focused `abcb585a-721d-498b-bc95-e1e3ca8961ed`: 3 passed, including exact
  node/byte/depth boundaries, recursive path/stable assets, optional/required Null and capabilities.
- Initial Scene compile found the incremental field classifier's second `ClassificationContext`
  constructor. Added the same bounded fields and selected the complete-classifier fallback above;
  a new context cannot act as a fresh document-wide allowance for touched data.
- First Scene cohort `e85c9f71-bf72-483c-a2c1-cccccaa679e3`: 2 passed, 1 failed. Literal
  aggregate-default test exposed use of `reference_traversal_limits`, which intentionally expands
  raw-value allowance for traversal overhead. It was the wrong adapter for a remaining projection
  ceiling. Reference-bearing fact collection now receives explicit remaining limits directly.
- Corrected Scene focused `b7bb6ab9-2562-4934-a0ea-2f9c76569371`: 3 passed. Repeated defaults
  admit 5 nodes/36 bytes and reject 4/36 or 5/35. The tests cross Scene/Prefab, field collections,
  defaults, both snapshot sides, known-unbound metadata and unavailable-version preservation.
- Initial root filter `028f6ceb-88f5-4bdf-b0e0-ca8959bdcb46` selected zero tests and failed;
  no acceptance is attributed to it. Corrected actual Host cohort
  `db171435-c3c3-40e1-8e96-7b7681a5e1db` ran 2 and passed both. The actual capture test verifies
  source bytes/receipts, recursive and default assets, loaded Prefab fields, unresolved stable
  references, exclusion of unreferenced files and truthful missing-path failure. The accounting
  test separately checks field/path growth and repeated storage.
- First combined all-feature Reflect/Scene `1f1808d9-a40c-44be-b678-88e2ef36d29e`:
  691 passed, 1 failed, 3 skipped across 45 binaries. The failure was an old internal assertion
  that an asset-bearing edit must use local classification. The source policy now requires full
  classification for its aggregate proof. The test explicitly checks that fallback and complete
  result equivalence; a new scalar-only Scene/Prefab case proves the local path remains available.
  Focused `d6bfba35-3423-4fda-b4a6-02b3cb9616db` passed all 8 field-candidate tests. This is
  an intentional locality tradeoff, not a claim that the former fast path remained qualified.
- Final focused Host cohort `76eeee9f-6364-477e-89a8-0647f11d2edf`: 3 passed, including
  actual `charge_published_document` with two complete KnownUnbound empty authored records and
  repeated default references, accepting exactly both retained copies and rejecting one byte
  less or a one-copy allowance. The original actual capture and projection tests still pass.
- Final combined all-feature Reflect/Scene `279a8132-69f1-4f2b-8b5c-36aab3512b80`:
  693 passed, 3 skipped across 45 binaries in 65.627 seconds after a 1m16s build. The scalar
  local-classification proof and intended reference fallback are both included.
- Final all-feature root/Tooling library `dcb74d67-edd0-4395-826b-2834287af366`:
  528 passed, 3 skipped across 2 binaries in 170.846 seconds. Existing source/persistence,
  closure publication and the actual field/default capture and retained-budget cases are included.
- Final independently compiled property/Prefab-source/source-context cohort
  `adf99c0f-bc52-4be9-ad63-e7de685323e2`: 15 passed, 1 skipped across 3 binaries in
  1.942 seconds after a 1m08s build, using the same root target.
- Selected actual Windows/DX12 navigation `ad875f5e-a4fe-4a0c-bc52-1e79577a095b`:
  2 passed, 246 excluded by filter, in 2.984 seconds after a 2m16s build. Retained Scene
  receipt/Prefab navigation and Canvas priority ran against final dirty source based on `5851523`.
  No later semantic source changes followed. This protects existing navigation, not a new native
  asset-field editor, nested preview or the complete three-trajectory platform group.
- Final workspace all-feature/all-target correctness Clippy passed in 3m00s with existing
  warnings. Scene no-default-feature compilation passed in 3.19 seconds with existing dead-code
  warnings; workspace formatting passes. Changed document links, diff whitespace and engineering
  memory validation/render checks pass. These checks do not establish performance or broader
  platform support.

## Independent Work And Review

- Faraday `01a17779-14a8-70e2-b81c-f2099f49365d` implemented only the bounded Reflect helper,
  export and three tests. Lovelace `01a17778-ce43-77e3-9bdc-3242157ffd73` implemented only the
  Host consumer/accounting and actual capture tests. Both ran no Cargo/fmt, made no commit and
  are closed; parent owns all executed results.
- Carver `01a17781-121d-7b83-9736-6068bb41d19c` reviews the frozen integrated owner/consumer
  correction and confirmed one P2: default asset references in a complete KnownUnbound record
  have a second manifest copy beyond the flattened fact projection, while empty authored values
  cannot pay for that copy through document-byte charging. It implemented conservative two-copy
  flat projection charge when unavailable entries exist, plus an actual `charge_published_document`
  regression over repeated empty complete records/default lists. The case accepts its exact
  retained budget and refuses one byte less or only one projection copy. Parent owns compilation
  and execution; Carver ran no tests and is closed. Parent also merged the suggested duplicate
  Schema lookup in field classification without changing its fallback conditions.
- Ampere `01a1778e-0c3a-7ec2-910e-7f902e1de5dd` independently reviewed only the final
  two-copy Host charge and actual published-document regression. It confirms source-level closure
  without new findings, checked multiplication/sums and the exact/under-budget proof. Conservative
  overcharge in mixed documents is intentional. It ran no tests and is closed.

# Follow-up

This is not a durable overlay format, active nested preview or a new import graph. Current
`PrefabInstance.overrides` remains exact patches; full production declarative overlay replacement,
canonical source/conflict encoding, captured-base activation, repair/history and conversion remain
within the active foundation Goal. Unknown Schema payloads are preservable, not executable
dependency declarations. No new native widget, task lifetime or renderer behavior is introduced.

Reference-bearing preparation now has a conservative complete-classification fallback. Attribute
its cost on a real consumer before claiming unchanged latency or restoring an incremental proof.
Existing actual closure and capture receipts remain authoritative; derived projection provenance
is not a substitute for dependency capture.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Source ownership and preservation](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [Overlay replacement](../../../../architecture/prefab-overlay-design.md)
- [Predecessor instance transaction](2026-10-26T083507Z-exact-source-owned-prefab-instance-transactions-and-complete-snapshot-custody-85b9ca4381b94cad9c61fb829f479247.md)
- `crates/nara_reflect/src/asset_reference.rs`, `crates/nara_reflect/tests/field_asset_reference_limits.rs`
- `crates/nara_scene/src/authoring_decode.rs`, `authoring_decode/field_candidate.rs`, `tests/field_asset_facts.rs`
- `src/project_host/authoring/project.rs`, `authoring/mod.rs`, `authoring/project/tests.rs`

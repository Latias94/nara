---
type: "Verification Evidence"
title: "Complete-value projection comparison without source history or templates"
description: "Share production full-value validation with a private history-free projection compiler; preserve lawful containers without selecting a durable format or public activation API."
timestamp: 2026-10-26T06:13:53Z
record_id: "047bc67b5dec41c2a940091b39ceedba"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/effective-candidate-and-editor-progress"
base_commit: "3b1cd87"
---

# Verification

Successor to the qualified typed-origin checkpoint and earlier private structural comparison.
Source classification's complete-value validator is extracted into
`authoring_decode/complete_value.rs` with its original effective/default and schema-error
lowering. It remains the production source classifier's validation core. The strict whole-Scene
entry and declarative compiler remain test-only, not a speculative public candidate contract.

# Result

- The private compiler constructs complete values directly after default materialization and
  exact declared-reference binding. It no longer derives them from creation templates and
  root-field edits or calls source edit/construction prepare to validate a projection.
- Added projection records reuse the existing allocator, exact unpublished structure semantics
  and typed addition ancestry. Final suppression filters both target and provenance maps.
  Existing candidates validate their complete provenance before becoming inspectable.
- Current known Schema admits complete values without an Edit capability or authoring Adapter.
  The source classifier keeps its existing preservation/readiness semantics. Unknown, wrong
  owner, future/unmigratable and undeclared-field values do not gain projection admission merely
  because they remain savable source data.
- Existing file/value counters admit the borrowed base and final aggregate effective values;
  exact/under-limit cases include defaults. This does not prove complete pre-copy admission:
  per-record default materialization still precedes the final aggregate check, and the private
  fixture does not own production binding/provenance retention budgets.
- No source revision, dirty state, history entry or inverse is published by private compilation.
  The test with no template/Edit/Adapter and a one-byte history budget still creates a valid
  added entity and cross-origin reference while preserving the original source/history.

# Evidence

Serial Rust 1.95.0, `CARGO_BUILD_JOBS=1`, existing root target:

- RED `65630337-0686-4284-b176-09ae73f02afe`: lawful whole value `{}` rejects as
  `Conflict::Component` through the old template/root-field path. The prior comparison expected
  that known conflict; the new test requires acceptance and exact distinction from `{ group: {} }`.
- Initial focused implementation run `cf1f8a74-c757-4a22-8c62-51c80c4518ce`: 21 passed,
  1 failed. The newly added no-template/Adapter/history fixture omitted the grammar's mandatory
  added-entity placement. Its `Placement` conflict correctly exposed that invalid fixture; an
  explicit root Last claim corrects it without relaxing layout admission.
- Full all-feature Scene nextest `cf791b96-6c78-4e12-84ee-d4c1adce6986`: 378 passed,
  3 skipped across 20 binaries in 23.419 seconds after a 1m35s build. Source/data/prepared
  authoring, effective references, format limits and the whole structural comparison are included.
- Final focused complete-value/structural cohort `cb4cf738-e21d-4ff9-9bf0-f6982b26f64b`:
  22 passed. The unavailable-version fixture now uses current version 2 with valid old/new
  versions 1/3 rather than a zero version. This test-only correction follows independent review;
  the production and compiler implementation is unchanged.
- Scene all-feature/all-target correctness Clippy passed with existing warnings. It reports no
  warnings in the new complete-value module or changed structural comparison. Workspace
  `cargo fmt --all` completed. Documentation links and `git diff --check` pass. Later integrated
  Editor checks remain separate; the Scene suite is not native evidence.

## Independent Review

Reviewer `01a176d7-cbf8-7953-b834-de50c87c4e3c` completed the frozen seven-file Scene review,
including the untracked new module, with no confirmed findings. It checked unchanged production
source validation/diagnostics, current Schema versus unavailable preservation, exact defaults and
references, addition-local provenance, suppression, no-op intent and failure/history isolation.
It independently verified the literal 4-node/12-byte aggregate boundary and retained the stated
pre-copy/private-compiler limitations. It ran no Cargo, made no changes and is closed. Parent
execution owns the 378-case result.

# Follow-up

The durable production `PrefabInstance.overrides` remains `ScenePatchDocument`. Complete
property/reference grammar, bounded source/conflict codec, captured actual base closure,
source overlay edit/repair/history and convert-to-local still block its replacement. The shared
validation extraction is not a production overlay switch, native execution credential or new
Host publication protocol. The overall foundation Goal remains active.

# Citations

- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [Typed origin](2026-10-26T041116Z-typed-prefab-instance-addition-provenance-and-complete-consumer-propagation-331ec66de92c4315971800928d8f2dc7.md)
- [Earlier structural comparison](2026-10-26T034052Z-private-prefab-structure-and-reference-comparison-with-owner-boundary-counterexamples-c823420a09444940993ed9d948a28273.md)
- `crates/nara_scene/src/authoring_decode.rs`, `authoring_decode/complete_value.rs`, `format.rs`
- `crates/nara_scene/src/prefab/overlay_structure_comparison.rs`, `overlay_structure_comparison/tests.rs`

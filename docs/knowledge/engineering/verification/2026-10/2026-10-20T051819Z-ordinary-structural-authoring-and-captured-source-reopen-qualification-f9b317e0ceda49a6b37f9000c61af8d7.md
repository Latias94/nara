---
type: "Verification Evidence"
title: "Ordinary structural authoring and captured-source Reopen qualification"
description: "Source-local structural transactions, exact unavailable preservation, captured-source Reopen, ordinary public and actual Windows widget trajectories."
timestamp: 2026-10-20T05:18:19Z
record_id: "f9b317e0ceda49a6b37f9000c61af8d7"
producer_id: "codex-root"
run_id: "ordinary-authoring-20261020"
related_plan: "docs/plans/2026-10-20-ordinary-authoring-workflow-plan.md"
git_branch: "main"
git_commit: "e673693"
---

# Verification

Starting source: `ff095c5`, main. Windows 11, Rust 1.95.0, one Cargo job and shared target.
Root runs every Cargo/fmt/Git operation serially; three lanes have disjoint implementation files
and independently review other owners. Unrelated untracked vendor remains untouched.

## Regressions And Passing Cohorts

- Starting-source structural tests: `cbf10750-36f5-4d2b-9553-8f85416466df`, four executed,
  one passed/three failed. Unknown-reference deletion still refuses; positive Move, Complete
  subtree Delete and deleting a reference source were unavailable before implementation.
- Final Scene/tooling libraries and integration tests: **271 passed**, none skipped,
  `4d6ba6a3-9884-46e0-99b8-26e0c8a11985`. Covers operation-relative direct/nested unavailable
  placement, exact Undo/Redo, recursive/default inbound-reference refusal, internal subtree
  references, unknown/Prefab barriers, cycles, stale placement, inverse budgets and selection.
- The new captured Save M1/later Move M2 test then exposed the existing dirty Reopen mismatch:
  `ad4e0394-e713-4ef9-8775-23faad301a67`, one failed. Its expected Opened assertion failed;
  unwind of the still-live fixture ended the process with Windows `0xc0000409`. This is recorded
  as a failed regression, not a stack/allocator diagnosis. Its cleanup-on-failed-Reopen path was
  tightened before the passing run.
- Corrected persistence/owned Save/Reopen cohort: **20 passed**, 236 excluded,
  `a23b606b-cf6a-43fb-9224-05e6de315267`. The deterministic held Save test runs confirmed and
  uncertain publication variants: disk keeps captured M1, working M2 remains dirty, and explicit
  Reopen recovers M1. Existing external manifest-drift, stale prepared result, native codec affinity,
  DetectOnly conflict and cleanup tests still pass. New compound uncertainty coverage is directly
  Scene; Prefab has symmetric source-reviewed implementation plus its existing persistence tests.
- Editor library and public generic composition tests: **176 passed**, 19 native tests excluded,
  `83228275-e4ff-4eb0-9c20-b9b880cbf921`. New ordinary Scene/Prefab consumers prove unknown-bearing
  Move/Reorder, stale rejection, exact history, Save/Reopen, subtree restoration/parent selection
  and refusal without mutation. Specialized UI Prefab generic-command bypasses remain rejected.
  An initial test compile found value-pattern bindings incorrectly dereferenced; corrected before
  this passing run. New Browser waiting uses a deadline instead of fixed immediate polls.
- Actual Windows/DX12 native widgets: **2 passed**, 185 excluded,
  `a2152c65-96a7-4732-af86-cee4794fa940`. New trajectory clicks production Up/Down/Parent/root/
  reparent/Remove/Confirm/Undo/Redo/Save/Reopen via owned Context input, submits/presents through
  the native backend, then retires graphics/session. The existing ordinary Scene/Prefab component
  workflow passes alongside it. No OS pointer or keyboard injection.
- The existing clean-room generated desktop starter test **passes** after adding the ordinary
  structural workflow, `6fe8fc2d-df3a-49b3-96a7-f9963e04db08`: create Transform-bearing roots/child,
  reparent/reorder, Delete/Undo/Redo, Save/Reopen, public strict `check_project`, then real external
  Play reaches Running through the accepted first-presentation handshake, Stop reaches Idle and
  Editor Exit retires. Source bytes stay unchanged until Save; strict Check does not rewrite source
  or game Rust. Actual execution took 259.359 seconds including its supervised Cargo build, using
  shared target and one job. This is one acceptance observation, not an iteration-latency benchmark.
- The initial shared native/document consumer cohort, `e27f0d53-e6bd-45f1-ba46-ac1f1c29f9cb`,
  executes 65 tests: 64 pass and one historical Check fixture fails before its intended assertion.
  It rebuilt a generation-2 Scene owner with no predecessor catalog. The fixture now retains that
  predecessor while modifying its tombstone, so bundle lineage remains valid and the test still
  must reject the deliberate runtime-owner fingerprint divergence. Independent review confirms
  no production validator change. The private prepared-component compile-fail test passes.
- Final shared native/document consumer cohort: **65 passed**, none skipped,
  `827c5438-8410-4ab6-a22c-4831950297df`, including exact native patch/inverse/history/reference
  budgets and the intended runtime-agreement divergence/unbound checks after the fixture repair.
- Locked workspace all-feature/all-target check passes. The separate no-default-feature Scene
  all-target check exposed two older serde-only integration binaries without their feature gate
  (`authoring_decode`, `data_authoring`). Added the same file-level serde guard used by neighboring
  serialization tests; the final minimal all-target check passes. Native reference tests with
  individually gated serialization cases remain enabled. Final formatting/whitespace checks pass.
  The two guarded serde-enabled integration binaries are separately re-executed: **19 passed**,
  none skipped, `bfd35b0a-9e8d-479a-9563-53c7918cf1de`.

## Implementation And Independent Review

Structural policy lives in a cohesive private `authoring_decode/structure.rs`; existing topology
helpers are shared with native policy through record admission. Tooling translates existing
revision-bound commands and changes selection only after publication. Frontend capabilities expose
the basic deletion refusal; the document owner performs final reference proof.

Reopen captures the exact Host's observed digest and at most one fully authoring-validated
uncertain-candidate digest. Exact readback still performs full captured-basis decode/classification
and owner source/revision/schema checks. Known Failed/Rejected/Saved clears the candidate proof;
uncertainty/unwind retains it, and preflight RequiresReconcile cannot erase it. No second document
or saved manifest is retained. Externally changed bytes keep exact preservation checks.

Independent read-only review covers Scene/history policy, tooling/frontend and external/native
consumers. A separate reader reviewed the final concrete persistence changes and both native and
authoring branches; no blocking correctness or contract finding remains.

## Commands

```text
cargo +1.95.0 nextest run -p nara_scene -p nara_tooling --all-features --lib --tests --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run --all-features --lib --locked -E 'test(owned_save) | test(owned_reopen) | test(project_host::persistence::tests)' --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara-editor --lib --test generic_component_composition --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_ordinary_structure_widgets_arrange_delete_undo_save_and_reopen) | test(native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source)' --test-threads=1 --no-fail-fast
cargo +1.95.0 nextest run -p nara-editor --test supervised_play_clean_room --locked --test-threads=1 --no-fail-fast
cargo +1.95.0 nextest run --all-features --test scene_authoring_session --test scene_patch_transactions --test scene_component_composition --test scene_inspector --test scene_file_workspace --test project_workflow_check --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 check --workspace --all-features --all-targets --locked
cargo +1.95.0 check -p nara_scene --no-default-features --all-targets --locked
cargo +1.95.0 nextest run -p nara_scene --features serde --test authoring_decode --test data_authoring --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 fmt --all -- --check
git diff --check
```

# Result

Domain, persistence, public application, selected actual native widgets, generated starter strict
Check/external Play, shared consumers, full-feature workspace and minimal Scene checks pass within
their recorded scopes. Existing compiler warnings remain; no warning-free or full-release claim.
Implementation and owning ADRs/guide are committed together at `e673693`. No source change follows
the qualifying runs other than the two conditional test gates described above; their disabled
configuration check and enabled 19-test rerun pass. Documentation closure does not require another
native or gameplay acceptance cycle.

# Evidence

Local raw logs: `target/ordinary-structure-domain-red.log`, `ordinary-structure-domain-green.log`,
`ordinary-structure-held-save-red.log`, `ordinary-structure-persistence-green.log`,
`ordinary-structure-editor-green.log`, `ordinary-structure-native.log`,
`ordinary-structure-starter-play.log`, `ordinary-structure-root-consumers.log`,
`ordinary-structure-root-consumers-final.log`, `ordinary-structure-workspace-check.log`,
`ordinary-structure-scene-minimal-check-final.log`, `ordinary-structure-serde-fixtures-final.log`.

# Follow-up

This slice does not add unknown-record deletion, Prefab-instance source-closure deletion, specialized
UI structure expansion, multi-document ownership, world-space transform compensation, Tile tools,
virtual-canvas presentation, production P99 or Rust iteration measurements. Native evidence is
Windows/DX12 only. The starter proves strict structural execution and READY rather than new pixel
output or gameplay semantics. Counts overlap; they are not a single complete release cohort.

# Citations

- [Plan](../../../../plans/2026-10-20-ordinary-authoring-workflow-plan.md)
- [ADR 0090](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)

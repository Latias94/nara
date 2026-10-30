---
type: "Work Progress"
title: "Explicit UI candidate API and Runtime-bound Reference presentation observation"
description: "Reference consumer failures reveal ambiguous candidate lookup and a real post-commit production observation defect; direct API replacement and exact owner/session repair are frozen for final qualification."
timestamp: 2026-10-30T13:39:04Z
record_id: "4ac94530888142ae8d956f4172287842"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md"
git_branch: "refactor/foundation"
git_commit: "c624673"
---

# Summary

The complete foundation Goal remains active. All current work is still uncommitted on
`refactor/foundation`, based on `c624673`; main has not advanced. Follow the
[preceding owner/native record](2026-10-30T132119Z-bounded-input-final-owner-and-native-qualification-before-reference-game-completion-3e6f71eb12e94826a83c0938e275993a.md)
for the actual Input145, UI121, root correction28, independent8, processed seven-child, native4,
minimal, Clippy and neutral-example evidence before the candidate API replacement below.

# Details

## Reference Failure And Real Owning Correction

Reference Game's Editor-feature run `5a583d81-b7eb-433c-82c0-c37977d11cfa` executed 159 tests
across 32 binaries: 152 passed, seven failed, zero skipped. Build took 10m58s, tests133.964s.
The seven failures report explicit Rust assertions before aborting: a shared fixture requested
a candidate after successful commit had correctly consumed it, including the resize wait.

The fixture now reads its already existing `ComponentPresentationState` accepted packet/receipt.
Capture/planner inputs use that retained frame; resize also requires exact slot/route/geometry,
matching transform and a current semantic receipt. No candidate-or-presented fallback, engine
compatibility API or weakened layout/activation assertion was added. The complete three affected
binaries pass 12/12, run `aacc00e5-bdec-47fd-82a3-07111c252456`, build56.23s/run7.488s.
This result precedes the additional production/API correction below.

The repeated consumer mistake exposed an ergonomic defect: `latest_ui_frame(s)` sounded like a
presented-frame query but returned only a candidate. Accepted ADR 0102 now names the actual stage:
`ui_candidate_frame` / `ui_candidate_frames`. All current Rust consumers/reexports in UI, facade,
examples, Editor, WGPU measurements and Reference are migrated directly; no old alias remains.
Historical memory and imported research are not mechanically rewritten.

The parent also found a real production defect in
`reference-game/src/ui_observation.rs::from_presented_observation`. The formal desktop flow is
Render -> Cleanup successful commit/consume candidate -> Last observer -> ArmedCandidate capture.
Its old candidate Arc comparison therefore discarded valid presented observations and could
prevent the formal candidate smoke from arming its measurement window. The component fixture and
desktop_render_probe do not qualify this production observation path.

The observer now compares the retained submission's RuntimeGeneration to the actual managed
World, resolves `ui_semantic_snapshot` through the retained receipt, and requires its actual frame
revision to equal the retained frame. Candidate presence is irrelevant. Pascal's read-only review
confirms process-unique non-reused Runtime identity, per-surface latest-epoch validation, rejection
after suspend/restore/retire, and the necessary frame-revision check because UiRouteStamp does not
carry frame_revision. It does not depend on one global last-observation store and therefore does
not reject a valid surface merely because another surface presents later.

## Scope Review And Frozen State

Ramanujan owns/froze only the seven `nara_ui` Rust implementation/example/test files for the
mechanical rename and getter Rustdoc. Pascal audited the other lookup consumers: desktop layout
fixtures have no native surface submission; Editor Canvas probing precedes external Game View
completion; product/font probes run in Sort before Render/Cleanup; WGPU measurements manually
plan the candidate. No other presented/candidate misuse was found in that bounded audit.
Both agents are closed. No other source writer remains active.

After root/Reference formatting, the 50 changed/new Rust/manifest/lock files are frozen with
LF-normalized aggregate SHA-256
`8b243a92a66130010eb6bef3c2091f16081135b8d0c6abf9af27040cd624d159`.
TEMP `nara-bounded-input-source-freeze.json` contains sorted normalized hashes for later staged-blob
comparison. Prior44/45-file freezes retain their historical scope. The parent has started serial
workspace all-feature/all-target correctness Clippy followed by all-feature UI tests against this
new API. Exec session `83666` is live; logs are TEMP
`nara-bounded-input-candidate-api-workspace-clippy.log` and
`nara-bounded-input-candidate-api-ui-tests.log`. No other Nara Cargo may start until terminal.

No native claim for the renamed API or repaired production observer has yet been made.

# Next Action

Collect the current Clippy/UI result. Check all Reference targets and run its affected UI binaries
with the final names. Build the actual Reference `desktop` binary into root target and run
`target/debug/desktop.exe --candidate-smoke` with `NARA_REFERENCE_UI_OBSERVATION=1` and DX12.
There is no project-root CLI flag/env in this parser: a development executable falls back to its
compiled Reference CARGO_MANIFEST_DIR. Require exit0, two correlated UI/GPU JSON records and
`desktop_candidate_smoke: ok`; reuse existing smoke validators without a new verification tool.
This is source-based native evidence, not a packaged artifact/release claim. The existing
desktop_render_probe does not call the repaired observer and cannot replace this smoke.

Requalify the changed UI inventory through the relevant root/native/neutral consumers, then write
final evidence, close ADR0102's exact checkpoint and update ADR0041/0108, ledger and plans. Inspect
and stage only intended paths, match source bytes to the final freeze, commit and locally ff main.
Do not push, change references/vendor, delete build output or reduce the complete active Goal.
Valid-reference/current-demand separation remains the next ordered foundation contract.

# Citations

- [Input plan](../../../../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md)
- [ADR0102](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)

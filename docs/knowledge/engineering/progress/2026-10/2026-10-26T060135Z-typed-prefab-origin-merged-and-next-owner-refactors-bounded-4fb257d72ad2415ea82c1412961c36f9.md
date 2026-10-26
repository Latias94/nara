---
type: "Work Progress"
title: "Typed Prefab origin merged and next owner refactors bounded"
description: "Qualified origin and fault-delivery commits are on local main; complete-value projection admission and a single Application no-action pass remain bounded owner work."
timestamp: 2026-10-26T06:01:35Z
record_id: "4fb257d72ad2415ea82c1412961c36f9"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_commit: "a4a6f61aaf59a6a2848676b9e2acced90f14f81d"
---

# Summary

Local `main` fast-forwarded to `a4a6f61`; no push or PR. Its predecessor `6b3d458` corrects
activation-local typed fault delivery and the Reference Game Exit caller. The origin commit
replaces misleading base attribution for instance additions through actual Scene expansion,
Runtime publication, retained custody, UI keys and all tracked consumers. Only the pre-existing
untracked `vendor/` remains outside owned work. The overall foundation Goal remains active.

The final complete Reference Game run passes 170/170. Three selected actual Windows/DX12
Editor regressions pass, separately from the Scene/Tooling/UI 545-case, Root 458-case and
independent consumer qualifications. Workspace all-feature/all-target correctness Clippy passes
with existing warnings. The linked verification records preserve original failures, exact reruns,
review corrections and native limits; this progress shard does not expand their scope.

# Details

## Full-Value Candidate Owner

Read-only sidecar `01a176be-aa6b-7af1-81ed-f2e1dbe671ff` bounded the next Scene seam. Its concrete
findings are incorporated in the Proposed overlay design's Full-Value Admission Boundary:
reuse complete-record classification without a temporary Session, creation template or Undo
budget. Preserve exact lawful full values and provenance, defaults-before-reference binding,
pre-copy aggregate accounting and separate native admission. The sidecar made no changes, ran
no Cargo and is closed. Do not introduce a public candidate/publication-token family before
the real compiler and existing Host holder establish its evidence flow.

## Application No-Action Progress

Read-only sidecar `01a176c2-699e-7d22-a2dc-f6facbe2eb12` confirms a public ergonomics gap, not
another confirmed defect in the corrected Exit caller. The current frame/no-frame branch is
correct but exposes internal lifecycle choreography. A proposed ordinary
`advance(&mut self, real_delta: Duration) -> EditorApplicationUpdate` should perform one bounded
Application pass without an artificial action batch, waiting or native event pumping. This
method is not implemented by this checkpoint.

The implementation cannot merely call both old drivers:

- `editor/src/platform/imgui.rs:2151` already advances Open and Session in maintenance.
  Replacement Preparing and Close initiation can themselves drive a Session. Combining these
  naively can double-drive, consume time twice or recursively drive a just-published successor.
- Genuine dispatch, prepared commits and frontend dispatch/drive/finish retain exact
  `actions_observed` and owned admission evidence. A no-action pass must not erase the action
  that defers a newly Prepared open, replace submission observations, settle a frontend candidate
  or silently rebase one. Existing candidate-staleness behavior remains.
- Preparing/Refreshing must maintain retained supervised work even without an ordinary frame.
  Preserve effects-only `SupervisedPlayWake`; turning quiet wakes into full drives would add
  frames/redraw cadence. Host wake/sleep policy stays outside the ordinary API.
- Zero delta still permits maintenance and observation publication. Only actual `exit_ready`
  proves physical completion, never absent Workspace/Session/frame or an empty action result.

Reuse the existing initial/replacement Open, supervised owner and Session/process-retirement
owners, not a trait framework or another state machine. Existing ADRs 0047/0118/0084 already own
admission, single-driver and physical close; the sidecar recommends implementation/rustdoc/guide
cleanup rather than a new ADR. This is a proposed bounded follow-up, not an Accepted API promise.

Essential regressions: ordinary Open/replacement/failed-Open retry using only the public pass;
held Open Host cleanup after frame disappearance with exact false/true `exit_ready`; dirty
Cancel/Discard and stale Exit admission; single time consumption and no premature Prepared
commit; native Paused/Step and quiet wake cadence; supervised Rust failure/repair/new generation.
Migrate progress-only Reference Game and independent panel helpers, plus native maintenance;
do not change real action submission or force direct Session consumers through Application.

The sidecar used pinned Godot only as limited evidence for separating time, maintenance and
presentation (`main/main.cpp:4948`, `:5112`; `editor/editor_node.cpp:937`, pin `81ab961`). It did
not execute Godot or qualify Nara behavior. It made no changes, ran no Cargo and is closed.

# Next Action

Continue the full rebaseline, starting with the full-value Scene owner and its real compiler
consumer. Keep the Application pass as a separate bounded owner change with its own regression
and native gate. Do not conflate either with durable overlay production replacement: complete
property/reference grammar, conflict persistence, source edit/repair/history, actual captured base
closure publication and conversion remain unimplemented. Unity/Godot remain the editor workflow
references; Bevy remains primarily the Rust ECS/scheduling reference.

# Citations

- [Origin qualification](../../verification/2026-10/2026-10-26T041116Z-typed-prefab-instance-addition-provenance-and-complete-consumer-propagation-331ec66de92c4315971800928d8f2dc7.md)
- [Fault/Exit qualification](../../verification/2026-10/2026-10-26T052536Z-retained-runtime-fault-delivery-and-reference-game-exit-driver-correction-19b2541631464d8798e3fa47ace8bfca.md)
- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Unity/Godot study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
- `editor/src/app.rs`, `editor/src/platform/imgui.rs`, `editor/tests/application_contract.rs`
- `editor/src/frontend/imgui/submission_tests.rs`, `docs/guides/editor-panels.md`

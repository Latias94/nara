---
type: "Work Progress"
title: "Owned native rendering migration and independent lifecycle review in progress"
description: "Work Progress for Owned native rendering migration and independent lifecycle review in progress."
timestamp: 2026-10-31T10:02:22Z
record_id: "37208eb577264f2788ff41b8155d9908"
producer_id: "codex-foundation-main"
run_id: "render-owned-native-session-2026-10-31"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "cd35da0"
---

# Summary

The complete foundation Goal remains active. `main` and `refactor/foundation` still point to
`cd35da0`. The actual owned native session/capture/result replacement is uncommitted and undergoing
integration; neither the full R8/R9a outcome nor this native replacement is qualified yet. Preserve
all current authorized changes and the unrelated untracked `vendor/`; do not push or open a PR.

# Details

- The user-supplied October 30 ZIP was already imported and reconciled in the linked research.
  Do not repeat extraction or restart the roadmap. Continue the selected native Host transition,
  followed by immutable output/producer-prefix and external Feature consumers.
- WGPU no longer implements ECS Resource or runs native submission in the render schedule.
  CPU capture/result custody uses the original shared connection, issued Runtime/connection/
  sequence identity, pre-admitted result storage, refused-input return and post-operation fault
  preservation. The original Device/Queue drain is captured before uploads and survives backend
  namespace clearing. A cancelled capture has no new native observation.
- Hilbert owns Winit session/pump and its tests. Root Faraday is extracting the same DesktopRun
  into the common project_host/desktop_run owner: code-first clear/UI products must work without
  serde or the full 2D project loader. Source/processed construction keeps the original content/
  configuration context and consumed publication. No second native facade or expanded example
  feature requirement is selected.
- Aquinas migrated Reference candidate/startup consumers from Last/GPU queries to the actual
  post-Host CPU observer and finite telemetry seal. Harvey migrated examples and boundary tests;
  the newly speculative two-window smoke option is being removed until its complete stream/output
  consumer exists. The ordinary one-window smoke retains actual resize and original target facts.
- Independent WGPU review found a real Drop-order race, incomplete pre-collection authentication,
  and cancelled status overwriting native state. Parent fixed the owning boundaries: transfer
  guard last, original-authority checks in Reset/Collect/Seal, and optional native status preserving
  backend/error on cancellation. Lovelace is adding controlled cross-thread actual-permit Drop
  tests in runtime_drop_tests.rs. Parent added replacement-before-collection and state tests.
- Independent native/Root review found premature Destroyed during Retiring could be reported as
  successful, and Wait maintenance wakes still drove gameplay. Hilbert is correcting both with
  the target's actual ProviderReleased phase and separate frame-demand/maintenance handling,
  including an actual retained-pump regression. Faraday also received the final observer-exit
  priority correction: validation failure must override a telemetry seal's scheduled success.
- Existing merged branch inventory is already just main and refactor/foundation. No new branch
  per slice is needed; the next exact commit can fast-forward main after qualification.

Actual results so far, with subsequent-change limits:

- First WGPU all-feature lib check passed before later review corrections.
- Initial focused20: 16 passed, four new fixture expectations failed. Stop requires continued
  drive before CloseIncomplete; faulted Runtime legitimately accepts original result retirement.
  Corrected tests preserve those production contracts.
- Combined owning CPU cohort: 68 passed, run `ad5f06e3-9d4d-4b2e-8f93-c4fed71905cc`.
- WGPU complete default library: 164 passed, six ignored, run
  `d91400f0-ea1f-4b11-8dea-064bc097997d`. This precedes the review-driven Drop/auth/status fixes.
- Actual Windows Winit original-pump early Exit, incomplete budget retry/Destroyed, construction
  failure, fail-stop child and ordinary windowed retirement: five passed, run
  `ee8e539d-7df8-4bac-8c9d-0c17e00f21ce`. This precedes the two later Winit corrections.
- Minimal WGPU library check passed after the authority/status fixes, with existing cfg warnings.
- No full Root/Reference/examples build or current native Editor acceptance exists yet. No global
  formatting has run while workers are editing. At capture no Cargo process is live.

# Next Action

Finish the bounded workers and inspect their diffs, then format after all writers freeze. Run
final WGPU transport/Drop and Winit owner/native cases, common minimal desktop and independent
presentation2d/UI feature consumers, Root/all-feature and Reference/example integration. Reconcile
actual post-Host first-present/supervised READY and telemetry-seal outcomes; execute the relevant
native Editor critical trajectories. Independently rereview corrections, record exact evidence,
make precise local Conventional Commit(s), fast-forward main, keep the overall Goal active.

# Citations

- `docs/research/2026-10-30-engine-direction-reconciliation.md`
- `docs/plans/2026-10-31-render-composition-and-host-execution-plan.md`
- `docs/architecture/render-host-ownership-design.md`
- `docs/architecture/adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md`
- `docs/verification/native-editor-qualification.md`

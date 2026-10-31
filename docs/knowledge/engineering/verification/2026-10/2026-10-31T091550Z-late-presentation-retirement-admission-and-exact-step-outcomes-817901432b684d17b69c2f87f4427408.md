---
type: "Verification Evidence"
title: "Late presentation retirement admission and exact Step outcomes"
description: "Verification Evidence for Late presentation retirement admission and exact Step outcomes."
timestamp: 2026-10-31T09:15:50Z
record_id: "817901432b684d17b69c2f87f4427408"
producer_id: "codex-foundation-main"
run_id: "render-late-presentation-retirement-2026-10-31"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
---

# Verification

Implementation **`525cd6f51648c5ffc81af908e277a1fe09aec9bd`** on `refactor/foundation`, following
the shared drain at `96bd0bf` and [original-result qualification](2026-10-31T082117Z-original-ui-result-custody-and-logical-close-qualification-6e1965c6428841ec9138088bef20f90c.md).
Windows 11/DX12, Rust 1.95.0 MSVC, serial Cargo with one job and the root target. The whole
foundation Goal and complete production Host transition remain active; no remote publication.

# Result

Actual next-boundary inspection exposed two late-result windows. A temporarily missing close port
fenced the original receiver but left its ledger unfenced; ordinary commit could drain Presented
and consume the original Input authority. The first lower Runtime Stop drive also entered Stopping
before close participants ran; the actual Game completion entry still published UI interaction and
World presentation. Completing a transport is not permission to reactivate either authority.

Root now checks the original receiver's monotonic close fence before draining ordinary results.
Game completion derives Present or Retire from the exclusively borrowed Runtime for each call;
the typed port rechecks that mode against its lifecycle state. Running/Paused may present;
Faulted/Stopping/CloseIncomplete may settle original transport without publishing UI/World
authority. Refusal preserves the original completion, and retry derives current admission again.

The private consumed result now distinguishes normal presentation handling from retirement.
Session clears its exact in-flight identity once and uses the actual returned outcome. A matching
pending Step retires as a presentation failure; it cannot report Applied or requeue another
candidate. Original native disposition remains input evidence, not the Session operation outcome.
Post-operation faults are still reconciled after consuming the real result, without replay.

ADR0102 records this lifecycle boundary. No ordinary completion compatibility alias, global Runtime
state mirror, new callback registry or universal execution context was added.

# Evidence

| Check | Actual result and coverage |
| --- | --- |
| Initial reproduction | Run `3f8bbf97-6393-4a73-9541-a1866a5e9f2b`: **0/2 passed**. The actual Game first-Stop case failed `!interaction_published`, proving the production defect. The missing-port case aborted on the recovery helper assertion; its first RetryClose drive had only reentered Stopping, so that failure is not independent proof of the asserted late publication. |
| First production correction | Run `d537058f-ec43-4acc-8a17-d90f4824c330`: Game case passed; missing-port case still hit the same fixture assumption. The fixture now explicitly drives the RetryClose transition before asserting settlement. No close state gate was weakened. |
| Integrated receiver/admission selection | **82 passed**, run `f68aef4b-5216-4b59-8258-371b4c6bedff`, build 1m02s/run 7.343s. All UI-render library tests plus Root UI and Game outcomes passed. This preceded the later precise private operation result. |
| Lifecycle-mode gate successor | **19 passed**, run `220c8434-c665-4484-956f-bde31c8170a4`, build 55.55s/run 3.290s. Exact Present/Retire state admission was included; the final operation-result successor below supersedes this narrower scope. |
| Final Root UI and Game outcomes | **37 passed**, run `b35fe931-735d-4e08-bf5f-232459bd60e9`, build 1m28s/run 6.062s. The real Pause → Step → first lower Stop → late Presented path asserts no new interaction/World presentation and a failed Step. Original completion refusal, consumed postfault, CloseIncomplete recovery and statistics-independent close remain covered. |
| Final native Editor critical trajectories | **3 passed**, run `597d7be4-6a2d-40df-882a-28db78a7d732`, build 2m14s/run 7.211s. Actual paused cadence, Game input/refocus/resize/Step and independent View close/reopen/recovery/retirement execute through final source. The other 300 cases were outside this selection. |
| No-UI build | Root `cargo check --no-default-features --features project-workflow-cli,tooling,presentation-2d,desktop-winit,render-wgpu --lib --locked` passed in 53.54s. This checks the shared Game result contract without the UI submitter; it is not execution of a no-UI native product. |
| Correctness Clippy | Root and `nara_ui_render`, all features/lib/tests, passed `-D clippy::correctness` in 1m29s with existing non-correctness warnings. |
| Static checks | Root `cargo fmt --all --check`, `git diff --check` and changed architecture local-link checks passed. Only documentation integration followed. |

Native checks selected `WGPU_BACKEND=dx12` and `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`. There is no
new Linux/macOS, monitor scanout, full-render-graph or custom editable Editor qualification.
Earlier supervised and neutral evidence retain their recorded source/scope; no broad green suite
is invented from these focused results.

Nash's requested Astra/max review supports the owned native direction and identified three P2
interface requirements: finite telemetry sealing with no new submissions; direct first-present
timing notification; and separately named existing Runtime close policy versus native drive budget.
All are incorporated into the selected design, not counted as implemented. Its follow-up agrees
with distinguishing retirement from presentation in the private result and requires the actual
pending-Step regression; the final37 selection supplies that execution. Parent reviewed and
integrated the final source. All agents are closed.

# Follow-up

Continue the same [complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
from the finite stock capture/result connection and owned native run session. Shared drain and
original logical close are qualified prerequisites. GPU ECS storage, scheduled native submit,
borrowed Winit runner, post-Host first-present/READY maintenance, telemetry seal delivery and
complete incomplete-session retirement are still unimplemented.

The selected ADR0032/0040/0078 refinements reconcile outside-World execution and custody before
that replacement. Root may reuse DesktopRun for an already-started Runtime, retaining the original
content/configuration context and pump rather than refilling a consumed publication. Keep actual
measurement/timing consumers in the transition; a last-completed observation alone cannot drain
telemetry or preserve parent-observed startup-marker timing. Then continue immutable outputs,
producer-prefix retirement, exact reconstruction input, graph/Feature and Nara-drawn composition.
This checkpoint does not shrink or complete the foundation Goal.

# Citations

- [Selected production design](../../../../architecture/render-host-ownership-design.md)
- [Original-result ADR0102](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md#original-backend-outcome-ownership)
- [Actual Game completion](../../../../../src/project_host/runtime/editor/game_view.rs)
- [Session operation result](../../../../../src/project_host/runtime/editor.rs)
- [Root original-receiver commit guard](../../../../../src/ui_presentation.rs)
- [Shared-drain qualification](2026-10-31T084653Z-shared-concrete-gpu-queue-retirement-qualification-ab68d8adf25f406d9222df6b3b2fcc70.md)

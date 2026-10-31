---
type: "Verification Evidence"
title: "Original UI result custody and logical close qualification"
description: "Verification Evidence for Original UI result custody and logical close qualification."
timestamp: 2026-10-31T08:21:17Z
record_id: "6e1965c6428841ec9138088bef20f90c"
producer_id: "codex-foundation-main"
run_id: "render-original-result-owner-2026-10-31"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
---

# Verification

Foundation R8/R9a on `refactor/foundation`, Windows 11/DX12, Rust 1.95.0 MSVC. The receiving
baseline is `5b6d217`, canonical fixture corrections are `c73e670`, and the original-result and
logical-close implementation is **`8e9d840dd97a647e36c04e9ac3679f7f4ccc9954`**. Cargo ran serially,
one job, using the root target. No remote publication occurred. The complete foundation Goal
remains active.

# Result

Backend UI permits retain pre-admitted result slots in their original nominal batch. Resource
replacement and equal submission values cannot receive another batch's permission. Dropping a
candidate or refused completion returns its prebuilt Aborted outcome without a World or another
allocation. Original receivers remain available independently of the mutable World ingress.
Compile and take validate that original ingress before planner mutation or queue consumption;
pending and returned results are observed under one batch lock.

The Root bridge registers a retained logical-close participant before transfer. Stop fences new
admission, releases original UI/Input authority, terminalizes Deferred attempts and waits for
outstanding backend permissions before Runtime Stopped. Missing or foreign original port, ledger,
planner or Input authority remains retryable without consuming it. Statistics are observations,
not close authority: their absence cannot prevent settlement. A held original result blocks a new
capture; after transport settlement, an independently delayed acceptance can be superseded without
reviving stale authority. The WGPU plugin returns buffered permissions during shutdown while the
World and frame slot still exist.

`WgpuRenderObservation` transfers exact CPU transaction, UI-submission and presentation facts to
Root consumers. It contains no GPU handles; counters cannot synthesize a receipt. Its UI fact
vector moves through publication rather than allocating after native submission. Root UI and
desktop first-present observers now consume this projection.

This is a production prerequisite, not the complete Host transition. The GPU backend still lives
in ECS, desktop verification still runs in Last, and native session/output/producer-prefix/Feature
replacement remains open. Proposed ADR0121 stays Proposed. Ordinary UI authoring and manual neutral
presenter interfaces are unchanged; ADR0102 records the hidden exchange's stronger lifetime rules.

# Evidence

| Check | Actual result and scope |
| --- | --- |
| Broad six-package all-feature library cohort | Run `ab991a7a-941b-4b01-9b6f-6b6745b72b87`: **1613 passed, 3 failed, 11 skipped**, 1616 executed, build 7m37s/run 462.850s. The three failures and their corrections are below. This is not recorded as a green broad run. |
| Final integrated owner selection | Run `10c87a67-6c6c-40ee-9487-72fea8fbe44d`: **85 passed**, build 2m39s/run 8.288s. Includes all UI-render library tests, Root UI/close, actual Game View completion, the WGPU buffered-shutdown regression, UI evolution and the Player golden. The other 1545 collected cases were outside this selection or ignored. It covers every broad-run failure correction and the subsequent diagnostic-owner change. |
| Final native Editor critical trajectories | Run `946d360d-37a6-4135-b50a-5802615f666b`: **3 passed**, build 1m35s/run 7.792s. Actual paused redraw, Game input/refocus/resize/Step, and independent View detach/minimize/reopen/recovery/retirement execute after the diagnostic-owner correction. |
| Native supervised READY and exit presentation | Earlier run `1f801a34-f85e-42a0-b462-0315106cd692`: **4 passed**, build 4m07s/run 118.753s. Three cases overlap the final critical group. The additional real supervised Cargo/READY/quiet-maintenance/child-exit trajectory passed in 111.600s. This run preceded the diagnostic-absence correction; the final critical run and owner selection qualify that successor. |
| Neutral Runtime/UI consumers | `nara_ui --example interaction_lab --all-features` passed with actual two-surface, routing, interaction, accessibility and close assertions; build 1m38s. Root `ui_presentation_gallery --all-features` passed with owned solid/Image/glyph packets, exact receipt, accessibility and retained retired-packet assertions; build 1m37s. Both ran without their windowed mode. |
| Correctness Clippy | All six affected packages, all features/all targets, passed `-D clippy::correctness` in 2m55s. Non-correctness warnings remain, including large owned error values; this is not a warning-free claim. |
| Minimal WGPU | `cargo check -p nara_render_wgpu --no-default-features --locked` passed in 6.24s. Two feature-specific unused-mut warnings remain. |
| Static integration | `cargo fmt --all --check`, staged/unstaged `git diff --check` and affected architecture local-link checks passed. No product source changed after final owner/native/neutral checks. |

Native commands selected `WGPU_BACKEND=dx12` and `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.
Unavailable native prerequisites were not converted into passing skips. The three critical
trajectories use the procedure's hidden native windows. The supervised maintenance case uses its
documented initially inactive visible Editor and ordinary child, without OS pointer injection.
These tests do not establish monitor scanout, Linux/macOS or the future Nara-drawn editable Editor.

The broad cohort exposed three precise corrections:

- The old Game View fixture demanded Stopped while it still held an original UI completion.
  That contradicts the selected logical-close barrier. It is replaced by a real no-UI Stopped
  scope rejection and a separate real Game/UI case proving that Stop waits, the original completion
  remains admissible while closing, and settlement releases the barrier. Exact UI buffer custody
  on actual domain rejection and post-operation fault remains covered.
- The Player source golden was checked out with CRLF while the generator emits canonical LF.
  `.gitattributes` now preserves that exact fixture as LF. Its byte-for-byte assertion remains;
  the test does not normalize the comparison.
- UI predecessor fingerprint `454b28fa...` predated `1193f4a`'s explicit AssetRef demand encoding.
  Independent source/history attribution reproduced the old full fingerprint with `[2]` and the
  current `7352339b...` with `[2, Immediate]`. Exactly three generation-two Image-reference leaves
  account for the difference; the fixture already held the current predecessor. Only the stale
  test constant/comment changes. Canonical hashing, component versions and lineage assertions
  remain intact.

Independent review corrected post-submit vector growth, replacement ingress escaping original
custody, the last-permit two-read race, and Root backpressure observing replacement storage.
Nietzsche cleared those owner corrections; Bernoulli cleared the later compile-ingress guard;
Boole cleared statistics-independent settlement and the meaningful buffered-WGPU shutdown test.
Rawls attributed the fingerprint difference. Hubble and Mill mapped the next owned native transition
and locked Windows pump behavior. Those last two are source/design evidence, not execution of
that future migration. All agents were closed after their work.

# Follow-up

Continue the [selected production ownership design](../../../../architecture/render-host-ownership-design.md)
with a complete owned native run session, finite capture/result connection and the same stock
local WGPU execution. Move exact UI/first-present maintenance, optional None observations and
deadline wakes together. Preserve original results across pre-application rejection, reconcile
post-operation faults once, establish original epoch retirement before submission, and retain
Runtime/driver/handle registry/windows/event pump through incomplete exit or timeout. Remove GPU
ECS storage, scheduled submit and missing-resource retirement inference with their real consumers.

Then implement immutable physical outputs, producer-local prefixes, exact reconstruction input,
external graph/Feature and policy consumers, independently paced streams and Nara-drawn clipped
composition. Original pixels, designated interactive display and passive mirrors remain distinct
authorities. Schema/Prefab evolution, joint motion/execution, material declarations and the complete
editable Editor retain their own unfinished units. This qualification closes none of those scopes.

# Citations

- [Complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [ADR0102 original-result contract](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md#original-backend-outcome-ownership)
- [Original UI result storage](../../../../../crates/nara_ui_render/src/integration.rs)
- [Root logical close](../../../../../src/ui_presentation/close.rs)
- [CPU WGPU observation](../../../../../crates/nara_render_wgpu/src/observation.rs)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [Predecessor device-owner qualification](2026-10-31T064658Z-coherent-wgpu-device-ownership-and-render-cost-comparison-qualification-18760d4fdea7435b90fb9bd2bee64510.md)

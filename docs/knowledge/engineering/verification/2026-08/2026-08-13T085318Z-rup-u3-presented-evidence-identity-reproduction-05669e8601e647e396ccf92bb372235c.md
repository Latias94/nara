---
type: "Verification Evidence"
title: "RUP-U3 presented evidence identity reproduction"
description: "Static owner-local reproduction proves App-frame/Render-transaction conflation, latest-candidate substitution, and pre-readiness telemetry admission."
timestamp: 2026-08-13T08:53:18Z
record_id: "05669e8601e647e396ccf92bb372235c"
tags: ["runtime-ui", "presented-evidence", "wgpu", "reproduction", "puec"]
status: "reproduced"
producer_id: "codex-root"
run_id: "presented-ui-evidence-correlation-correction"
related_plan: "docs/plans/2026-08-13-001-refactor-presented-ui-evidence-correlation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "a2fd15f"
verified_by: "static-owner-local-audit"
---

# Verification

The RUP-U3 product-evidence path was inspected at commit `a2fd15f` before Product-Validated
publication. The audit followed the exact identities from `FrameExecutionStart`, through
`WgpuCapturedFrame`, WGPU submission/completion telemetry, UI backend acceptance, receipt commit
and the Reference Game candidate observation.

# Result

The shared evidence contract is not sufficient for RUP-U3 publication:

- completion telemetry records the App-frame identity, while the candidate probe compares it with
  the Render transaction's `RenderFramePacket::frame_index()`;
- `ReferenceUiObservation::from_world` reads the latest compiled candidate, not the exact immutable
  frame whose backend acceptance advanced the receipt;
- the candidate starts telemetry before product readiness, drains all historical samples after the
  first ready observation and counts a global four-sample warmup, so pre-readiness work can satisfy
  the product measurement;
- when the historical sample count is too small, the probe rejects immediately instead of remaining
  pending until enough post-readiness samples arrive or the existing deadline expires.

RUP-U3 is therefore paused. The Accepted UI/Render decisions are unchanged; the focused Presented
UI Evidence Correlation Correction owns the repair.

# Evidence

- `crates/nara_render_wgpu/src/frame.rs` retains `app_frame_index` separately from the render packet.
- `crates/nara_render_wgpu/src/backend.rs` starts the backend transaction from
  `packet.frame_index()` but previously passed only `app_frame_index` into completion telemetry.
- `crates/nara_render_wgpu/src/lib.rs` test
  `exact_fixed_step_does_not_require_app_and_render_frame_counters_to_match` proves the counters
  may legitimately differ.
- `reference-game/src/bin/desktop_candidate_smoke/mod.rs` compared the completion sample's single
  frame counter with `WgpuFrameTransactionStats::frame_index()` and read
  `ReferenceUiObservation::from_world` before assembling its evidence.
- `src/ui_presentation.rs::commit_presented_attempt` is the only owner that consumes the exact
  backend acceptance and commits the matching receipt, but it did not publish one immutable
  observation binding the receipt, packet and backend transaction for a first-party evidence
  consumer.

This is a static owner-local reproduction. No Cargo command or desktop probe was used to establish
the defect; executable counterexamples and product canaries belong to PUEC-U1 through PUEC-U3.

# Follow-up

Execute `docs/plans/2026-08-13-001-refactor-presented-ui-evidence-correlation-correction-plan.md`.
Publish an independent conformance record, then explicitly resume RUP-U3 without replacing the
pinned UI Foundation or Product Composition baselines.

# Citations

- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/plans/2026-08-13-001-refactor-presented-ui-evidence-correlation-correction-plan.md`
- `crates/nara_render_wgpu/src/backend.rs`
- `crates/nara_render_wgpu/src/frame.rs`
- `crates/nara_render_wgpu/src/telemetry.rs`
- `src/ui_presentation.rs`
- `reference-game/src/bin/desktop_candidate_smoke/mod.rs`

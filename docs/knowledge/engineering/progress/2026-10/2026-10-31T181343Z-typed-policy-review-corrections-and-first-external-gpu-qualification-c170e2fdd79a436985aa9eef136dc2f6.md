---
type: "Work Progress"
title: "Typed policy review corrections and first external GPU qualification"
description: "Records actual independent-policy CPU and DX12 evidence, structural review corrections, and the still-open full integration qualification."
timestamp: 2026-10-31T18:13:43Z
record_id: "c170e2fdd79a436985aa9eef136dc2f6"
producer_id: "codex-foundation-main"
run_id: "render-typed-policy-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "ebfcdce7523c7e8fa2ed4c8a6e276839e545360a"
---

# Summary

The complete foundation Goal remains active. This is uncommitted integration on
`refactor/foundation` above `ebfcdce`; main remains at that green predecessor. All writers and
reviewers have been closed. The supplied archive's SHA256 still matches the receiving record.
No roadmap reset or toolkit adoption follows from this recheck.

# Details

The selected typed policy replacement now has source through Runtime, Scene, Canvas, embedded
Game and independent image consumers. Capture, window presentation, stock policy and domain
contributors are explicit installations. Original inputs retain their definitions across capture
withdrawal and reconstruct CPU instances on a replacement Host. UI permissions travel separately.
The old central production payload, stock-combination image facades and old plugin names are gone.
Remaining numerical draw controls are explicitly test-only; an unused legacy UI constructor was
removed. Feature names are now `stock-sprite` and `stock-ui`, without compatibility aliases.
Generic UI types are `WgpuUiSubmission`, `WgpuUiImageSubmission` and `WgpuUiSubmitFailure`.

Independent reviews and the real external GPU consumer produced these owning corrections:

| Finding | Correction and scope |
| --- | --- |
| P1: UI drawn into U could authorize presentation of unrelated final F | Bind the exact operation/instance/capture to the selected final texture. Check the actual original UiReport and its facts Arc before publication consumes the watermark. Runtime completion capacity still admits before native work. |
| P2: public validation/adaptation panic escaped structured errors | Catch validation at original input construction; retain original inputs and allow retry. Host construction still retires only the failed installation's CPU cache. |
| P2: cache capacity and root tuple metadata were undercounted | Replace retained Vec capacity with a private fixed64-slot table, charge the full table, original fixed wrappers/counters and exact submission-root increment. Admit predictable custody before allocation. |
| P1: ordinary policy content rejection faulted the whole Runtime | Preserve structured policy errors through ordinary rejection; the next frame can repair content. Foreign source/topology faults retain System classification. Real transport regressions are source-ready. |
| P2: external policies could not deliver Runtime UI permissions | Add public atomic `capture_with_ui`, admission inspection and failure retaining the original sidecar. Existing `nara_ui_render::advanced` exposes the concrete reader/frame/outcome exchange. Seven external CPU cases are included in the focused passing group. |
| Actual DX12 failure: legal alternate final usages differed from stock target allocation | Match physical usage as a superset of the exact logical declaration. Preserve graph access, import/write, owner, extent, format and descriptor checks. Independent narrow review found no P1/P2; raw native adapters remain trusted. Do not change the fixture to require stock-only COPY_DST. |
| Buffered Game interruption changed pre-submit Aborted into Target fault | Explicit cancellation maps prepared attempts to Aborted and bound attempts to Deferred. Preserve the original retry/identity facts. |

The external renderer is a scalar-field heatmap with its own shader, uniform and native Feature.
It uses normal ECS capture and the same Host rather than stock quads or direct-program-only
submission. Its window observer retains a sibling input from the same installation/capture Arc
and public same-frame topology; that is not a claim to retrieve private connection transport.

# Verified State

- Owner all-feature/minimal lib checks passed before the final UI bridge/name refinement.
- Root and native Editor all-feature lib checks passed before that final refinement. A neutral
  Root union with presentation2d/UI and no render-wgpu also passed; later minimal cfg cleanup
  has not yet been rechecked here.
- Neutral Render initially ran39/40 successfully; a new fixture had invalid equal-order Views.
  After fixing its authored order, all four image-adaptation tests passed, run
  `62794c79-2b0f-4ddb-91d5-ba8e991a6299`.
- Independent policy all-target check passed after correcting the example's system result to
  the actual Bevy error contract. Independent CPU3 passed, run
  `913c734f-57d1-4346-8aab-ecb3b7ae4f31`.
- Independent actual DX12 offscreen2 first failed with `IncompatibleRead(0)`, exposing the
  physical-usage mismatch. After the owning fix both passed, run
  `e167e50c-f121-4286-8497-c600d4a65721`; hardware fallback was explicitly disabled.
- Focused owner policy/plugin and external UI42 passed, run
  `ede3be36-b448-47ff-ba56-5ddd12f91357`. Initial test compilation found only new-fixture tuple
  arity and SystemState Result mistakes, both corrected without changing production admission.
- Root and independent policy workspaces were formatted after writer freeze. A subsequent
  unused test-only helper deletion remains to be covered by final formatting checks.
- Broad owner336 is currently running as nextest
  `d8b2413d-f9dd-4d0a-b5bb-067dfca8979c`, exec session49202. No final result is claimed here.

# Next Action

Read the terminal broad-owner result, fix actual failures, then qualify affected Root/Game/UI,
Editor, minimal features and selected explicit native workflows. The independent window example
still needs build and real presentation/reconstruction execution. Requalify the independent image
consumer, relevant direct program controls, source/processed and Reference paths. Run appropriate
correctness Clippy and final formatting/diff checks. Only then capture qualification, update the
implementation ledger, commit precisely and locally fast-forward main. Do not push or open a PR.

Complete View/history groups, independently paced streams, self-drawn editable Inspector and
remaining foundation units stay open. Full0121 remains Proposed. Use root target, serial Cargo
and CARGO_BUILD_JOBS=1; do not touch vendor or reference checkouts.

# Citations

- [Previous progress](2026-10-31T173542Z-typed-policy-capture-and-original-installation-custody-replacement-in-progress-30844a32a42b458ba9fb6361183f0673.md)
- [Selected production design](../../../../architecture/render-program-production-design.md#selected-policy-capture-replacement)
- [Complete unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Independent policy](../../../../../tests/fixtures/render-policy-consumer/README.md)

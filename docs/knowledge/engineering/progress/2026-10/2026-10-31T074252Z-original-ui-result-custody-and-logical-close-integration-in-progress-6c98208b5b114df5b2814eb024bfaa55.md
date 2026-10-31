---
type: "Work Progress"
title: "Original UI result custody and logical close integration in progress"
description: "Work Progress for Original UI result custody and logical close integration in progress."
timestamp: 2026-10-31T07:42:52Z
record_id: "6c98208b5b114df5b2814eb024bfaa55"
producer_id: "codex-foundation-main"
run_id: "render-logical-close-2026-10-31"
---

# Summary

Complete foundation Goal remains active, continuing from `5b6d217` on `refactor/foundation`.
Current production WIP prepares the owned result/close boundary required before moving GPU
execution from the World. It does not implement a runner-local Host, immutable output leases or
the full R8/R9a outcome. All changes described below are uncommitted at capture.

# Details

UI backend outcome permissions now retain original pre-admitted batch slots, result capacity
and prebuilt abandonment outcomes. Explicit commits require the nominal owner, slot and full
identity; permit Drop returns Aborted without World access or allocation. A stable receiver is
retained independently by the original queue and Root close participant. Queue transfer refuses
a replaced outcome ingress before consuming a candidate. Root backpressure uses the original
receiver even after ingress replacement; compile admission reads pending/returned under one lock.

Root registers a logical-close participant and configured typed port. Original ledger/planner
claims and exact Input reservation queries fence foreign replacements before authority mutation.
Stop terminalizes original attempts, refuses retry/interaction and waits outstanding permits
before Stopped. Current WGPU shutdown drops buffered permissions into their original sink.

A CPU-only `WgpuRenderObservation` replaces root UI and desktop READY reads of the actual backend.
Exact prepared submission facts move to it without post-submit allocation. The existing rendering
system still publishes within the current frame; post-Host maintenance and actual native ownership
relocation remain the next full consumer change.

Evidence at capture:

- Initial UI-render integration21 passed, run `c65e1c7c-4104-43dd-a535-f21bfd9c0804`; before
  final queue binding/claim regressions. CPU observation5 passed, run
  `058a41b3-d558-40d7-a736-1ec5f29eda87`; final broad cohort will cover current source too.
- Root all-feature lib check passed34.96s before subsequent review corrections.
- First Root/App102:100 passed/two failed, run `fc1872ba-d0b9-425a-b9ac-3925190b979e`.
  Blanket Submitted backpressure pinned an already-drained delayed OneShot acceptance; original
  unsettled transport now blocks while drained old authority may be superseded. A Deferred close
  fixture also assumed actual Task worker joins finish in the same CPU call.
- Targeted2 passed, run `c3d0e83d-73a3-4588-ab3e-5e3af42132bf`. Diagnostic successor
  `75b8302b-0e29-44bd-9e86-a012b1ae1f44` reproduced the second fixture premise: actual evidence
  showed UI Complete with only `nara.tasks.pools` pending. The final helper immediately asserts
  UI completion, then independently drives/yields real Tasks boundedly. Before-Drop Stopping
  assertions remain unchanged.
- Corrected Root/App103 passed, run `433049a4-1ea7-4fe4-b757-ddb84a6ac671`; before the final
  original-receiver getter correction and strengthened normal-drive-after-replacement case.
- **Running:** final all-feature six-package library cohort, exec session3448, temporary log
  `nara-original-ui-result-and-close-owner-cohort.log`. No final result at capture. Covers
  App/Input/UI/UI-render/WGPU/root; it must finish before another Cargo command.

Poincare implemented the CPU projection and is closed. Hume implemented App/Root close plus narrow
Input/UI ownership queries and is closed. Nietzsche's actual source review prompted moving
preprepared facts rather than extending after submit, queue/receiver binding, one-lock compile
admission and original receiver backpressure. Final rereview is pending; all writers are frozen.

# Next Action

Read the current final cohort and review result, fix genuine failures, then run selected native
Editor/ordinary first-present/UI consumers, neutral interaction/presentation controls, relevant
feature/Clippy/fmt. Review the final diff, record evidence, commit/local ff main. Continue complete
Host migration including retained incomplete run custody after this prerequisite. Preserve
vendor/refs, one Cargo/root target, no push/PR, and the complete active Goal.

# Citations

- [Complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Selected ownership design](../../../../architecture/render-host-ownership-design.md)
- [Prior qualified device owner](../../verification/2026-10/2026-10-31T064658Z-coherent-wgpu-device-ownership-and-render-cost-comparison-qualification-18760d4fdea7435b90fb9bd2bee64510.md)

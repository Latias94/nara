---
type: "Work Progress"
title: "Runtime windows share program execution and producer-local world permissions"
description: "Work Progress for Runtime windows share program execution and producer-local world permissions."
timestamp: 2026-10-31T16:36:43Z
record_id: "0e74956d20af46cbad08c17a9da21368"
producer_id: "codex-foundation-main"
run_id: "render-runtime-program-20261101"
---

# Summary

Uncommitted successor of `6385032` on `refactor/foundation`. The complete foundation Goal remains
active. Ordinary Runtime/window execution now uses the same Host-local typed-program transaction
as stock image consumers. Independent review found and corrected producer-scoped interaction,
first-window authority, exchange identity and submitted-resource observation gaps. Qualification
is still running; this record is not a passing production gate.

# Details

- Root DesktopPresentationDriver owns one Host and lazily admitted producer. Original move-only
  results, refusal/retry, post-operation faults, UI/Input settlement, READY, telemetry sealing and
  target/native retirement stay with that owner.
- A final physical-resolution `textureLoad` pass joins the program's existing encoder and
  submission. Its known metadata and custody entry join complete preflight before native work.
  Surface present, output watermark, UI completion and physical prefix settlement stay distinct.
- Runtime world guards now belong to ProducerState and the keyed device-loss registry. A shared
  private revoker slot makes producer Drop revoke picking before returning, without waiting for
  Host maintenance. Runtime commit and Editor Game both use that slot; pixels/prefixes retain
  their separate physical lifetime. Kepler's final read-only review closes the immediate-Drop P2
  in its scoped source-review evidence.
- Missing first targets do not pin a foreign registry. Capture identity comes from the managed
  Runtime owner; driver-port inputs are opaque. UI result facts freeze actual cache plus graph
  resource bytes before submitted resources can release. Pauli's original-three rereview remains
  pending at this capture.
- Valid Runtime packet admission and actual native submission have separate counters. Existing
  CPU Runtime tests now use the production Host. The former Backend Runtime executor is deleted;
  remaining quad/window/Canvas numerical oracles compile only for tests. Backend storage and its
  obsolete public reexport are internalized, without aliases.
- Initial nextest `b28fe5e0-eb10-4d91-bf5f-997c89a8ab1a`: 24 passed, one fixture aborted when it
  attempted capture after replacing World RuntimeGeneration. The kernel had correctly faulted
  and refused that operation. The corrected regression verifies refusal and restores ownership
  before retirement. A subsequent build exposed a removed private UI report still consumed by a
  native adapter test; that report was restored, with no weakened test. No test ran in that failed
  build. Current broad owner/root-driver/product-capability run is exec60496, not yet terminal.
- Same supplied ZIP SHA-256 remains
  `99589eef3287cb9e353501bf1d27d1f4287ec825dbaa45c97abee38033ed2c36`.
  Existing research disposition still covers its ten studies and cross-owner workflows; no new
  roadmap or reference checkout update was introduced.

# Next Action

Finish the current serial owner run, then exact final-cost/producer native tests, ordinary DX12
window retirement modes, actual UI/telemetry product and affected Editor trajectories. Check
minimal/public consumers, compile-fail identity input, correctness Clippy and formatting. Record
actual results and limits before committing and locally fast-forwarding main; no push/PR. Then
continue typed policy capture installation, remove central payload collection with real consumers,
and complete policy/history/multi-stream/custom composition. Do not promote full Proposed0121 or
mark the complete foundation Goal finished.

# Citations

- `docs/plans/2026-10-31-render-composition-and-host-execution-plan.md`
- `docs/architecture/render-program-production-design.md`
- `docs/research/2026-10-30-engine-direction-reconciliation.md`
- Parent source session `01a1629b-6c28-7203-87c5-85328214df4a`; Pauli
  `01a192b3-4bd8-73e1-bdea-477cf1d091dc`, Kepler `01a192b3-826e-7261-97e8-55b322c0939e`;
  no Cargo/fmt by workers or reviewers.

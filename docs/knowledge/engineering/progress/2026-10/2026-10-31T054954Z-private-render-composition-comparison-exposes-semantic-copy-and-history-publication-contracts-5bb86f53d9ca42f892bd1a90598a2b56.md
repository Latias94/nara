---
type: "Work Progress"
title: "Private render composition comparison exposes semantic copy and history publication contracts"
description: "Work Progress for Private render composition comparison exposes semantic copy and history publication contracts."
timestamp: 2026-10-31T05:49:54Z
record_id: "5bb86f53d9ca42f892bd1a90598a2b56"
producer_id: "codex-foundation-main"
run_id: "render-composition-2026-10-31"
---

# Summary

The full foundation Goal remains active. Cooked delivery is committed/qualified at `e2cb5c1` and
`main = refactor/foundation = a0995e7` at entry. The next R8/R9a unit now has a complete owner plan
and private executable comparison under `tests/fixtures/render-composition`. No production render
API, Runtime/Editor Host integration or Accepted render contract has been replaced at this point.

# Details

- Source mapping confirmed real production gaps: adapter initialization requires a surface,
  Runtime GPU ownership is an ECS resource, Editor output targets overwrite their old pixels,
  and final whole-queue drain cannot serve independent stream retirement. The unit plan records
  the exact incumbent owners and required replacement, rather than treating test-only offscreen
  device creation as production Ready/epoch/loss/close support.
- Private `graph.rs` uses candidate-owned versions, whole single-sample 2D textures and buffers,
  explicit initialization, exactly one producer, deterministic order-independent topological
  compilation and bounded counts/modeled bytes. Native execution allocates distinct versions,
  initializes/copies before kernels, inspects native error scopes and retains exact resources
  through a read-only accepted-prefix completion observation. ReadyGraph and actual submission
  are now distinct. Imported meaning/initialization and device custody remain trusted comparison
  assertions, not a production output lease or sandbox.
- Graph, viable ordered/slot controls and an actual ECS schedule use the same native blur,
  complement, crisp HUD, four-bin luminance, preserved normalized successor and vertex histogram
  kernels. An independent CPU oracle checks linear premultiplied values and actual raw counts.
  The ECS control uses explicit `SingleThreadedExecutor` through the complete Bevy substrate;
  its native capture/encoder are private comparison resources, not gameplay World queries.
- Independent review exposed semantic-copy conflation: initialization may copy compatible Scene
  bytes into a new FinalComposite semantic, while reads must match their exact semantic ports.
  Buffers likewise distinguish16-byte integer counts from16-byte float frequencies. These fixes
  are in the private graph and Proposed0121, together with Store postconditions, separate native
  group failure domains, submission-versus-output publication and empty-history reset revisions.
- History publication is being strengthened: fixed complete groups, checked predecessor access,
  retained cleanup after all pointer changes, explicit reset revision and same-capture deduplication.
  The original generic private History<T> tests pass but do not prove native epoch/publication.
  Mendel owns its next correction; do not treat the transitional source as final.
- Ohm owns `graph_gpu.rs` evidence corrections: distinct slot/capture pixels and pointer assertions,
  local texture/buffer plus imported Weak custody, later canary rejection/panic and actual encoder
  validation failure, ready-without-submit. Earlier passing tests were weaker on these points.
  Parent owns docs, fixture wiring, oracle/controls/workload, `gpu.rs` and `native.rs` integration.

Actual evidence before the ongoing history/test corrections:

| Check | Result |
| --- | --- |
| First window-free harness |1 passed, `efc78b08-c8ac-49e9-9b2c-4c505c42dd44`, actual NVIDIA RTX4090/DX12,1.204s run. No surface/window. |
| Initial graph model |46 passed `72da189a-b578-4d0a-b234-7d784a225d2e`; shared workload47 passed `f80a1ad2-49e6-45d2-a9fe-cc1545f58628`. Later semantic corrections supersede this source scope. |
| Revised private CPU model |107 passed `92effd52-a503-45b6-a3d8-da1b65da18c1`,6.187s; included duplicated graph/history modules across test binaries. Graph tests are now wired only to the CPU model binary; final unique counts must be recorded after history source freezes. |
| First revised native suite |5 passed `558c7945-296d-4b0a-b31b-dd224fd24f4f`,5.029s, RTX4090/DX12. Includes graph images/buffers, controls, history and original fault/custody cases. Review then required stronger history/fault observations; this is not their final evidence. |
| All three viable controls |1 passed `853b7f15-d7d6-4767-9a9d-6489fe3f01b9`,1.143s, actual ordered/slot/ECS-schedule kernels and CPU numeric comparisons. |
| Native import admission |1 passed `3899fccb-cf88-44af-869c-b1c1ac73b8d1`,1.110s, actual wrong-shape/usage and foreign-candidate inputs reject before encoding. |

Initial exact-wgpu API errors were corrected from installed primary source: InstanceDescriptor
requires its no-display constructor and owned Instance::new input; vertex layouts are Option
entries; Bevy0.20 schedules use `set_executor(SingleThreadedExecutor::new())`, not ExecutorKind.
These were compile errors in the disposable experiment, not production rendering regressions.

# Next Action

Finish the fixed-group/cleanup history owner and stronger native fault/custody tests, then run
final CPU and real GPU cases serially, inspect diffs before fmt, independently review and record
the exact final source. Measurement128 warmups/1000 samples remains required before interface
freeze and has not run. Continue independently paced output streams, actual recovery-input and
epoch custody, Nara-drawn composite/picking/UI completion, selected production typed Features and
Host integration. Do not mark R8/0121 complete, change production closed-topology tests or create
compatibility aliases from a private comparison. Preserve full Goal active, root target/one Cargo,
vendor/refs and concurrent changes; no remote publication.

# Citations

- [Complete owner plan](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Foundation R8/R9a](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r8-external-render-feature-and-host-local-execution)
- [Proposed0121](../../../../architecture/adr/0121-composable-render-programs-and-host-execution.md)
- [Private comparison](../../../../../tests/fixtures/render-composition/README.md)

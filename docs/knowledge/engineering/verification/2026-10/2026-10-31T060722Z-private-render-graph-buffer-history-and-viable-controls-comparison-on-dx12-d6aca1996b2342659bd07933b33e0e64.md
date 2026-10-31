---
type: "Verification Evidence"
title: "Private render graph buffer history and viable controls comparison on DX12"
description: "Verification Evidence for Private render graph buffer history and viable controls comparison on DX12."
timestamp: 2026-10-31T06:07:22Z
record_id: "d6aca1996b2342659bd07933b33e0e64"
producer_id: "codex-foundation-main"
run_id: "render-composition-2026-10-31"
---

# Verification

Private executable comparison for foundation R8/R9a, starting from `a0995e7` on
`refactor/foundation`, Windows11, Rust1.95.0, NVIDIA GeForce RTX4090/DX12. All Cargo work was
serial, one job, existing root target. The comparison exports no engine API and changes no
production render implementation or Accepted render contract. Proposed0121 remains Proposed.

The final21 Rust/WGSL files under `tests/fixtures/render-composition`, sorted by portable path
and hashed as `path/NUL/bytes/NUL`, have SHA-256
`5974fd12508e92ff32ef04a3e116403416b92f24b31a00e99848ed9d9715ea6c`.

# Result

The private candidate now executes real shared/branching texture and buffer work, not only a
graph parser. The same kernels run through viable direct-order, two-pass slot and actual serial
ECS-schedule controls. They retain complete physical versions and agree with independent CPU
equations for3-tap separable blur, premultiplied complement, crisp HUD, four-bin luminance,
preserved raw/normalized buffers and a Float32 vertex-input histogram. All use64-by64 linear
premultiplied RGBA32Float, explicit initialization and exact numeric oracles. No performance
winner or production interface is selected from these results.

The graph validates candidate ownership, unique producers/operation keys, exact input meaning,
native accesses, initialization-copy compatibility, cycles and bounded model costs. Resource
reservation and operation declaration order are independent. Three declaration permutations
produce the same execution order and pixels. Each local output has distinct native storage;
an older original/composite and raw statistics survive a newer capture.

Ready preparation, actual submission, output/history pointer publication and accepted-prefix
completion are separate operations. Native error scopes reject invalid encoding before submit.
Private fixed publication groups prevent partial slot commits; revisions/reset stamps distinguish
empty-history generations, and per-slot capture watermarks survive reset/regrouping. Same-capture
siblings, stale predecessors, retired groups and reincarnated slots reject. Old values move into
explicit `RetiredValues` only after the complete mutation, so user destructor panic cannot split
the committed group. This owner is private, bounded and caller-driven; it proves no GPU epoch.

# Evidence

| Check | Actual result |
| --- | --- |
| Final CPU model/history tests | **61 passed**,8 native cases are now ignored by default (7 at that run); run `e71a0cde-f886-47cd-9e52-48c0e62f5900`, build23.14s/run1.636s. Includes48 graph cases,12 history cases and1 full-workload declaration comparison; duplicated module tests were removed from GPU binaries. |
| Revised real GPU suite | **7 passed**, run `df8f6bba-894d-4daa-9c0b-e59b2a08d26a`, run6.809s. All explicitly required the actual adapter; none silently skipped. |
| Post-Ready submission failure successor | **1 passed**, run `cd633c3b-6943-4907-b445-ef75b8fb0b95`, build4.41s/run0.994s. An exact imported buffer is deliberately destroyed after Ready; actual queue validation refuses submission, then new work succeeds on the same device. This is not evidence that a failed attempt had zero physical uses. |
| Correctness Clippy | Independent comparison all-targets passed in46.93s; subsequent native-admission-only check passed in1.07s after its added failure case. Command uses `-D clippy::correctness`, not `-D warnings`; shared test modules and large private error warnings remain. |
| Formatting | Explicit fixture `cargo +1.95.0 fmt --all --check` passed before the final test addition; a final fixture formatter ran after that addition. No unrelated root source was formatted or changed. |

The seven-case native group covers:

- Actual offscreen initialization and float readback without any surface/window.
- The same image/buffer numeric workload under ordered, slot and real ECS-schedule controls.
- Graph declaration permutations, original pixels, raw counts and normalized vertex histogram.
- Exact physical history inputs across two Views/two Feature instances, distinguishable slot/
  capture pixels, reordered groups, late old publication, reset-before-submit Ready cancellation
  and complete successor publication. Pixel and `Arc::ptr_eq` assertions detect wrong association.
- Later callback rejection/panic and actual invalid-encoder validation after a canary copy has
  already been encoded. A successful control changes the canary; failed candidates leave it
  unchanged. Ready creation alone does not change it. Independently healthy work then progresses.
- Returned submission/drop: real kernels use the imported and locally allocated resources.
  `observed_pending=true` was actually printed in this run; imported texture, local composite
  texture and local buffer Weak references remained alive during pending and released after the
  exact original prefix. These are wrapper/custody observations, not driver-heap deallocation.
- Actual native shape/usage and foreign-candidate import rejection before encoding.

Independent reviews and dispositions:

- Tesla mapped current production ownership: surface-dependent initialization, Runtime ECS GPU
  resource, overwriteable Editor viewport texture, exact target receipts and whole-queue drain.
  The unit plan records required production integration rather than upgrading test helpers to Ready.
- Mendel challenged semantic-copy compatibility, raw/frequency buffer meaning, native group failure,
  empty-history reset ABA, post-operation Store, physical outputs and recovery input separation.
  Private model fixes and Proposed0121 clarifications landed; the latter production gaps remain.
- Volta implemented actual kernels and reviewed the corrected graph; no remaining material model
  findings were reported. Ohm implemented the graph/native comparison and independently reviewed
  fixed groups/cleanup, ordinary errors and real destructor-panic tests without remaining findings.
- Ohm exposed weak first-generation native oracles (same slot pixels, first-callback-only fault,
  imported-only Weak). The final native suite above executes strengthened assertions. Mendel's
  final source review found no further material blocker for this private probe and explicitly
  retained its production/epoch/physical-memory limits. The later added actual submit-failure case
  has direct execution evidence; no new production code changed after that review.

# Follow-up

This checkpoint is a private comparison prerequisite, not completed R8, a public Feature API,
production window-free GPU Host, multi-window presentation or a Nara-drawn Editor. It proves
whole-resource single-sample64-by64 scope only. Imported semantic/initialization/immutability and
device identity remain trusted adapter assertions; foreign graph handles do not prove cross-device
or epoch authentication. Raw Device access is trusted Rust, not a sandbox. Same-queue callback
registration is serialized in this fixture; no cross-thread submission guarantee follows.

The private history owner commits logical output sets, not atomic execution of several GPU
submissions. Complete native groups, current Host epochs and exact output/slot binding still belong
to the selected caller. Host/device loss, supported capabilities, actual independently paced
streams and producer-local retirement, retained reconstruction inputs, Runtime/Scene View/Editor
integration, designated UI completion/passive mirrors and two-window/image native consumers
remain in the complete unit plan. Shared attachments must retain Store postconditions; native
callbacks cannot be statically proved to obey arbitrary shader/access declarations.

Measurement entry remains128 warmups/1000 samples with setup/readback/wait separated. No cost
comparison, speedup, memory reduction or interface freeze has occurred. Shared physical imports
and modeled resource bytes are not a measured GPU retained-memory budget. Continue the actual
production replacement and remove displaced central stock paths only after the complete consumer
selects and qualifies it. The full foundation Goal remains active.

# Citations

- [Complete unit plan](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Proposed0121](../../../../architecture/adr/0121-composable-render-programs-and-host-execution.md)
- [Comparison workspace](../../../../../tests/fixtures/render-composition/README.md)
- [Initial integration checkpoint](../../progress/2026-10/2026-10-31T054954Z-private-render-composition-comparison-exposes-semantic-copy-and-history-publication-contracts-5bb86f53d9ca42f892bd1a90598a2b56.md)

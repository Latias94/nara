---
type: Verification Evidence
title: Foundation U2 and U3 recovery and device admission
description: Superseded Image generations retire logically and textures reject unsupported live-device extents.
timestamp: 2026-10-19T06:47:27Z
record_id: c4414057815b42c689d43e67392a5c80
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Scope And Result

U2 landed in `8b3e26b`; U3 landed in `ebbe005`.
These correct B12 and B13, respectively.
The broader plan remains active; these results do not close U1, U4-U15 or any triggered follow-up.

Image owns one publication-eligible generation per asset stream.
Superseded handles and attempt records retire before Poll/Spawn can retain successors behind an obsolete running task.
Same-generation order, fixed Poll cutoff, source/version guards and exact decoded-byte charges remain intact.
TaskPools retains physical task/capture/close ownership; logical retirement is not forced I/O cancellation.

The concrete WGPU texture owner checks the live requested device extent before Image/Glyph reuse or allocation.
Unsupported content returns typed rejection; expected creation OOM uses the existing upload-deferral route.
CPU import limits remain device-independent.
Failed admission does not publish or charge a GPU cache entry, and reconstruction re-admits against new capabilities.

# Verification

Root-owned serial Cargo, existing target, Rust 1.95.0, Windows/DX12.
No worker ran Cargo or committed shared-checkout changes.

| Evidence | Result | Nextest run |
| --- | --- | --- |
| U2 two-worker/barrier regression before implementation | Failed as expected: desired B remained unpublished until obsolete A released | `fe0b90fe-0768-409f-a43e-980824802841` |
| Image full library after implementation | 75 passed | `3e374911-24fa-4051-a9d0-0bb09a04f707` |
| U3 auxiliary extent regression before implementation | Failed as expected: create_texture validation panic at 8,193 versus device limit 8,192 | `9ab00ea9-ad29-4cb4-b4dd-3b3085c2fb74` |
| U3 primary/auxiliary limits, budget preservation and smaller replacement-device regressions | 4 passed | `9701a020-0825-44bd-b286-46820401f79e` |
| WGPU sprite-submitter library | 82 passed | `a413d7d7-a302-4100-b2f2-07ad8defda81` |
| WGPU all-features library, adapter required | 115 passed; 2 ignored native qualification tests skipped | `6eeb955e-30bf-4b05-9a5b-ec9b7f48759f` |

Image pressure coverage holds A physically running across 32 failed and 64 coalesced successors, checks bounded logical records and charges, then proves incomplete close followed by physical completion and release.
GPU coverage admits 8,192-by-1, rejects 8,193/16,384 horizontal and vertical excess without charging, recovers a legal frame, and rejects a retained 4,096-wide snapshot after replacement with a 2,048-limit device.
Tests use real WGPU devices and readback; actual allocation OOM was not injected.
These overlapping totals must not be summed as unique tests.

Focused command forms were `cargo +1.95.0 nextest run -p nara_image --lib --locked --no-fail-fast --test-threads=1` and the corresponding `nara_render_wgpu --all-features --lib` selection with `WGPU_BACKEND=dx12`, `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.
An independent correctness reviewer traced Image task lifetime and all changed texture paths and found no actionable defect.
The review was static and explicitly did not qualify real OOM behavior or concurrent U1 edits.
`cargo +1.95.0 fmt --all` and `git diff --check` passed at integration.
Existing unrelated dead-code warnings remain.

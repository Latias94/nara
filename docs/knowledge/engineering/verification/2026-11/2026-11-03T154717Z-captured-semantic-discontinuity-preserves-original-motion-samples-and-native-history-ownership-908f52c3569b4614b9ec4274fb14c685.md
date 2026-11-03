---
type: "Verification Evidence"
title: "Captured semantic discontinuity preserves original motion samples and native history ownership"
description: "Original lineage revision fences late captures; real borrowed motion, captured shader clocks, selective two-View history and original submission retirement pass on DX12."
timestamp: 2026-11-03T15:47:17Z
record_id: "908f52c3569b4614b9ec4274fb14c685"
producer_id: "codex-foundation-main"
related_plan: "docs/plans/2026-11-03-joint-motion-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "6157402"
---

# Verification

Windows 11, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, root `target`, serial Cargo invocations.
`git_commit` names the integrated base; this record covers the semantic-discontinuity successor.
Native selection is `WGPU_BACKEND=dx12`, `NARA_WGPU_FORCE_FALLBACK=0`; the fixture rejects CPU
adapters and acquisition failure rather than passing by a skip.

| Gate | Result |
| --- | --- |
| Focused original-capture/adaptation CPU gate | 3/3, nextest `0d7cff6b-5e73-4f5c-a8fb-5155db500562` |
| Default renderer CPU cohort before callback-cut regression | 248/248, 42 ignored native tests outside that gate, `1a5e0819-77c6-4a0b-b854-b3cdab314556` |
| Final all-feature renderer CPU cohort, including callback cuts | 388/388, 52 ignored native tests outside that gate, `cf943170-fc69-43c3-b666-8c2e2a64a1c8` |
| Actual joint motion/history native regression | 1/1, `93809960-b0fd-47af-a981-6e34047c30b0` |
| Final complete independent native history cohort, with direct exact-Step fixed observation | 7/7, `70ff18c9-9fd6-4ca1-b7e9-f2c129438c92` |
| Correctness Clippy | Root renderer all features/library/tests and independent history target pass; unrelated existing warnings remain |
| Reused independent motion consumer after cross-workspace dependency links | Feature-unified 12/12, `40e721cb-1685-4519-94ae-76a057f34dcd` |
| Formatting | Root and independent workspaces pass `cargo fmt --all` |

## Result

The CPU history lineage now owns one shared checked semantic revision, while each clone freezes
its revision with the domain sample. A cut advances only the current owner. Old raw captures,
accepted inputs and prepared candidates cannot refresh through admission, first preparation,
image-target adaptation or a replacement Host. Successful deduplication and the lineage identity
remain intact; ordinary per-slot reset retains its separate meaning.

The independent native consumer reuses the separately authored animation and actual borrowed
Rapier solver. It freezes the actual Presented stamp and poses with virtual shader time, real
time, interpolation and pause state. The existing accumulation shader uses quantized original
virtual time; the CPU oracle checks every float history pixel and the composed RGBA8 image.
Zero/multiple ticks, delayed execution, Pause, kinematic correction plus KeepLocal reparent,
exact Step and another presentation at the same fixed tick all execute through the managed owner.

Both pre-cut first-preparation and already-prepared inputs refuse. Only the first View's histories
reset; the other View and an independent lineage continue their original predecessors. Old output
reads remain exact and the original submission prefix remains observable. Producer/readback/Host
retirement returns resource and candidate metadata accounting to zero.

Independent correctness review found no confirmed P1/P2 in the production revision checks or
submission custody path. Testing review requested direct exact-Step evidence. That operation
intentionally runs only FixedUpdate and private cleanup, so it cannot return a new Presented
sample. The fixture now observes the exact operation's fixed stamp/poses, asserts no presentation
update during it, and separately checks the next paused presentation against those exact poses.
It does not change the managed driver's exact-Step contract to render in the same operation.
The reviewer withdrew the original same-operation presentation premise and closed the finding
after inspecting those direct fixed and subsequent Presented assertions. No P1/P2 remains in the
two reviewed scopes; reviewers did not independently rerun the reported tests.

The first native integration attempt failed because the second precomputed hierarchy `from`
anchor became stale after moving its preceding sibling. The fixture now resolves each exact
current placement before the corresponding move. The corrected case and complete cohort pass;
the production hierarchy writer was not weakened.

## Limits and next action

No native-window presentation, camera-cut/material-generation classification, performance,
general rollback or production parallel-profile claim is added. A cut after final revision
validation may precede publication; domains needing strict cross-thread order serialize complete
Host operations. Already submitted native work remains under its original physical custody.
The mid-native-attempt cutoff cleanup path was traced in independent review rather than injected
by this native fixture. The original capture/time binding is qualified here, not arbitrary shader
motion reconstruction or pixels for a drawn Sprite scene.

Next, select and exercise the bounded prediction/effect admission and explicit recoverable-state
model with the concrete motion/audio consumer. R4c freeze and full R10/foundation completion remain
open. Do not replace this gate with editor source-history transactions or fresh-process restore.

## Sources

- [Owning contract](../../../../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md)
- [Current guide](../../../../guides/render-programs.md)
- [Joint unit](../../../../plans/2026-11-03-joint-motion-execution-plan.md)
- [Independent consumer](../../../../../tests/fixtures/render-policy-consumer/README.md)

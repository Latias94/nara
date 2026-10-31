---
type: "Work Progress"
title: "Ordinary stock image consumers through typed programs integration"
description: "Work Progress for Ordinary stock image consumers through typed programs integration."
timestamp: 2026-10-31T15:29:37Z
record_id: "158cff8e7dbd4b6fb373f8908539f5af"
producer_id: "codex-foundation-main"
run_id: "render-stock-policy-20261031"
---

# Summary

Continue the complete active foundation Goal from `fbed94f` on `refactor/foundation`.
Shared stock preparation is committed and locally fast-forwarded to main. The next
uncommitted checkpoint migrates actual stock image consumers through typed programs;
ordinary window capture/policy, history and the complete render outcome remain open.

# Details

Two independent source mappings selected a typed policy capture/build seam and one
native final-target transaction. The selected migration shape is recorded in the
production design. A Sprite writer now retains all ordered world Views, flattened
exact batches and prepared-buffer ranges; a later View Clear still clears the whole
attachment. A private typed WorldCanvas composite shares the incumbent shader and
mapping implementation while consuming declared graph resources.

The actual Scene/Canvas/Game and independent image paths now build a stock program
and call the same private `execute_program_in_namespace` as public programs. The
duplicate image preparation/encoding/submission block is removed. Only previously
admitted initial target storage is privately bound as a local final version;
successors allocate normally after complete preflight. Image output and graph storage
share one actual allocation guard. UI joins each original permit to one exact
packet/logical-target fact before native work, without zipping replay packets.

Initial focused native run `748f741f-5501-4ea6-9fe5-e4fca2e46525` executed86:
84 passed and2 failed. Both failures exercised assumptions from the displaced path:
the target-only allocation fault hook no longer reached successor allocation, and
shared program budget refusal now has the structured Program ResourceCapacity reason.
The assertions retain exact accounting/retry checks. The fault oracle now exercises
the actual shared program texture-view allocation helper, which checks OOM before
passing a possibly invalid handle onward.

Known Canvas native custody/metadata now joins complete preflight before allocations,
including no-Sprite/UI builds. An independent Canvas review found a P2: accepting an
R8 destination would discard color/alpha while claiming to preserve them. Destination
scope now explicitly admits supported RGBA8/BGRA8 linear/sRGB formats; a native zero-
budget/pre-allocation rejection test was added. This correction postdates the start
of the broad owning run and needs its final focused evidence.

Writers `01a19286-5fd4-7432-b082-f01d4092fd23` and
`01a1928b-529a-7621-b57e-9cce99272766` are frozen and closed. Read-only reviews for
image/native integration, stock policy and Canvas are in progress. No user/vendor
changes were reverted; no remote publication occurred.

# Next Action

Complete the active serial owning run, then final focused Canvas/affected checks,
no-default portability, independent program/output consumers, selected actual native
Editor image/Game/Canvas trajectories and correctness Clippy. Resolve review findings
before a precise implementation commit and local main fast-forward. Keep the overall
Goal active. Stock policy currently consumes the transitional central payload;
replace actual Runtime/window capture and policy installation next before deleting
the old payload and facade family. Do not claim alternate policy or history completion.

# Citations

- [Production integration design](../../../../architecture/render-program-production-design.md)
- [Complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Previous preparation qualification](../../verification/2026-10/2026-10-31T144610Z-shared-stock-native-preparation-and-whole-candidate-preflight-qualification-01b79b28279b48618dd19ec377ad2cc8.md)
- [Imported research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)

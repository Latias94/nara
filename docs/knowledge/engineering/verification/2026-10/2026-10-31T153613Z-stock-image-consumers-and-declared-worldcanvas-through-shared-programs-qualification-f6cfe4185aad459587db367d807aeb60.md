---
type: "Verification Evidence"
title: "Stock image consumers and declared WorldCanvas through shared programs qualification"
description: "Verification Evidence for Stock image consumers and declared WorldCanvas through shared programs qualification."
timestamp: 2026-10-31T15:36:13Z
record_id: "f6cfe4185aad459587db367d807aeb60"
producer_id: "codex-foundation-main"
run_id: "render-stock-policy-20261031"
---

# Verification

Receiving baseline: `fbed94f` (the shared-preparation handoff), branch
`refactor/foundation`. This record accompanies the stock image implementation commit.
Windows11, Rust1.95.0, locked WGPU30.0.1, `CARGO_BUILD_JOBS=1`, serial Cargo/nextest
and the existing root `target`. Native tests select DX12; independent consumers use
the production adapter with `NARA_WGPU_FORCE_FALLBACK=0`. No new performance or
physical-residency measurement was made.

# Result

Actual Editor Scene, authoring UI Canvas, embedded Game and independent image output
now execute typed stock programs through the same private native transaction as
public texture/buffer programs. The duplicate image preparation/encoding/submission
block and per-image mutable WorldCanvas cache are removed. The existing stock image
facades and central capture payload still bridge their callers; ordinary Runtime/
window capture and policy installation are the following replacement, not completed
by this checkpoint.

One Sprite writer owns all admitted ordered world Views, flattened exact batches
and prepared-buffer ranges. Whole-attachment Clear/Load semantics, empty batches
and logical target/order validation survive the change. With no Sprite capture,
stock image construction uses initialization only; compiling Sprite support does
not activate that Feature. UI uses its physical-resolution viewport once and an
explicitly preserved FinalComposite. The bridge validates each actual permit against
one exact packet/logical-target fact before native work; replay packets are not
zipped with move-only permits.

Mapped Game packets retain WorldCanvas facts. Their real production path now writes
a graph-owned sRGB world raster, samples it through the typed Canvas composite into
physical-resolution Scene color, and then draws full-resolution UI. The old and new
Canvas paths share one shader, pipeline/layout helper and mapping/scissor math.
The typed adapter allocates no hidden world texture. Mapping storage keeps its real
shared byte guard; its two known custody entries and engine metadata join complete
preflight even without Sprite/UI features. Supported destinations are RGBA8/BGRA8,
linear or sRGB, with matching alpha interpretation. Single-channel destinations
refuse before native allocation rather than discard color/alpha.

Previously admitted initial target storage can privately realize the graph's local
final version. It is still subject to graph ownership, selected-output, descriptor,
Host/epoch, producer/source/frame and unique-writer/initialization checks; it is not
an initialized import. Graph output and image receipt share one actual allocation
guard. Successor storage allocates normally after complete preflight. Pure preflight/
capacity refusal preserves the exact initial texture/view/guard; entering native work
transfers that attempt to original-prefix custody and consumes the initial slot,
including failure, so a destroyed failed output is never reused for retry.

Shared graph allocation checks OOM before consuming a possibly invalid texture/view
or buffer. The existing native failure oracle now targets this actual allocation
path. After target activation, logical resource refusal reports Program ResourceCapacity;
inactive target reservation still reports OutputCapacity. Independent consumer tests
use those exact reasons while retaining exact-byte and same-capture retry assertions.
No compatibility aliases or second native execution loop were added.

Independent read-only reviews challenged ordered Views, implicit activation, Canvas
color/custody and actual target ownership. The Canvas format P2 and initial-storage
preflight P2 were repaired and rereviewed. Reviewer
`01a1927e-4ebc-7d11-b188-e0933e4a58e8` reports no remaining P1/P2 in the bounded
image migration; `01a19290-1ec8-7172-ac77-400290abd168` closes the Canvas format
finding. The intermediate WorldCanvas rejection is replaced by the production
composite above and independently reread before native qualification.

# Evidence

| Executed check | Actual result and scope |
| --- | --- |
| Broad owning WGPU library, including native tests |312 passed /1 deliberately excluded, run `f8c142d9-39c4-42bb-916d-0fc35c1ac07b`,148.303s; before the final destination, mapped-Game Canvas and initial-storage corrections |
| Final affected Host/program/Sprite/Canvas/stock cohort |88 passed /226 skipped, run `22d96a6a-8798-4efb-abbd-08c38f321035`,52.591s; includes the final actual image path, original-allocation retry regression, bound-local provenance and native Canvas numeric controls |
| Final no-default owner/native subset |14 passed /177 skipped, run `ca2e34da-c1a1-4448-afae-d47598002b82`,5.546s; Canvas preflight/cost/pixels, local binding and exact output-budget/allocation retry without Sprite/UI support |
| Actual selected native Editor trajectories |8 passed /295 skipped, run `16593c61-80a5-4112-b98d-4dc863d15d49`,20.603s; compiled after mapped-Game Canvas integration and before the final initial-storage refusal correction |
| Final independent program workspace |8 passed /0 skipped across3 binaries, run `9ade27c9-9a8f-40cc-904f-c0cd2917c134`,4.030s |
| Final independent image-output workspace |3 passed /0 skipped across2 binaries, run `949c6615-26b1-4fd5-842e-8ae22dcd7d1c`,2.921s |
| Final correctness Clippy |`nara_render_wgpu` and `nara-editor`, all-features/all-targets, `-D clippy::correctness`: passed46.39s; existing non-correctness/transition warnings remain |
| Final wasm32 no-default check |passed2.46s; compile-only evidence, not browser runtime |

The eight hidden-window Editor trajectories cover paused redraw resume; Game reopen,
refocus, resize and Step; detached/minimized View recovery and retirement; Scene
reimport pixels; fixed-world Game UI/Input coherence; real Canvas pick/move/resize/
Save/Reopen; retained Scene/Prefab navigation and Close; and Canvas priority during
unrelated Close. They do not imply scanout or a completed Nara-drawn editable toolkit.
Final affected88 separately covers the later initial-storage refusal/retention change;
this record does not claim a repeated Editor8 run after it.

The Canvas native test exercises integer enlargement, fractional reduction and empty
coverage across sRGB RGBA/BGRA and linear RGBA. It checks every destination pixel
against the captured mapping and independent color oracle, with quantization tolerance1,
transparent controls and an exact payload budget. The ordered-View native case checks
split affine geometry, a later full clear and a clear-only transaction with one graph
resource. The new target regression compares actual texture/view and guard identity,
zero upload/watermark change, retained bytes and successful same-input retry.

Initial focused run `748f741f-5501-4ea6-9fe5-e4fca2e46525` was84/86; old-path fault
injection and error expectations were migrated rather than weakening their accounting
assertions. The first public image run was2/3 for the same structured-error transition;
the final public3 above covers the corrected real consumer. Intermediate87 passed
before the final Game/initial-storage changes and is superseded by final88.

All nextest commands use locked dependencies, one test thread and no fail-fast.
Native owner runs use `--run-ignored all`; Editor uses `--run-ignored only` with the
eight explicit names. The broad WGPU filter excludes only
`tilemap_and_changing_hud_costs`, its existing separate release measurement.
Independent manifests use `--offline` and the root target. Formatting, whitespace,
current document links and memory validation are checked during final integration.
TEMP logs are not committed.

# Follow-up

Keep the complete foundation Goal active. Continue ordinary Runtime/window execution
and exact policy capture/build installation through this same Host, then remove the
central WgpuFramePayload/CollectSprite/CollectUi/quad-only prepared path and combined
facades with their real consumers. The selected policy shape is documented but is
not a newly qualified public policy interface. Qualify an independent alternate policy
through actual capture selection and final-target adaptation.

View/instance/history publication groups, full source/processed interleaving,
independently paced two-window/image streams, remaining loss/withdrawal semantics and
clipped Nara-drawn composition retain the complete render unit's acceptance. Full
ADR0121 remains Proposed. Legacy Runtime/window WorldCanvas cache and toolkit storage
retain separate accounting until their owning transition; pipeline/driver overhead
and physical residency are not part of the represented payload budget. No Linux/macOS,
browser runtime or complete custom Editor claim follows from this checkpoint.

# Citations

- [Production integration design](../../../../architecture/render-program-production-design.md)
- [Complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Accepted program/target ownership](../../../../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs)
- [Program guide](../../../../guides/render-programs.md)
- [Image-output guide](../../../../guides/render-image-outputs.md)
- [Independent image consumer](../../../../../tests/fixtures/render-output-consumer/README.md)
- [Integration progress](../../progress/2026-10/2026-10-31T152937Z-ordinary-stock-image-consumers-through-typed-programs-integration-158cff8e7dbd4b6fb373f8908539f5af.md)
- [Previous preparation qualification](2026-10-31T144610Z-shared-stock-native-preparation-and-whole-candidate-preflight-qualification-01b79b28279b48618dd19ec377ad2cc8.md)

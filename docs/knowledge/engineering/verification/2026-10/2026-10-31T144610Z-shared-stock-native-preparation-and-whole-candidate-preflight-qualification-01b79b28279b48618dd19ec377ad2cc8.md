---
type: "Verification Evidence"
title: "Shared stock native preparation and whole-candidate preflight qualification"
description: "Verification Evidence for Shared stock native preparation and whole-candidate preflight qualification."
timestamp: 2026-10-31T14:46:10Z
record_id: "01b79b28279b48618dd19ec377ad2cc8"
producer_id: "codex-foundation-main"
run_id: "render-stock-preparation-20261031"
---

# Verification

Receiving baseline: `36b133c8b2b4e06b3ec976c66498a08526cc469a`, branch
`refactor/foundation`. Windows11, Rust1.95.0, locked WGPU30.0.1, serial Cargo and
nextest with `CARGO_BUILD_JOBS=1`. All workspaces reuse the root `target`. Actual
native checks use Windows/DX12; independent consumers select the production adapter
without a dummy window or private fixture executor. No new performance measurement
was made.

# Result

Every original captured Feature preflights before local native allocation, native
state creation or upload. First-party Sprite/UI adapters register their exact
lowered batches in one candidate-local Image collector. The same borrowed cost
calculation admits automatic custody entries and modeled engine metadata before
native work in both typed programs and the existing stock Runtime/Editor path.

One private `NativePreparation` now carries the original Device/Queue pair,
submission reservation and shared logical payload budget. Stock pipelines,
Image/glyph/fallback caches and instance limits have one shared native owner.
Actual Image, glyph, fallback, instance and material storage retains its byte guard;
cache hits and submitted uses retain the original allocation. Per-Image completion
callbacks and the duplicate mapped-buffer wrapper are removed. The public names
`with_resource_byte_limit` and `retained_resource_bytes` replace output-only names
without compatibility aliases.

Cache age uses the Host maintenance frame independently of producer capture rate.
Failed preparation discards only instance states it actually touched; pure
preflight or early allocation refusal preserves existing caches. Instance guards
commit only after complete encoding and immediately before native submission.
Structured stock Backend errors preserve deferred/content/device classification
through program and Host error handling.

Independent review found two P2 admission gaps: incomplete stock automatic
custody/metadata reservation and prefix-observer growth after native work. Both
were fixed in their owning code and rereviewed. The final prepared stock draw and
clear-only image path also reserve their final custody slot before preparation.
Reviewer `01a1925a-a45f-7181-b26d-2a6d25f0ef0d` reported no remaining concrete P1/P2
within this preparation prerequisite.

Typed stock adapters execute in production-program native tests. Ordinary
Runtime/Editor capture and policy still use the central stock payload path. This
record qualifies shared preparation and preflight, not that subsequent migration,
full ADR0121, the complete render unit or the complete foundation Goal.

# Evidence

| Executed check | Actual result and scope |
| --- | --- |
| Broad owning WGPU library, including native tests |305 passed /1 deliberately excluded, run `852d0456-d72f-4067-805a-47a87fb76ddd`,150.340s; before the final prepared-draw slot/metadata correction |
| Final affected Host/submission/program/stock/preparation/Image working-set cohort |90 passed /216 skipped, run `201d096e-0d7e-49cd-99b8-a442d967f240`,59.423s; covers the final custody slot, byte-interface rename and dynamic retained-entry growth |
| Selected actual native Editor workflows |4 passed /299 skipped, run `e0aade75-ebf8-47bb-907e-f2e9b1ef763c`,12.170s; input/reopen/pause/refocus/resize/step, detached View retirement, scene reimport pixels and fixed WorldCanvas coherence |
| Final independent program workspace |8 passed /0 skipped across3 binaries, run `451083ac-7737-48d6-8e7a-bde5f1756985`,3.146s;5CPU plus3actual DX12 cases, including invalid capture preflight before a zero-byte-budget refusal |
| Final independent original image-output workspace |3 passed /0 skipped, run `2069c048-404c-4c4e-bd1d-b48e3ad348d4`,2.987s |
| Final independent program doctests |2 compile-fail semantic-port examples passed,0.94s |
| Correctness Clippy |`nara_render_wgpu` + `nara-editor`, all-features/all-targets, `-D clippy::correctness`: passed36.12s; existing warnings and private adapters awaiting ordinary policy migration remain |
| WGPU wasm32 no-default check |passed2.54s; compile-only portability evidence |
| Actual ordinary Runtime/window smoke |`window_surface_retirement_smoke`, exit0; initial/resized presentation and original surface/provider/native-close path |

The broad command uses `--all-features --run-ignored all` with
`-E 'not test(tilemap_and_changing_hud_costs)'`. That separate manual measurement
requires `--release`; its debug-profile assertion was observed in an earlier
diagnostic run and neither modified nor reported as a product regression. The
final affected filter selects `host::`, `submission::`, `program::`, `stock_quad::`,
`preparation::` and `backend::image_working_set` with the same native-test settings.
All nextest commands use `--locked --test-threads=1 --no-fail-fast`.

The Editor4 run predates the final one-slot custody correction. Final affected90
covers that changed responsibility; this record does not claim another Editor4
run after it. Independent workspaces use locked offline dependencies and the
root target. The ordinary window smoke did not repeat its four separate ingress
mutation modes. TEMP logs are not committed. Formatting, whitespace, current
document links and memory validation are checked during integration.

# Follow-up

Continue ownership transition step4 by moving ordinary Runtime, Scene, Canvas,
embedded Game and image consumers into the same typed Feature/program policy.
Delete the displaced central payload fields, package matches, quad-only prepared
containers and combined submission facades when their actual consumers migrate.
Keep UI replay packets separate from move-only permits, logical window routing,
one full-resolution UI overlay and original completion/physical-close custody.

Then complete View/instance/history groups, alternate policy, source/processed
capture interleaving, independent two-window/image streams, producer-local close,
loss/recovery and clipped Nara-drawn composition. Full ADR0121 remains Proposed.
Toolkit storage, the current fixed WorldCanvas cache, pipeline/driver overhead
and physical GPU residency remain outside the represented shared payload budget.
This evidence does not qualify general streaming performance, browser runtime,
Linux/macOS execution or a complete custom Editor toolkit.

# Citations

- [Production design](../../../../architecture/render-program-production-design.md)
- [Complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Accepted finite program boundary](../../../../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs)
- [Public program guide](../../../../guides/render-programs.md)
- [Public image-output guide](../../../../guides/render-image-outputs.md)
- [Independent program consumer](../../../../../tests/fixtures/render-program-consumer/README.md)
- [Predecessor qualification](2026-10-31T133536Z-finite-typed-render-programs-through-the-original-host-production-qualification-a32bacec9d8b4572801df1f79fc31c01.md)

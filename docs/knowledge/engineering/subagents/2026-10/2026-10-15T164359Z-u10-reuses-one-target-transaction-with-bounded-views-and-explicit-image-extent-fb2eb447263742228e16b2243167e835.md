---
type: "Subagent Finding"
title: "U10 reuses one target transaction with bounded views and explicit image extent"
description: "Subagent Finding for U10 reuses one target transaction with bounded views and explicit image extent."
timestamp: 2026-10-15T16:43:59Z
record_id: "fb2eb447263742228e16b2243167e835"
status: "open"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
subagent_id: "/root/u10_view_presentation"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
---

# Finding

Keep one exclusive physical target transaction per immutable packet and extend it to bounded
ordered views. Independent offscreen/window transactions can use the existing explicit native
sequence. Ordinary Runtime mixed-target camera lists remain an explicit limitation, not an
implicit grouping or a silently dropped view.

# Evidence

`ExtractedViews` and sprite batches already have stable view order/index. The actual constraints
are the single-view packet, view-zero payload validation/lowering, and one-view backend/image
execution. Image target extent is currently inferred from a view's viewport, which cannot
represent split views correctly. The existing Editor preview session owns the real image extent;
there is no observed production `Assets<RenderImage2d>` resolver to use instead.

Toolkit registration generation currently leaks into a WGPU composite receipt, while the native
presenter repeats content/geometry/receipt joins. RenderTime supplies interpolation alpha, but no
previous/current transform history was found; interpolation must not be described as connected.

# Recommendation

Reuse the capture/execution owners; keep exact target/device/content/display identities under one
private presentation join and return an opaque token. Put toolkit binding state in the native
adapter. Pass the preview owner's explicit physical extent and validate each viewport against it.
Use one invertible affine camera mapping for extraction and picking. Do not add a render graph,
target scheduler, new simulation mirror, or another material queue. Preserve U9's per-view batch
reader so U10 can consume each view serially and execute the UI overlay once.

# Disposition

Read-only U10 preparation from `/root/u10_view_presentation`; no writes or Cargo. Root accepted
the one-target scope with real same-target multi-view and explicit native offscreen evidence
required. Implementation remains queued after U9. Future ordinary Runtime multi-target support
can add ordered target runs inside the existing frame capture owner when a consumer requires it.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md (U10, AE4)
- docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md
- crates/nara_render/src/lib.rs
- crates/nara_render_wgpu/src/frame.rs
- crates/nara_render_wgpu/src/editor_execution.rs
- src/project_host/runtime/editor/preview.rs
- editor/src/platform/viewport.rs

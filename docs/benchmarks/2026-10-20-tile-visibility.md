# Tile Queue Locality

Status: Source correction `15e65649eeb103fc93371137fa72ad748ef74c08` passes focused source and
WGPU correctness checks and three optimized measurement processes at `fce2e70`.
The [L3 plan](../plans/2026-10-20-tile-visibility-locality-plan.md) owns this slice.

## Attribution And Correction

The frozen [production baseline](2026-10-20-production-costs.md) processes every cell despite a
fixed camera covering only a small part of the map. Temporary private system-body intervals
in a separate exploratory run place the 65,536-cell static P50 at 3.009 ms extraction, 18.932 ms
queue and 4.074 ms sort/batch. The separately observed queue material/candidate phase is
13.635 ms. Changing-cell observations are similar. These intervals end before local destruction;
stderr probes contaminate the outer managed-drive timer. They are attribution evidence, not
production timings, an allocation profile or additive independent percentiles. All probes are removed.

The correction removes the whole-world temporary queue-candidate vector. A lazy iterator resolves
each sprite's material once, then projects it independently for each admitted view. Only an actual
clip-space instance whose conservative bounds intersect the clip rectangle enters that view's
queue. Final ordering and adjacent-only batching retain the existing keys. Material/UV/Image
authorization and diagnostics still run even for offscreen content or no views.

Clipping consumes the same center and axes uploaded by the backend. A magnitude-relative f32
roundoff allowance keeps cancellation near a boundary conservative; non-finite coefficients or
possible f32 intermediate overflow retain the instance. Exact boundary contact also retains it.
This avoids an independent transform arithmetic order, an invented pixel-snap expansion or a
viewport-derived second camera. Empty views still execute their clear/load policy.

This is an uncached per-view correction. Extraction and material resolution remain O(N); public
tile fields, cleared dirty counters, replacement and Image/TileSet generations are reobserved,
without a new cache invalidation protocol. Fewer submitted instances legitimately reduce GPU admission
charges. Identical GPU over-budget rejection for invisible authored content is not promised.

Pinned Bevy `85503941` consumes per-view visible entities in
[sprite queueing](../../repo-ref/bevy/crates/bevy_sprite_render/src/render/mod.rs); Godot
`81ab961f` uses transformed canvas bounds in
[canvas culling](../../repo-ref/godot/servers/rendering/renderer_canvas_cull.cpp) and retained dirty
quadrants in [TileMapLayer](../../repo-ref/godot/modules/tilemap/tile_map_layer.cpp).
They support deliberate view ownership; they do not require adopting retained chunks without pressure.

## Correctness Evidence

The source cohort passes 91 tests across filesystem and sprite libraries/contracts, including five
new visibility tests. The WGPU cohort passes 133 tests with one explicitly ignored measurement.
A new real Runtime consumer compares every pixel with independently projected, unclipped extracted
geometry for two affine cameras, negative scale, parent-induced shear and mixed transparent
Sprite/Tile order. Captured packets render after Runtime retirement. Existing GPU cases retain
palette materials, Image revisions, split views, fixed-canvas composition/native UI and admission
failure/retirement coverage. These overlapping scopes are not a new aggregate test count.

Large-coordinate cancellation, non-finite/overflow retention, inclusive edges, offset viewports,
per-view sorting, material diagnostics and direct Tile mutation are source-test evidence. The new
GPU differential alone does not prove every material, shared-target policy or extreme-coordinate
backend behavior. No surface-present or release-platform qualification follows from offscreen tests.

## Optimized Results

All three independent processes use the same RTX 4090, NVIDIA 617.14, Vulkan adapter and pass
queue-count, changing-cell/native-UI pixels, semantic label, glyph ink and retirement oracles.
The [summary](data/2026-10-20-tile-visibility/runtime-summary.json) binds 1,962 numeric rows,
three nextest identities, uncompressed raw SHA-256 digests and 126 per-process groups. It keeps the
[temporary body attribution](data/2026-10-20-tile-visibility/exploratory-system-bodies.json.gz)
separate. For 65,536 authored cells, per-process P50 / P95 milliseconds are:

| Scenario / interval | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| Static / managed drive | 4.238 / 4.805 | 4.165 / 4.603 | 4.394 / 4.888 |
| Static / WGPU preparation | 0.276 / 0.314 | 0.274 / 0.384 | 0.288 / 0.311 |
| Static / encode and submit | 0.154 / 0.177 | 0.151 / 0.254 | 0.164 / 0.180 |
| Static / GPU drain | 0.208 / 0.556 | 0.189 / 1.299 | 0.230 / 0.601 |
| One cell and HUD label / managed drive | 4.362 / 4.732 | 4.645 / 5.688 | 4.521 / 5.069 |
| One cell and HUD label / WGPU preparation | 0.339 / 0.484 | 0.394 / 0.569 | 0.344 / 0.458 |
| One cell and HUD label / encode and submit | 0.164 / 0.436 | 0.178 / 0.236 | 0.176 / 0.357 |
| One cell and HUD label / GPU drain | 0.239 / 0.911 | 0.267 / 1.397 | 0.246 / 0.748 |

The frozen baseline's managed-drive P50 spans 25.882-34.020 ms and WGPU preparation
2.346-2.858 ms across these same two scenarios. Independent desktop cohorts do not isolate every
small difference, but the large owner/queue/upload reduction is consistent across processes.
The new largest retained managed-drive sample is 5.688 ms in this stress fixture. No retained
geometry cache is justified by the present 8 ms investigation trigger.

Each large-map frame now queues 1,972 world candidates and prepares 164,880 logical instance
bytes including the unchanged 89 UI quads, versus 65,536 and 5,250,000 bytes before. This is
approximately 96.9% less instance payload, not a 96.9% reduction in total memory. Every sample
has zero pending/lost retirements; explicit backend cleanup returns tracked current payload to zero.

## Reproduction And Remaining Limits

Use the same populations 64/4,096/65,536, fixed 320x180 world canvas, 960x540 destination,
static/one-cell-and-HUD-change scenarios, two warmups and sixteen retained samples per group.
The collector now reports actual queue counts rather than asserting authored population equals
submitted instances. For large maps, 1,856 cells have positive geometric overlap; 1,972 conservative
candidates include the two touching rows. Native UI remains a separately prepared overlay.

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:NARA_RUNTIME_COST_OUTPUT='<distinct absolute JSONL path>'
cargo +1.95.0 nextest run --release -p nara_render_wgpu --features sprite-submitter,ui-submitter --lib --locked --offline --run-ignored only --test-threads=1 -E 'test(tilemap_and_changing_hud_costs)' --success-output immediate
```

Packet construction/capture between phase timers is excluded. GPU drain serializes samples;
encode-to-completion overlaps submission/drain. Logical tracked bytes exclude driver padding,
destination texture and several CPU/GPU allocations; they are not whole-process residency.
Independent processes are not pooled; n=16 nearest-rank P95 equals maximum and is not a stable
tail estimate. The 8 ms investigation trigger remains local, not a supported-map or frame-time SLO.

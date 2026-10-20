# Tile Visibility Locality

Status: In progress after L2 integration at `8afbe42`. Selected L3 brief belongs to the
[locality follow-up](2026-10-20-measured-work-locality-plan.md).

## Evidence And Selected Direction

The [baseline](../benchmarks/2026-10-20-production-costs.md) records a 65,536-cell stress map
with approximately 1,856 geometrically visible cells. Managed drive P50 is 25.882-34.020 ms;
WGPU preparation is 2.346-2.858 ms. The drive includes UI and several ECS phases, so first
attribute extraction, queue and sort/batch separately using temporary private system probes.
Remove them before public measurement. Do not sum/subtract unrelated percentile observations.

Prefer uncached conservative per-view clipping if it removes the measured queue/sort/upload
cost. Build the actual clip-space quad through the existing ViewMapping2d path; reject only an
unambiguously separated AABB from center and absolute half axes. Retain uncertain, overflow or
non-finite cases. Preserve transform arithmetic order at large-coordinate cancellation edges;
precomposed coefficients require their own numerical proof. Use extracted view/canvas mapping,
not a second frustum derived from native window dimensions.

Start at queue conversion while retaining current material/snapshot authorization and diagnostic
order. Move filtering before resource resolution only if the next measurement justifies that
additional boundary change. Extraction has existing no-camera consumers observing complete
geometry; do not silently change their semantics to satisfy a benchmark.

Pinned Bevy per-view visibility and Godot transformed-rect canvas culling support this owner
boundary. Godot TileMap dirty quadrants are a later incremental alternative, not a reason to add
cache ownership before measurement. No cache means direct public tile size/layer/sort/Y-anchor,
TileSet/Image changes, cleared dirty counters and runtime/entity replacement are re-observed each
frame without a new invalidation protocol. It still scans O(N) cells; the 8 ms trigger may remain.

## Acceptance

- [ ] Attribute the current managed consumer and select the smallest effective owner correction.
- [ ] Compare clipped output to an uncached/unclipped reference, including actual pixel readback:
  rotation, negative scale, shear, partial/edge overlap, large coordinates, multiple views,
  shared-target clear/load, fixed canvas and native UI.
- [ ] Preserve authored source order, layers, sort groups, world-Y and adjacent-only batching;
  TileLayer is a sort layer, not a camera mask. Check semitransparent Sprite/Tile/material mixing.
- [ ] Preserve budget/refusal behavior, immutable Image/TileSet authorization, old packets and
  physical resource retirement. Exercise map/cell replacement/removal and direct public edits.
- [ ] Keep Editor preview hit candidates coherent with the actual per-view queued geometry.
- [ ] Update only the new collector's actual queued counts; historical raw data stays immutable.
  Repeat static/changing real-GPU measurements, independently review, document and commit.

Root owns serial Cargo/fmt/Git, one build job and shared target. Allocate disjoint source/test
paths before implementation; preserve concurrent work and read-only repo-ref. Tile authoring
tools, chunk streaming and speculative render frameworks remain outside this unit.

Initial lanes: root owns temporary attribution, the runtime collector, integration and reports;
`persistence_worker_seam` researches the numerical/view boundary and will own the private queue
correction after attribution; `save_consumer_migration` owns independent correctness/GPU oracle
coverage; `persistence_adversarial_design` reviews authorization, order and numerical failures.
Workers do not run Cargo, format, stage or revert concurrent changes.

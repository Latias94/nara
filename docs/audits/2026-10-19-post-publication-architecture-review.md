# Architecture Review After TileSet Publication

Status: reviewed implementation and scoped follow-up recommendations.

## Verdict

Keep the current component/resource/plugin split, thin ECS substrate, frozen Rust recipe and one
managed execution owner. The new TileSet consumer strengthens these boundaries rather than
requiring a second engine kernel. The next refactor should reduce caller knowledge around
concrete asset publication and source admission. There is no evidence for replacing the ECS,
introducing a universal importer executor or treating missing game features as core defects.

The product remains Rust-first general-purpose 2D with integrated authoring. Genre rules such as
encounters, crop state or inventory belong in game/optional domain modules. Tile tools remain
deferred until the current content contracts and their authoring mutations are settled.

## Corrections Completed In This Slice

| Risk | Owning correction | Evidence |
| --- | --- | --- |
| Strict content could admit an invisible out-of-range cell | Tilemap-owned atlas membership proof during project closure admission | `tests/project_content_boot.rs` |
| Source loading inferred Image from the consumer instead of source metadata | Traverse declared effective references and dispatch admitted source kind; retain codec kind validation | Nested/default and both wrong-consumer-kind regressions |
| Live Image replacement could detach pixels from the atlas that validated them | Immutable ImageContentSnapshot retained by a concrete TileSet asset owner | `crates/nara_tilemap/src/content.rs`; real Sprite + two-TileSet consumer |
| Failed desired B could erase published A revocation or never recover absent B metadata | Separate latest desired and published dependency roles; bounded Host catalogue observation and serial hydration | Hidden-scene Editor recovery and late-completion rejection |
| Atlas changes could reinterpret a row-major TileIndex without migration | Preserve first accepted layout through automatic source/dependency revocation; require explicit reconstruction | Layout/recreation owner regression |
| Mixed Image generations could repeatedly replace one GPU cache entry | Exact snapshot versions/bindings, finite residency, submission retention and namespace invalidation | Native DX12 readback and cache capacity tests |
| Missing TileSet could emit white fallback cells | Skip an unresolved handle during extraction; explicitly untextured maps remain valid | Sprite extraction regression |

Appearance builders preserve captured pixels because tint/sampler/alpha do not alter the atlas
extent proof. Changing Image/atlas through the code-first builder clears captured content.
Code-first TileSets intentionally follow their caller-owned live handles; file-backed publication
uses retained snapshots. A renderer guard must not erase that supported distinction.

## Remaining Refactor Order

### M01: Deepen The Concrete TileSet Interface

Priority P2, current extension cost rather than a failing workflow.

`TileSetAssets::request/reconcile` makes callers supply several stores and state/event resources.
`src/project_host/runtime.rs::publish_snapshot_tilesets` and the Editor's
`authoring_runtime/tilesets.rs::with_assets` are two real call sites that know the resource join.
Before adding another placement, move this join into a concrete TileSet SystemParam or owned
command seam. Keep pure reconciliation logic testable and keep filesystem authority in the Host.

Do not add a generic World mutation service, erased asset executor or universal publication
transaction. A useful refactor removes repeated knowledge from both consumers and preserves
explicit source request, rejection, revocation, retry and final-holder semantics.

### M02: Share Private Source Admission Facts

Priority P2, triggered before a third file-backed asset domain.

`src/project_content.rs::load_tileset` and Editor
`image_import/transaction.rs::refresh_tile_set_source` repeat capability-bound metadata identity,
kind, digest, bounded source reading and decode ordering. Extract shared private admission facts
and a receipt only when implementing the next domain. Strict closure budgets and incremental
Editor scheduling remain different owners and should not be forced into one scheduler.

The Editor currently processes at most two TileSet sources per advance. It rereads on catalogue
generations because FileIdentity alone cannot detect in-place byte changes; replacing those
checks with identity-only caching would be incorrect.

### M03: Design Semantic Authoring Before Tile Widgets

This is a future capability boundary, not a newly confirmed defect.

Keep atlas-local TileIndex until layout editing or durable per-tile metadata creates a migration
requirement. Then choose tileset-local logical identity/remapping and coordinate-based edits with
bounded inverse history before brushes/terrain. A small command payload does not prove cheap
whole-document validation. Do not place gameplay occupancy or mutable encounters in render cells.

### M04: Measure A Real Saved Map Before Scaling Work

The current renderer still extracts cells and projects them per view. Capture a representative
saved/reopened map workload, frame tail and allocation budget before culling, chunk caches or
streaming ownership work. Existing startup-wide leases do not establish partial region residency;
prove two regions sharing one Image before an acquisition/release design.

## Mature Engine Comparison

Pinned source snapshots are background evidence, not compatibility requirements:
Bevy `85503941e488152711a4695bbe248e9ed9f453fc`, Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`.

- Bevy's `bevy_app/src/plugin.rs` keeps startup plugin contribution distinct from ECS data.
  Nara should retain that separation while its managed Runtime owns execution/retirement.
- Bevy's `bevy_asset/src/loader.rs` distinguishes asset handle dependencies from dependencies
  whose values/bytes were read. Nara's exact TileSet/Image snapshot follows this responsibility
  distinction without adopting Bevy's complete generic executor.
- Godot's `modules/tilemap/tile_set.cpp` propagates mutable resource changes into derived work.
  Nara's immutable publication is a deliberate alternative suited to retained packets and strict
  revocation. Neither inspected reference proves whole dependency-bundle atomicity.

## Verification And Limits

Actual commands and run IDs belong in the
[verification record](../knowledge/engineering/verification/2026-10/2026-10-19-tileset-publication-and-recovery.md).
Three mistaken review claims about version mutation, cross-handle failure churn and current-slot
authority were withdrawn after source inspection and behavioral checks; they are not remaining
debt. Keep findings tied to a concrete consumer and invariant instead of accumulating speculative
infrastructure work.

The standalone HTML outside the repository includes the repaired issues and this refactor order.
Tile painting, terrain tools, general streaming, physics/audio and cross-platform native release
qualification are outside this slice.

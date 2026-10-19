# TileSet Publication and Dependency Recovery

Status: implemented and reviewed following `eaa140b`; final serial verification is recorded in
[engineering evidence](../knowledge/engineering/verification/2026-10/2026-10-19-tileset-publication-and-recovery.md).

## Outcome and Order

1. Reject persisted cells outside their admitted TileSet before strict Check, startup or Export
   publishes content. Preserve empty maps, untextured cells and defensive code-first rendering.
2. Discover schema-declared assets by admitted source kind, including custom/nested/default
   references, without component-name loading exceptions or an erased importer framework.
3. Publish each file-backed TileSet with the exact immutable Image content that passed the shared
   domain extent proof. Valid Image replacement may advance Sprites independently while a failed
   TileSet successor retains a coherent previous picture; removal and identity invalidation revoke it.
4. Recover the latest desired dependency after failure, including a path with no admitted metadata
   yet, while observing the published dependency for revocation. Bound desired/published state,
   retained allocations, candidate overlap and exact-version GPU cache entries.
5. Prove Runtime and managed Editor consumers, failure/recovery, stale results and physical cleanup;
   then review the engine architecture, interfaces and module boundaries against the resulting code.

Tile painting, brushes, terrain tools, layout editing and tile-ID remapping are explicitly deferred.
The current persisted identifier remains atlas-local `TileIndex`; membership validation does not
invent a new durable TileId format. The final architecture review precedes those tools.
Live successors retain the first successfully admitted atlas layout for that entry. Layout changes
require explicit removal/reconstruction and strict source admission until a migration workflow exists.

## Ownership and Lanes

The root integrates runtime, retained publication, render consumers, contracts and verification.
One worker owns strict source discovery/membership in `src/project_content.rs`, Tilemap domain
helpers and the existing content boot tests. Read-only reviewers examine invariants and the final
architecture. Assigned writable paths are disjoint; no lane edits `AGENTS.md` or `repo-ref/`.

Cargo build/test work is root-only, serial, with `CARGO_BUILD_JOBS=1` and the existing `target`.
Unrelated `myeasylog.log` and `vendor/` remain untouched. The user's commit request authorizes
completed, verified implementation commits; no push, PR or platform release is requested.

## Design Invariants

- Host owns filesystem capabilities, metadata observations and unresolved semantic demand.
- TileSet owns source interpretation, exact extent and membership validation, and coherent content.
- Image owns immutable pixel identity/retention; a Copy handle alone does not retain a generation.
- A concrete TileSet execution owner remains separate from the Tilemap data/schema plugin.
- A failed desired B dependency must not erase the published A revocation dependency. Retargeting
  replaces the latest desired demand instead of accumulating history or polling blindly each frame.
- Prepared packets carry exact content; adjacent batches and backend cache reuse distinguish exact
  snapshots even when their logical Image handle matches. Actual holders determine retirement.
- Capacity rejects before publication and preserves eligible last-good; deletion overrides it.
- Accepted ADRs are refined alongside durable changes, and proposed designs remain distinct from
  implemented/verified claims.

## Verification

Start with strict membership and custom/nested/default source-kind regressions. Then exercise a
Sprite, two TileSets sharing an Image and UI content across same-size changes, incompatible resize,
A-to-B retarget/failure/recovery, absent B metadata, dependency removal, stale completion and close.
Check old/new exact pixels and UVs together, unchanged-frame upload reuse, bounded retention and
capacity rejection. Run relevant native Windows/DX12 readback after render integration, plus public
consumer and feature-minimal builds. Record exact commands/results in engineering memory and the
standalone HTML outside `docs/`.

Finally use independent reviewers for architecture, API/ownership, modularity and correctness.
Separate confirmed defects from extension risks; implement concrete corrections before declaring
this scope complete. Do not turn the review into speculative new engine domains.

The [post-publication architecture review](../audits/2026-10-19-post-publication-architecture-review.md)
owns remaining interface/module recommendations. Tile authoring stays deferred.

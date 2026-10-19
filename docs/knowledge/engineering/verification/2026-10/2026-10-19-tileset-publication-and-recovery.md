---
type: "Verification Evidence"
title: "TileSet exact publication and dependency recovery"
description: "Strict atlas membership, exact Image retention, desired dependency recovery, GPU coexistence and reviewed module seams."
timestamp: 2026-10-19T04:07:18Z
record_id: "4c525db8a5884da0813d4f67606ce73e"
producer_id: "codex-root"
run_id: "tileset-publication-20261019"
related_plan: "docs/plans/2026-10-19-tileset-publication-and-recovery-plan.md"
git_branch: "refactor/foundation-contract-hardening"
---

# Verification

Windows, Rust 1.99.0, existing `target`, root-owned Cargo execution and
`CARGO_BUILD_JOBS=1`. The preceding foundation work was committed as `eaa140b`.
This record covers the authorized strict TileIndex and TileSet/Image publication follow-up.

# Result

Strict startup content now joins persisted cells to the admitted TileSet atlas and dispatches
schema-declared nested/default references by admitted metadata kind. TileSetAssets owns one
latest desired source and a separate eligible published Image snapshot. Extent failure, absent
metadata and decode failure retain coherent last-good; source/dependency identity revocation
removes authority, while later admitted desired content recovers without a visible Scene panel.

The first published atlas is retained across automatic source revocation. Layout changes require
explicit consumer reconstruction; ordinary Image retargets and appearance changes remain live.
The concrete asset plugin remains independent of Tilemap schema/data and introduces no erased
import executor. WGPU cache versions distinguish exact snapshots, count actual retained holders,
bound resident Image allocations and stop reuse across device namespaces.

# Evidence

| Selection | Result | Run |
| --- | --- | --- |
| Final Asset, Image, TileSet, Sprite and root Host libraries | 436 passed, 1 ignored skipped | `36767d6c-bff8-451a-89dc-dcfa6bb80295` |
| Native Scene reimport and delegated-window texture bridge | 2 passed | `6512ed6f-0a98-44ae-be9a-a2447c70e618` |
| Strict content and Runtime boot consumers | 21 passed | `23c37190-a363-4eac-97ba-d0982b38ad55` |
| Image/TileSet libraries before final review closure | 112 passed | `9319c94a-d4b7-492a-97f1-e6d983fbc96d` |
| Asset and Sprite libraries | 104 passed | `d20f1095-dad3-4334-a44a-9ca98d94e893` |
| Latest exact snapshot/TileSet/real Sprite + two-TileSet integration | 16 passed | `a76900e0-8377-492a-82d5-07e0e2df887c` |
| Latest Sprite library after missing-handle correction | 26 passed | `505b2ce9-411a-43a9-88b1-d78f3da14871` |
| Latest generic Editor hidden-scene and malformed-source recovery | 2 passed | `6bafc8f6-0c87-48d0-963a-82edd1b9bbc2` |
| Windows/DX12 WGPU all-feature library suite | 111 passed, 2 native qualifications skipped | `c13d5784-cbc1-4714-81f1-22747dccbb51` |

The GPU regression first failed with one resident version instead of two in
`b76f438e-9556-4c3a-82c4-ec834649bb2f`. Its green test reads actual old/new pixels together
and verifies no repeat-frame upload. Capacity, final-holder release, namespace reset and
fallback-binding accounting have separate behavioral regressions. These totals overlap and
must not be summed as unique tests.

`cargo check --locked -p nara --all-features --lib`, no-default `tooling`, and no-default
`runtime-2d,tooling` builds passed. `cargo fmt --all` and `git diff --check` passed at the
recorded integration point. Existing deprecation/dead-code warnings remain.
The final `cargo check --locked --workspace --all-targets --all-features` also passed in 1m 32s.
Final library regressions include automatic source revocation retaining the layout tombstone,
identical-source capacity retry after a retained generation drops, and no white missing-handle cells.

Focused command shapes:

```powershell
cargo nextest run --locked -p nara --no-default-features --features presentation-2d,serde,tooling --test project_content_boot --test project_runtime_boot --test-threads=1 --no-fail-fast
cargo nextest run --locked -p nara_tilemap -p nara_image -p nara_sprite_render --all-features -E 'test(content::tests) | binary(tileset_content)' --test-threads=1 --no-fail-fast
cargo nextest run --locked -p nara --all-features --lib -E 'test(generic_editor_reconciles_tileset_latest_dependency_and_revocation_without_a_visible_scene) | test(generic_editor_rejects_a_malformed_tileset_before_a_late_image_can_publish)' --test-threads=1 --no-fail-fast
cargo nextest run --locked -p nara_render_wgpu --all-features --lib --test-threads=1 --no-fail-fast
cargo nextest run --locked -p nara-editor -p nara_render_wgpu --features nara_render_wgpu/editor-imgui --lib --run-ignored only -E 'test(native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes) | test(secondary_bridge_rejects_foreign_replay_stale_and_unsampled_images)' --test-threads=1 --no-fail-fast
```

Review corrections were verified against owning code. Three claims were withdrawn: next_version
is a non-mutating preflight, dependency observations track Image state while TileSet failures
write another handle, and published-slot equality compares current managed storage authority
with current storage rather than with the retained snapshot. Experimental helpers prompted by
those mistaken claims were removed. Tests continue to prove valid replacement and raw ABA.

# Follow-up

The component/resource/plugin and single managed Runtime direction remains appropriate. Two
concrete modularity pressures remain: multi-resource TileSet orchestration appears in Runtime
and Editor, and source-admission identity/digest sequencing is duplicated in cold/incremental
Hosts. Prefer a concrete domain seam and private capability-bound helper when extending another
real asset domain, rather than a generic loader framework. Tile tools, durable tile identity,
layout migration, Tiled import and map-scale optimization remain deferred.

Selected hidden-window Windows/DX12 Scene reimport and delegated-window bridge trajectories also
executed and passed with Rust 1.99. They do not establish full Rust 1.95 native Editor release
qualification, OS scanout or Linux/macOS evidence. No global memory bound is claimed beyond the
named owners. Initial known TileSet sources and their refreshed dependencies are covered; automatic
discovery of every newly opened document's asset demand is a broader authoring workflow.

# Citations

- [Execution plan](../../../../plans/2026-10-19-tileset-publication-and-recovery-plan.md)
- [Accepted ownership contract](../../../../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md)
- [Original dependency study](../../../../research/2026-10-18-tileset-dependency-and-editor-loading-research.md)

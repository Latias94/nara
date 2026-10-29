---
type: "Verification Evidence"
title: "Processed TileSet ownership and exact Scene resource baselines qualification"
description: "Qualifies actual Runtime dependencies, shared source and processed TileSet publication, concrete Scene resource baselines and retained native pixels through public games and DX12 consumers."
timestamp: 2026-10-29T11:42:36Z
record_id: "e5b31550eacc4fd1a3d4b6168fb4df52"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "72472738ba357d7c030b80c78cc08bd617b8e49a"
supersedes: "839822b154f24bf6b658c0c9954fa2b1"
---

# Verification

Continue the complete active foundation Goal and ordered R6/R7 delivery plan from `c9cb764` on
long-lived `refactor/foundation`. The independent rendering contract amendment is `eb35d7a`;
production implementation is `7247273`. Rust1.95.0, Windows11/PowerShell7, serial Cargo, jobs1 and
the existing root target apply to all workspaces. No remote publication occurred. The pre-existing
untracked vendor directory and pinned reference checkouts were preserved.

This record closes the named TileSet/native prerequisite and concrete resource-baseline correction.
It does not close the whole foundation Goal, R6/R7, first-party processed Host delivery or R8/R9.

# Result

`TileSetDescriptor` owns complete TileSet/Image Products and atlas/material policy independently
of source metadata or receiving handles. Primary and auxiliary selections are explicit. The strict
`runtime_tileset` version-one codec uses Core's bounded envelope, visits its actual required Image
and accounts for its fixed-size decoded value. Source and descriptor reuse the original exact
extent algorithm through one private helper.

`RuntimeAssetCodec::visit_required_runtime_products` is mandatory. The content Host checks actual
decoded references against the same captured base's Product membership and this member's Required
declarations before returning the candidate. Soft or omitted declarations refuse. Empty inventories
are explicit; wrapper codecs delegate. Concrete errors propagate and inventory panics release the
original reservations. This query neither acquires another asset nor proves its family/residency.

The existing TileSet owner retains a closed Source or Product declaration and shares claim,
value/state baseline, actual Image snapshot, extent, capacity, atlas barrier and retirement logic.
Source requires real database/provenance facts. Product requires complete typed bindings and the
actual declared Image; it creates no source records, hashes or database. Source APIs cannot take
over processed declarations. Neutral content receipts and public finite publication inputs replace
the old source-only/private names without aliases. Source root callers use the same owner.

Managed Runtime Tilemap decode supports existing complete primary/auxiliary bindings without a
source database. Unknown/type-conflicting Products reject without allocating identities. Persisted
cell membership asks the concrete content owner for the complete reference's admitted atlas;
source-only selection restrictions remain in source adapters. Actual dependency family, Image
extent and out-of-atlas cells refuse before live publication.

`SceneProductResource` now captures a required concrete owner baseline once and revalidates it
through native waits and final commit. Missing and changed resources have distinct diagnostics.
Same-tick edits, content ABA, equal writes and equal data in a new owner reject without publishing
candidate Scene/resources/bindings. The original candidate remains under existing retry/discard
custody. Returning the same unchanged Rust owner after temporary removal preserves its logical
incarnation; the ECS slot is not a separate revision authority.

Reference Game and the two-level consumer use stable owner identities plus checked write counters.
Ordinary game writes do not allocate new identities. Actual mutation sites are encapsulated, clones
that become independent resources get new incarnations, and snapshot data equality remains about
gameplay values. WaveSnapshot exposes immutable content and complete publication rather than
untracked field writes. No World-wide resource tracker or second Scene transaction was added.

Independent consumers publish actual Image/TileSet values and native Tilemaps through the existing
Scene transaction. Original Runtime Image acquisitions retain base/output custody through escaped
Image, TileSet and prepared captures. A separate DX12 readback renders exact old/new atlas tiles
after native owner/store retirement, including primary and auxiliary Product selections.

# Evidence

| Scope | Actual result |
| --- | --- |
| Asset/Image all-feature default test selection | 290 passed, one default skip, five binaries; run 15.171 s |
| Scene all-feature default test selection | 781 passed, three default skips, 37 binaries; run 45.855 s |
| Root all-feature library | 926 passed, four default skips; run 327.444 s |
| TileSet/SpriteRender all-feature default test selection | 131 passed, no skips, four binaries; run 9.411 s |
| Two independent packages, nine selected binaries | 73 passed, no skips; run 7.113 s |
| Root public Scene/product/content integrations | 11 passed, no skips, three binaries; run 50.305 s |
| WGPU all-feature library, required DX12 adapter | 146 passed, four default ignored cases; run 88.626 s |
| Windows/DX12 Editor critical trajectories | All three executed/passed; 283 outside-selection skips; run 8.019 s |
| Reference Game all-feature library plus 12 integrations | 71 passed, no skips, 13 binaries; run 32.444 s |
| Two-level game library, checkpoint and gameplay | 30 passed, no skips, three binaries; run 3.015 s |
| Six owning packages correctness Clippy | All features/all targets/locked passed 3m25s |
| Both independent packages correctness Clippy | All targets/locked passed 42.11s |
| Reference Game correctness Clippy | All features/all targets/locked passed 2m57s |
| Two-level game correctness Clippy | Default features/all targets/locked passed 1m03s |
| Minimal features | Asset/Image/TileSet no-default lib; root runtime-core+serde and runtime-2d+serde lib checks passed |
| Additional minimal Scene coverage | No-default all-targets check passed 22.05s; earlier TileSet no-default tests 26/26 passed |
| Final static checks | Root/independent/Reference/sample formatting, changed-document links/fences and Git whitespace passed |

Nextest IDs: Asset/Image `343ef9d9-42a1-43d6-a783-2a82291e35d2`; Scene
`c9c52ee1-2b08-43e1-82a2-d29ddb98a333`; root `19547799-a41c-4ecd-a221-2df61ba50496`;
TileSet/SpriteRender `ad9f874c-642a-4943-b171-3c81d6f066ed`; independent
`dc3f183b-f447-4655-863e-370701f443b5`; root public
`b62ff04c-3579-4b60-8a4e-c88b45d2b308`; WGPU `76d28688-e39b-4fec-9fc2-4aa0adb3e5da`;
native `f87b721f-fa23-4a91-98e7-6e7b0ebf905f`; Reference
`204a6d19-2d9c-4835-aeac-2b4b03fbf908`; two-level
`2f4b2e4b-e703-4bed-ad8c-2fcb78759a09`.

Root public binaries are `runtime_scene_activation`, `scene_product_public_surface` and
`project_scene_content`, including existing negative public-surface compilation. Independent
packages are `nara-scene-module-consumer` and `nara-asset-domain-consumer`; selected binaries are
`runtime_tileset_delivery`, `runtime_image_delivery`, `runtime_scene_assets`,
`runtime_scene_delivery`, `runtime_base`, `bundle`, `compound_workflow`, `workflow` and `import`.
Reference integrations are `first_wave`, `desktop_flow`, `desktop_parity`, `desktop_render`,
`ui_product`, `ui_authoring_roundtrip`, `project_manifest_ingest`, `plugin_composition`,
`asset_task_flow`, `image_asset_safety`, `project_content_boot` and `editor_ui_refresh`.

Native selection follows the qualification procedure's exact resume/redraw, Game input/reopen/
resize/Step and independent-view detach/recover/retire names. Both native and WGPU commands set
`WGPU_BACKEND=dx12` and require an adapter. Readback checks all 4096 pixels of the selected nonzero
atlas tile: old red and replacement green remain exact after native stores retire. It does not
qualify monitor scanout, new UI controls, other platforms or the future rendering graph.

Nonfatal style, unused-code and large-error warnings remain. These are modeled content costs,
not RSS or allocator isolation. No full-workspace, complete packaged Player or performance gain
is claimed by the named cohorts.

## Independent Review And Integration Corrections

The descriptor/codec and native-owner writers used disjoint files and froze before integration.
Separate reviews checked TileSet identity/publication, Scene transaction baselines and actual game
mutation/reset semantics. The baseline and real-game reviews found no actionable remaining issue.
The TileSet review found one P2 inherited by both declaration paths: Image Loading/reload-failure
state changes republished unchanged TileSet content. The fix compares complete desired content and
its current usable publication, source hash and owner state before skipping publication. Real
Source/Product regressions preserve new Image publication, appearance/hash edits and source
rejection recovery. A test that fabricated private retry state was removed in favor of the actual
source request/reject/reconcile workflow.

Integration repaired the root's new descriptor-error diagnostic arm and removed private-counter
calls accidentally used by external tests while retaining observable Scene/identity/binding/value
assertions. The new consumer initially reused a one-pixel producer fixture; actual execution exposed
that it could not exercise tile 1. It now imports the real 16x16 meadow PNG before source removal.
All final cohorts above use that corrected fixture. Lock refresh changes only declared dependency
lists; package versions remain unchanged. Offline metadata initially lacked platform crates, so the
Windows-filtered refresh used the configured local proxy where needed.

The focused Astra max rendering review found five still-needed proposed contracts: preserving
writes, exact cross-capture output leases, predecessor-checked history publication, a versioned
material declaration joining semantic fields/GPU ABI/resource access, and same-generation recovery
input custody. `eb35d7a` records those amendments against pinned Bevy/Godot evidence. They remain
Proposed design work, not new renderer implementation evidence.

# Follow-up

Continue the first-party closed source/processed Host/request selection, independent Runtime codec
contributions and complete Product CPU reads, then reuse current top-level Image/TileSet residency
and Scene/native transactions for startup and later travel. The selected original base/plan and
cleanup custody remain authoritative. An independent aggregate is not stock extraction routing.
Empty source-facet composition, processed input maps, target executable selection, packaging,
CAS/group publication and rendering/custom-drawn UI implementation remain explicit open work.
The foundation Goal stays active; this record does not reduce its objective.

# Citations

- [Ordered delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Runtime TileSet guide](../../../../guides/runtime-tilesets.md)
- [Runtime base guide](../../../../guides/runtime-content-base.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [Integration progress](../../progress/2026-10/2026-10-29T101227Z-shared-tileset-content-ownership-and-concrete-resource-baseline-integration-839822b154f24bf6b658c0c9954fa2b1.md)
- [Predecessor actual-output qualification](2026-10-29T094316Z-actual-runtime-decoded-output-charging-and-shared-pixel-retention-qualification-14f023431b4c462dbeb1051f1739662e.md)

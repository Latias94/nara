---
type: "Work Progress"
title: "Canonical source locations and bound dependency integration in progress"
description: "Records the scoped source-locator replacement, actual owner verification and remaining integration and Source Scene work."
timestamp: 2026-10-29T16:58:12Z
record_id: "ca6a465101fd4a01ac81129384857e92"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/foundation"
git_commit: "4c405c1"
---

# Summary

The complete foundation Goal remains active. `main` and the long-lived `refactor/foundation`
branch start this unit at `4c405c1`; the current source-location replacement is uncommitted.
Keep the root target and serial Cargo with `CARGO_BUILD_JOBS=1`; preserve untracked `vendor/`.

# Details

- ADRs 0007/0038 adopt `AssetSourcePath`, explicit `AssetPathScope` and borrowed
  `AssetReferenceResolver`. Records, server source aliases and watcher events use canonical
  locations. Sidecars and authored paths remain local aliases; narrow readers retain their
  capability and bound scope. Source/processed content keeps distinct authority.
- Core, Image/Watch, Reflect/Scene, domain collections, source/Authoring Hosts, Editor and public
  consumers have production/call-site replacements. Integration, formatting and independent
  review are still underway. Do not infer whole-unit qualification from the owner tests below.
- TileSet binds its actual canonical Image dependency in each source request/publication input.
  The desired declaration retains it through automatic reconciliation; scope changes rotate
  revision and publication claim even for equal source/record/hash. Remove the temporary
  Host-per-frame scope driver. `TileSetContent::reconcile()` is parameterless again.
- Identity discovery does not activate an asset owner: unrelated Unknown, Scene or unregistered
  domain metadata may be indexed. Overlapping readers bind observed canonical locations against
  actual configured scopes and preserve exact UUID/case and metadata-digest conflicts.
- The user reiterates that foreseeable identity, dependency and ownership contracts should be
  designed before freezing the foundation. Implementation depth can be staged along the same
  model. A new read-only rendering counterexample review is running. Requested Astra max failed
  its usage limit before work; a current Codex agent performs that bounded review instead.

Actual verification at this checkpoint:

- `nara_asset` all-feature owner: 162 passed, run `85145594-f858-4be7-a48a-b59379d4cd9b`.
- `nara_asset --no-default-features --locked` check passed after the serde-only retained-cost cfg.
- Image/Watch all-feature owner: 195 passed, one existing skipped test, run
  `f3aec7c9-9668-4881-bbb2-60510b147f87`. Initial Image fixture attempted to clone a borrowed
  non-Clone value; use its actual `share_retained()` owner before dropping storage. The Watch
  rename fixture now correctly retains metadata dirtiness after source recreation; production
  coalescing behavior is unchanged.
- No root/Scene/TileSet/new independent consumer or native result qualifies this unit yet.

# Next Action

Finish API/call-site integration, run serial owning/root/independent and relevant native tests,
address concrete independent-review findings, reconcile guides and ledger, then commit the
qualified source-location unit and locally ff main. Continue the real Source Scene Door closure,
input/target delivery and render/custom-UI foundation work; do not mark the whole Goal complete.

# Citations

- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
- `docs/architecture/source-location-and-reference-resolution-design.md`
- Predecessor qualification: `verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md`

---
type: "Work Progress"
title: "Durable build recipe successor and source provenance replacement in progress"
description: "Work Progress for Durable build recipe successor and source provenance replacement in progress."
timestamp: 2026-10-30T19:31:04Z
record_id: "d866a6d48d1c440e802557c7954186ec"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-local-artifact-build-plan.md"
git_branch: "refactor/foundation"
git_commit: "96f4519"
---

# Summary

Continues full active foundation Goal from the qualified retained-pack checkpoint 96f4519.
R6b/CP5 design and implementation are in progress. No full durable-store/cook/group-publication
qualification is claimed; current work replaces misleading source recipe types and adds one
concrete target-sensitive CPU Image transform before the tracked persistent builder.

# Details

- New local-artifact-build design/plan and pinned source study reconcile current code against
  Bevy85503941e488152711a4695bbe248e9ed9f453fc and Godot81ab961f91066a63bcbf3ec340962bb1a1ada558.
  ADR0007/0033/0103 now separate source recipe, closed build recipe, actual bytes and group.
- Baseline ImportArtifactKey hashes recipe inputs. ImportArtifactRecord derives a file path
  despite actual Image import publishing no cache file. Selected replacement ImportRecipe has
  source identity/hash, observed dependency digest, importer identity/revision and settings hash;
  ImportRecipeDigest has its own v1 hash domain. Synthetic record/path/profile/output fields
  and compatibility names are removed. Pixel content hash and custody remain separate.
- Source-only Settings remains DeserializeOwned. Full builder must execute the same normalized
  values it records, observe raw/presence versus processed inputs and consume actual target
  facts. Full public builder/store seam is still unresolved; no generic erased executor selected.
- Consumer study found compound TextBundle's three Products share one actual Runtime member,
  and its settings need semantic sorting of continuity rows/retired IDs. Source raw whitespace
  affects its explicit continuity hash; metadata whitespace need not alter normalized settings.
- Existing target is admission-only. Selected CPU Image cook uses Reject or explicit proportional
  FitNearest, retaining Product and actual nearest RGBA texels. A valid4096x1 PNG and actual
  Runtime roundtrip must produce valid distinct4K/2K outputs. It must share owning Runtime limits
  and admit logical output/allocation overlap before allocating. It does not silently resize atlases.
- No Nara Cargo was running at start. F free fell to about25GiB due shared machine use. No
  deletion/compaction performed. Root target and strict serial Cargo policy remain.

Active disjoint writers, no Cargo/fmt/commit permission:

- Jason 01a18e2e-e87d-7d40-8e10-c3db0fbf2e31: nara_asset and existing nara_image only, six-field
  source recipe/core migration; excludes new cook files.
- Mencius 01a18e2f-26d4-7623-ab13-b911435ef605: other Rust consumers under src/tests/examples/
  reference-game and other crates; excludes docs/new build/cook modules.
- Popper 01a18e40-d6c5-7ce3-9977-a708470c4b99: new nara_image/src/cook.rs and tests/image_cook.rs
  only. Parent wires exports and shares Runtime constants after existing-file writer freezes.

# Next Action

Collect final API lists and source-ready, inspect source/consumer diffs; parent wires cook module
and shared codec limits. Independently review source recipe ownership and actual Image cook.
Freeze all writers before fmt; run focused owning/consumer serial nextest using root target,
minimal/all-target checks and affected native evidence. Preserve exact budget assertions by
removing true obsolete allocation costs, not widening caps. Continue tracked builder/CAS/group
implementation under the full parent plan, not merely source recipe documentation closure.

# Citations

- [Design](../../../../architecture/local-artifact-build-design.md)
- [Plan](../../../../plans/2026-10-31-local-artifact-build-plan.md)
- [Pinned comparison](../../../../research/2026-10-31-local-build-recipes-and-publication.md)

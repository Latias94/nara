---
type: "Subagent Finding"
title: "Source-carrying export still lacks an independent verified Runtime content entry"
description: "Distinguishes build-machine Cargo and qualified source-carrying delivery from the remaining processed Runtime catalog and subsequent Scene loading boundary."
timestamp: 2026-10-28T22:54:32Z
record_id: "b9156a4b7e2043f09f3ab41b78cf3bf8"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
subagent_id: "01a184b4-1645-74c0-85b2-02027e94e25e"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/installed-owner-creation-evolution"
---

# Finding

The current source-driven Check/Play and source-carrying Export do not establish R6a/R7's
independent verified Runtime content entry. The exported application reads the package's copied
source closure and prepares products. It does not require the original development directory
after publication. This audit found no run-machine Cargo or processed disk-cache fallback in
that path; build-machine Cargo is allowed and must not be mislabeled as the gap.

# Evidence

1. `src/project_content/domains.rs:60,328` exposes `ProjectAssetDomain::Settings`, `Product` and
   `prepare(ProjectAssetSource)` and reads metadata/main source before preparation. The external
   text consumer in `tests/fixtures/module-consumer/asset-domain-consumer/src/lib.rs:40` reads
   a declared auxiliary source. No processed Runtime bytes/decode contract follows from that.
2. `src/project_content/delivery.rs:18` retains source observations/read authority, not decoded
   products. `project_workflow/export.rs:554` maps them into `project/<original path>`;
   `export/stage.rs:94` reads the held source authority during build/publication. This correctly
   needs the source then, but the resulting run machine still needs the copied source members.
3. `src/project_host/product_launch/root.rs:15,43` selects adjacent `project/` for installed
   bin/tool layouts; failure to open it does not fall back to the original development root.
   `runtime/action.rs:1662` still enters `check_project_source_*`. Existing CLI tests at
   `tests/project_workflow_cli.rs:425,524` detach the development coordinate and then require
   gameplay to refuse missing package-local auxiliary source. Their source-carrying scope is real.
4. Startup is not the only boundary: `src/project_content.rs:2139,2193,2246` decodes TileSetSource
   and reads image metadata/source through ImageImporter. Subsequent Scene preparation still
   calls the source loader at `runtime/scene_requests/preparation.rs:69`. The domains SourceCache
   at `domains.rs:400` is per-load Arc byte deduplication, not a processed disk-cache fallback.
5. `src/project_tooling/export/cargo.rs:96,141` fixes host target, release/profile/features and
   two starter binaries, then runs both locally for agreement. `project_workflow/export/build.rs:21`
   fixes a five-file generic build inventory. `project_tooling/bootstrap.rs:45` infers desktop
   features/tool layout. Those are Host tool/target artifact/runner policy coupling; they do
   not prove Cargo must be installed on the eventual run machine.

Coordinates describe the inspected `9a4ee37` branch with uncommitted creation-owner integration.
This was static read-only review, with no Cargo/runtime/format operation or vendor/reference
access. It does not rerun the cited existing CLI tests or qualify a delivery replacement.

# Recommendation

Use the existing external text consumer for one two-Scene Caption/text-bundle headless product.
Supply two stable subproducts and one auxiliary source; where no intrinsic ID exists, retain
author-persisted continuity facts and cover clean cache, rename, reordering and ambiguity. Let
that concrete package own source/import and processed Runtime format/decode. Root retains
capture, aggregate budget and Scene publication; Image/TileSet keep their domain semantics.

Build an immutable base directory containing declared run configuration, startup Scene, later
Scene and processed products. A declared pass-through Runtime format is legitimate; undeclared
authoring metadata scans/importers are not. Require start and later Scene change while original
source/cache are unavailable and Cargo is absent on the run path, reproducible content digests,
wrong target/Schema/member refusal and retained last-good after a failed candidate. Keep foreign
target build-only evidence separate from actual target execution. Local CAS is not a prerequisite.

Reconcile ADR 0103's typed source/import, concrete Runtime decode and optional cook/Editor facets.
Separate Host tool, target executable and optional runner in ADR 0110; adopt only actually used
portions of Proposed 0086/0088 after design review. Move starter's fixed bins/profile/inventory
out of generic export. Replace mandatory source checks/loaders only in the new processed path;
keep explicit development/source-carrying mode without processed-catalog-to-source fallback.
Delete displaced domain registration/parser paths only after their real consumers and Editor
facets migrate, avoiding two permanent production protocols or premature caller removal.

# Disposition

Recorded as the next delivery-boundary investigation in the full foundation Goal, independently
of creation-only Schema qualification. No replacement API, selected detailed wire format,
published artifact or platform claim is introduced here. A bounded plan/ADR reconciliation and
complete consumer must precede implementation. General decoder services, remote CAS, patch/DLC
and all-target guarantees remain outside this recommendation.

# Citations

- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Integrated design](../../../../architecture/foundation-architecture-reassessment.md)
- [ADR 0103](../../../../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md)
- [ADR 0110](../../../../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)

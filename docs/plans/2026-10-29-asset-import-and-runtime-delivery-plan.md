# Asset Import Ownership And Processed Runtime Delivery

**Date:** 2026-10-29. **Status:** Source/import ownership qualified; compound Runtime delivery open.
**Baseline:** `13a2744`.
**Parent:** [Foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md), R6/R7.

## Problem

At the baseline, the compiled source-domain parser imports the root facade to name its input,
error and reservation types. Expressing a pure text importer therefore requires Scene and 2D
features. Registered project domains also rely on source preparation as their only Runtime input.
Source-carrying Export
correctly runs without the development directory, but still reads package-local source and
authoring metadata during startup and later Scene acquisition.

The [delivery audit](../knowledge/engineering/subagents/2026-10/2026-10-28T225432Z-source-carrying-export-still-lacks-an-independent-verified-runtime-content-entry-b9156a4b7e2043f09f3ab41b78cf3bf8.md)
identifies these distinct gaps. Build-machine Cargo and an explicit source-carrying mode remain
valid. Preserve actual filesystem unlink and reviewed artifact reclamation; imported project
copies remain outside installation disposal authority.

## Ordered Development

1. Move the executing source/import contract into `nara_asset` under its existing serde feature.
   Keep typed settings/product, immutable primary bytes, bounded auxiliary reads and domain
   diagnostics. Preserve the caller's concrete read errors. Migrate Check, source-backed Play,
   Editor metadata validation, source Export and independent consumers, then remove the root-owned
   parser protocol without aliases. Root retains source receipts, aggregate budgets, private
   heterogeneous storage, task custody and Scene publication.
2. Extend the independent text package with a compound text bundle and a concrete processed
   Runtime codec. Exercise two persistent subproducts and an auxiliary input. Separate product
   continuity from display name, ordering, content digest and runtime handle. If the source lacks
   intrinsic stable keys, require author-persisted continuity with explicit ambiguity handling;
   generated cache never supplies identity authority. Do not freeze a universal product executor
   or extend every asset reference before the complete consumer establishes its requirements.
3. Add one verified immutable local base catalog containing declared Runtime configuration,
   startup Scene, later Scene and processed resources. Use the existing Scene owner for a declared
   pass-through format or a specifically admitted Runtime representation. Startup and subsequent
   Scene requests capture the same base authority. Missing Runtime support or catalog failure
   refuses without source/import-cache fallback. Preserve existing transactional Scene commit and
   last-good products.
4. Separate the starter's fixed Cargo bins/profile/features and local acceptance runner from
   generic executable selection. Bind target artifacts and content compatibility explicitly.
   Foreign-target build-only evidence stays distinct from actual target execution.

Each step is a reviewable dependency unit, not completion of all R6/R7. Local CAS/group publication,
remote services, patch/DLC and universal all-platform qualification remain later work in the
foundation plan.

Step 1 is implemented as `c352614`, with independently consumed parsing, Check/Play/Scene and
Editor metadata workflows. The [qualification](../knowledge/engineering/verification/2026-10/2026-10-28T233934Z-source-import-contracts-below-project-hosts-with-typed-read-errors-e617bfdd9a104fb69bdf3a20936f7d57.md)
records actual tests, the corrected held-parser fixture, dependency proof and limits. Steps 2-4
remain open; source preparation does not imply processed Runtime support.

## Owning Decisions

[ADR 0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md) already admits
typed operation facets below root orchestration. Its source-domain refinement names the new
executing contract independently of Runtime and cook. Selecting the source facet does not select
Runtime decode or cook. [ADR 0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
continues to own explicit source-carrying delivery.

Reconcile the actually consumed identity, local catalog and build selection contracts before
steps 2-4. Proposed ADRs 0083/0086/0087/0088 are design evidence, not wholesale accepted authority.
Adopt narrowly scoped decisions only after their detailed contract and consumer are reviewable.

## Reference Constraints

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` separates source readers, processing and
the processed output loader. Its glTF index-based labels do not prove rename/reorder continuity.
Pinned Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558` exports imported resources plus Runtime
remap metadata; that metadata is distinct from authoring import settings. Its import IDs and
name/path fallbacks do not establish automatic continuity after arbitrary source edits.

Use those responsibility boundaries, not their complete watcher, processor, DCC matching or
platform-variant systems. Reference checkouts remain read-only.

## Verification

For step 1, compile the independent parser with `nara_asset` and serde only. Run its actual public
Check/Play/Scene-change tests, Editor metadata publication/cancellation tests and source Export
consumer. Cover concrete input failure propagation, domain diagnostics, reservation-before-call,
panic containment and retained generation release. Check affected feature combinations and
correctness Clippy. Run Cargo serially with the existing root target.

For subsequent delivery, require clean content builds with reproducible content digests, two
Scenes, unavailable authoring source/cache and no Cargo on the run path. Reject wrong target,
Schema, codec, duplicate identity and corrupted/missing members. Failed candidates preserve the
current admitted Scene and products. Qualify actual platform execution separately from static
checks, source-only tests and future custom-drawn Editor UI.

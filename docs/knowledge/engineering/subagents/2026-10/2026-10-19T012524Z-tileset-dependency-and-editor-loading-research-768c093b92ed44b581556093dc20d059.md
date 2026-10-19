---
type: "Subagent Finding"
title: "TileSet dependency and Editor loading research"
description: "Source-confirmed Image lifecycle gaps and a reviewed sequence for coherent TileSet publication."
timestamp: 2026-10-19T01:25:24Z
record_id: "768c093b92ed44b581556093dc20d059"
producer_id: "codex-root"
run_id: "tileset-loading-research-20261018"
git_branch: "refactor/foundation-contract-hardening"
---

# Scope

The user requested continued architecture research after completion of the U1-U5 contract goal.
Three read-only Codex lanes traced runtime reload, generic Editor loading and pinned Bevy/Godot
source. Independent source cross-check and adversarial design review followed. No Rust changes,
Cargo commands, behavioral reproductions, new implementation goal or commits were made.

# Findings And Disposition

The [research report](../../../../research/2026-10-18-tileset-dependency-and-editor-loading-research.md)
owns detailed evidence and the proposed R0-R5 sequence. Four current Image paths need regressions:
metadata dirty facts lost by coalescing, recipe loss after cross-update removal/recreation,
background hydration rejecting explicit user actions, and metadata retirement not reaching Scene
preview's typed publication. The separate strict tile-membership gap is an admission limitation;
defensive renderer skipping is intentional. These cases were source-traced, not executed.

The key design recommendation is a concrete TileSet publication retaining the exact Image content
it validated if visible last-good is selected. Copy handles stay identities. Desired and published
dependencies differ during failed A-to-B retargeting. Unresolved B retains bounded Host demand
until metadata/source observation can admit an identity. CPU sharing, candidate overlap, GPU
exact-version reuse and submitted-holder retirement require separate capacity policies.

The design reviewer found two initial omissions in those unresolved-dependency and retention
policies. Both were incorporated and the final review reported no material remaining decision
gap, at confidence 75. That is a review judgment, not passing implementation evidence. The optional
user preference between visible last-good and temporarily unavailable TileSets was requested;
the report recommends the former but does not turn it into an Accepted ADR.

The standalone HTML report outside the repository was updated. Its original seventeen findings
and historical results remain separate from this follow-up. Documentation checks found 108 local
Markdown links and 79 local HTML links valid, no duplicate IDs or broken anchors, and no whitespace
errors. The first new HTML research link was corrected to the actual workspace file before the
final check.

# Next Action

Use R0 as the next implementation scope: behavioral regressions and owning Image/runtime fixes
before extending their policy into TileSet execution. R1-R3 require concrete admission and
publication decisions; R4 gates layout-changing edits and brushes. Generic Editor TileSet
hydration and in-place TileSet reload remain unsupported, not current regressions. The completed
previous goal and its test results retain their recorded scope.

# Citations

- [Research and acceptance matrix](../../../../research/2026-10-18-tileset-dependency-and-editor-loading-research.md)
- [Follow-up assessment](../../../../audits/2026-10-18-next-foundation-refactor-assessment.md)
- [Previous implementation evidence](../../verification/2026-10/2026-10-18T164837Z-component-plugin-and-asset-contract-refactor-b4c2497eff5d4bdb90cebe1de995ceb3.md)
- [Concrete asset execution contract](../../../../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md)
- [Editor Image contract](../../../../architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md)

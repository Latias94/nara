---
type: "Work Registration"
title: "Editor Image Import and Reimport v1: EIR-U5 authoring publication active"
description: "Records EIR-U4 completion and activates only same-generation UI authoring publication and referenced-image hydration."
timestamp: 2026-08-20T12:15:02Z
record_id: "245355740b684b4d93569c53f383d8e4"
status: "active"
producer_id: "codex-root"
run_id: "eir-u5-authoring-publication-20260820"
related_plan: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6797956"
supersedes: "369ef53ed4104ca88f11a30a9449643b"
registration_id: "editor-image-import-reimport-v1-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
---

# Scope

Execute EIR-U5 only. EIR-U6, the native product trajectory, and unrelated Editor successors remain
inactive.

# Current Claim

EIR-U4 completed across `1620786` and correction `6797956` with a closed UI-neutral image import
projection, one private egui Inspector draft, exact frame-bound Host ingress, target-bound fallback
facts, honest source-verification state, and bounded native polling that returns to idle after
terminal or blocked reconciliation states.

# Latest Links

- docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md
- docs/knowledge/engineering/verification/2026-08/2026-08-20T121501Z-eir-u4-image-import-inspector-and-native-wake-d109ae1760a2463a862ffd4c0148caaf.md

# Handoff

Implement EIR-U5 from the accepted Editor image-store facts. Publish image choices and Canvas
prepared resources from one immutable authoring generation, preserve last complete projection on
failure, and hydrate only images referenced by the accepted generic-authoring closure. Do not begin
the native end-to-end trajectory or add a generic importer registry, watcher, cache, or plugin lane.

# Citations

- `1620786` (`feat(editor): add image import inspector`)
- `6797956` (`fix(editor): bind image import fallback facts`)
- `d109ae1760a2463a862ffd4c0148caaf` (EIR-U4 verification)
- `docs/architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md`

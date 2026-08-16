---
type: "Work Registration"
title: "Editor Scene Composition and Scene View v1: ESV-U3 active"
description: "Closes ESV-U2 at commit 74dc037 and activates only the private edit-preview target and Host transaction."
timestamp: 2026-08-16T09:17:46Z
record_id: "736273e588c9490fb1ad9594211c7c98"
resource: "docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md"
tags: ["editor", "scene-view", "preview", "esv-u3", "active"]
status: "active"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "74dc037"
supersedes: "ad1c66d7dec742b59507f64ba14cd951"
registration_id: "editor-scene-composition-view-v1-codex-root"
source_workspace: "F:/SourceCodes/Rust/nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-16T091730Z-esv-u2-revision-bound-scene-composition-2ea0e3e881ff4c6a9d98e52e0937445a.md"
---

# Scope

ESV-U3 only. ESV-U4 and ESV-U5 remain inactive and dependency-ordered.

# Current Claim

ESV-U2 is verified at commit 74dc037. ESV-U3 is the sole active unit for private edit-world materialization, exact preview candidates, target lease publication, and bounded recovery.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-16T091730Z-esv-u2-revision-bound-scene-composition-2ea0e3e881ff4c6a9d98e52e0937445a.md

# Handoff

Implement ESV-U3 only. Materialize the accepted document into a dedicated preview World, preserve SceneEntityId through ordered sprite/hit facts, and publish a target lease only from the exact U1 composite receipt. Keep egui composition controls, pointer picking, Game View, docking, and general render scheduling inactive.

# Citations

- `docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md`
- `docs/architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-16T091730Z-esv-u2-revision-bound-scene-composition-2ea0e3e881ff4c6a9d98e52e0937445a.md`
- Commit `74dc037`

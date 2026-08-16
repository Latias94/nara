---
type: "Work Registration"
title: "Editor Scene Composition and Scene View v1: ESV-U2 active"
description: "Closes ESV-U1 at commit 1f97c48 and activates only revision-bound Scene Composition and canonical tree actions."
timestamp: 2026-08-16T08:49:17Z
record_id: "ad1c66d7dec742b59507f64ba14cd951"
resource: "docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md"
tags: ["editor", "scene-view", "scene-composition", "esv-u2", "active"]
status: "active"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "1f97c48"
supersedes: "68dab7e400b14d59b3deec1be3f63df4"
registration_id: "editor-scene-composition-view-v1-codex-root"
source_workspace: "F:/SourceCodes/Rust/nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-16T084916Z-esv-u1-editor-scene-view-execution-contract-1de9903b1b7b4623a0633b2a6565e0b0.md"
---

# Scope

ESV-U2 only. ESV-U3 through ESV-U5 remain inactive and dependency-ordered.

# Current Claim

ESV-U1 is verified at commit `1f97c48`. ESV-U2 is the sole active unit for revision-bound Scene
Composition, canonical hierarchy projection, single-owner selection, and bounded undoable actions
in the existing Scene and Tooling owners.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-16T084916Z-esv-u1-editor-scene-view-execution-contract-1de9903b1b7b4623a0633b2a6565e0b0.md

# Handoff

Implement ESV-U2 only. Reuse existing Scene patch/history/persistence authorities; add create,
restore, admitted component add/remove, and cycle-safe move/reparent actions with exact document
and base revision. Keep WGPU preview materialization, egui Scene View, picking, Game View, docking,
and general registries inactive.

# Citations

- `docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md`
- `docs/architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-16T084916Z-esv-u1-editor-scene-view-execution-contract-1de9903b1b7b4623a0633b2a6565e0b0.md`
- Commit `1f97c48`

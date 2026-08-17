---
type: "Work Registration"
title: "Editor UI Prefab Authoring and Canvas v1"
description: "Work Registration for Editor UI Prefab Authoring and Canvas v1."
timestamp: 2026-08-17T14:59:54Z
record_id: "a234ba7c987f4e22b2e5756269470b3d"
status: "active"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "dc7ea07"
supersedes: "fdc10a41bc044ad5b3a93ed662abf4d8"
registration_id: "editor-ui-prefab-authoring-canvas-v1-codex-root"
---

# Scope

EUA-U2 through EUA-U5

# Current Claim

EUA-U1 is complete at dc7ea07; EUA-U2 is the sole active implementation unit.

# Latest Links

- [EUA-U1 verification](../../verification/2026-08/2026-08-17T145954Z-eua-u1-prefab-document-workspace-and-persistence-authority-e6b43a9f24ed42deb3c8d19a03a55cdf.md)

# Handoff

Implement the immutable toolkit-neutral EditorUiAuthoringFrame, typed field descriptors, and bounded Begin/Preview/Commit/Cancel gesture transaction. Keep egui and WGPU ownership out of the shared seam; use focused representative tests.

# Citations

- [Editor UI Prefab Authoring and Canvas v1 plan](../../../../plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)

---
type: "Work Registration"
title: "Reference-game-driven foundation refactor: RGF-U13"
description: "Record the physical-input playability correction while retaining the human Windows hand-feel check as the final U13 gate."
timestamp: 2026-07-20T04:41:52Z
record_id: "63bbfbc8a6544b719c605234b3f58c5d"
tags: ["rgf-u13", "reference-game", "desktop", "input", "playability"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6c8d28b"
supersedes: "437f122f2a8143f18470951cd1741a80"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/progress/2026-07/2026-07-20T044044Z-rgf-u13-physical-input-playability-correction-6ffe86e749164d568ae28e8bb9fe0dc4.md"
---

# Scope

RGF-U13 desktop-profile startup, physical input, render/HUD, Retry, truthful shutdown, and a human-observable control window.

# Current Claim

Commit 6c8d28b corrects the imperceptibly short post-input window and all automated plus Win32 pixel probes pass. U13 remains active until the user confirms WASD/Retry/close hand feel.

# Latest Links

- docs/knowledge/engineering/progress/2026-07/2026-07-20T044044Z-rgf-u13-physical-input-playability-correction-6ffe86e749164d568ae28e8bb9fe0dc4.md

# Handoff

Ask the user to test the running Windows binary. On a positive manual result, record final U13 verification, complete this registration lineage, and activate U14; otherwise keep U13 active and refine the desktop cadence or projection.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u13-complete-the-desktop-input-and-render-wave`
- `docs/knowledge/engineering/progress/2026-07/2026-07-20T044044Z-rgf-u13-physical-input-playability-correction-6ffe86e749164d568ae28e8bb9fe0dc4.md`
- Commit `6c8d28b`

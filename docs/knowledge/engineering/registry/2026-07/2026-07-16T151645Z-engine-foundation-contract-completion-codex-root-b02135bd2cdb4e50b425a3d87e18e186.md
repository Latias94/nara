---
type: "Work Registration"
title: "Reference-game-driven foundation refactor"
description: "Execute the reference-game-driven successor plan with explicit schedule-anchor and persistent-composition gates."
timestamp: 2026-07-16T15:16:45Z
record_id: "b02135bd2cdb4e50b425a3d87e18e186"
status: "active"
producer_id: "codex-architecture-review"
run_id: "session-2026-07-16-architecture-review"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "559a54d"
supersedes: "61e0215a3ccb43eba92e2e06ea5360c9"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/subagents/2026-07/2026-07-16-bevy-godot-early-architecture-research.md"
---

# Scope

Continue the active RGF implementation. RGF-U28 proves public semantic schedule/set anchors before U6; RGF-U29 proves explicit persistent component composition before U12. Trigger-gated Editor, package, localization, settings, and schema metadata work remains outside the first-playable critical path.

# Current Claim

RGF-U4 is landed and RGF-U5 correction remains the current implementation lane. The plan now adds RGF-U28 after U4 before U6 and RGF-U29 after U4 before U12; neither reopens landed U4.

# Latest Links

- docs/knowledge/engineering/subagents/2026-07/2026-07-16-bevy-godot-early-architecture-research.md
# Handoff

The existing implementation Codex should finish its admitted current unit, then follow the revised dependency table. Do not begin U12 before U29 or expand reference-game schedule dependencies in U6 before U28. Read ADRs 0003, 0006, 0081 and their ledger rows first.

# Citations

---
type: "Work Registration"
title: "Reference-game-driven foundation refactor"
description: "Execute the reference-game-driven successor plan with exact schedule-anchor and two-phase persistent-apply gates."
timestamp: 2026-07-16T16:39:14Z
record_id: "daa83b6521074f95b4a62fbd838a2425"
status: "active"
producer_id: "codex-architecture-review"
run_id: "session-2026-07-16-architecture-review"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "559a54d"
supersedes: "b02135bd2cdb4e50b425a3d87e18e186"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/subagents/2026-07/2026-07-16-bevy-godot-early-architecture-research.md"
---

# Scope

Continue the active RGF implementation. RGF-U28 proves the exact four public semantic anchors before U6. RGF-U12 and RGF-U29 proceed in parallel after U4, then converge before U26 first materializes content into a target World. Trigger-gated Editor transport, package implementation, localization, settings, and schema metadata remain outside the first-playable critical path.

# Current Claim

RGF-U4 is landed and RGF-U5 correction remains the current implementation lane. The revised plan adds RGF-U28 after U4 before U6, while RGF-U12 and RGF-U29 are independent post-U4 lanes that both gate RGF-U26; neither reopens landed U4.

# Latest Links

- docs/knowledge/engineering/subagents/2026-07/2026-07-16-bevy-godot-early-architecture-research.md
# Handoff

Finish the currently admitted implementation unit, then follow the revised dependency table. U12 and U29 may proceed independently after U4, but U26 must wait for both and must exercise U29 target-World checks; U6 must wait for U25 Continue and U28. Read ADRs 0003, 0006, 0081 and their ledger rows first.

# Citations

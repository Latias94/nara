---
type: "Work Registration"
title: "Reference-game-driven foundation refactor: RGF-U17"
description: "Activate the honest editor Save, reopen, and Host-owned Play product loop after U13 completion."
timestamp: 2026-07-20T13:18:27Z
record_id: "cf84977808624c2b8fb53ecd3b1d5c66"
tags: ["rgf-u17", "editor", "persistence", "play", "tooling"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "71cc7c18f3b15900145ea1cd9e89366b7e1679e0"
registration_id: "reference-game-foundation-rgf-u17-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
---

# Scope

RGF-U17 known-schema editor edit, receipt-backed Save, dirty close/reopen, Host-owned scheduled Play, Pause, exact StepFixedTick, Stop, Restart, and bounded retirement.

# Current Claim

U13 and U24 satisfy the dependencies. ScenePlaySession still owns a bare World, MarkSaved can claim persistence without I/O, and dirty close/reopen plus concrete Host lifecycle commands remain unimplemented.

# Latest Links

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u17-complete-honest-editor-save-reopen-and-play`

# Handoff

Characterize the current tooling workspace and play contracts, then replace false save and bare-World ownership through the smallest dependency-ordered U17 slices without editing the active plan body.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u17-complete-honest-editor-save-reopen-and-play`
- `docs/knowledge/engineering/verification/2026-07/2026-07-20T120801Z-rgf-u13-desktop-first-playable-final-verification-88640a2745fa4e439483d35df4b9a219.md`
- Commit `71cc7c1`

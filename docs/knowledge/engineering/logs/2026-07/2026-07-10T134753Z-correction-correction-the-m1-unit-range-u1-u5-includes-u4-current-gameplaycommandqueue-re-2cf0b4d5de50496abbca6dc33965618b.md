---
type: "Memory Event"
title: "Correction: Correction: the M1 unit range U1-U5 includes U4. Current GameplayCommandQueue re"
description: "Correction: the M1 unit range U1-U5 includes U4. Current GameplayCommandQueue remains frame-oriented and is cleared in CoreStage::Last, so U"
timestamp: 2026-07-10T13:47:53Z
record_id: "2cf0b4d5de50496abbca6dc33965618b"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "423e0f5"
event_kind: "Correction"
---

# Event

Correction: the M1 unit range U1-U5 includes U4. Current GameplayCommandQueue remains frame-oriented and is cleared in CoreStage::Last, so U4 must be implemented before the M1 continue/revise/abort gate.

# Impact

# Citations

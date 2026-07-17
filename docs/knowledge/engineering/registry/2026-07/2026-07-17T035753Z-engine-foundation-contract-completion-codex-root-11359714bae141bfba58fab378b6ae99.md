---
type: "Work Registration"
title: "Reference-game-driven foundation refactor"
description: "Reopen RGF-U5 after a safe Bevy change-detection bypass invalidated the first closure review."
timestamp: 2026-07-17T03:57:53Z
record_id: "11359714bae141bfba58fab378b6ae99"
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6e7dbfb"
supersedes: "a08a5a1abe424c2b94cdf130a2667d95"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/subagents/2026-07/2026-07-17-rgf-u5-runtime-closure-review.md"
---

# Scope

Re-close RGF-U5 before admitting the next implementation unit.

# Current Claim

The first U5 closure is superseded for RV-03 only: raw managed World mutation could bypass epoch detection through Bevy bypass_change_detection. Structural scope sealing and final verification are in progress; ADR 0084 remains Proposed.

# Latest Links

- docs/knowledge/engineering/subagents/2026-07/2026-07-17-rgf-u5-runtime-closure-review.md
# Handoff

Do not enter RGF-U28/U12/U29 until the structural scope fix, independent review, verification evidence, ledger correction, and precise commit are complete.

# Citations

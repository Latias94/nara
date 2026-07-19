---
type: "Work Registration"
title: "Reference-game-driven foundation refactor: RGF-U13"
description: "Record automated completion of the desktop production wave while retaining the manual Windows play check as the final U13 gate."
timestamp: 2026-07-19T19:38:16Z
record_id: "437f122f2a8143f18470951cd1741a80"
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "198a680"
supersedes: "efb5a01385c3477386810dfa9a159aef"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-07/2026-07-19T192736Z-rgf-u13-desktop-production-wave-automated-verification-9d99f3f9c3c2450ea98dfa0952a1ded3.md"
---

# Scope

RGF-U13 desktop-profile startup, ordered physical input, single-target rendering, HUD, Retry, and truthful shutdown.

# Current Claim

Implementation commit 198a680 and all automated gates pass. U13 remains active only because the documented human Windows play check has not yet been performed.

# Latest Links

- docs/knowledge/engineering/verification/2026-07/2026-07-19T192736Z-rgf-u13-desktop-production-wave-automated-verification-9d99f3f9c3c2450ea98dfa0952a1ded3.md
# Handoff

Run the reference-game desktop binary, verify WASD, HUD/terminal rendering, Enter Retry, and normal close, then close U13 and activate U14. Do not reopen U13 for watcher, global-runtime, packet-cost, or governance work; those are routed to U8, U23, U14, and post-RGF cleanup.

# Citations

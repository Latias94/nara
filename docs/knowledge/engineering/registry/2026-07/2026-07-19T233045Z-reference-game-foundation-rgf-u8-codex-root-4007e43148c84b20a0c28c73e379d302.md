---
type: "Work Registration"
title: "Reference-game-driven foundation refactor: RGF-U8"
description: "Close domain-owned asset task integration and bounded observable watcher admission at implementation commit 60292e7."
timestamp: 2026-07-19T23:30:45Z
record_id: "4007e43148c84b20a0c28c73e379d302"
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "60292e7"
supersedes: "8112eb5e569940a09e7f1f7df245b251"
registration_id: "reference-game-foundation-rgf-u8-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-07/2026-07-19T233045Z-rgf-u8-domain-owned-task-integration-verification-67f01d8db42b423ba82110d3f00633f7.md"
---

# Scope

RGF-U8 task integration ownership, Poll-entry cutoffs, and asset watcher admission/observability.

# Current Claim

Implementation commit `60292e7` and all U8 gates pass. `nara_asset` owns its four integration
phases, `nara_tasks` remains domain-neutral, and watcher loss is bounded, sticky, and observable.

# Latest Links

- docs/knowledge/engineering/verification/2026-07/2026-07-19T233045Z-rgf-u8-domain-owned-task-integration-verification-67f01d8db42b423ba82110d3f00633f7.md

# Handoff

Keep automatic watcher recovery deferred until a Host-authorized full-rescan workflow exists. The
U13 human Windows play gate remains active; RGF-U19 may run independently without reopening U8.

# Citations

- docs/architecture/adr/0080-domain-owned-task-update-integration-sets.md
- docs/migrations/2026-07-engine-foundation.md#rgf-u8-1-domain-owned-task-integration-and-bounded-watcher-admission
- commit `60292e7`

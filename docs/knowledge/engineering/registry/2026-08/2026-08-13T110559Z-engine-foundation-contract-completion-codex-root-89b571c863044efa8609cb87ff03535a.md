---
type: "Work Registration"
title: "Presented UI evidence correction: PUEC-U3 active"
description: "Records receipt-bound presented observation conformance and advances the correction to post-readiness product measurement."
timestamp: 2026-08-13T11:05:59Z
record_id: "89b571c863044efa8609cb87ff03535a"
tags: ["runtime-ui", "puec-u2", "puec-u3", "wgpu", "presented-evidence"]
status: "active"
producer_id: "codex-root"
run_id: "presented-ui-evidence-correlation-correction"
related_plan: "docs/plans/2026-08-13-001-refactor-presented-ui-evidence-correlation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "89aadf2"
supersedes: "3b54121221f24d2280c683d38ce79a76"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-13T110513Z-puec-u2-receipt-bound-presented-observation-97aa78c6bbf74fada0b7d99219e271a6.md"
---

# Scope

Replace the Reference Game candidate's cross-resource joins with a bounded post-readiness state machine consuming the exact owner-produced observation.

# Current Claim

PUEC-U1 is complete at aed0729 and PUEC-U2 is complete at 89aadf2; PUEC-U3 is the sole active unit. RUP-U3 remains paused without changing its pinned UI Foundation or Product Composition baselines.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-13T110513Z-puec-u2-receipt-bound-presented-observation-97aa78c6bbf74fada0b7d99219e271a6.md
# Handoff

Implement AwaitPresentedReady -> Warmup -> Measure -> Seal in desktop_candidate_smoke. Pre-ready samples are discarded; each retained sample must match the observation's backend/device generation and exact Render transaction.

# Citations

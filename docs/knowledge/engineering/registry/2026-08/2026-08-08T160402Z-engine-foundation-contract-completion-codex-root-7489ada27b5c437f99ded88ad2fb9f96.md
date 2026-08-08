---
type: "Work Registration"
title: "UI Foundation correction: UIF-U3 presentation closure active"
description: "Makes target presentation resource-complete, atomically budgeted, capability-closed, and commit-authoritative."
timestamp: 2026-08-08T16:04:02Z
record_id: "7489ada27b5c437f99ded88ad2fb9f96"
status: "active"
producer_id: "codex-root"
run_id: "uif-u3"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2fde103"
supersedes: "15bf5064d7f1471aa3dc1f1cf4021f2b"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-08T160401Z-uif-u2-root-envelope-and-order-safe-fallback-5dfa71eb2270485699155b13fb595f07.md"
---

# Scope

UIF-U3 only: early candidate text-work admission, one atomic target presentation plan, complete
packet-owned image/glyph resources, owner-local hard budgets, bounded uniform radii, exact backend
retry generations, and a backend-issued one-shot presentation commit.

# Current Claim

UIF-U0 through UIF-U2 are complete at `2fde103`. UIF-U3 is the sole active unit. UIF-U4/UIF-U5 and
both product successor plans remain inactive.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-08T160401Z-uif-u2-root-envelope-and-order-safe-fallback-5dfa71eb2270485699155b13fb595f07.md

# Handoff

Replace the stateful per-packet builder path with one deep presentation planner that preflights a
complete target attempt, stages atlas/resource mutations, and commits planner state plus packet
publication only after all content and capability checks pass. Enforce kernel text work before
measurement callbacks can multiply shaping, target-plan primitive/resource/atlas/retention limits
once, and concrete backend upload/instance/residency limits independently. `ContentRejected` and
`Deferred` must retain last-presented state and cannot advance a receipt. Implement the already
admitted bounded uniform radius for fill/image/border, and make backend acceptance return a
non-constructible one-shot commit whose consumption is the only receipt publication path. Do not
add a RenderGraph, capability registry, generic provider seam, or same-target multi-surface policy.

# Citations

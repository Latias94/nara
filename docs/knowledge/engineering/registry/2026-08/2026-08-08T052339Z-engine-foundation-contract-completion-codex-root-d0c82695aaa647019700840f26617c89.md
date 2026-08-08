---
type: "Work Registration"
title: "Ordered UI kernel correction: NUI-U15B product integration active"
description: "Activates bounded Reference Game UI product integration against the frozen NUI-U15A public inventory and semantic baseline."
timestamp: 2026-08-08T05:23:39Z
record_id: "d0c82695aaa647019700840f26617c89"
status: "active"
producer_id: "codex-root"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3c81645"
supersedes: "b47fe204c2fb4706a00429c0d8ee86c8"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "verification/2026-08/2026-08-08T051354Z-nui-u15a-neutral-ui-conformance-and-public-boundary-freeze-32535c96ab3b49d3affa4ecdcbb22e39.md"
---

# Scope

NUI-U15B owns the bounded Reference Game UI product integration only: six capture states, the separate paused lifecycle condition, representative physical targets, pointer/keyboard/gamepad/accessibility intent parity, package launch, and presentation-free headless parity through the frozen U15A interfaces.

# Current Claim

NUI-U15A is complete at 3c81645 and its public inventory, named invariants, exact assertions, and proving commit are frozen by verification record 32535c96ab3b49d3affa4ecdcbb22e39. NUI-U15B is the sole active unit. Any shared UI behavior, public-interface, or U15A assertion change stops the affected path and reopens U15A; product-only needs remain in Reference Game data or adapters.

# Latest Links

- verification/2026-08/2026-08-08T051354Z-nui-u15a-neutral-ui-conformance-and-public-boundary-freeze-32535c96ab3b49d3affa4ecdcbb22e39.md

# Handoff

Implement the bounded Reference Game states, layouts, inputs, package path, and headless parity through the frozen interfaces. Reuse U15A structural, routing, receipt, resource, and accessibility proofs instead of multiplying smoke, digest, hash, pixel, host, resolution, or input matrices. Do not redesign engine modules unless independent evidence satisfies R45 and the affected path is formally reopened.

# Citations

- `3c81645 feat(ui)!: freeze conformance boundaries`
- `docs/knowledge/engineering/verification/2026-08/2026-08-08T051354Z-nui-u15a-neutral-ui-conformance-and-public-boundary-freeze-32535c96ab3b49d3affa4ecdcbb22e39.md`
- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u15b-complete-the-reference-game-product-integration`

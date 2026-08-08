---
type: "Work Registration"
title: "UI Foundation correction: UIF-U2 root envelopes active"
description: "Makes root-local failure independent while preserving global identity, current order, inert fallback, and modal safety."
timestamp: 2026-08-08T14:20:33Z
record_id: "15bf5064d7f1471aa3dc1f1cf4021f2b"
status: "active"
producer_id: "codex-root"
run_id: "uif-u2"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d43de14"
supersedes: "f2c14c24f0db45639dd4ae460129c852"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-08T142032Z-uif-u1-presentation-reconciliation-correction-055102d378e94c819ef419be51d29159.md"
---

# Scope

UIF-U2 only: whole-snapshot preflight, ordered root envelopes, root-local visual fallback rebound to
the current layer/ordinal, inert retained content, modal barrier safety, and candidate-wide compile
budgets.

# Current Claim

UIF-U1 is complete at `d43de14`. UIF-U2 is the sole active unit. UIF-U3 through UIF-U5 and both
product successor plans remain inactive.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-08T142032Z-uif-u1-presentation-reconciliation-correction-055102d378e94c819ef419be51d29159.md

# Handoff

Split whole-snapshot identity/capacity admission from root-local validation before any compiler
revision, cache or LRU mutation. Preserve every known root envelope and its current structural
ordinal even when its body fails. Cache root-local layout/paint facts only, rebind them through the
current layer and ordinal, and make fallback inert by construction. Current modal membership and
barrier policy belong to the envelope; an invalid modal with retained visuals must still block
underlay input, while an invalid barrier with no safe fallback rejects the candidate and preserves
the prior presented session. Reuse the existing compiler/snapshot Interface and neutral
`layout_gallery`; do not add normalized IR, provider registries or a new example framework.

# Citations

---
type: "Work Registration"
title: "UI Foundation correction: UIF-U1 presentation reconciliation reopened"
description: "Closes remove/reinsert state resurrection and makes required lifecycle reconciliation precede input rejection."
timestamp: 2026-08-08T11:42:25Z
record_id: "f2c14c24f0db45639dd4ae460129c852"
status: "active"
producer_id: "codex-root"
run_id: "uif-u1-correction"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d397ed1"
supersedes: "43dccd4631f7494a94ffeb7f67271798"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-08T113450Z-uif-u1-atomic-presented-session-interaction-d13d3f5c4c68440fa5ddacb88ee64eab.md"
---

# Scope

UIF-U1 correction only: presentation-time reconciliation for removed or changed controls,
remove/reinsert state retirement, and cleanup precedence over otherwise rejected input.

# Current Claim

The first UIF-U1 evidence at c2806a5 is historical but not the current conformance baseline. UIF-U2
is suspended until this focused correction is implemented, verified and recorded.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-08T113450Z-uif-u1-atomic-presented-session-interaction-d13d3f5c4c68440fa5ddacb88ee64eab.md

# Handoff

Move presented-frame reconciliation to the owning publication transition, retire widget reducer
state for removed controls before stable-key reuse, and ensure required cleanup commits before an
unsupported or capacity-rejected input is classified.

# Citations

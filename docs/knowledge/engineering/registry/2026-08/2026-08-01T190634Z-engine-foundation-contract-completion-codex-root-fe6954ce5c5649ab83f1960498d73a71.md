---
type: "Work Registration"
title: "Reference-game spatial authority: RGS-U3 active"
description: "Closes RGS-U2 at the reviewed runtime hierarchy boundary and activates completed 2D transform projection."
timestamp: 2026-08-01T19:06:34Z
record_id: "fe6954ce5c5649ab83f1960498d73a71"
tags: ["rgs-u2", "rgs-u3", "spatial-authority", "active"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "51b3fe45d3e8c5525f4e2d83f996545854e62a5a"
supersedes: "510bd4496c5b48fb9b411b5d06b10ea2"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-01T190359Z-rgs-u2-runtime-hierarchy-boundary-dd74d84c7e3b487b999a890d6f5485f9.md"
---

# Scope

Continue the focused spatial-authority plan from the verified runtime hierarchy into completed 2D global projection and consumer migration.

# Current Claim

RGS-U2 is complete at 51b3fe4. RGS-U3 is the sole active implementation unit; parented Transform2d and inherited Visibility remain fail-closed until all world-space consumers use completed globals.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-01T190359Z-rgs-u2-runtime-hierarchy-boundary-dd74d84c7e3b487b999a890d6f5485f9.md
# Handoff

Implement RGS-U3 transform propagation, freshness barriers, and camera/sprite/tilemap migration. Do not expand into persistent order, visibility inheritance, runtime reparent, 3D, physics, or release evidence.

# Citations

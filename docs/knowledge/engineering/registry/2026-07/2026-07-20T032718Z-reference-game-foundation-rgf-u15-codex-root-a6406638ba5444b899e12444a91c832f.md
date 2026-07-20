---
type: "Work Registration"
title: "Reference-game-driven foundation refactor: RGF-U15"
description: "Record local CI implementation at 188a493 while hosted Windows/Linux evidence remains pending."
timestamp: 2026-07-20T03:27:18Z
record_id: "a6406638ba5444b899e12444a91c832f"
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "188a493db69be89692f0f89e4a10b5e59ff27a94"
supersedes: "051981e6878b45fa8ad1dc9e311bcf9e"
registration_id: "reference-game-foundation-rgf-u15-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/progress/2026-07/2026-07-20T032511Z-rgf-u15-local-three-workspace-ci-progress-4d51f0975e544d819969109f52b2e546.md"
---

# Scope

RGF-U15 minimum disposable hosted CI for root, reference-game, and module-consumer workspaces.

# Current Claim

Implementation commit `188a493` and all local equivalents pass. The workflow and mutation-tested
policy are present, but U15 remains active because no disposable hosted Windows/Linux run has been
observed.

# Latest Links

- docs/knowledge/engineering/progress/2026-07/2026-07-20T032511Z-rgf-u15-local-three-workspace-ci-progress-4d51f0975e544d819969109f52b2e546.md

# Handoff

Push or open a PR only with user authorization, retain the resulting six-job Hosted evidence, then
close U15 if both operating systems pass. Do not claim U7 packaging or U21 publication. U13 still
requires manual Windows WASD and Retry confirmation before U14 may start.

# Citations

- docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u15-establish-minimum-three-workspace-ci-feedback
- .github/workflows/ci.yml
- tests/ci_policy.rs
- commit `188a493`

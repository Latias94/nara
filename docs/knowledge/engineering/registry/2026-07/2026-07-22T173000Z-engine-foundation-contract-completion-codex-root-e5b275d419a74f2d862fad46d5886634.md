---
type: "Work Registration"
title: "Reference-game runtime authority and delivery: RGD-U5"
description: "Prove real Headless, Desktop, and Editor Host semantic parity with one bounded reference-game command stream."
timestamp: 2026-07-22T17:30:00Z
record_id: "e5b275d419a74f2d862fad46d5886634"
tags: ["rgd-u5", "reference-game", "host-parity", "headless", "desktop", "editor"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "549d5c25a4091585c8cca3dc51e1f7748fd2cd9d"
supersedes: "fff63256432e49e0a55f0927cb5cb681"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
---

# Scope

RGD-U5 real Headless, Desktop, and Editor product-Host parity for one bounded reference-game
semantic command stream and stable snapshot envelope.

# Current Claim

Active: construct the parity oracle through only public Host/product APIs and compare the bounded
semantic envelope emitted by isolated child processes.

# Latest Links

- docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md
- docs/knowledge/engineering/verification/2026-07/2026-07-22T172958Z-rgd-u4-fresh-runtime-session-reconstruction-verification-0714cd8b40bb48d2a76a03ab75bde09d.md

# Handoff

Begin with failing public-path parity tests. Keep Desktop Winit execution on the child process main
thread, bound timeouts and cleanup, and reject private Host/Runtime construction, raw World reads,
generic observation buses, and release-sized evidence envelopes.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u5-prove-real-three-host-semantic-parity`
- Commit `549d5c2`

---
type: "Work Registration"
title: "Ordered UI kernel correction: NUI-U17 runtime ownership checkpoint active"
description: "Closes reviewed NUI-U14 packet authority and activates the deep runtime UI ownership checkpoint before reference-product expansion."
timestamp: 2026-08-06T15:58:45Z
record_id: "b5ccca73e9b34f4b8168cbda5a92ad83"
status: "active"
producer_id: "codex-root"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4a79e00"
supersedes: "51cc4a4fed354e57b12d4d6c16c4c61e"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "verification/2026-08/2026-08-06T052759Z-nui-u14-owned-ui-and-image-packet-through-wgpu-059e76de3be74d5894fe41135b3c7670.md"
---

# Scope

NUI-U17 deep runtime UI ownership: per-surface candidate and presented sessions, exact immutable
behavior snapshots, fail-closed arbitration, pause-safe bounded intent delivery, explicit route
ownership, and private lifecycle modules.

# Current Claim

NUI-U13, NUI-U5B, and NUI-U14 are committed and reviewed. Renderer-neutral authoring, explicit
content/presentation composition, resource-complete packets, last-good packet retention,
fail-closed clipping, one Sprite image identity authority, and closed WGPU outcomes are in place.
NUI-U15 is blocked until NUI-U17 closes the runtime lifecycle and interface invariants without
introducing Reference Game dependencies into engine crates.

# Latest Links

- verification/2026-08/2026-08-06T052759Z-nui-u14-owned-ui-and-image-packet-through-wgpu-059e76de3be74d5894fe41135b3c7670.md
- docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u17-deepen-runtime-ui-ownership-and-interfaces

# Handoff

Split `nara_ui` runtime responsibilities into private lifecycle modules before adding product UI.
Bind behavior to the exact presented frame and receipt, isolate interaction state by surface,
publish typed bounded intents through a pause-safe journal, make route conflict and retirement
explicit, and keep technical schedule stages private. Use focused UI/input/gameplay tests; do not
expand the reference-game matrix, smoke binaries, hashes, or scripts during this unit.

# Citations

- `be6fa05 docs(ui): define runtime ownership checkpoint`
- `c1226ec feat(ui)!: split content and presentation ownership`
- `4a79e00 fix(render): preserve exact packet authority`

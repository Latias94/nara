---
type: "Work Registration"
title: "Ordered UI kernel correction: NUI-U10 frame compiler active"
description: "Records NUI-U9 bounded text closure and activates the renderer-neutral immutable UI frame compiler."
timestamp: 2026-08-05T11:11:50Z
record_id: "47e5678cceeb49de8736c56c11c83d4d"
status: "active"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "95e79e853a25fa922d1715f0326b1ffa1b360056"
supersedes: "91b96c611e684005a83a7e774317c02e"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "verification/2026-08/2026-08-05T111150Z-nui-u9-bounded-deterministic-text-module-9b67d709856c4004b32df7db37c0d2ae.md"
---

# Scope

NUI-U10 renderer-neutral immutable UI frame compiler and headless snapshot proof

# Current Claim

NUI-U9 is complete: exact font provenance, bounded deterministic shaping/measurement/raster output,
no system discovery, Rust 1.95 compatibility, and independent direct consumption are proven.

# Latest Links

- verification/2026-08/2026-08-05T111150Z-nui-u9-bounded-deterministic-text-module-9b67d709856c4004b32df7db37c0d2ae.md

# Handoff

Implement NUI-U10 proof-first as a deep `nara_ui_kernel` module. Consume persistent hierarchy
order and `nara_text` measurement through Nara-owned types; keep ECS, Scene, Host, window, WGPU,
Taffy nodes, compiler IR, and cache internals outside the public boundary. Prove stable structured
keys, bounded fallible snapshot construction, Flex/Absolute logical layout, one immutable frame for
paint/hit/focus/semantics, root-local atomic failure, exact last-good compatibility, and presented
receipt matching. Preserve the concurrent user-owned files and keep Cargo serial.

# Citations

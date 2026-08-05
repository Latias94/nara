---
type: "Work Registration"
title: "Ordered UI kernel correction: NUI-U11 input timeline active"
description: "Records NUI-U10 immutable frame-kernel closure and activates the bounded normalized input timeline."
timestamp: 2026-08-05T12:22:12Z
record_id: "3c1ccdb07e28445fb62b0e0c68176d20"
status: "active"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3fc486a016c7c894d7a11413e2df769be6f6b13e"
supersedes: "47e5678cceeb49de8736c56c11c83d4d"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "verification/2026-08/2026-08-05T122212Z-nui-u10-renderer-neutral-immutable-ui-frame-kernel-524095a3d0614d7390aa2b2f8e8d3442.md"
---

# Scope

NUI-U11 normalized input timeline and presented-route correlation

# Current Claim

NUI-U10 is complete: exact structured root-scoped identities, bounded fallible authoring,
renderer-neutral Flex/Absolute layout, one immutable frame for paint/hit/focus/semantics,
root-local atomic failure, inert exact last-good visuals, and exact submitted-packet receipts are
proven.

# Latest Links

- verification/2026-08/2026-08-05T122212Z-nui-u10-renderer-neutral-immutable-ui-frame-kernel-524095a3d0614d7390aa2b2f8e8d3442.md

# Handoff

Implement NUI-U11 proof-first in `nara_input`, `nara_winit`, `nara_window`, and only the minimum
App/gameplay integration required by real consumers. Own a 4,096-entry fixed-size globally ordered
timeline with nonwrapping `u64` sequences, opaque low-level route stamps, event-time position and
presentation facts, and atomic capacity/sequence preflight before any legacy projections. Give
observation, UI, and action independent cursors. During pause, advance the action cursor across
non-action entries, retain action-bearing edges, and skip UI-consumed edges on resume. Make route
invalidation and surface retirement fail closed even when ordinary timeline capacity is exhausted.
Keep `nara_input` below text, UI kernel, window backends, render, and WGPU. Preserve concurrent
user-owned files and keep Cargo serial.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u11-normalize-the-input-timeline-and-consumer-cursors`
- `docs/architecture/adr/0039-main-loop-time-pause-and-runtime-state.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`

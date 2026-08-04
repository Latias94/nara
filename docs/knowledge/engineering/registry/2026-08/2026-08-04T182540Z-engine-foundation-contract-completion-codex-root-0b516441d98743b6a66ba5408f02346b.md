---
type: "Work Registration"
title: "Ordered UI kernel correction: NUI-U5 active"
description: "Completes render/UI boundary closure and activates the split between content authoring and presentation composition."
timestamp: 2026-08-04T18:25:40Z
record_id: "0b516441d98743b6a66ba5408f02346b"
status: "active"
producer_id: "codex-root"
run_id: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
source_session: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e1a45953c012eeb339b0b064ac83c3f44caa1eeb"
supersedes: "0941dd3ad7944d789b3ed80f13b29762"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-04T182540Z-nui-u4-closed-first-party-render-and-ui-internals-af4e0733217a483cb73239914fd21ef1.md"
---

# Scope

Execute the ordered persistent structure, engine-boundary correction, and renderer-neutral UI
product plan through its reference-game proof.

# Current Claim

NUI-U4 is complete at `e1a4595`: arbitrary render topology is deleted, first-party ordering is
closed and deterministic, technical UI stages and mutable batch resources are private, and NUI-U5
is the sole next unit.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-04T182540Z-nui-u4-closed-first-party-render-and-ui-internals-af4e0733217a483cb73239914fd21ef1.md

# Handoff

Implement NUI-U5 proof-first. Redefine the existing pre-1.0 `runtime-2d` and `runtime-ui` features
as content/authoring capabilities without compatibility aliases. Make desktop presentation and the
WGPU backend explicit recipe contributions, preserve exact failure-before-mutation capability
validation, and prove that the same reference content recipe resolves on headless and desktop
Hosts. Keep Cargo serial and preserve the concurrent user-owned UI test edits.

# Citations

- docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md
- docs/architecture/adr/0106-content-and-presentation-product-composition.md
- docs/architecture/adr/implementation-status.md

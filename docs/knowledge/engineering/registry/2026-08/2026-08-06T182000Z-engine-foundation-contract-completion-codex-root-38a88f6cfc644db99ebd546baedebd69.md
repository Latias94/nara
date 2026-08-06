---
type: "Work Registration"
title: "Ordered UI kernel correction: NUI-U15 bounded product proof active"
description: "Activates the Reference-Game product proof only after the runtime UI module and interface checkpoint has closed."
timestamp: 2026-08-06T18:20:00Z
record_id: "38a88f6cfc644db99ebd546baedebd69"
status: "active"
producer_id: "codex-root"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "79910fa"
supersedes: "54b90cb0a87a424b8340f002394a2755"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "verification/2026-08/2026-08-06T181000Z-nui-u17-deep-runtime-ui-ownership-3eb9a95fdf4a43caa78aea9fad953e13.md"
---

# Scope

NUI-U15 is the bounded product-consumer proof for the authored HUD, terminal overlay, Retry path,
surface mapping, desktop presentation, package boundary, accessibility, and presentation-free
headless runtime.

# Current Claim

NUI-U17 is complete and no longer blocks product integration. NUI-U15 may exercise the closed
authoring, presented-session, action-journal, packet, and product-composition contracts. It may not
derive shared component, layout, editor, renderer, registry, digest, or smoke infrastructure from a
Reference-Game-only requirement.

# Latest Links

- `verification/2026-08/2026-08-06T181000Z-nui-u17-deep-runtime-ui-ownership-3eb9a95fdf4a43caa78aea9fad953e13.md`
- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u15-complete-the-reference-game-product-proof`
- `docs/architecture/reference-game-ui-product-slice.md`

# Handoff

Reconcile the existing Reference-Game product candidates against the single U15 coverage matrix.
Keep one canonical logical surface, use targeted resolution/input/host cases rather than a Cartesian
matrix, and delete duplicate smoke, digest, or implementation-coupled fixtures after preserving
their unique assertions. Product-specific state, copy, layout, and visuals remain in authored data
or Reference-Game adapters; widen engine APIs only with independent evidence.

# Citations

- `7546bc9 refactor(ui)!: isolate exact runtime interaction authority`
- `5bbc66b refactor(reference-game)!: consume ui actions by cursor`
- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#verification-contract`

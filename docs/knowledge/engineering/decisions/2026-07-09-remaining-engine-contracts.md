---
type: Decision
title: Remaining engine-wide contracts after external audit
timestamp: 2026-07-09T19:10:28+08:00
tags:
  - architecture
  - adr
  - runtime
  - render
  - input
  - migration
  - facade
status: accepted
---

# Decision

The external audit's remaining concerns were valid, but they are best handled as cross-cutting ADRs
before more code is added. The already-handled items were project manifest authority, event/resource
queue lifetime, asset load/cache/lifetime policy, scene/prefab provenance, and observable asset
reload scheduling failures.

This session added the next six policy contracts:

- ADR 0039: main loop, time domains, pause, redraw/background policy, and runtime state transitions.
- ADR 0040: render resource lifetime, backend cache ownership, and render submitter plugin
  ownership.
- ADR 0041: input routing, action maps, text/IME, UI focus, pointer capture, and accessibility.
- ADR 0042: shared runtime service/backend boundary for physics, audio, text, scripting,
  networking, and similar systems.
- ADR 0043: scene/prefab/patch document-level migration policy.
- ADR 0044: root facade and prelude layering policy.

# Context

The high-risk pattern was not one missing subsystem. It was several lifecycle contracts that would
otherwise be reinvented by UI, render, editor, physics, asset, and scripting work:

- single-delta app loop semantics would make pause, time scale, fixed update, and background
  behavior ambiguous;
- GPU cache lifetime would drift between sprites, UI, text, and future 3D;
- UI/editor/gameplay input would compete without a single routing/action/text model;
- runtime services could leak native handles or worker threads into persistent ECS data;
- component migrations alone would not cover scene/prefab/patch document shape changes;
- a wide root prelude would teach examples and AI agents to import backend/tooling internals.

# Alternatives

- Keep these as open questions until implementation pressure appears. Rejected because the
  contracts affect many future crates and would be painful to retrofit.
- Implement code first and document afterward. Rejected because these are boundary decisions, not
  local implementation details.
- Write one large umbrella ADR. Rejected because each topic has different ownership, success
  metrics, and follow-up work.

# Consequences

- `docs/architecture/open-questions.md`, `docs/architecture/nara-foundation.md`, and `AGENTS.md`
  now point future work at these contracts.
- The next implementation slice should prioritize app loop/time/state, input routing/action maps,
  render cache lifetime, document migration scaffolding, and root prelude cleanup.
- No Rust code was changed by this memory entry.

# Citations

- [ADR 0039](../../../architecture/adr/0039-main-loop-time-pause-and-runtime-state.md)
- [ADR 0040](../../../architecture/adr/0040-render-resource-lifetime-and-submitter-ownership.md)
- [ADR 0041](../../../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md)
- [ADR 0042](../../../architecture/adr/0042-runtime-service-and-backend-boundary.md)
- [ADR 0043](../../../architecture/adr/0043-scene-prefab-and-patch-document-migration-policy.md)
- [ADR 0044](../../../architecture/adr/0044-root-facade-and-prelude-layering-policy.md)
- [Architecture open questions](../../../architecture/open-questions.md)
- [Foundation architecture](../../../architecture/nara-foundation.md)
- [AGENTS.md](../../../../AGENTS.md)


---
type: "Memory Event"
title: "Architecture Decision: Accepted ADR 0017 render graph policy: Phase 1 uses explicit render phases and s"
description: "Accepted ADR 0017 render graph policy: Phase 1 uses explicit render phases and static passes while keeping the renderer graph-ready."
timestamp: 2026-07-08T02:55:51Z
event_kind: "Architecture Decision"
---
# Event

Accepted ADR 0017 render graph policy: Phase 1 uses explicit render phases and static passes, while modeling views/targets/resources so the renderer is render-graph-ready. Full RenderGraph waits for a concrete second use case such as postprocessing, editor viewport composition, render-to-texture, or 3D shadows.

# Impact

# Citations

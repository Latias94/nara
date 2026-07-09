---
type: Decision
title: Schema capabilities, plugin groups, and editor workspace contracts
timestamp: 2026-07-09T19:37:16+08:00
tags:
  - architecture
  - adr
  - reflection
  - plugins
  - tooling
  - editor
status: accepted
---

# Decision

Three additional contracts from the external audit were promoted into ADRs:

- ADR 0045: component schema capability metadata.
- ADR 0046: plugin metadata and default plugin groups.
- ADR 0047: editor workspace and scene document state.

# Context

The previous ADR set covered manifest authority, event/resource queue lifetimes, time/pause/state,
asset lifecycle, scene/prefab provenance, render resource lifetime, input routing, runtime services,
document migration, and facade/prelude layering.

The remaining unclosed audit items were still cross-cutting:

- component fields need shared eligibility metadata before save, network, animation, script, and
  editor tooling each invent their own flags;
- plugin composition needs stable IDs, capabilities, requirements, and explicit default groups
  before more backend/domain plugins appear;
- editor tooling needs a UI-agnostic workspace layer above individual `SceneAuthoringSession` and
  `SceneEditorState` objects.

# Alternatives

- Leave these as open questions. Rejected because each touches several future modules and would be
  costly to retrofit.
- Implement code first. Rejected because these are product-boundary contracts.
- Fold them into existing ADRs. Rejected because each has separate ownership, success metrics, and
  implementation follow-up.

# Consequences

- `docs/architecture/open-questions.md`, `docs/architecture/nara-foundation.md`, and `AGENTS.md`
  now reference the new contracts.
- The next implementation plan should consider component capability metadata, plugin groups, and
  `EditorWorkspace` alongside the app loop/time/state work.
- No Rust code was changed by this decision record.

# Citations

- [ADR 0045](../../../architecture/adr/0045-component-schema-capability-metadata.md)
- [ADR 0046](../../../architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md)
- [ADR 0047](../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [Architecture open questions](../../../architecture/open-questions.md)
- [Foundation architecture](../../../architecture/nara-foundation.md)
- [AGENTS.md](../../../../AGENTS.md)


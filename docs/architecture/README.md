# Architecture Document Map

This page identifies the authority and maintenance role of Nara's architecture documents. It is
an index, not another source of architecture decisions.

## Authority Order

1. [Accepted ADRs](adr/README.md) own durable decisions.
2. [ADR implementation status](adr/implementation-status.md) owns repository evidence and remaining
   gaps.
3. The active [startup scene activation and atomic Retry
   plan](../plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md) owns
   current execution order.
4. [Open questions](open-questions.md) own unresolved, trigger-gated decisions.
5. Design harnesses, appendices, and guides are non-normative. They must yield to Accepted ADRs and
   must not authorize implementation by themselves.

## Document Roles

| Document | Role | Current state and activation rule |
|---|---|---|
| [Nara Foundation Architecture](nara-foundation.md) | Accepted architecture summary | Summarizes current ADR authority; it is neither an implementation ledger nor a roadmap. |
| [Runtime Composition Interface Design](runtime-composition-interface-design.md) | Canonical harness for runtime-composition scenarios | Valid during the active RGF plan; rebaseline against landed owners and Accepted ADRs at RGF closure. |
| [Source Extension Package Interface Design](source-extension-package-interface-design.md) | Canonical harness for source-package scenarios | Tracer-gated; illustrative interfaces do not authorize a package kernel or public API. |
| [Multi-Role Extension Package Tracer](multi-role-extension-package-tracer-design.md) | Scenario appendix | Owns only the multi-role tracer; shared package decisions remain in the source-package harness. |
| [Extension Contract Kernel Interface Design](extension-contract-kernel-interface-design.md) | Candidate-interface appendix | No production consumer exists; it must not create a crate or public API before a named tracer admits it. |
| [Asset Import Host Interface Design](asset-import-host-interface-design.md) | Import specialization appendix | Dormant until ADR 0087 or an Accepted successor admits a concrete importer workflow. |
| [Extension Package Concept Guide](extension-package-concept-guide.md) | Explanatory guide | Explains vocabulary only and never adds decisions. |
| [Render Capability Demand and Pressure Matrix](render-capability-pressure-matrix.md) | Render evidence/admission appendix | Evaluates whether a capability deserves a tracer; it is not a renderer roadmap. |
| [Render Extension Capability Interface Design](render-extension-capability-interface-design.md) | Future render canonical harness | Inactive and needs rebaseline; the RGF closure architecture handoff owns activation against the landed render owners and evidence. |
| [UI Product Boundaries, Editor Dogfooding, and Porting Strategy](ui-product-boundaries-editor-dogfood-and-porting-strategy.md) | UI boundary canonical draft | Evidence-gated; it does not select a final editor toolkit or a shared game/editor widget core. |

`Canonical harness` means the primary non-normative scenario workbench for one subject. `Appendix`
means a focused specialization that cannot redefine its canonical harness. `Guide` explains the
current model. `Needs rebaseline` means its scenarios may remain useful, but file maps, type names,
implementation order, and authority claims must be reconciled before execution.

## Rebaseline Rules

- RGF closure updates this index, the foundation summary, runtime-composition status, and a durable
  architecture handoff bound to the reviewed release commit.
- Future render work starts only after its activation record binds the landed owners and symbols,
  current ADR statuses, relevant open questions, and differences from package/runtime harnesses.
- A design draft may propose names and test scenarios. Only an Accepted ADR selects a durable
  invariant, and only an active plan may order implementation.

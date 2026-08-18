# Architecture Document Map

This page identifies the authority and maintenance role of Nara's architecture documents. It is
an index, not another source of architecture decisions.

## Authority Order

1. [Accepted ADRs](adr/README.md) own durable decisions.
2. [ADR implementation status](adr/implementation-status.md) owns repository evidence and remaining
   gaps.
3. The [Editor UI Prefab Structural Composition v1](../plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md)
   is the sole active implementation plan at EUSC-U5 under registration
   `9123772c74da4f56a63d86ddcd742c9c`. EUSC-U1 is complete at `fb66b84` with verification
   `8386ed22067743af8cfb1f88b3ffd314`; EUSC-U2 is complete at `4a6cd14` with verification
   `7c4c0f3ba8924f7f84668341aa0011ad`; EUSC-U3 is complete at `278b986` with verification
   `14822e4a808f412b8216eb02401eaca8`; EUSC-U4 is complete at `9540deb` with verification
   `af32c829b365491e84fe0af17c46d6eb`. The completed
   [Editor UI Prefab Authoring and Canvas v1](../plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md)
   closes at `ed9f1cd` with conformance record `cf11b0c7d23a44e7b127b41199ee4ee7`
   and completed registration `fcbd471b11774b599cd190ead6827654`. Its prior units remain verified at
   `dc7ea07`, `16b0d30`, `bca3e62`, and `155d147` with evidence
   `e6b43a9f24ed42deb3c8d19a03a55cdf`, `19920c861d394a4fb43db6f60e4cf2a5`,
   `45077ece034c42569c6a460a562ab0b1`, and `984fe0030b4843c2a1165a9e812955f0`.
   The completed
   [Editor Game View v1](../plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md) remains immutable
   lineage evidence at `b5697f3` with conformance record `9d0b0a17beef4c39bea70a8d6f025455` and
   completed registration `84ab40a2fd9b425a9d9827fd9a62e142`.
   The completed Editor Scene Composition and Scene View, Editor Authoring Loop, Fresh Project,
   Capability Directory Publication, Runtime UI, and focused correction plans remain immutable
   lineage evidence; the [UI program roadmap](../plans/ui-program-roadmap.md) is navigation only.
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
| [UI Foundation Design](ui-foundation-design.md) | UI deep-module design blueprint | Non-normative module map, reference-pressure matrix, component admission map, and scale vocabulary. ADR 0102 and ADR 0107-0109 own durable decisions. |

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

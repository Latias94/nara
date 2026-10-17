# Architecture Document Map

This index identifies each document's owner and authority. It does not duplicate decisions,
implementation progress, or execution history.

## Document Responsibilities

User instructions define the authorized task. Within that scope:

- [Strategy](../../STRATEGY.md) defines product scope and non-goals.
- [Accepted ADRs](adr/README.md) define durable architecture contracts. Reconcile a conflicting
  contract before changing the affected boundary.
- [ADR implementation status](adr/implementation-status.md) summarizes implementation evidence and
  remaining gaps. It does not introduce new architecture decisions or task requirements.
- A selected plan organizes multi-stage work and its relevant verification. Local fixes do not
  require a plan, and plan or registration metadata does not grant execution authority.
- [Open questions](open-questions.md), draft designs and research describe unresolved choices or
  background evidence. Current guides explain supported workflows and yield to Accepted ADRs.

## Execution and Evidence Navigation

Use [engineering memory](../knowledge/engineering/index.md) to resume work or recover its reasoning,
verification and handoff context. Registration is continuity evidence, not a lock or an approval
gate. Read records relevant to the task rather than treating the whole bundle as required context.
Historical plans and verification retain their original source, toolkit and platform scope;
their completion conditions do not automatically apply to later tasks.

| Subject | Owning sources |
|---|---|
| Current architecture and implementation | [Foundation summary](nara-foundation.md), [ADR catalogue](adr/README.md), [implementation ledger](adr/implementation-status.md) |
| Managed runtime, immutable composition, assets, hierarchy, 2D order and views | [ADR 0117](adr/0117-2d-foundation-ownership-and-authoring-contracts.md); [completed foundation plan](../plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md) |
| Project creation, compiled schema, editing, Check and Export | [Product recipe guide](../guides/rust-product-recipes.md); [completed first-author workflow](../plans/2026-10-16-0940-feat-first-2d-author-workflow-plan.md) |
| Persistent schema, codecs and custom scalar authoring | [Component guide](../guides/persistent-components.md); [completed custom-authoring plan](../plans/2026-10-16-1101-feat-custom-component-authoring-plan.md) |
| Workspace/frontend boundaries, creation templates, panels and native views | [ADR 0118](adr/0118-editor-workspaces-frontends-and-creation-templates.md); [completed Editor foundation plan](../plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md) |
| Image import and authoring publication | [ADR 0115](adr/0115-editor-image-import-metadata-and-authoring-publication.md) |
| Supervised external Play and saved-source admission | [ADR 0116](adr/0116-standalone-supervised-editor-play.md) |
| Historical plan and verification lineage | [Engineering registry](../knowledge/engineering/registry/), [verification records](../knowledge/engineering/verification/), and [UI roadmap navigation](../plans/ui-program-roadmap.md) |

## Design Workbenches

| Document | Role and reuse condition |
|---|---|
| [Runtime Composition Interface Design](runtime-composition-interface-design.md) | Runtime scenario harness; rebaseline its interfaces and authority against landed owners before reuse. |
| [Source Extension Package Interface Design](source-extension-package-interface-design.md) | Source-package scenario harness; a named tracer must justify any package kernel or public API. |
| [Multi-Role Extension Package Tracer](multi-role-extension-package-tracer-design.md) | Scenario appendix; shared decisions remain in the source-package harness. |
| [Extension Contract Kernel Interface Design](extension-contract-kernel-interface-design.md) | Candidate-interface appendix without a production consumer; not authority for a crate or API. |
| [Asset Import Host Interface Design](asset-import-host-interface-design.md) | Import appendix; ADR 0115 admits the concrete image workflow, not a broader registry, watcher, cache, project plugin, or multi-product graph. |
| [Extension Package Concept Guide](extension-package-concept-guide.md) | Explanatory vocabulary; no additional decisions. |
| [Render Capability Demand and Pressure Matrix](render-capability-pressure-matrix.md) | Evidence/admission appendix; not a renderer roadmap. |
| [Render Extension Capability Interface Design](render-extension-capability-interface-design.md) | Inactive render harness; rebaseline against current render owners and evidence before activation. |
| [UI Product Boundaries, Editor Dogfooding, and Porting Strategy](ui-product-boundaries-editor-dogfood-and-porting-strategy.md) | UI boundary guide; ADR 0118 owns the selected Dear ImGui frontend and finite platform qualification. |
| [UI Foundation Design](ui-foundation-design.md) | Module and scenario blueprint; ADR 0102 and ADR 0107-0109 own its durable decisions. |

A harness is a subject's scenario workbench. An appendix specializes that workbench, and a guide
explains the current model. Their examples may remain useful after their file maps or names age.

## Maintaining These Documents

- Before reusing a design workbench, bind it to landed owners and symbols, current ADR statuses,
  relevant open questions, and observed consumer pressure.
- Update the current contract and direct navigation affected by a change. Keep implementation
  status concise and link to verification for execution details.
- Retain historical reasoning and evidence within their original scope. Obsolete drafts and
  duplicated process artifacts may be retired after checking their consumers and references;
  unrelated history does not need to be rewritten for an implementation change.
- A draft may propose names and scenarios. A durable invariant needs an Accepted ADR; unresolved
  proposals and historical acceptance checklists do not expand the current task.

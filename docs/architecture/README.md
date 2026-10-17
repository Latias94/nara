# Architecture Document Map

This index identifies each document's owner and authority. It does not duplicate decisions,
implementation progress, or execution history.

## Authority Order

1. [Strategy](../../STRATEGY.md) owns product scope and non-goals.
2. [Accepted ADRs](adr/README.md) own durable architecture decisions.
3. [ADR implementation status](adr/implementation-status.md) owns current evidence and remaining gaps.
4. A plan whose frontmatter and engineering-memory registration are both active owns the current
   implementation sequence and its verification gates.
5. [Open questions](open-questions.md) own unresolved, trigger-gated decisions.
6. Design harnesses, appendices, research, and guides are non-normative. They yield to Accepted
   ADRs and cannot authorize implementation by themselves.

## Execution and Evidence Navigation

The completed [Foundation Correctness Closure plan](../plans/2026-10-17-1856-fix-foundation-correctness-closure-plan.md)
is closed by [registration `94169fb22505473c89504d4b41cab1de`](../knowledge/engineering/registry/2026-10/2026-10-17T123810Z-foundation-correctness-closure-codex-root-20261017-94169fb22505473c89504d4b41cab1de.md)
and [verification `4490cc23b14742749a37e7a9cb1c7cbf`](../knowledge/engineering/verification/2026-10/2026-10-17T115334Z-foundation-correctness-closure-u1-u5-verification-4490cc23b14742749a37e7a9cb1c7cbf.md).
The active successor is the [Dear ImGui 0.21 upgrade](../plans/2026-10-17-dear-imgui-021-upgrade-plan.md),
registered in engineering memory. It requalifies the dependency upgrade without reopening the
completed foundation work.

Use the [derived engineering index](../knowledge/engineering/current-state.md) to find current
registration heads, then read their immutable source records. Completed plans and their
verification retain the original source, toolkit, and platform scope. A successor never starts
automatically, and later path changes do not extend predecessor evidence.

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

## Rebaseline Rules

- Before reusing a design workbench, bind it to landed owners and symbols, current ADR statuses,
  relevant open questions, and observed consumer pressure.
- Put progress and platform qualification in the ledger or verification record, rather than this
  index. Preserve historical records instead of rewriting their scope.
- A draft may propose names and scenarios. Only an Accepted ADR selects a durable invariant, and
  only an active plan orders its implementation.

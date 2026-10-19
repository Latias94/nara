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
| Managed runtime, immutable composition, assets, hierarchy, 2D order and views | [ADR 0117](adr/0117-2d-foundation-ownership-and-authoring-contracts.md) |
| Project creation, compiled schema, editing, Check and Export | [Product recipe guide](../guides/rust-product-recipes.md) |
| Persistent schema, codecs and custom scalar authoring | [Component guide](../guides/persistent-components.md) |
| Workspace/frontend boundaries, creation templates, panels and native views | [ADR 0118](adr/0118-editor-workspaces-frontends-and-creation-templates.md); [native qualification procedure](../verification/native-editor-qualification.md) |
| Source-built stateful native panel composition | [Editor panel guide](../guides/editor-panels.md) |
| Image import and authoring publication | [ADR 0115](adr/0115-editor-image-import-metadata-and-authoring-publication.md) |
| Supervised external Play and saved-source admission | [ADR 0116](adr/0116-standalone-supervised-editor-play.md) |
| Historical plan and verification lineage | [Engineering memory](../knowledge/engineering/index.md) and [UI plan history](../plans/ui-program-roadmap.md) |
| Foundation audit corrections and follow-up triggers | [Foundation correction plan](../plans/2026-10-19-1335-refactor-foundation-correction-plan.md) maps A01-A09, B01-B14 and M01-M04 to ordered units and consumer/measurement gates |

## Background Designs

The [2026-10-18 foundation audit](../audits/2026-10-18-foundation-contract-audit.md) records
confirmed ownership repairs and proposed refactor dependencies. Its standalone HTML deliverable
is kept outside the repository. The audit does not change Accepted ADRs.

The [post-publication architecture review](../audits/2026-10-19-post-publication-architecture-review.md)
records the TileSet/Image corrections and the remaining concrete module-interface refactor order.
Its implementation and verification limits remain separate from future tile authoring tools.

The [Bevy/Godot architecture comparison](../audits/2026-10-19-nara-bevy-godot-architecture-review.md)
records current execution/Scene interface hazards and the workflow triggers for asset, Editor,
schema and residency refactors. It is source-review evidence, not a new architecture contract.

The [expanded foundation audit](../audits/2026-10-19-foundation-audit-expansion.md) adds twelve
focused research lanes across tasks, components, reflection, extensions, runtime, Editor, input,
UI/text, filesystem, assets, rendering and module integration, with independent challenge and
explicit correctness/capability distinctions.

The [interface and module review](../audits/2026-10-19-interface-and-module-boundaries.md)
separates synchronous owner contracts from triggered asynchronous preparation and private module
extraction. The [Pele study](../audits/2026-10-19-pele-interface-and-theming-study.md) adds a focused
comparison of semantic theme tokens, layout invalidation and adapter lifecycle ownership.

The [UI foundation design](ui-foundation-design.md),
[UI boundary study](ui-product-boundaries-editor-dogfood-and-porting-strategy.md), and
[render pressure matrix](render-capability-pressure-matrix.md) provide design context when a task
reaches those boundaries. They are not current implementation status or additional task gates.

Earlier composition and package-kernel sketches are covered by the
[runtime design history](runtime-composition-interface-design.md) and
[dormant kernel proposal](extension-contract-kernel-interface-design.md). Other package, import
and render-extension drafts remain background material for a triggered investigation; they are
outside the normal authoring and implementation reading path.

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

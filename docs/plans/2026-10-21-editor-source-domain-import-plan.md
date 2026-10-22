# Editor Source Domain Import Qualification

## Outcome

Use the independent `consumer.text` package as a trusted compiled Editor contribution. An ordinary
standalone Editor must import missing canonical metadata and explicitly reimport changed typed
settings through displayed-frame actions, then allow the same sources to pass Check and Play with
the game's compiled domain recipe. Project Open must remain data-only.

## Design Boundaries

- Select import contributions in the Editor executable's own composition. Game recipe composition
  and Editor composition are separate; neither implicitly loads project code into the other.
- Add a source-backed metadata import facet, not a renderer/native-resource publication contract.
  The concrete package owns typed settings and source preparation. The Host owns exact input
  observations, bounded execution, canonical metadata publication and cleanup custody.
- Reuse the existing filesystem publication and task/runtime owners. Preserve Image's concrete
  last-good pixels, hydration and authoring-generation semantics.
- Preserve stable identity on Reimport, reject stale displayed actions and conflicting or invalid
  existing metadata, and refresh Browser discovery after successful publication. Failed source
  validation must not overwrite accepted metadata.
- No cooked artifacts, watchers, dynamic ABI, general package loader, automatic reimport or generic
  GPU publication is included.

## Work

1. Reconcile the narrow source-backed import contract with ADRs 0103/0115 and define its public
   composition, action and result surfaces from the text consumer.
2. Implement Host admission, domain preparation and metadata publication through existing owners.
   Connect ordinary Editor Open, immutable frames and exact action results to the selected
   compiled contribution.
3. Add an independently compiled text-import panel and exercise Import/Reimport, failed parsing,
   stale actions, identity preservation, project close and source-backed Check/Play. Use the same
   public methods as the native composition.
4. Run focused tests serially, verify the native Editor workflow where supported, review the
   diff and record the actual evidence and limits before a scoped Conventional Commit.

## Evidence Strategy

Begin with a public-consumer characterization showing the missing Editor composition/action
surface. Test the explicit workflow rather than a new internal importer registry. Keep failed
publication cleanup, cancellation and close under existing Host custody, and run the relevant
Image/persistence regressions when touching shared orchestration.

Use one Cargo build job and the existing `target`; no concurrent Cargo commands. Changes to the
public Editor/Host contract need focused external-consumer and owner tests. Native claims follow
the existing native Editor qualification procedure and remain platform/toolchain scoped.

## Completion

Completed with the [public extension ergonomics follow-up](2026-10-22-public-extension-ergonomics-plan.md)
in the commit containing this plan. The [qualification record](../verification/2026-10-21-editor-source-domain-import.md)
records 17 independent-consumer cases, 181 Editor/panel cases, 124 Host/source-export cases and
one explicit Windows/DX12 native canary, together with lint, formatting and minimal-feature checks.
The record distinguishes reproduced failures, the active-abort diagnosis and the advanced
trusted lane's initial source-index limit from the supported ordinary Import/Reimport workflow.

# Reviewed Prefab Extraction

The Project Session owns extraction of a source-owned Prefab branch into a new Prefab.
Scene owns source meaning and correspondence; Workspace owns working copies, history and
checkpoints; the concrete Host owns files, publication and physical retirement. A frontend
does not construct inverse patches, copy files or infer publication from displayed phases.

The complete workflow and remaining qualification are tracked in the
[extraction plan](../plans/2026-11-02-prefab-extraction-plan.md). The public Session/Application
APIs and independent recovery consumer are implemented. This guide does not claim that a
native extraction control, strict Check/Play after extraction or the complete foundation
rebaseline has been qualified.

## Prepare And Review

Begin with an admitted Project Session and an open Prefab document. Create an
`EditorPrefabExtractionRequest::new(document, root, target)` using:

- The exact `EditorDocumentId`, not whichever tab happens to be active later.
- A directly source-owned `SceneEntityId` identifying the branch root. The extracted Prefab
  includes that entity; extraction does not add an observable wrapper root.
- An `AssetMetaIdentity` fixing the new Prefab's stable identity and alias in the selected
  project's Prefab scope. The destination and metadata sidecar must be available for creation.

Populate `save_current` with open, otherwise unchanged documents whose reviewed working copies
must also be saved. `compilation_limits` bounds Scene construction and interpretation.
`begin_prefab_extraction_with_limits` additionally selects finite Host work and native-handle
limits before capture; `begin_prefab_extraction` selects the defaults. Neither grants permission
to run project code.

Retain the returned `EditorPrefabExtractionId` and continue ordinary Session/Application
maintenance. Acceptance immediately fences conflicting source writes. A previously accepted
Save is drained and installed before extraction captures its inputs. Navigation does not
retarget the operation. A pending worker is not a completed Review.

Use `prefab_extraction_view()` to observe the operation and its actual capabilities. When a
Review is available, `prefab_extraction_review(operation)` borrows the prepared selection,
complete source set, unchanged Schema basis and exact encoded outputs. The same immutable
inputs support construction, occurrence correspondence and saved-generation validation.
Review and confirmation revalidation publish no files.

## Selective Save Semantics

Each open source has one disposition:

| Disposition | Working copy and history | Disk result |
| --- | --- | --- |
| `ReplaceSource` | Install the verified extracted/rebound source with its prepared history transition | Publish its exact reviewed output |
| `SaveCurrent` | Preserve the reviewed working meaning and history | Save that working copy through its original checkpoint |
| `Guard` | Preserve the original working copy, history, saved state and reload state | Keep the original saved file unchanged |

Closed dependants are rewritten from their captured sources. Saved-generation validation uses
original disk Guards, not unsaved working copies as substitutes for missing saved dependencies.
Q, its metadata sidecar and each original-source output have distinct roles; an unchanged Guard
has no output.

An invalid save selection retains its original preparation. Use
`prefab_extraction_selection(operation)` and `revise_prefab_extraction(operation, save_current)`
to correct that selection, or revise a complete unpublished Review when `can_revise()` permits.
Revision keeps the same corpus, source/history inputs and cumulative allowances. It supersedes
the old encoded proposal rather than retaining two independently confirmable Reviews.

## Confirm, Observe And Retire

`confirm_prefab_extraction(operation)` revalidates the original complete Project and Workspace.
Once confirmation begins publication, selection revision is closed. The existing concrete
owners publish the immutable recovery package, Pending manifest, Q, sidecar and sparse original
outputs, then prepare the future Project and bind all live installation destinations before
publishing Ready. This is recoverable progressive publication, not multi-file atomic Save.

After accepted Ready, installation moves prepared state into the original Workspace,
persistence Hosts and Session destinations. It does not reload a second Project or recreate
history to simulate the original operation. Already accepted file results retain their actual
receipts or current-content observations; an observation is never a historical replacement receipt.

Borrow `prefab_extraction_diagnostics()` for the retained owning report. Host work, open-handle
capacity, Scene capacity, stale inputs and publication failures remain separate refusal kinds.
Retry uses the original allowance; it cannot raise capacity or clear physical uncertainty.
Only select cancellation/retry when the concrete view permits it. Cancellation and Close wait
for actual worker return and necessary native cleanup. Dropping an owner is not cleanup evidence.
Retain and acknowledge the exact terminal operation before starting a replacement.

`EditorApplication` exposes the same request, view, review, revision, confirmation, cancellation,
retry and acknowledgement operations. Replacement Open cannot hide a fenced operation. Close
and Exit first retire it, then resume ordinary dirty-document Save/Discard arbitration.

## Recover In A New Process

Ordinary Open refuses Pending without loading project source or executing project code. Its
`project.editor.source-authoring-pending` diagnostic does not identify the recovery package kind.

A headless consumer can enable `project-workflow-cli` with default features disabled; Tooling
and an old Workspace are unnecessary:

1. Call `PendingAuthoringRecoveryInspection::inspect(root, limits)` with an actual directory
   capability and explicit finite operation limits. It authenticates the manifest/package
   binding and digest before classifying the persistent header.
2. Dispatch `PendingAuthoringRecoveryKind`. Transfer the same inspection into
   `PrefabExtractionRecovery::inspect(inspection, compilation_limits)` for extraction, or into
   the authored-upgrade owner for that other kind. Wrong-kind refusal retains the inspection.
3. Advance read-only inspection to Review. Borrow fixed inputs, dispositions and actual current
   member observations; then explicitly confirm before advancing publication.
4. Observe each finite stage and retain the actual owner on refusal. Explicit Retry and Close
   preserve original work and custody. Capacity refusal needs a new process/operation selecting
   adequate limits at entry, not a raised allowance on the old owner.
5. Only actual Complete can yield `into_outcome()`. The outcome retains the accepted Ready
   evidence, actual prepared Project, Schema and remaining native resources, without a resumable
   publication cursor. A subsequent ordinary Open admits its own fresh generation and history.

Recovery consumes stored P/Q/dependant outputs and original disk Guards. It does not rerun
extraction, regenerate Q identities/input names, execute a converter or reconstruct old nominal
tokens, Workspace checkpoints, native handles or receipts. Header inspection conservatively
admits parser/error storage; fitting the transport byte ceiling alone does not guarantee admission.

The [independent CLI-only consumer](../../tests/fixtures/prefab-extraction-recovery/README.md)
documents its finite command interface and process-test setup. It is a qualification companion,
not an additional command silently installed by the production `nara` CLI. Filesystem durability,
Check/Play, native UI behavior and additional platforms retain their separate acceptance gates.

---
type: "Work Progress"
title: "Session extraction captures real reviews with shared authoring maintenance and concrete Play operation identity"
description: "Unpublished Session/Application integration, original budget causes and physical-return cancellation; confirmation/publication remains open."
timestamp: 2026-11-03T06:53:27Z
record_id: "0752d732f6ec4fa4b8af57788cd92062"
producer_id: "codex-foundation-main"
run_id: "extraction-session-review-20261103"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "88fd7289c0d680481e0f964561650de05f90cbad"
---

# Summary

Full foundation Goal remains active. Private final Workspace/file installation has its predecessor
qualification. The working tree now adds actual unpublished Session/Application extraction
capture, complete borrowed Review, write admission and physical-return cancellation. Confirmation,
durable publication coordination, final Session lane/execution installation, new-process/public
Check/Play/native qualification and the complete CP2 unit remain unfinished.

Main and refactor/foundation remain at `88fd728`; all changes are authorized uncommitted WIP.
No push, artifact deletion, dependency update or vendor/repo-ref mutation occurred.

# Details

`EditorPrefabExtractionRequest` fixes the selected source-owned root, destination metadata,
explicit SaveCurrent set and source compilation limits. Session acceptance immediately fences
conflicting mutations, Save/Reopen, competing source operations, new Play and Workspace intents.
An already accepted Save retains its original attempt: acknowledgment stays Busy until its actual
Saved result is observed and installed, then complete capture begins. Failed Save ends extraction
without a Review. Rendering/navigation remain with their current owners.

`PrefabExtractionReviewRequest` consumes actual frozen Workspace inputs and the complete physical
capture request. One worker executes capture, core preparation, selective Review, fixed outputs
and recovery-data preparation without filesystem publication or project code. It retains unchanged
nominal Schema authority, real source/history captures and the original cumulative Host/Source
allowances. `OwnedJob` pre-admits slot/callback storage and returns the actual preparation. The
Session releases task storage and captured inputs only after physical return and callback disposal;
logical cancel/Close cannot release the fence earlier. A Review has no pending work but retains
the write fence. Public wrappers borrow original output bytes and expose no mutable candidate.

The shared Host limits move from the Schema-upgrade policy to
`AuthoringSourceOperationLimits` in `authoring/source_operation_policy.rs`, retaining exactly
200M work/64 native handles and existing accessors. Upgrade-specific history remains in its own
policy. All current production, test and independent recovery fixture consumers use the new type;
there is no old alias. Source operation limits do not grant source or project execution authority.

The frame now carries one `authoring_operation_work_pending` observation aggregated by Session.
Remove the per-Schema/per-upgrade/per-extraction pending fields and migrate native maintenance,
Root/Editor tests and the independent creation-metadata consumer. Detailed operation views retain
their own phases/fences; a persistent fence alone does not create a native 16ms polling loop.
ADR0118 records this frontend-neutral maintenance contract.

Supervised Play retirement now uses a finite `EditorSourceOperationId` enum with independent
Schema-upgrade and extraction IDs. Exact request Arc, custodian registration, incumbent basis and
recovery digest remain required. The owning current fenced operation validates each enum variant;
cross-kind IDs and foreign acknowledgments refuse. ADR0116 records this selected identity seam.
This does not yet connect extraction confirmation to physical Play retirement.

Application delegates begin/limits/view/review/cancel/diagnostics to Session. Replacement Open
refuses during either source operation's fence while retaining the incumbent/cancel facade.
Close/Exit asks the operation's `can_cancel` and waits actual release before ordinary dirty-source
arbitration; current capture/Review cancellation publishes no project source.

Independent review found P2: generic bridge errors erased Host work/handle refusal. Preserve a
bounded Copy capacity cause across capture, Task admission, Project/Inventory/Decode/Output and
recovery-data admission wrappers. Runtime maps it to WorkLimit/OpenHandleLimit and bounded
diagnostics. A follow-up review caught the final Recovery `Data(Admission(...))` outlet; it now
uses the same classification. The complete bridge exact/one-short control reaches actual recovery
encoding and verifies that low work is not misreported as invalid source or a handle error.

The new owner-consumption diagnostic methods move bounded reports instead of cloning them:
Scene saved-generation `into_report`, private Output/Recovery failure `into_error`, and Tooling
persistence `validate` forwarding complete original source/history/saved validation. They grant
no new source replacement or publication right.

Initial evidence and corrections are retained without broadening their scope:

- Root all-feature library check16.81s, then Root/Editor all-feature library check21.06s passed
  before the final Recovery error outlet correction and later test-only navigation changes.
- Initial Session/retirement run `4eac6625-6a74-4447-8921-84427296a3fd`:14/15 passed. A new
  test performed direct source edits without publishing their current frame, then expected Play
  Busy. Preserve frame authority by publishing before testing the source fence.
- Next `5ae08bd1-ef01-4700-973c-f0edd01c0793`:15/16 passed, including the added real recovery
  exact/one-short control. The remaining new test selected an entity in an inactive document;
  repair its action sequence with explicit SetActiveDocument before each selection and restore
  the intended final active document. Do not weaken the existing navigation check.
- The first Editor test build found one missed custodian test call after the finite operation
  identity change; migrate it explicitly, with no compatibility overload. It did not run tests.

Final run results and limits are recorded below before registry publication.

Threads are all source-ready and closed: Godel (neutral limits), Euler (private Review bridge),
Nash (concrete retirement identity), Dirac (real Session tests), Mendel (Application facade),
Sagan (capacity cause), Schrodinger (independent review and final Recovery outlet recheck).
Mendel did not produce App tests; the parent implements their real Review/replacement and
Close/Exit consumers. Static review is separate from actual execution evidence.

# Next Action

Consume the retained Session Review through the existing concrete recovery/file/future-Project
owners. Confirm must first validate the actual complete Project/Workspace and join source-tool
and supervised Play physical retirement. Once Pending may have changed, cancellation and ordinary
Close must follow actual publication ownership rather than current capture-only `can_cancel`.
Tasks must preserve unstarted/returned durable owners on refusal; no worker error may drop an
uncertain publisher or grant a new allowance. Final Session adoption holds original Host locks,
Workspace/leaf/lane and execution targets across Ready and only moves preprepared state afterwards.
Preserve active/preview navigation, selective SaveCurrent/Guard history/conflicts and unopened Q.

Complete Review save-selection revision/retry, actual new-process recovery, independent public
follow-up edit/Undo/JSON/RON Save/Reopen, strict Check/Play and native Editor extraction remain
within the full plan. Do not commit the entire WIP as complete CP2 or shrink the overall Goal.

# Citations

- [Extraction plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Session operation](../../../../../src/project_host/runtime/editor/prefab_extraction.rs)
- [Core preparation bridge](../../../../../src/project_host/authoring/project/prefab_extraction/session_review.rs)
- [Actual Session controls](../../../../../src/project_host/runtime/editor/prefab_extraction/tests.rs)
- [Application facade](../../../../../editor/src/app/prefab_extraction.rs)
- [Application controls](../../../../../editor/src/app/persistence_tests/prefab_extraction.rs)
- [Shared limits](../../../../../src/project_host/authoring/source_operation_policy.rs)
- [Play retirement identity](../../../../../src/project_host/runtime/editor/external_play/owner_binding.rs)
- [Frame maintenance contract](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)

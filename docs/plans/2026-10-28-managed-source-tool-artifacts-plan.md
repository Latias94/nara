---
title: Managed source-tool artifacts and project-owned template import
date: 2026-10-28
status: in-progress
related_plan: 2026-10-28-local-source-tool-release-plan.md
---

# Managed Source-Tool Artifacts And Project-Owned Import

## Scope And Result

Implement the [artifact ownership design](../architecture/source-tool-artifact-ownership-design.md)
as the replacement for mixed installed/project content. Continue the actual independent
EnemyStats consumer through installation, parallel versions, exact dependencies, activation,
template import, editing/history/persistence, withdrawal, real unlink and interrupted recovery.
The full foundation Goal stays active. Completing prerequisites or writing this plan does not
complete this lifecycle.

Keep the generic exact-object `unlink_file` production primitive. It is necessary for real
reclamation. The replacement changes the source-tool owner's disposal permission rather than
weakening that filesystem guarantee or removing deletion from the product.

## Dependency Order

### 1. Complete Existing Ownership Observations

Fix actual retained-source alias checks at FilePublicationHost and source admission. Protect
both the retained original object after rename and the current bound parent/name; uncertainty
must not silently relinquish either obligation.

Expose physical work truthfully through existing Tooling observations. Include selection
cleanup after its logical phase is Idle and cleanup retained by superseded Hosts in the existing
HostCustody. Preserve executor CleanupFailed independently from successful return and ordinary
Pending; no terminal failure may become deletion permission or endless maintenance polling.
Queries are read-only and do not perform cleanup.

Use independent native-object/held-work tests. These corrections apply under both models and
must not wait for the store replacement. They do not by themselves introduce a reclaimer.

### 2. Reconcile Owners And Fix Store Admission

Reconcile ADRs 0020/0035, 0047/0118 and 0070 before changing the affected production boundary.
Constrain integrated installations to `.nara/source-tools/artifacts/` while retaining the
existing finite publisher, receipt, installed graph, requirements and trusted execution owner.
Preserve unready publication custody. Ordinary editable-source admission rejects the store and
actual protected-object aliases. Only that subtree is disposable; no blanket `.nara` cleanup.

Replace APIs that allow mixed project-source ownership in an installation. Update public
consumers directly; there is no supported older release to preserve. Local/Git/Cargo source
packages continue to belong to their existing workspaces and toolchains.

### 3. Import A Real Project-Owned Template

Prepare a finite candidate from captured template bytes and an absent project destination.
Preparation/drop performs no writes. Confirm through existing Tasks and file publication,
retain actual outcomes after cancellation, then expose the project-owned copy through the
existing source owner. Update/install/remove must never overwrite or reclaim that copy.

Qualify the actual independent text sample first. Native Scene/Prefab/asset templates require
owner-prepared identities, sidecars and reference closure remapping before claiming support.
Do not generalize an opaque text copy into native-format qualification.

After replacement coverage exists, remove the old mixed content record, accepted-touch
interception and optional-ledger gates on ordinary Save/Reopen/Close. Preserve the existing
document history, checkpoints, source identity and physical persistence custody. A malformed
artifact management record must not block an unrelated ordinary document's persistence.

### 4. Join Withdrawal With Actual Quiet And Review

Reuse original-basis graph withdrawal and reverse required closure, including disabled members.
Withdrawal and physical return remain separate outcomes. A disposal review requires current
graph absence, complete tool-lane return, current source users and no unresolved cleanup failure.
Include old Hosts in the existing custody, deferred promotions, selection/content IO and actual
child/pipe return. Do not infer this from Cancelled, Withdrawn or phase Idle.

Prepare one bounded move-only review with no writes. Confirmation revalidates its original
observations and fences relevant enable/request/graph/source admission before publication.
Changed observations require a new review; do not refresh the old candidate behind the caller.

### 5. Publish Intent, Unlink And Preserve Partial Results

Publish the exact bounded removal intent before starting the first selected deletion. Wait for
accepted publication and physical IO return. Then invoke generic exact-object unlink for each
authorized whole artifact, including alternate streams. A changed artifact may be explicitly
disposed of without claiming it was unchanged. Unknown siblings are retained.

Retain each actual member result before subsequent work. Expose successful unlink, observed
missing, definite refusal/failure, remaining intent and result-publication debt distinctly.
Cancellation stops new member work; later failure never rewrites earlier receipts as rollback.
Do not use recursive directory deletion or construct deletion authority from serialized IDs.

### 6. Qualify Recovery And Remove Superseded Mechanisms

Fresh Open reads recovery without execution or automatic deletion. A new review is required
after interruption. Test partial unlink and failed/uncertain result publication, equal-byte
object replacement, object rename, source ownership, stale graph review, disabled dependent,
held child/pipe/IO, incomplete installation, unknown sibling and damaged artifact.

The actual consumer must install two versions, retain exact required bindings, import/edit/Undo/
Save/Reopen a project copy, withdraw/cascade, wait for actual return, explicitly reclaim the old
artifact and observe recovery on fresh Open. Join Session/Application and affected native UI
behavior without fixture-owned lifecycle implementation.

Delete old readers, aliases, conversion helpers and replaced record/API/test paths. Canonical
formats remain 1. No migration guide is needed. Record implementation evidence separately from
the selected design and preserve only useful historical verification.

## Verification And Delivery

The parent owns serial Cargo execution with `CARGO_BUILD_JOBS=1` and the shared root target.
Workers have disjoint file ownership and do not run Cargo or workspace formatting. Review
diffs before formatting/staging, obtain independent source review, run focused nextest and
affected public/independent/native workflows, then create precise local Conventional Commits
and ff main. Do not push or open a PR.

The [protection predecessor](2026-10-28-protected-source-tool-content-plan.md) keeps its actual
verification scope. Its unresolved mixed-content all-stream proof is replaced by this explicit
ownership decision; it must not become an excuse to remove unlink, retain all artifacts forever
or report reclamation complete before the actual consumer works.

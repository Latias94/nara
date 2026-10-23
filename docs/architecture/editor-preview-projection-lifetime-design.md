---
title: Editor Preview Projection Epochs
date: 2026-10-24
status: Adopted epoch contract; bounded implementation qualified
---

# Editor Preview Projection Epochs

## Problem And Scope

R5c separates document truth from disposable execution. The current Scene preview nevertheless
uses a native `SceneAuthoringSession` to replace a complete Scene in one long-lived authoring
World after every source revision. Each replacement consumes another instance claim and one
claim per entity. Exact lifetime history eventually refuses ordinary preview publication while
the document remains editable. This obeys ADR 0058 but is not a suitable unlimited-edit policy.

This slice qualifies sustained preview editing, not durable property addressing, Prefab overlays,
checkpoint restore, multiple independent Play sessions or a universal lifetime framework.
The [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md) stays active.

The owning ADRs 0058/0026/0047/0111 now adopt the same-World Scene epoch contract; ADR 0115 retains
the shared task/image owner. The [bounded measurements](../benchmarks/2026-10-24-preview-projection-lifetime.md)
record the frozen incumbent and candidate without selecting full-owner reconstruction for production.
Actual implementation and public/native coverage belong in the ledger and qualification evidence,
not this document's status alone.

## Alternatives

1. **Incumbent:** same-identity-domain complete replacement. Retain it as the exact behavior/cost
   baseline and preserve this deliberate mode for ordinary Scene consumers. Enlarging the claim
   limit or silently clearing claims does not solve the ownership mismatch.
2. **Full authoring Runtime epoch:** rebuild the actual App and first preview before retiring the
   predecessor. Measure a bounded cadence as a reconstruction-cost control, not a per-edit straw
   man. This is not selected for production: the shared owner also holds task pools, image versions,
   queued imports, filesystem publication and cleanup. Existing TrustedCompiled content refresh
   does not qualify their continuity for arbitrary Authoring edits. ADR 0115 requires recoverable
   preview failure not to close the shared task/image owner.
3. **Scene-owned preview identity epoch in the same World:** prepare a detached successor domain
   bound to that exact World, then publish through the existing Scene transaction. Keep tasks,
   images and filesystem custody in the unchanged authoring Runtime. This is the next concrete
   candidate, subject to the complete ownership proof below; a Host resource assignment is not it.
4. **Owner-specific incremental projection:** preserve unchanged object incarnations through typed
   deltas, with an explicit epoch fallback for unsupported changes. It may reduce execution cost,
   but scalar-only updates do not solve unbounded structural churn. Defer its production interface
   until measured pressure justifies the extra delta/fallback contract; do not compare it to a
   deliberately weak epoch implementation or claim it qualified by the epoch tests.

## Exact Epoch Ownership Proof

One active Scene alone is insufficient: another consumer may have retired a claim or minted an
unregistered token. The Identity owner must validate all of the following, before scratch entity
allocation and again at final Scene admission:

- The installed domain belongs to this World and matches the captured predecessor domain.
- Its only active Scene group is the exact retained projection, including an empty Scene.
- Its entire instance-claim set is that one instance, and every retained Scene reference claim
  belongs to the exact current group. There is no unrelated retired Scene history to discard.
- Every registered entity belongs to the exact retirement membership and has only its expected
  Scene axis. The persistent logical ledger is empty, including Unloaded and Deleted records.
- Every live private marker for the predecessor domain belongs to that membership. An adopted
  but unregistered token is responsibility too; registered-entity counts cannot prove its absence.
- First publication may use an absent domain or an otherwise empty domain with no claims, logical
  records or live markers. Any unexpected responsibility rejects; it is never migrated implicitly.

Publishing one clean successor projection establishes the same invariant for its next epoch.
This inductive proof avoids a second ownership registry or a public capability framework. An
ordinary domain with a richer history is not eligible even if it currently has one live Scene.

The old domain stays installed during preparation. Candidate identity tokens must be minted in
the detached successor, never adopted in the old domain and retagged. Reuse Scene's component,
asset-binding, hierarchy, exact-retirement and lifecycle-support admission. Failure discards only
unpublished candidates and preserves the accepted Scene/domain. Scratch allocation is not a claim
of zero World mutation. The exclusive publication tail runs no codecs or callbacks after its final
validation; it retires the exact predecessor membership and installs the successor as one commit.
Candidate disposal must validate tokens against their detached domain, not the installed predecessor.

## Observation And History

An epoch ends the old domain's observation lifetime; it does not evict history from a continuing
domain. Old `WorldEntityLocator` values reject the new domain. Domain-free runtime references
belong to their admitted epoch/timeline and may become Missing or StaleRuntime after it ends;
they do not carry a perpetual Tombstoned promise. Process-unique incarnations must never alias.
This does not permit persistent Deleted resurrection, a same-timeline restore or a global Expired API.

The Editor must capture source document/revision, omissions, domain snapshot and render-source
generation for one accepted projection. Reserve the renderer producer when the preview owner is
installed, before Scene commit; a failed reservation or candidate cannot relabel last-good as the new revision.
The renderer producer and monotonic frame stream remain unchanged across same-owner identity
epochs; this preserves native target/texture reuse. The document/presentation receipts fence
pixels and picking, while domain locators fence ECS lookup. Old captured pixels
may remain visible but cannot authorize new-revision picking or mutation. Task/image state,
pending Save/import work and physical close owners remain unchanged by a preview-only epoch.

Do not add another authoring history or document store. If a concrete projection owner replaces
the preview's native authoring materializer, it owns only materialization and its exact receipt.
Normal `SceneAuthoringSession` and Scene replacement remain available to their actual consumers.

## Entry Measurements And Stop Conditions

Before production selection, run the same Transform edit workload at 64 and 1,024 entities, with
two warm-ups and sixteen retained publications per process. Collect three optimized processes
serially in the existing target, one build job. Separate source edit, Runtime setup, publication,
and retirement; report actual domain claims and live population. The full-owner control rotates
at a declared finite cadence rather than on every edit. Report crossing samples separately.
This bounded sample supports nearest-rank P50/P95 and gross regressions, not a stable P99 claim.

Correctness targets are exact: zero rejected legal publications across multiple small-budget
epochs; steady claims equal the current population plus one instance; no foreign identity or
resource loss; no new task workers or image store per preview epoch. With the same population,
reject a candidate whose publication P95 exceeds twice the incumbent in all three matched
process comparisons, and investigate before selecting it. Do not move this threshold after results.
Timing compares like-for-like publication; startup/retirement of the full-owner control remain
separate costs and are not subtracted from percentiles to invent an end-to-end metric.

Each run stops at its fixed workload or a correctness failure. Stop the design trial if exact
ownership requires weakening Deleted/no-alias or physical-custody guarantees. Unsupported sources
must reject with last-good intact, not receive fallback zeros or a hidden history reset. Allocation
bytes and native edit-to-present need their own collectors; no such measurement is claimed here.

## Qualification And Ordered Integration

1. Run the real authoring-owner incumbent/control collector and a small-budget saturation witness.
2. Qualify Identity's complete predecessor-domain proof, including foreign retired claims,
   persistent Unloaded/Deleted state, unregistered tokens, empty Scenes, moved resources and hooks.
3. Reuse Scene's prepared publication for the epoch candidate; test stale preparation, invalid last
   component, independent membership, hierarchy/lifecycle rejection and detached candidate disposal.
4. Before adopting changed production semantics, reconcile ADRs 0058, 0026/0047 and 0111, and
   preserve ADR 0115's task/image ownership. Integrate preview publication without altering the
   qualified Play-source, per-document persistence or saved-content refresh owners.
5. Exercise the public workflow: bound Scene -> repeated field edits -> Undo/Redo -> structural
   add/remove -> Save/Reopen -> presented preview/picking -> Close, with unknown records and repeated
   Prefab instances. Hold Save and image work across epochs, reject stale input/receipts, retain
   physical cleanup and qualify the affected actual Windows/DX12 trajectory.

Selection and implementation claims belong in the implementation ledger with committed evidence.
This design document alone is not executed qualification and does not complete the broader R5c plan.

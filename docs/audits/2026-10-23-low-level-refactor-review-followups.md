# Low-Level Refactor Review Follow-ups

This records counterexamples found during implementation and independent review of the
[low-level plan](../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md). It supplements the
[original audit](2026-10-22-low-level-module-and-interface-review.md) and
[reassessment](2026-10-23-low-level-architecture-reassessment.md); it does not rewrite what those
source baselines proved. Passing an earlier cohort does not close a subsequently discovered path.

## Spatial Publication

| Finding | Counterexample | Owning correction | Evidence |
| --- | --- | --- | --- |
| SF1: Missing structural membership silently omits renderables | An unplaced spatial Sprite/Tilemap has a global but no captured structural order entry | O(1) captured `structural_index`; extraction admits every live renderable before publishing output | Owner regression in the 191-test U4 cohort |
| SF2: Frame edits become artificial interpolated motion | First/Prepare changes local x from 0 to 100, fixed simulation leaves it unchanged, alpha 0.5 renders 50 | Capture actual local/parent starting values after Prepare and deferred application | First edit and Prepare parent-frame regressions in U4 |
| SF3: Movable history launders foreign input | Move a private simulation/history resource across Worlds and derive a local presentation | History and uncommitted samples retain World/runtime origin; read admission precedes history use | Foreign-history and runtime-generation regressions in U4 |
| SF4: Equal topology counters authorize foreign order | Safely transfer the same roots and foreign sibling order with equal generation/root epoch | Completion state, order and fact share exact World-bound Hierarchy publication identity | Actual safe roots/order transfer and same-World old order regressions in U4 |
| SF5: Slot-local sequence repeats publication identity | Publish X at A's sequence 2; publish Y using an older slot's sequence 1; restore X slot while observed rows describe Y | Allocate spatial publication identity independently from movable slot storage; verify restored old cuts cannot match new observed/presentation association | Shared checked allocator and actual slot/history restore regressions pass in final 329-test cohort; independent rereview has no blocker |

The first snapshot implementation also duplicated Hierarchy's adjacency graph and performed
per-entity World/hash lookup for unchanged sources. The measured correction reuses validated
parent-before-child order and contiguous source observations. Residual full source scans remain
part of the supported mutable-ECS contract. The
[cost report](../benchmarks/2026-10-23-low-level-spatial-cuts.md) separates initial and final processes.

## Asset Intent And Custody

| Finding | Counterexample | Owning correction | Evidence |
| --- | --- | --- | --- |
| AF1: Retained desired recipe overrides a newer publication | Reload retains sRGB; public importer commits Linear; source-only Modified uses stale retained sRGB | Bind retained recipe to publication, value slot and metadata observation; successful publication satisfies/replaces intent | Public reimport and own-successor regressions in the 257-test U5 cohort |
| AF2: Rejected Remove revokes publication but leaves reusable desired settings | Remove queue admission fails; Image never receives cleanup request; Modified reuses old settings | Retain removal before request admission and invalidate recipe by publication identity; require exact recovery or metadata re-admission | Rejected-removal regression in U5 |
| AF3: Host rescan keeps native watcher alive after Stop | A returned strong rescan owner outlives runtime shutdown | Runtime exclusively owns replacement watcher/tail; Host holds Weak receipt; exact session drop outside mutex | Watcher 29-test cohort, callback lifetime test and independent no-blocker review |
| AF4: Removal gate blocks newly observed metadata after queue rejection | Remove request rejected; later MetaModified request rejected; Modified never reaches metadata admission | Retain metadata identity at removal and allow only a later unresolved observation to retry full identity/settings admission | Cross-frame/both-orders rejection regressions pass in final 329-test cohort |
| AF5: Coalescing loses the relative metadata/removal order | Same-frame Removed then MetaModified collapses to Removed plus a boolean; resolver records the observations in the opposite order | Preserve latest metadata relative to latest removal in semantic coalescing; reverse order must remain ineligible | 27 three-event coalescing sequences and complete Image runtime both-order push/batch regressions pass in final 329-test cohort; independent rereview has no blocker |

Metadata observation is permission to revalidate, not proof of source identity or publication.
Wrong stable ID, path and kind must still reject. Loading/failure/rejection must not erase the
observation, and a later successful public owner write must supersede retained reload intent.

## Gameplay Success

The first exact-batch implementation retained the App's reporter but checked only that reporter's
fault state. A system could replace the World reporter, report directly to the replacement and
return normally. Capture/Ack could then succeed before App's later authority check.

The correction compares the current World reporter with the exact retained reporter at every batch
success gate. Missing/replaced reporters synchronously record an authority fault on the retained
owner. App still owns complete fault-bridge/mutation-epoch validation. Normal/deferred replacement,
with and without direct fault reporting, must block success; Stop remains reachable. The U10 owner
cohort and renamed-root 9-test public consumer pass. Commit: `cfde813`.

## Interpretation

These failures concern lost facts at boundaries: which publication, which observation came later,
and which owner remains responsible. Their repairs belong to the owners of those facts. They do
not justify changing the ECS substrate or introducing a universal transaction/import/task framework.
The replacement interfaces must carry the facts privately so ordinary callers do not reconstruct
them from loosely related counters or fallback values.

The implementation ledger and fresh verification records determine closure. Native Windows/DX12
acceptance, Linux FS evidence, headless tests and performance observations retain separate scopes.

Final combined owner run: `3bb6abef-0e57-4bf0-b7dd-160cf4d191b6`, 329/329 passing.
The initial 328/329 run exposed a new test's fixed two-drive shutdown assumption; bounded owner
polling replaces it under the existing pollable-close contract. This does not change production
shutdown semantics.

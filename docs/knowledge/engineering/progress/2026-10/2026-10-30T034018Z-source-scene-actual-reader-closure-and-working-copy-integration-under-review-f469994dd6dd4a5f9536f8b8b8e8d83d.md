---
type: "Work Progress"
title: "Source Scene actual-reader closure and working-copy integration under review"
description: "Tracks finite source data custody, Product travel, admitted Editor inputs and independently found ownership and budget corrections before final qualification."
timestamp: 2026-10-30T03:40:18Z
record_id: "f469994dd6dd4a5f9536f8b8b8e8d83d"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "3ee2da1"
---

# Summary

The complete foundation Goal remains active. Canonical source locations and typed UI Image
binding are committed as `a56a1d3`, with qualification/contract closure `3ee2da1`. Local `main`
was fast-forwarded from `4c405c1` to `3ee2da1`; the working branch remains `refactor/foundation`.
No push or PR occurred. The next Source Scene unit is a large uncommitted implementation under
review, not a qualified successor. Preserve all working changes and untracked `vendor/`.

# Details

## Owning Replacement

- `ProjectSceneContent` keeps raw authored data, effective compiled candidate, actual canonical
  location and admitted document identity separately. `ProjectSourceContent` replaces the old
  assets-only context and retains the actual flat selected/dependent Scene data through escaped
  `SceneSource` custody. It contains no recursive Scene owners or World instances.
- The finite effective-reference worklist uses canonical seen/pending keys, visiting self/cyclic/
  diamond sources once. The production lazy Prefab compiler/cache serves each Scene. Only the
  selected root is materialized. Runtime Schema/native and Tilemap checks cover dependencies too.
- Catalogue observations retain actual reader bindings. Scene data can live in an actual Assets
  or Prefabs reader without kind-guessed directories. Prefab StableId/Product requests now use
  actual readers and a canonical cache key; bare Prefab Paths retain their explicit alias scope.
  Saved expected Prefab digests become canonical at ingress. Delivery merges the captured index,
  without another Scene source/path discoverer.
- Source primary Product requests retain original selection, enter Preparing and resolve actual
  metadata in the existing worker. Only startup/declared Scene roots authorize activation;
  dependency membership does not. Ready uses captured facts, original context and exact current
  Scene receipt. The new owner tests also cover cancellation and foreign definitions/context.
- Authoring includes demanded open Scene snapshots and prepared Schema-successor snapshots,
  exact nominal basis, source/persistence observations, flat raw/effective data and participating
  revisions. Save A does not save B. Selected unavailable/stale working copies refuse disk fallback.

## Review Corrections Still Being Integrated

Independent Runtime review found actual-reader Prefab/canonical-cache and containing-key budget
gaps. Both are replaced in the current source; regressions cover cross-root same-named Prefabs
and an 8 KiB containing location under a 4 KiB compiler work cap. Host identity copy admission
is separate from reusable compiler scratch.

The Authoring review found a default 32 KiB whitespace-file refusal caused by conflating parser
memory with decode work, missed preallocation of open Scene/successor inputs, native policy's
longest-prefix guess, double charging compiler results, Path root-self discovery and unrelated
pending-capture cancellation. Lagrange is fixing `src/project_host/authoring/**` with bounded
production decode evidence, explicit metadata declaration binding, exact transfers and tests.
Do not sample its source until the source-ready signal.

Parent owns Editor integration. `AuthoringSceneCaptureInputs` is admitted before all ordinary
selection/snapshot copies and moves into the worker request. Snapshot cost queries borrow source
data. The driver no longer rebuilds all open selections each frame: currentness checks borrow the
root and actual demanded observations. Pending work survives unrelated edits; exact selected
observations fence publication. Schema successor preadmits the added Scene array and prepared
snapshot payload. The old post-native-publication source-capture discard is removed: accepted
captures retain their actual dependency data until replacement/retirement. Gauss is independently
reviewing these Editor changes while the source owner fixes its separate write set.

## Actual Preliminary Evidence

All Cargo operations remain serial, one build job and root `target`. No current Cargo session
is running at capture time. Later source changes mean these are preliminary cohorts only.

| Cohort | Actual result | Nextest run |
| --- | --- | --- |
| Independent Source Scene consumer | 10 passed, 2 fixture failures out of 12 | `83eebc81-f4f6-4129-ab9f-fb4c16379159` |
| Root content/requests/Scene dependencies/source Export | 68 passed, 9 failed, 930 unselected; 77 executed | `806898bb-bfd3-487b-a646-3f42d1f8de80` |

The independent fixture fixes preserve normalized record order and reuse the original nominal
root capability for an existing plan. A rerun encountered the removed source-release method's
stale caller before execution; parent removed that caller rather than restoring data discard.
The 77 cohort passed all eight new Authoring Scene tests, source Product travel cases except a
foreign-budget fixture, the actual cyclic-source Export/relocation test, and the long containing
identity regression. Its failures were four exact catalogue counters (now include the actual
Scenes reader/entries), two new Runtime tests inheriting intentionally malformed unrelated
metadata, two Editor tests exposing premature capture discard, and one foreign-plan fixture
exhausting the live Host's two generations. Fixes keep exact counters, valid unused metadata,
actual retained data and a separate foreign preparation budget; no production limit is raised.
All corrected paths and later review fixes still need serial execution.

Temporary logs: `nara-source-scene-independent-tests.log`, `nara-source-scene-owner-tests.log`,
`nara-source-scene-integration-check.log`. Earlier import/type errors were build-only samples,
not executed tests. The previous source/UI qualification remains separate and unchanged.

# Next Action

Finish Lagrange source-ready fixes and Gauss Editor review; integrate and freeze all writers.
Run the corrected independent 12, root owner cohort, affected source/Editor/Schema/Export tests,
format, public consumers and required native coverage. Review the complete diff before precise
Conventional Commits and local ff main. Do not claim Source Scene qualified from the preliminary
cohorts or mark the complete Goal finished. Input/target delivery, render composition and Nara's
own custom-drawn Editor remain open.

# Citations

- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
- `docs/architecture/adr/0047-editor-workspace-and-scene-document-state.md`
- `verification/2026-10/2026-10-30T015832Z-canonical-source-locations-and-typed-ui-image-binding-qualification-0ce5bafdc65f4893aa0079009668e41a.md`

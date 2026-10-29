---
type: "Work Progress"
title: "First-party processed Runtime integration and explicit source facets in progress"
description: "Records the plan-owned Runtime codec agreement, complete typed collections, native residency and shared startup/travel replacement before integration qualification."
timestamp: 2026-10-29T12:21:06Z
record_id: "2c878dfe537f42eab9df31e120d1afb6"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "76a8571"
supersedes: "e5b31550eacc4fd1a3d4b6168fb4df52"
---

# Summary

Continue the complete active foundation Goal from local `76a8571`, on the existing
`refactor/foundation` branch. Main is still `76a8571`; the current production replacement is
uncommitted. No remote action, build-directory cleanup or reference modification was performed.
The pre-existing untracked vendor directory remains outside the work.

The user prioritizes foreseeable engine contracts and total evolution cost over minimal initial
code. Keep concrete workflow evidence and staged implementation, but resolve expensive identity,
dependency, ownership and execution-selection seams before freezing their interfaces.

# Details

An independent source-facet audit found default Asset/Image plugins install source mechanisms.
The adopted split preserves native `AssetPlugin`/`ImagePlugin` identities and makes
`AssetSourcePlugin`/`ImageSourcePlugin` explicit. Watch depends on the source facet; queued reload
failure, generation, last-good, task return and physical close semantics remain. Authoring Host
owns a real database/importer through its existing driver without queued reload execution.

An independent codec-ownership review selected a dedicated plan-owned definition set and
fingerprint over an otherwise unused ECS registry plugin. `RuntimeConfigurationPlan` retains
selected typed family/codec bindings, author-versioned definition IDs, concrete native/CPU roles
and frozen actual codec limits. Catalog fingerprint is required with no old-field fallback.
Selectors are globally unambiguous, multiple formats per family aggregate into one collection,
and generic acquisition must prove the original typed selection before callback execution.
ADRs 0007/0033/0103 and the delivery plan have been amended around these decisions.

Typed collections now have closed Source/Processed storage. Source aliases use real records and
pure family membership; processed maps complete Products to original acquisitions, retaining
their original base/output reservation. Iteration returns each decoded value once; source record
inspection is explicit. Runtime closure preparation traverses Required members once, prepares
each member once per selected family/definition, shares native acquisition views and validates
actual TileSet Image family/extent and Runtime Tilemap cells before publication.

Existing Scene requests now have Source/Processed contexts and public Source/Product selections;
worker cancellation keeps the same slot until physical return. Hosted processed Scene custody is
distinct from independent `RuntimeContentScene::scene_source()` so public independent aggregate
consumers remain valid while the first-party Host requires its resource residency transaction.
Startup and native residency writers are still active at this capture. Public headless/desktop
actions have new processed constructors and execution-only options; source Check receipts remain
optional and real rather than synthesized.

Writers at this capture: Boole `01a1877e-67e4-7ff2-b2cd-e9e6678c4613` owns source plugin split,
then exact-codec migration in independent runtime_base; Beauvoir
`01a18782-33bb-74f2-af86-1701bdcd477c` owns shared content_residency and its descendants; Rawls
`01a18787-02bd-7343-a6de-1b4ca0e0e89f` owns catalog format/tests and independent Image/TileSet
delivery; Lagrange `01a18789-d69a-7030-b957-95288a52296d` completed requests/hosted custody and now
owns a new independent processed_host test; Socrates `01a1878c-41a7-7f71-adb4-b7627de5c0b5` owns
runtime.rs plus processed_start tests. Parent owns codecs/prepared Runtime content, composition,
product recipe, public actions, other consumer migration and documents. No worker Cargo/fmt.

No Cargo verification has run in this unit yet. Previous TileSet qualification is unchanged and
does not qualify these new replacements. Do not commit or merge this WIP before final integration,
fresh focused source/processed consumers, native checks and independent review.

# Next Action

Finish/freeze writers, inspect the integrated diff, then format and run focused Cargo serially
with Rust1.95.0/jobs1/root target. Preserve all old meaningful budget, panic, physical cancellation,
native claim/retry and last-good tests while migrating fixtures to explicit codec selection.
Qualify startup plus later Scene travel, actual stock resources and typed compound CPU reads with
source unavailable. Reconcile real findings, update actual evidence, commit and locally ff main.
The full foundation Goal remains active; packaged Player/build selection/input-map and rendering
remain separately scoped pending work.

# Citations

- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
- `docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md`
- `docs/architecture/adr/0007-asset-identity-and-import-pipeline.md`
- `docs/architecture/adr/0103-concrete-domain-owned-asset-import-execution.md`
- Predecessor qualification record `e5b31550eacc4fd1a3d4b6168fb4df52`.

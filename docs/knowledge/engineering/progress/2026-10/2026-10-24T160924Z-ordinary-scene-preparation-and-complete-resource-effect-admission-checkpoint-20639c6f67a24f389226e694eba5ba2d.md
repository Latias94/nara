---
type: "Work Progress"
title: "Ordinary Scene preparation and complete resource-effect admission checkpoint"
description: "Owned unbound Scene preparation, separate execution and Host bases, and cached resource-target admission are implemented; final serial qualification remains in progress."
timestamp: 2026-10-24T16:09:24Z
record_id: "20639c6f67a24f389226e694eba5ba2d"
status: "implementation-in-qualification"
producer_id: "codex"
run_id: "session-01a1629b-ordinary-scene-prepare"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
---

# Summary

HEAD remains `ffe4fe2`; the ordinary Scene responsibility unit is implemented but not yet finally
qualified or committed. All current code/document changes are main-owned. Preserve the existing
untracked `vendor/`; no worker writes are pending. User permits precise local commits, not remote
publication. The full foundation goal remains active and must not be closed for this unit.

Public `prepare_scene` and `prepare_scene_replacement` return a move-only `PreparedScene` owning
actual unbound native values. Cancellation requires no World, even after the World drops. One-shot
commit rechecks current eligibility and acquires current Identity proofs before allocating Scene
targets. Ordinary spawn and authoring sync use the same kernel. Advanced native retirement remains
at the later target-bound stage with explicit discard custody. Final proof intervals use `&World`
after materializing deferred work; registered owner checks cannot hide another flush.

# Details

## Owning Corrections And Independent Review

- Initial read-only review `01a16e54-1c9d-70e1-a7ec-bf5cab054c52` identified late Hierarchy observers
  consuming target allocator slots before rejection, and managed authority without a registry
  falling back to standalone. Both were reproduced and fixed. Public run
  `201e8579-bf61-4603-8725-4130f688dc10` had four passed and those two failed before correction.
- A cancellation fixture's one apparent leaked entity was actually Bevy DefaultQueryFilters.
  The corrected fixture explicitly models that normal resource and compares entity roster and
  next allocation against a control; it does not merely replace zero with one. Native partial
  failure, retained assets, wrong World preflush refusal and reverse independent tokens are covered.
- Final native review `01a16e78-761c-7dc3-ab49-57de01280458` traced two additional resource-effect
  defects. Hierarchy publication initializes GenerationState and removes completion, so its shared
  fresh/bound support validator now admits those effects. Red run
  `0a8e8464-0558-4c0d-91b2-dab70ff0a7b4` failed both new tests; green
  `1ef7516a-3ade-4c23-b5f2-98a19a7587ee` passed both.
- Resource-value absence is not fresh entity storage in Bevy 0.20.0. Roots/GenerationState can
  reuse a cached entity with entity-scoped observers. The correction belongs in
  `nara_ecs::validate_resource_insertion`, which now supplies the actual cached target to its
  existing observer validator. Reflect asset/binding support, Identity and Scene occurrence also
  use this primitive. Red `3d871120-7415-4312-b496-46d2c6394082` failed both primitive and
  Hierarchy integration cases before the one-line owner fix.
- The primitive test has fresh/cached positive controls and both observer scopes. Its initial
  direct-trigger control incorrectly assumed entity-only lifecycle routing always sets Bevy's
  destination archetype flag: `f21485ab-d4e4-4969-a2e8-dc8776512a63` had 856 passed, one fixture
  failed, one ignored. Actual Bevy observer registration/bundle insertion source confirms the
  flag dependency. The fixture now enables that flag with a no-op component observer only after
  its negative-admission assertion for the entity-only control. Entity+component insertion needs
  no such aid. No production safety check or negative assertion was weakened.
- That reviewer source-checked the actual final cached-target fix and all entry points and
  reported no remaining scoped finding. It executed no Cargo. All review/research handles are
  closed. Pipeline simplification removed overwritten detached-domain state, misleading old-name
  aliases, a second deep predecessor clone and an unnecessary candidate Vec. The protected public
  proof cases were not deleted or weakened.

## Execution Definition Versus Host Composition

The original common guard applied the ordinary E==H policy to explicit product projection. That
broke real Editor previews: their finite typed-preview execution definition E is deliberately
separate from the fixed authoring Host's managed composition H, as ADR 0114 permits. Root run
`5c42037f-3c7c-4701-996e-7e056ebea689` was interrupted after these known failures rather than
treated as qualification. The public projection counterexample failed independently in
`d510e65f-c5c1-4555-8608-4e3a8b262575`.

The correction retains both facts, not an authority-check escape. `SceneHostCompositionBasis`
is genuine Standalone or the original valid managed snapshot. Capture rejects broken managed
state. Ordinary entry requires E==H before native preparation; both ordinary and advanced pass
the same captured definition into the common kernel, without resampling. Advanced/projection can
explicitly choose frozen E while final validation preserves original H and all binding/assets/
lifecycle/Identity/occurrence facts. ADRs 0081/0098 reconcile the old overbroad same-Host wording;
0038/0058 retain ordinary and Identity rules; 0100 adopts complete resource-effect admission.
No synthetic schema plugin, second source truth, global epoch or compatibility flag is introduced.

Two old UI tests materialized against a separate empty registry before App seal. They now use
StartupStage::Scene and the sealed managed snapshot, preserving their original key/provenance
assertions. The unchanged UI collision characterization is not the next key-adoption fix.

## Intermediate Evidence And Live Verification

- The independent module cohort passed 47 tests, one ignored timing experiment, in
  `4811c036-4148-4434-9f94-bd885f6fcef5`, before the final cached-resource primitive correction.
- The earlier 833-test owner cohort passed in `61d8cf0d-59cd-480e-b6bf-35602158d70c`, one ignored;
  selected real Editor preview cases passed 15 in `7e94cd09-589d-4486-87a0-7541e76b69e7`. These
  are not substitutes for the latest-source checks after the primitive/fixture correction.
- Current live exec session is `96706`. It sequentially runs final core (ECS/Identity/Hierarchy/
  Reflect/Scene/UI/tooling), the 588-case selected root/product/public cohort, then six selected
  actual hidden-window Windows/DX12 Editor trajectories. It stops on the first failed stage.
  Logs are outside the repo at `%TEMP%/nara-scene-prepare-final-core.log`,
  `nara-scene-prepare-final-product.log`, and `nara-scene-prepare-final-native.log`.
  Do not start concurrent Cargo. Rust 1.95.0, CARGO_BUILD_JOBS=1 and existing target are used.
  The command began at this checkpoint and has no final result yet.

Code review: skipped (ce-code-review unavailable)

The actual portable invocation `01a16e75-ae68-7920-b49b-76afb7a7b8ec` returned terminal failed
after three failed native child-agent capability discoveries. It produced no receipt/artifact
and launched no descendants. It is closed. The independent scoped reviews and explicit final
main-owned diff scan are separate evidence, not a portable completed receipt.

# Next Action

1. Collect session 96706 and inspect actual final results; fix supported failures, not expectations
   that conceal a contract regression. No production source changes are planned after this freeze.
2. Serially rerun the independent 47-case cohort on final source, then reference-game first_wave/
   authoring (31 prior cases). Run workspace all-feature/all-target check, Scene no-default/all-
   target check, affected Clippy with correctness denied, Scene doctests and all three workspace
   fmt checks. Native qualification is limited to actually executed Windows/DX12 trajectories.
3. Recompute the LF-normalized Rust source manifest: the earlier c470cbe4 manifest is stale after
   the final cached-target fixture change. Record all final commands, IDs, limits and source basis
   in a qualification shard. Update implementation ledger/design status only from actual results.
4. Read final diff and active plan scope. Check local links, render/check/validate engineering
   memory after final body updates, then stage only intended files and make a local Conventional
   Commit. Preserve vendor. Do not mark the current prototype or complete foundation goal done
   just because the owner tests or documents pass.
5. Continue the separately recorded Runtime/Canvas complete-key adoption and ADR 0102 refreeze.
   Read the next-unit finding record: complete context/placement, exact bytes versus shared
   context, 256-byte cost pressure, scoped encoded_len discrepancy, and logical/input/incarnation
   separation all remain design/implementation obligations. No new UI key implementation exists.

# Citations

- [Owning design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Ordinary preparation](../../../../../crates/nara_scene/src/spawn/preparation.rs)
- [Cached-target primitive](../../../../../crates/nara_ecs/src/__private.rs)
- [Independent public consumer](../../../../../tests/fixtures/module-consumer/tests/scene_preparation.rs)
- [Next-unit findings](../../subagents/2026-10/2026-10-24T154955Z-runtime-and-canvas-complete-key-lowering-findings-e010e14b557f42aea70f859c48c9f47f.md)
- [Active foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)

---
type: "Verification Evidence"
title: "Untargeted native preparation before Scene entity allocation"
description: "Reflect-owned native values retain their asset context until target binding; allocation and admission ordering regressions qualify the production prerequisite."
timestamp: 2026-10-24T06:00:59Z
record_id: "c2218c6824cd4aa7a8b9b657fedc9630"
producer_id: "codex"
run_id: "session-01a1629b-unbound-native"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "bcaa4c7"
verified_by: "red-green owner regressions; serial nextest; public consumers; workspace check; Clippy correctness; rustdoc; completed native full review"
---

# Verification

All Cargo commands were root-owned and serial with `CARGO_BUILD_JOBS=1`, reusing the checkout
target. Reviewers ran no Cargo. Results below distinguish intermediate evidence from final coverage.

- Strengthening the existing Scene native-apply failure regression first failed (`b9c9b350`): after
  rejection, the next target entity was `3v0`, compared with `1v0` in an untouched World. The
  earlier test already proved no surviving entities; it had not proved absence of target allocation.
- Untargeted value preparation made that same regression pass (`f1cc8cd2`). A subsequent ordering
  regression failed (`10a3153a`) because the first implementation ran native preparation before an
  already-failing identity-capacity check. Restoring identity admission before value preparation
  made both focused cases pass (`519062e6`).
- Three new Reflect ownership tests cover scratch asset association, callback count, incomplete
  target binding, partial-value disposal and foreign World refusal. Their first suite run stopped
  (`52137124`) on an incorrect zero-entity test assumption: Bevy's initial World has an existing
  baseline entity. The tests now compare exact initial entity-ID vectors. The Scene allocator
  regression remains unchanged and separately proves that specific allocation boundary.
- After that correction, the full Reflect/Scene suite passed 540 tests, 1 skipped (`700ad8d0`),
  followed by 27 independent consumer cases, 1 skipped (`7630da85`). After adding the ordering
  proof, the serial owner run passed 541, 1 skipped (`6b28e6d1`), and the independent cohort passed
  27, 1 skipped (`aca90349`). Consumers cover managed travel, native close/retry/cancellation,
  ordinary parse/spawn/end and property editing; six cases remain the explicitly experimental
  source-context trial, not production source-identity qualification.
- `cargo check --workspace --all-features --all-targets --locked` passed with warnings in 2m06s.
- Focused Reflect/Scene all-target/all-feature Clippy with `-D clippy::correctness` passed with
  warnings. This is not warning-clean evidence or a performance claim.
- Reflect rustdoc passed 4 cases and Scene rustdoc passed 3. Formatting uses `cargo fmt --all`.
- Review follow-up strengthens the existing retained-replacement retry case with a deferred
  asset-resolving native value and adds its cancellation companion. Both pass (`3f0aae3c`), proving
  one preparation, the same scratch handle across refused finalization/retry, and one disposal on
  cancellation or eventual instance retirement. A test-helper import error was corrected before
  this run and is not behavioral red evidence.
- Final owner qualification including that follow-up passes 542 tests, 1 skipped (`c13ddfe8`).
  Focused Reflect/Scene Clippy correctness passes again with warnings. The later delta is test-only;
  the earlier workspace check, public-consumer run and rustdoc results retain their stated scope.

# Result

Reflect now prepares native values without runtime target IDs while retaining the original
`ComponentApplyBatch`, World binding, asset baseline and scratch catalogue in one move-only owner.
Failed preparation reports its input index and disposes partial values without publishing the
scratch catalogue. An incomplete target list returns that same owner; successful binding only
moves values and cannot replay preparation or transfer handles into another batch.

Scene uses this phase after existing topology/identity-capacity admission and before allocating
its target entities. It preserves schema-admitted entity/component diagnostics. A failing native
value no longer forces scratch Scene entity allocation/rollback. Later preparation/publication
still revalidates the World, assets, targets, persistent bindings, hierarchy and native claims.
The existing immediate component stage uses the same value-preparation helper, not a parallel
codec protocol. Component metadata location retains an entity index rather than cloning each ID.

This is a production kernel prerequisite, not an allocation-free complete publication or a public
Scene prepare/preview/cancel workflow. Retained advanced Scene publication still owns scratch
entities after target binding and needs explicit disposal. Ordinary deferred World work and
arbitrary captured callback/destructor side effects are not transactional rollback promises.
No persistent file format, source identity, occurrence claim or UI key semantics changed.

# Evidence

The full native `ce-code-review` run `nara-unbound-review-20261024` completed with a
`status: complete` / `Ready to merge` receipt for the six-file production/contract snapshot against
`bcaa4c7`. Seven leaf lenses cover correctness, project standards, testing, maintainability,
performance, reliability and adversarial combinations. No primary/actionable finding survived.
The merge leaf's initial output-limit failure was resumed in the same task; final merge and report
artifacts completed. No empty result from the failed attempt was counted as approval. With no
selected findings, no validator was launched. Native same-family contexts do not establish
cross-model independence; no external review provider was used.

Reliability identified a concrete combined-coverage gap in the earlier component-free retained
replacement fixture. The caller addressed it through the existing Scene-owned prepared operation,
not a new harness or API. Bounded follow-up `01a16c7f-489e-77e2-a1c8-b1528af0d49d` reviewed that
test-only delta and found no P1/P2 defect. It checks the same retained candidate, real scratch asset
resolution, one preparation/drop and predecessor preservation on cancellation. This follow-up
does not extend the original receipt to unrelated files or claim existing-catalogue/native-editor
qualification. All review contexts were explicitly closed.

The independent canonical-Prefab design sidecar `01a16c09-ef09-79d1-b3aa-a298cc8a0a55` confirmed that
the future resolver must lend an immutable canonical key/document binding. Requested `AssetRef`
is a lookup coordinate; active recursion must compare resolved document/product keys. Repeated
siblings and discovery deduplication remain distinct from ancestry, and effective patched-source
association must allow valid override-added entities. These recommendations are recorded in the
Proposed source design, not implemented by this commit or accepted as all of ADR 0083.

# Follow-up

Continue the actual source-owned versus World-bound preparation comparison, including ergonomic
ordinary prepare/preview/commit/cancel and retained code/data custody. The hidden unbound component
batch is an internal seam for existing owners, not the new ordinary Scene API.

Adopt real project universe/native identity policy and canonical source admission before replacing
Prefab requests/results and provenance. Preserve the stable Prefab rename workflow when migrating
the existing mixed native input; do not invent a fallback or second permanent source registry.
Then integrate occurrence publication/end and Runtime/Canvas keys with the owning ADR amendment
and neutral refreeze. R2/R5b and all later foundation waves remain open; the full goal stays Active.

No new native Editor trajectories or runtime performance measurements were run for this unit.
Existing public managed-travel evidence qualifies its window-free contract only. Preserve untracked
vendor contents and concurrent work; no push or remote publication is authorized or performed.

# Citations

- [Reflect preparation owner](../../../../../crates/nara_reflect/src/persistent_apply.rs)
- [Scene preparation ordering](../../../../../crates/nara_scene/src/spawn.rs)
- [Allocator and admission regressions](../../../../../crates/nara_scene/src/tests.rs)
- [Retained replacement and cancellation tests](../../../../../crates/nara_scene/src/spawn/publication.rs)
- [Independent managed-travel consumer](../../../../../tests/fixtures/module-consumer/tests/domain_reconciliation.rs)
- [ADR 0081](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Source/occurrence design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)

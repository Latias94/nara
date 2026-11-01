---
type: "Work Progress"
title: "Registry construction and immutable authority replacement in integration"
description: "Consuming builder admission and private App ownership landed; downstream readers and independent consumers are migrating before qualification."
timestamp: 2026-11-01T01:55:31Z
record_id: "282689551434409ab234964eb2e997ab"
producer_id: "codex-foundation-main"
run_id: "registry-ownership-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-11-01-component-registry-ownership-plan.md"
git_branch: "refactor/foundation"
git_commit: "5acab02"
---

# Summary

Continue the complete active foundation Goal from clean baseline `5acab02` on
`refactor/foundation`; `main` is the same baseline. Only the preexisting untracked vendor tree
was present at entry. The current registry ownership replacement is uncommitted and not
qualified. The [selected unit](../../../../plans/2026-11-01-component-registry-ownership-plan.md)
belongs to foundation R12 and preserves R5/App/Scene authority.

# Details

- ADR0081 now selects separate consuming `ComponentRegistryBuilder` and immutable
  `ComponentRegistrySnapshot`, private App phases and explicit installed-but-unadmitted refusal.
  It retains complete admission, exact definitions, provider atomicity and World eligibility.
- Parent core removed public `ComponentRegistry`, `from_snapshot`, Frozen/NotFrozen errors and
  obsolete mixed-reader branches. Snapshot owns queries, migrations and codecs; builder owns
  construction and isolated provider callbacks. `freeze(self)` returns the sole snapshot or
  `ComponentRegistryFreezeError`, with `error`, `into_parts` and explicit `into_error` disposal.
  Failed validation preserves the original candidate. Callback replacement tokens remain private.
- Provider APIs are `preflight(&Builder)`, `register_into(&mut Builder)` and
  `validate_snapshot(&Snapshot)`. Hidden resolved registration is
  `register_resolved_schema_provider`; no old alias remains. Private common validators preserve
  receipt/mode behavior. Registry snapshot debug formatting contains catalog metadata only.
- The private App resource owns a stable installation token and Building/Preloaded/Admitted
  phases. Admission mutates that owner in place; mutation revision hooks and sticky runtime
  authority remain. Failure restores its builder before returning the original error.
  `component_registry` returns only admitted snapshots; `component_registry_is_installed`
  separately preserves owner presence. Provider plugin preflight is the narrow
  `validate_schema_provider_for_plugin`, replacing the old mixed-registry borrow.
- Parent Host integration removes SchemaValidationInput's registry wrapper and duplicate native
  snapshot field; its canonical `snapshot()` borrows `bindings.native_snapshot()`. Runtime
  equality now directly compares exact snapshots. Root persistence/preview/Inspector consumers
  accept snapshots rather than constructing wrappers.
- Source-ready workers migrated first-party domain registration/inventory, Scene readers and
  execution, Reflect owner tests, Tooling/Editor and selected root test files. Scene adds public
  tests for Building/unfinished Preloaded prepare/export refusal and owner installation between
  preparation and commit. No matching or native validation is intentionally weakened.
- Reflect all-feature library check passed19.51s. Root/Editor library check reached the Root and
  found one remaining inferred `shares_snapshot` call; parent changed it to `ptr_eq`. A rerun is
  live at this capture. No owner test/native/independent qualification or global fmt has run.

# Next Action

- Poll exec53651 Root/Editor all-feature lib check before another Cargo. Jobs1/root target only.
- Collect the remaining external-consumer source-ready result, freeze all code writers, inspect
  diffs, then fmt and serially run Reflect/Scene/domain/Tooling tests, actual public/independent
  consumers and affected native Editor trajectories selected by the unit. Fix compiler errors in
  their owning write sets; no compatibility aliases, fake snapshots or phase-guard relaxation.
- Parent owns registry.rs/provider.rs/lib.rs, root source and docs. Harvey
  `01a194ca-25ef-7cd2-9317-73ed1827bab1` owns frozen plugin.rs and now independently reviews Scene.
  Dalton `01a194ca-5aa2-7c71-9d34-37a1c0545e9e` owns frozen Scene except scene_components and a
  one-name test fix. Nietzsche `01a194ca-8fa3-7cb3-885e-a4dd0e7f8280` owns frozen domain modules
  and selected root test files. Avicenna `01a194ca-bb49-7622-80f2-d053e64017fe` owns frozen Reflect
  tests/asset_reference. Ramanujan `01a194cc-d6e4-7683-ae0c-b12a34bddc90` owns frozen Tooling/Editor
  and now independently reviews core/App. Sagan `01a194cd-0684-7482-976f-03370f294bee` owns
  tests/fixtures/root public tests/examples/Reference/samples migrations. No agent has Cargo,
  fmt or commit authority.
- Preserve vendor and read-only references, no push/PR. Complete this actual replacement before
  local commits/ff main. The complete foundation Goal and Proposed0120/0121 scopes stay open.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Registry ownership unit](../../../../plans/2026-11-01-component-registry-ownership-plan.md)
- [ADR0081](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Prior qualified render unit](../../verification/2026-11/2026-11-01T013746Z-shared-stock-composition-original-captures-and-native-scene-effects-qualification-2f250ecb33b24b5091f28a141cd6b244.md)

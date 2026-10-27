---
type: "Work Progress"
title: "Caller-owned property value budget correction in progress"
description: "Source Revert cohorts pass; final review found an implicit property payload ceiling and a shared resolver budget correction is being implemented."
timestamp: 2026-10-27T00:38:31Z
record_id: "97d2113cb1104680bf471d7d3b7f036f"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/declarative-prefab-overlays"
git_commit: "3636b9a8d8550c5cccac9fd1e8d43b4e6d4d9c3d"
supersedes: "c8fb754f923b49de9812cb196b66a3d7"
---

# Summary

The full foundation Goal remains active. Continue on `refactor/declarative-prefab-overlays`,
HEAD `3636b9a`; main is still `47a2000`. All current implementation changes remain uncommitted.
Do not commit or merge while the newly confirmed property-budget defect is unresolved.
Keep original untracked `vendor/` untouched and `repo-ref/` read-only.

# Details

The source observation/Revert, rejected capture and Inspector integration described in the
[prior progress record](2026-10-26T234134Z-source-owned-prefab-revert-integration-and-review-corrections-c8fb754f923b49de9812cb196b66a3d7.md)
now has passing owner, Host, public, independent and selected native evidence. Scope remains
source-local Revert, not conversion, foreign-source navigation or complete foundation delivery.

The parent also completed an intentional API/diagnostic cleanup: `PrefabExpansionFailureKind`
replaces `PrefabOverlayConflict` without an alias and is owned by `prefab/declaration.rs`.
The generic diagnostic is now `scene.prefab-compilation-failed` with `failure-kind` and a neutral
publication-refusal message. Current consumers were updated; immutable historical memory was not
rewritten. Kind/getter semantics still distinguish semantic failure, source admission and work
limits, with no fabricated budget/authority repair location.

## Executed Evidence Before The New Property-Budget Correction

- Final new-kind Reflect/Scene/Tooling: **903 passed**, four ignored; run
  `e09be9e5-827f-4ed8-ae9f-1cf8e4940618`, build 2m40s, execution 55.437s.
- Final new-kind root/Editor ordinary libraries: **770 passed**, 34 ignored; run
  `691f7aca-2d89-4796-b11a-6c71c908a775`, build 2m46s, execution 204.598s.
- Previous final focused/native group: **22 passed**, including six actual Windows/DX12
  trajectories; run `7c13febb-51d3-4244-b95b-f1fdae95ea38`, build 19.73s, execution 51.355s.
  This ran after the Source classification label and tab corrections, before the mechanical
  failure-kind/diagnostic rename.
- Independent module consumer: **19 passed** across `prefab_sources`, `property_authoring`,
  `durable_identity`, `nested_schema`; run `a289444b-f546-429f-aeb4-2f065f4f8f56`, build 4.73s,
  execution 1.637s. An initial test-only fixed-two-record helper assumption was corrected to
  verify the third retained unknown overlay record explicitly.
- Selected Reference Game: **13 passed** across the seven established tests; run
  `0b3b43c4-88c1-4f50-9b6e-7770d635c1af`, build 2m13s, execution 1.880s.
- Workspace all-feature/all-target check passed in 1m22s. That and the independent/Reference Game
  results precede the final mechanical failure-kind rename.

These cohorts overlap and do not qualify the pending property-budget correction. No Cargo command
is currently live. Reuse root `target`, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, strictly serial.
All earlier writers/reviewers are closed except Russell below.

## Open Confirmed Defect And Active Writer

Russell, `01a17ab7-3661-7650-8ef1-05bfa8196b7c`, found that ordinary Reflect property resolution
rechecks the complete value with a fixed 16,384-node admission limit. A valid component with two
roughly 9,000-element List fields and a scalar `x` can pass the Prefab owner's explicit limits but
fail an overlay Set of `x` as `ComponentPropertyError::ValueBudget(NodeLimit)`. Wildcard mapping
then mislabels this as a source `Component` failure and attaches a Revert location.

Russell is now the sole code writer for:

- Reflect `property.rs`, `effective.rs`, `lib.rs`, focused property/effective tests.
- Scene overlay `properties.rs`, `components.rs`, and focused component-program regressions.

Do not overwrite those files before source-ready/freeze. No Cargo/fmt/commit is delegated.
The selected correction preserves ordinary arbitrary-value APIs' real default admission while
adding explicit caller-limit resolution/owned replacement through the existing walker and
materialization-cost traversal. Current WIP names include
`resolve_component_property_with_limits` and `replace_in_owned_value_with_limits`.
All compiler property resolution/replacement/key-preservation paths must receive the owner's
residual limits. Typed budget refusal must remain `WorkLimit` without a repair location;
semantic selector/value errors remain distinguishable. No second parser, proof wrapper, new
generic budget framework or unrelated zero-residual redesign is requested.

The parent reconciled ADRs 0011 and 0038 for this ownership rule. Implementation ledger and final
qualification record have not been updated to claim the correction implemented or verified.

# Next Action

Collect Russell's source-ready signatures and review its exact change. Then format intended
source and run focused large-value/property and failure tests before broader relevant owner,
Host/public and affected native checks. Include a positive large-component native/Authoring
case under sufficient explicit caps, a true WorkLimit refusal without location/slot, and real
semantic-error controls. Requalify the renamed API through minimal Scene, workspace targets,
independent consumers and Reference Game as appropriate. Preserve baseline warnings.

After all blockers close: update current contracts/guides and implementation evidence, validate
and render memory, inspect and stage precise paths, Conventional Commit, and optionally locally
fast-forward main under the user's standing authorization. Do not push or open a PR. Main is not
attached elsewhere; only this worktree exists.

Formatting intentionally includes two small previously unformatted affected standalone fixtures,
`nested_schema.rs` and `prefab_sources.rs`; their diffs were inspected. Root `cargo fmt --all`
reaches these files through existing module paths. Do not repeatedly undo this known formatter
output or treat it as concurrent user work.

Remaining separate scope: automatic foreign-source navigation; complete nested dropdown
Revert/Undo/Redo/Save/Reopen control trajectory; convert-to-local; shared Canvas capture; inherited
Inspector selection; repeated Schema/value work; all remaining foundation delivery waves.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Prefab design](../../../../architecture/prefab-overlay-design.md)
- [ADR 0011](../../../../architecture/adr/0011-component-schema-ids-and-migrations.md)
- [ADR 0038](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)

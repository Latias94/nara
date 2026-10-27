---
type: "Work Progress"
title: "Selected Prefab semantic compilation and exact history footprint comparison in progress"
description: "Private selected-demand and same-owner replay implementations are integrated; first test build exposed fixture API errors and executable qualification is pending."
timestamp: 2026-10-27T02:13:13Z
record_id: "d4d8d5cf51814d6aa3eb72e85e3d84f3"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-27-prefab-conversion-plan.md"
git_branch: "refactor/prefab-convert-to-local"
git_commit: "13c6fb8"
supersedes: "48bc7f5499704cd39c2f617a29916625"
---

# Summary

The complete foundation Goal remains active. `main` and branch
`refactor/prefab-convert-to-local` were fast-forwarded to `13c6fb8` after the qualified
source-intent checkpoint `f2fc60a`. No push or PR occurred. All comparison implementation below
is uncommitted and must not be represented as qualified conversion support.

The [conversion plan](../../../../plans/2026-10-27-prefab-conversion-plan.md) and its three
current-mechanism probes were committed at `13c6fb8`; their passing run is
`42469edb-ab1e-4303-883f-7f602780128f`. The new private selected compiler and same-owner history
comparison have not yet passed an executable run at this capture boundary.

# Details

## Selected Semantic Compilation

Worker Noether (`01a17b0a-a7fa-7990-896e-810a63c5a616`) owns only:

- `crates/nara_scene/src/prefab.rs`
- `crates/nara_scene/src/prefab/overlay/compile.rs`
- `crates/nara_scene/src/prefab/overlay/compile/staged.rs`
- `crates/nara_scene/src/prefab/overlay/compile/staged/values.rs`
- `crates/nara_scene/src/prefab/overlay/compile/selected_tests.rs`

The crate-private entry is `try_compile_selected_authoring_instance_with_loader`, taking a real
`SelectedInstanceSource::{Scene, Prefab}`, direct anchor, exact Authoring basis, existing options,
optional database and mutable-working-set loader. `SelectedInstanceCompilation` owns an optional
`SelectedInstanceFragment { roots, entities, provenance }`, typed omissions/failure, diagnostics,
effective assets, work bytes and budget status. No partial `ExpandedSceneCandidate` is constructed.

The same staged compiler now borrows a `SourceTopology` and optionally selects one direct anchor.
Selection covers slot opening, binding demand, component programs and value interpretation.
Real source shells/IDs remain reference/topology context; unrelated payloads and independent links
on ordinary local children remain inactive. The final private fragment contains only typed
projected records owned by the selected root anchor. Eleven new tests compare full/selected
values, layout, references/assets and ancestry, plus missing/conflicted sibling/local-child
isolation, actual dependency/binding demands, omissions, exact basis and existing limits.

Noether reported source-ready/frozen before the first build. That build found fixture
`BTreeMap<AssetRef, ...>` uses even though `AssetRef` has no `Ord`; Noether is correcting only its
test source container to an appropriate existing key/container. It must report frozen again
before formatting/building. Do not add `Ord` to the production reference type for this fixture.

## Existing Source History

The parent owns `authoring.rs`, `authoring/prepared.rs`, `authoring_decode.rs`,
`authoring_decode/structure.rs` and new `authoring/prefab_conversion.rs` plus its tests.

- `AuthoringHistoryScope::{Ordinary, PrefabConversion(footprint)}` travels with the existing
  prepared edit and the same history entry. Ordinary APIs still create only Ordinary scope.
- The closed footprint stores the actual anchor and created local IDs, with modeled retained
  bytes. It admits exactly one complete detach/restore declaration and one insertion/removal per
  created entity. Directions cannot mix; insertion is parent-first and inverse removal leaf-first.
  Inserted records have no nested Prefab/children; hierarchy is expressed by exact placements.
- `prepare_scoped_authoring_edit` is visible only inside the source owner. Conversion structure
  admission reuses the existing application, final classification, manifest preservation and
  inverse creation. It does not set `history=true` or relax ordinary populated Add/template/
  deletion/Restore guards. Replay receives the retained private scope through the existing stack.
- Scope bytes reduce the forward preparation allowance and contribute to forward/inverse maximum
  history cost and replay cost checks. The existing old-entry eviction policy is unchanged.
- A current interpreted record can be copied without an initial creation template or arbitrary
  field Edit capability. It still requires an actual data adapter, Scene capability, exact current
  component version and explicit matching Schema owner; complete final source validation remains.
  This is a private comparison choice requiring owning-ADR reconciliation before public support.

Six source-history tests use manually localized records deliberately to isolate replay from
compiler/ID generation. They cover Scene/Prefab one-entry Undo/Redo with unrelated linked and
unavailable records, same-anchor raw preservation, ordinary inverse non-authority, narrow patch
shape, metadata bytes, final reference/future-version refusal and empty projection history.
They do not prove real conversion or a dependency fence.

Read-only reviewer McClintock (`01a17b1c-3ad3-7410-83f9-10d716b7bebc`) is reviewing only those
parent-owned history/admission changes. Collect concrete findings before claiming readiness.

## Executed State At This Boundary

The parent ran `cargo +1.95.0 fmt --all` after the first source freeze, then serial nextest:

```text
cargo +1.95.0 nextest run -p nara_scene --all-features --locked --lib -E 'test(prefab_conversion) | test(selected_)' --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

Exec session `71653` ended **101**, with 21 test-fixture compile errors and no executed tests:
the AssetRef ordering issue above, and the parent's mistaken `ComponentSchema::with_capability`
and nonexistent `ComponentCapability::Save` calls. The parent corrected its fixture to actual
`with_capabilities([Scene, Inspect])` and read-only field capabilities. No production error was
reported in that build, but this is not a successful compilation or semantic result. No Cargo
command is live. Reuse root target and `CARGO_BUILD_JOBS=1`, strictly serial.

# Next Action

Collect Noether's fixture freeze, then format and rerun the focused compiler/history cohort.
Resolve actual failures without relaxing ordinary guards or masking missing selected data.
Collect the independent history review. Review the final selected demand and allocation behavior
independently after the first passing execution.

Then integrate a private source-owned localizer consuming the complete semantic fragment, generating
fresh persistent local IDs, rewriting only Schema-declared references and lowering the exact
transaction/footprint. Preserve raw anchor/components/local children and unrelated records.
The selected compiler and replay remain experiments until that complete owner workflow passes.
For unclaimed but interpretable source records, explicitly settle whether new local records acquire
the exact admitted Schema-owner claim; do not silently bypass the explicit-owner insertion gate.

Public Scene/Prefab conversion interfaces, Host nominal-source capture/actual dependency fences,
Editor review and native history/persistence are still absent. Do not adapt a contextual conversion
into a plain prepared token in a way that drops dependency evidence. Do not use World export,
native-filtered preview, synthetic source placeholders, a second compiler/history owner or a
generic permission framework to complete the workflow.

Preserve untracked `vendor/`; `repo-ref/` is read-only. No migration guide, compatibility reader,
legacy aliases, separate target directory, concurrent Cargo, remote push or PR.

# Citations

- [Comparison baseline](2026-10-27T014146Z-complete-prefab-conversion-source-constraints-and-comparison-plan-48bc7f5499704cd39c2f617a29916625.md)
- [Conversion plan](../../../../plans/2026-10-27-prefab-conversion-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)

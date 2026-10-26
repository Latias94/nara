---
type: "Work Progress"
title: "Staged Prefab structural demand and selected value program qualification"
description: "Production interleaved expansion is replaced; owner qualification passes while reusable Editor capture and preview remain open."
timestamp: 2026-10-26T17:59:42Z
record_id: "19ce2446522e4c28a5fbd70d31ab827f"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/declarative-prefab-overlays"
---

# Summary

The full foundation Goal remains active. This is a bounded production composition/source-admission
checkpoint, not completion of Prefab authoring or the foundation rebaseline. Branch
`refactor/declarative-prefab-overlays` remains based on local main `47a2000`; the large replacement
is still uncommitted. No merge, push, PR, native Editor or performance qualification was performed.
Existing untracked `vendor/` remains unread and untouched; all builds used the root `target` with
`CARGO_BUILD_JOBS=1`, and no Cargo commands ran concurrently.

The previously executed cross-instance demand failure is resolved by replacing the interleaved
DFS, rather than adding suppression-prefix heuristics. The production entry now registers borrowed
source shells, closes explicit structural paths, composes inner-to-outer hierarchy, enters remaining
live slots, composes component presence and selects value programs before interpretation.

# Details

## Implementation

- New production owner: `prefab/overlay/compile/staged.rs` and `staged/{values,output}.rs`.
  Typed original ancestry remains independent of physical placement and anchor survival.
  ADR 0101 declaring roots, nested roots and containing children retain distinct sequence roles.
- Component Add/Replace/Suppress presence and same-layer declaration conflicts remain checked even
  if a later whole-component operation discards an earlier value program. Duplicate/overlapping
  property paths are checked independently of Schema interpretation. A retained Properties suffix
  still needs an interpretable base.
- Defaults precede exact binding consumption. Only retained programs introduce binding demand.
  Final surviving IDs are allocated before value interpretation; distinct unpublished IDs represent
  dead intermediate reference targets. Final reference validation rejects retained dangling targets
  while allowing later Properties to overwrite an intermediate dead reference. Private skeleton
  ID lengths no longer inflate materialized values.
- Borrowed root declarations are not copied or charged as materialized values before selection.
  The selected program checks node/byte/depth admission before its first recursive clone.
  `MaterializedValueDepth` preserves the fixed interpretation-depth failure as a typed budget fact.
- Runtime Prefab decoding now admits bounded raw composition sources and local structure, without
  eager native component canonicalization. Only selected native component migrations contribute to
  `PrefabExpansionReport::source_upgrade_required()`.
- Authoring dependency backing retains one `Arc<PrefabDocument>`, exact shared Schema basis,
  canonical key and source receipts. `publish_document()` and `fork_prefab()` are fallible strict
  standalone publication. Editor Open/fork consumers propagate that error and source diagnostics.
- Removed the old production DFS, selection/destination suppression authorities, component fragment
  interpreter and obsolete provenance construction helpers. Nine component helper tests now exercise
  the actual public expansion entry with real frozen registries and complete references.
- Provenance formulas now model selected request/key bindings and complete final paths. The removed
  canonical occurrence BTreeMap is no longer charged; shell/scope ancestry uses working scratch.
  Exact and one-under tests, long-key pre-structure refusal, the default 64 MiB provenance limit and
  World/identity preservation controls remain strict.

## Review Corrections

Independent reviewers Lovelace (`01a17933-1d1d-7472-aa92-f44df774107c`) and Herschel
(`01a17950-cb11-7810-afe0-81d1fb25f308`) inspected bounded production paths read-only. Their
source-confirmed corrections include actual request-path admission before clone/loader entry,
property declaration conflicts surviving whole-component shadowing, selected raw depth admission
before clone, and preserving generated-ID budget errors from the final reference visitor.
New production regressions cover these cases; Herschel's final narrow source recheck closes its
two findings without claiming test execution. All agents used in this checkpoint are closed.

Initial expanded qualification exposed private-locator inflation, source diagnostic regressions
and obsolete occurrence-map formulas. These were corrected at their owners; tests were not skipped
or relaxed to accept a different budget kind. The separate consumer initially compared a changing
complete declaration inventory with its pre-edit manifest. Its corrected helper separately asserts
exact unavailable entries/counts and exact declarations matching the accepted source, both after
Save and after Reopen.

## Verification

- Scene all features: run `6c2a6c16-c544-4c71-9aa1-a402954cb46e`, **497/497 passed**
  across 26 binaries; three existing ignored tests remain independent prepare/commit/allocation
  observations requiring their own execution. This is not a measured performance result.
- Actual-file Host cohort: run `6798f99a-4272-44ca-990c-baee7d14af4b`, **52/52 selected passed**,
  434 filtered. Covers `project_host::authoring::` and `project_content::tests::runtime_content_`,
  including invalid inherited payload repair versus strict standalone Open, actual selected closure,
  source receipt fences and live compiler/decode reservation boundaries.
- Independent module consumer: run `9fa47e03-d167-433d-9fef-0227674d07b7`, **18/18 passed**,
  zero skipped, across `durable_identity`, `nested_schema`, `prefab_sources`, `property_authoring`.
  Uses its existing separate lockfile and root `target`; source/inverse/history and public
  Save/Reopen assertions exercise ordinary Editor APIs without native window qualification.
- `cargo +1.95.0 check -p nara_scene --no-default-features --lib --locked` passes after the final
  core corrections, with the existing Reflect provider/Scene diagnostic dead-code warnings.
- Root all-feature library check passed earlier in this checkpoint; the final Host test build also
  compiles the current root library. Previous Reflect 291/291 evidence remains in the predecessor;
  this checkpoint changes no Reflect runtime logic.
- `cargo +1.95.0 fmt --all` ran after writer freeze. The excluded consumer's changed property test
  was formatted separately. Final document/link/memory checks are recorded with the capture.

Commands used the selected Rust 1.95.0 toolchain, `--locked`, serial builds and `--test-threads=1`.
The Source, budget and independent-consumer corrections are in the current dirty working tree,
not in a newly created commit. No full workspace, Reference Game or native rerun is implied.

# Next Action

1. Keep the complete foundation Goal active. Implement reusable actual-demand capture through a
   real second-document workflow, rather than relying on the startup closure cache.
2. Integrate nested native preview and fence actual captured sources, exact basis, containing
   source lineage/revision and closure generation. On-disk receipts do not prove unsaved sources.
3. Finish structured conflict discovery/observation and source-owned convert-to-local. Preserve
   exact source/history and explicit Apply/Revert/source context semantics from the Unity/Godot
   study; do not make widgets coordinate persistence or reconstruct dependency graphs.
4. Resolve the known repeated-descriptor work gap: effective asset admission does not cover later
   whole-value validation/legacy EntityRef descriptor scans. It remains a source-confirmed P2 with
   no new executed counterexample or closure here.
5. After the complete owning workflow and independent review, run affected external/Reference Game
   and native qualification, inspect the final diff, then create precise Conventional Commits and
   locally fast-forward main. Remote publication remains unauthorized.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Current Prefab design](../../../../architecture/prefab-overlay-design.md)
- [Unity/Godot Editor study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
- [Predecessor checkpoint](../../sessions/2026-10/2026-10-26T163304Z-shared-prefab-compiler-owner-and-host-verification-before-staged-structural-demand-replacement-45d427ddb20d40b0acde05d288566775.md)
- [Staged compiler](../../../../../crates/nara_scene/src/prefab/overlay/compile/staged.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/tests/property_authoring.rs)

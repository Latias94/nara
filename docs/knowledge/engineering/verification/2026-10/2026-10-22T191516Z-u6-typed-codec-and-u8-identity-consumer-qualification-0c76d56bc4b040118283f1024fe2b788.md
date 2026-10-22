---
type: "Verification Evidence"
title: "U6 typed codec and U8 identity consumer qualification"
description: "Typed registry closure and installed identity operations pass owning suites and the independent durable workflow."
timestamp: 2026-10-22T19:15:16Z
record_id: "0c76d56bc4b040118283f1024fe2b788"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "8d3515c"
---

# Verification

Windows 11, Rust 1.99, one serial Cargo job, existing root target. Independent module consumers
retain their own manifest/lockfile and dependency resolution, including their separately pinned
proc-macro dependencies. No native Editor claim is made by this slice.

# Result

U6 retains the component T across public preparation and encoding; heterogeneous erasure is private.
The registry selects the encoder's component and owns absence. Deferred owning closures and a shared
scratch AssetServer still participate in failure-atomic batch publication. The old mismatch variant
and manual consumer selection are removed, including diagnostics and guide text.

U8 supplies World-oriented adoption, registration and retirement operations. First adoption admits
target Add/Insert effects even before a marker exists. Exact retirement preparation reads the domain
without extracting it, and normal index publication takes only a short mutable borrow. Detached
candidate operations remain explicit. Scene registration no longer removes/reinserts an installed
domain just to commit indexes.

# Evidence

- Identity and Scene owning serde cohort: 311/311 pass, one unrelated platform skip, run
  `8e2602f2-ce1e-4462-b66c-c67376746eb8`. Command selects `-p nara_identity -p nara_scene
  --features nara_identity/serde,nara_scene/serde --test-threads=1 --no-fail-fast`.
  New assertions cover first target observers, zero resource lifecycle callbacks on rejected
  preparation, unchanged indexes/entities/roots, installed domain after failure, stale incarnation
  and moved-World rejection.
- Reflect full serde nextest cohort: 196/196 pass, run `f52fa409-2dd3-4d61-8f39-80b1e3151e8c`,
  including existing derive UI, binding/lifecycle, scratch asset and durable-reference tests.
- `cargo test --locked -p nara_reflect --doc`: both new mismatched-candidate compile-fail cases
  pass; one existing inventory example remains ignored.
- Independent `module-consumer` package, `--test durable_identity --test manual_component_codec`:
  10/10 pass in `b7cb07c9-ecf8-447a-92e9-9c328fe0bff6`. Durable restoration/deletion uses the new
  World functions without `resource_scope`. Mixed eager/deferred asset candidates share handles,
  retain dropped input data, and reject a later candidate without publishing earlier components/assets.
- Render/Sprite/Tilemap cohort passed 140/140 in `9086becb-d7b9-42ca-8f46-f14940309ed2`, including
  typed Sprite/Tilemap codec consumers. Subsequent U4 review fixes need their own fresh qualification.
- Independent Identity review found a missing Mut import in one leftover Scene resource_scope.
  Replacing that scope with the short installed-domain commit fixed both the compile error and
  unnecessary lifecycle boundary; final 311-test run includes the fix. Final focused review has no
  blocker. Independent U6 review has no implementation blocker; its stale guide finding was fixed.

# Follow-up

The active Goal still includes U3/U4/U5/U10/U11 integration, measurement and native evidence.
Broader package/history/restore opportunities in the existing ADR ledger retain their prior scope.

# Citations

- [Plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
- [Typed registry](../../../../../crates/nara_reflect/src/registry.rs)
- [Identity operations](../../../../../crates/nara_identity/src/domain.rs)
- [Manual codec consumer](../../../../../tests/fixtures/module-consumer/tests/manual_component_codec.rs)
- [Durable identity consumer](../../../../../tests/fixtures/module-consumer/tests/durable_identity.rs)
- [Guide](../../../../guides/persistent-components.md)

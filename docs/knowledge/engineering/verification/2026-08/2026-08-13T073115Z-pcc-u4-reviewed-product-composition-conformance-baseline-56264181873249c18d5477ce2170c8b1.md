---
type: "Verification Evidence"
title: "PCC-U4 Reviewed Product Composition Conformance Baseline"
description: "Supersedes the initial PCC-U4 evidence with the reviewed narrow-feature, backpressure, and product-profile module proof."
timestamp: 2026-08-13T07:31:15Z
record_id: "56264181873249c18d5477ce2170c8b1"
tags: ["product-composition", "pcc-u4", "baseline", "review", "product-recipe", "wgpu"]
status: "completed"
producer_id: "codex-root"
run_id: "product-composition-contribution-correction"
related_plan: "docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ae4caf0"
verified_by: "cargo-nextest-cargo-check-clippy-formal-review-memory"
supersedes: "180d0dfe0a41417a9633c11cccb91eab"
---

# Verification

This record is the reviewed successor to Product Composition Conformance Baseline
`180d0dfe0a41417a9633c11cccb91eab`. The initial record remains immutable historical evidence, but
its proving commit contained a root integration test that referenced Sprite and UI submitter IDs
outside their owning Cargo features. The documented base-only `render-wgpu` ceiling therefore did
not compile at that commit.

Commit `ae4caf0` closes the review findings without changing the product-composition decisions:

- the base-only composition assertion now uses feature-independent plugin identities and compiles
  under the exact narrow ceiling it claims to prove;
- `ProjectProfilePlugins` and its closed contribution-intersection lowering moved into the private
  `product::profile` module, keeping the public recipe and product-group surface focused;
- the WGPU UI collection early-return has an integration regression that preserves the existing
  exact outcome reservation and leaves the next backend frame unread until the prior transaction
  completes.

The Runtime UI Product Slice continues to pin UI Foundation baseline
`a628828a8ff941239d79a0b8b3d60316`. This record changes only the independent
`composition_baseline_record` and does not claim Runtime UI Product Validation or Editor
completion.

# Result

- Cargo features remain a compiled availability ceiling; requested capabilities and the resolved
  plugin plan alone select concrete WGPU and Winit contributions.
- The documented `render-wgpu`-only request compiles and excludes both WGPU submitter
  contributions under a narrow ceiling and under `--all-features`.
- Product-profile lowering is a private deep module. `ProductRecipe`, concrete product groups, and
  direct-`App` contribution plugins remain the existing public interfaces.
- A blocked WGPU UI collection cannot drain the queued successor frame or complete either attempt
  early. The old and new attempts retain distinct exact-once outcome permissions.
- The final formal review has no unresolved P0 or P1 finding in the PCC scope.

# Evidence

All Cargo commands ran serially with `CARGO_BUILD_JOBS=1` and reused the workspace `target`
directory.

- Narrow compiled ceiling:
  - `cargo nextest run --locked --no-default-features --features render-wgpu --test
    plugin_composition -E 'test(render_wgpu_profile_satisfies_the_shared_render_plugin)'` —
    **1/1 passed**.
- All-features requested-subset matrix:
  - the six named base, 2D, UI, WGPU contribution, and desktop accessibility composition tests —
    **6/6 passed**;
  - the complete root `plugin_composition` target — **52/52 passed**.
- WGPU UI contribution and exact-outcome lifecycle:
  - `cargo nextest run --locked -p nara_render_wgpu --no-default-features --features
    ui-submitter` — **66/66 passed**;
  - the new blocked-collection regression passed independently before the full target gate.
- `cargo check --locked --all-features --lib --tests --examples` passed.
- Strict focused Clippy passed for `nara_render_wgpu` and the root product-composition library/test
  target with only the repository's existing explicit lint allowances.
- `cargo fmt --all -- --check` and scoped `git diff --check` passed.
- Formal review covered correctness, project standards, testing, maintainability, API contract,
  reliability, and adversarial failure cases. Independent validation confirmed the narrow-feature
  compile defect and missing WGPU backpressure regression. It reclassified the module-size concern
  as non-blocking; the private extraction was retained because it reduced product-composition
  coupling without widening the public API.

# Follow-up

PCC remains complete. RUP-U3 is the sole active product unit and consumes this record as its
`composition_baseline_record`. Product-specific glyph, package, physical-surface, authoring, and
accessibility evidence remains RUP-U3 work and must not be folded back into the composition
foundation without a new owner-local reproduction.

# Citations

- `docs/knowledge/engineering/verification/2026-08/2026-08-13T052431Z-pcc-u4-product-composition-conformance-baseline-180d0dfe0a41417a9633c11cccb91eab.md`
- `docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md`
- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `crates/nara_render_wgpu/src/lib.rs`
- `src/product/profile.rs`
- `tests/plugin_composition.rs`

---
type: "Verification Evidence"
title: "RGS-U2 runtime hierarchy boundary"
description: "Verifies the Nara-owned non-linked runtime hierarchy, failure-atomic Scene replacement, product composition migration, and temporary spatial fail-closed boundary."
timestamp: 2026-08-01T19:03:59Z
record_id: "dd74d84c7e3b487b999a890d6f5485f9"
tags: ["rgs-u2", "spatial-authority", "hierarchy", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "51b3fe45d3e8c5525f4e2d83f996545854e62a5a"
verified_by: "codex correctness/api/standards/testing/reliability/maintainability/adversarial review"
---

# Verification

RGS-U2 replaces the scene-owned hierarchy prototype with one Nara-owned, non-linked Bevy
relationship Module. It also closes the exact Scene Instance replacement/unload transaction around
relationship hooks, separates persistent Scene components from runtime topology, migrates UI and
product composition, and keeps parented `Transform2d` and inherited `Visibility` fail-closed until
RGS-U3 completes their runtime projections.

# Result

Passed at `51b3fe45d3e8c5525f4e2d83f996545854e62a5a`.

- `nara_hierarchy` is the sole first-party runtime `Parent`/`Children` owner.
- Scene, Prefab, replacement, authoring, and Project Content all reject deferred spatial
  projections before candidate publication. Flat spatial components and pure structural hierarchy
  remain valid.
- Exact replacement and unload preflight all fallible identity, persistent-component, relationship,
  hook, and observer conditions before publishing new global state.
- The root facade exposes query-oriented hierarchy facts while construction, completion, reverse
  mutation, retirement, and prepared identity ports remain provisional or internal.
- The formal correctness, API-contract, standards, testing, reliability, maintainability, and
  adversarial reviews have no remaining P0 or P1 finding. The final shared fail-closed repair was
  independently re-reviewed by the correctness and adversarial lanes.

# Evidence

- `cargo nextest run --locked -p nara_ecs -p nara_identity -p nara_hierarchy -p nara_scene -p
  nara_ui -p nara_ui_render -j 1 --no-fail-fast`: 174 passed.
- `cargo nextest run --locked -p nara --features runtime-2d,serde --test plugin_composition --test
  product_capabilities --test project_content_boundary -j 1 --no-fail-fast`: 54 passed.
- `cargo check --workspace --locked`: passed.
- `cargo check --all-targets --locked` in `reference-game/`: passed with existing dead-code
  warnings only.
- Strict Clippy passed for `nara_hierarchy` and `nara_scene`, and for root library/tests with only
  explicit pre-existing allowances (`dead_code`, `drop_non_drop`, `needless_return`,
  `result_large_err`, `collapsible_if`, `derivable_impls`, `double_must_use`, and
  `too_many_arguments`).
- `cargo fmt --all -- --check`, `git diff --check`, and staged diff checks passed.
- Focused regressions cover exact reverse-projection corruption variants, relationship observer
  rejection, zero-edge eligibility, asset/binding publication ordering, direct Scene/Prefab/
  replacement/authoring fail-closed behavior, inherited visibility, and flat spatial acceptance.

# Follow-up

RGS-U3 must implement bounded `GlobalTransform2d` completion and migrate camera, sprite, and tilemap
consumers before removing `scene.hierarchy-projection-unavailable`. Persistent sibling order,
runtime reparent, visibility inheritance, 3D, physics, and lifecycle ownership remain outside this
slice.

# Citations

- `docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md#rgs-u2-establish-the-runtime-hierarchy-and-replacement-boundary`
- `docs/architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md`
- `crates/nara_hierarchy/src/lib.rs`
- `crates/nara_scene/src/spawn.rs`
- `crates/nara_scene/src/validation.rs`
- `tests/product_capabilities.rs`
- `tests/project_content_boundary.rs`
- Git commit `51b3fe45d3e8c5525f4e2d83f996545854e62a5a`

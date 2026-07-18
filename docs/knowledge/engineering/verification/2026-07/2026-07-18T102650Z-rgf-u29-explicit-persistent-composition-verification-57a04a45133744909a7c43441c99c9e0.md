---
type: "Verification Evidence"
title: "RGF-U29 explicit persistent composition verification"
description: "Commit e95cd4b closes frozen registry binding and guarded target-World persistent apply without changing runtime-only ECS behavior."
timestamp: 2026-07-18T10:26:50Z
record_id: "57a04a45133744909a7c43441c99c9e0"
tags: ["rgf-u29", "verification", "persistent-components", "scene", "bevy-ecs"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e95cd4b"
verified_by: "focused nextest,root integration,reference-game,workspace nextest,workspace check,clippy,fmt,independent review"
---

# Verification

RGF-U29 was verified against commit `e95cd4b` on
`refactor/engine-foundation-contracts`. The reviewed scope is the complete persistent path from a
provider-declared Rust component and frozen `ComponentRegistry`, through codec preflight and
`PreparedComponentCandidate` binding, into Scene, Prefab, authoring, Play Mode, and direct
target-`World` apply.

# Result

Passed. Persistent Scene/Prefab composition is now exactly the explicit post-expansion component
record set rather than a mixture of document data and Bevy lifecycle side effects.

- Provider validation rejects Scene-capable components with `#[require]` metadata or intrinsic
  component hooks. Inspect-only and runtime-only components retain normal Bevy behavior.
- Codecs produce a non-applicable `PreparedComponentCandidate`. Only a frozen registry can bind its
  stable component identity, Rust `TypeId`, registration function, and apply validator into a
  `PreparedComponent`; external code cannot construct or forge the applicable type.
- Candidate preparation distinguishes complete values, asset-free deferred work, and possible
  `AssetServer` access through closure types. Asset-free Sprite/UI/Tilemap values neither claim nor
  insert an `AssetServer`.
- Fresh Scene targets flush and validate support types, required-component metadata, all five
  lifecycle hooks, and event-global/component-global observers before allocation. Existing or
  reserved targets additionally validate entity and entity+component observer scopes before their
  first persistent mutation.
- Private per-target receipts plus a World-global bidirectional stable/runtime binding authority
  reject identity collisions, temporal rebinding, missing authority, and cross-World use.
- Persistent publication finishes before runtime `Parent` and `SceneEntitySource` projection.
  Post-publication runtime hooks/observers remain legal, while a later persistent apply rechecks and
  rejects any matching lifecycle work.
- The Bevy-version-coupled metadata/observer probe stays inside `nara_ecs::__private`; no raw
  `ComponentId`, hook-introspection mirror, or Bevy observer topology enters Nara's gameplay-facing
  API.

This unit proves guarded pre-mutation eligibility. It does not claim rollback for arbitrary hook,
observer, or native-service side effects and does not turn `World` into a general transaction
database.

# Evidence

- `cargo nextest run --locked -p nara_ecs -p nara_ecs_derive -p nara_identity -p nara_reflect -p nara_reflect_derive -p nara_scene -p nara_sprite -p nara_tilemap -p nara_ui -p nara_tooling --test-threads=1`:
  235 passed, no failures.
- `cargo nextest run --locked -p nara --test scene_component_composition --test scene_sprite_serialization --features runtime-2d --test-threads=1`:
  18 passed, no failures.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test authoring --test-threads=1`:
  6 passed, no failures.
- `cargo nextest run --workspace --locked --test-threads=1`: 845 passed, 3 declared conditional
  skips, no failures. The first cold run reached the command's ten-minute wrapper limit while
  compiling independent fixtures; the cache-warm rerun completed with the cited full summary.
- `cargo check --workspace --locked` and `cargo fmt --all -- --check`: passed.
- Focused strict Clippy over every affected crate passed with only the documented pre-existing
  allowances for `result_large_err`, `double_must_use`, `too_many_arguments`, `derivable_impls`,
  and two unrelated `nara_app` `collapsible_if` findings.
- Formal correctness, standards, testing, maintainability, performance, API-contract, reliability,
  and adversarial review found no remaining P0/P1. Follow-up validation specifically closed the
  building-registry, late-required-metadata, and asset-free candidate findings without a new P0/P1.
- Regression coverage includes every lifecycle event and observer scope, dynamic deferred hook
  registration, late required metadata through Scene and direct apply, empty persistent sets,
  existing-target retirement, `AssetServer` support observers, bidirectional/temporal binding
  conflicts, missing binding authority, nonempty unowned targets, and compile-fail external
  construction.

# Follow-up

RGF-U29 closes the target-World eligibility prerequisite; it does not close the active reference
game plan. RGF-U12 and U29 now converge at RGF-U26, whose manual counterfactual must materialize the
real authorized startup snapshot through this guarded path. RGF-U24 must reuse the same guard inside
its unpublished candidate runtime rather than treating candidate-World disposal as a replacement
for direct/existing-target eligibility.

The broader product review remains valid: keep U26/U25 bounded to the ownership question and move
quickly toward U24, the U6 authoritative headless wave, and U13 desktop closure. Authoring degraded
mode, generalized World transactions, and broader render or scripting contracts remain outside
this unit.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#rgf-u29`
- `crates/nara_ecs/src/__private.rs`
- `crates/nara_reflect/src/persistent_apply.rs`
- `crates/nara_reflect/src/registry.rs`
- `crates/nara_scene/src/spawn.rs`
- `tests/scene_component_composition.rs`
- `tests/scene_sprite_serialization.rs`
- `reference-game/tests/authoring.rs`
- Commit `e95cd4b`

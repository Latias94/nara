---
type: "Verification Evidence"
title: "SRT-U4 atomic product scene replacement"
description: "Verifies bounded stable-ID candidate overlays, identity-bound exact retirement, a prepared failure-free scene commit tail, and hierarchy-only complete validation."
timestamp: 2026-08-03T05:36:51Z
record_id: "5175acec97f041c2af61585559c01e04"
tags: ["srt-u4", "scene-replacement", "hierarchy", "identity", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25e7f6c"
verified_by: "Codex adversarial correctness and API review plus serial Cargo gates"
---

# Verification

SRT-U4 extends the existing hierarchy-aware Scene replacement kernel with one provisional advanced
product transaction. A scoped writer accepts owned runtime values only by stable `SceneEntityId`;
exact additional retirement requires `WorldEntityToken` authority from the active identity domain.
The caller-controlled overlay completes before the deferred baseline is flushed, every recoverable
check completes before old authority changes, and the remaining commit tail is composed from
prepared infallible tokens.

# Result

Passed at `25e7f6c`.

- Overlay limits and additional-retirement limits are separate, required, and checked against
  public engine ceilings before scratch candidate allocation. Zero, exact-limit, plus-one, missing,
  duplicate, authored-component collision, unregistered-component, resource, hook, and observer
  cases are deterministic and fail closed.
- Additional retirement rejects foreign-World tokens even when their `Entity` bits equal a local
  entity. It also rejects missing, duplicate, scene-owned, persistent-axis, hierarchy-linked, and
  lifecycle-active entities while preserving unrelated same-shaped runtime entities.
- `LifecycleFreeInsertionPlan` now flushes and validates the complete insertion set before returning
  an exclusive commit guard. Persistent component publication similarly has an owned prepared batch;
  no recoverable preparation remains after hierarchy or identity authority changes.
- Complete hierarchy validation uses filtered `QueryState`s for `Parent` and `Children` participants
  plus declared additions. A sparse World fixture with 4,096 unrelated entities observes exactly one
  parent participant and one children participant while retaining missing reverse-edge and cycle
  checks.
- The candidate World and stable-ID-to-Entity map remain private. The root exports only the
  provisional entry through `advanced_prelude`; ordinary gameplay composition receives no raw World,
  provider registry, scene session, or commit token.
- Initial specification-review findings covering pre-flush rejection, late fallible retirement,
  public ceilings, migration, invalid hierarchy, same-shape retirement, and hierarchy scan evidence
  were closed. Independent correctness and API-contract re-reviews reported no findings and no
  testing gaps.

# Evidence

- `cargo check -p nara_scene -p nara --tests --locked -j 1` passed.
- `cargo nextest run -p nara_ecs -p nara_identity -p nara_reflect -p nara_hierarchy -p nara_scene
  --locked --test-threads=1`: 288 passed.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test-threads=1`: 58
  passed.
- Strict changed-target Clippy passed for `nara_ecs`, `nara_identity`, `nara_reflect`,
  `nara_hierarchy`, `nara_scene`, root all targets, and reference-game all targets with only explicit
  pre-existing lint allowances. `cargo fmt --all` and `git diff --check` passed.
- A broader workspace nextest attempt intentionally excluded `architecture_docs`. It first found the
  pre-existing stale fixture locks at
  `tests/fixtures/derive-dependencies/renamed-root/Cargo.lock` and
  `tests/fixtures/runtime-runner/renamed-root/Cargo.lock`; after excluding those fixtures it stopped
  after 160 passing tests on the pre-existing trybuild diagnostic wording drift in
  `scene_component_composition::prepared_component_cannot_be_forged_outside_the_registry`.
  SRT-U4 changes no manifest, lockfile, or that compile-fail fixture, so these unrelated harness
  defects were not folded into the scene transaction commit.
- No Cargo commands ran concurrently in this checkout. The documentation-only
  `tests/architecture_docs.rs` suite was intentionally not run under user and plan direction.

# Follow-up

SRT-U5 replaces the reference game's hand-copied live-World reset with this transaction, removes
duplicate spatial/runtime persistence, owns the exact current-generation projectile set, and proves
the authored topology through headless and desktop products. It must not widen the focused Trial
into a general scene manager or accept broad ADR 0089 travel semantics.

# Citations

- `docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md#srt-u4-compose-atomic-scene-replacement-extras`
- `crates/nara_ecs/src/transaction.rs`
- `crates/nara_identity/src/domain.rs`
- `crates/nara_hierarchy/src/validation.rs`
- `crates/nara_reflect/src/persistent_apply.rs`
- `crates/nara_scene/src/product_transaction.rs`
- `crates/nara_scene/src/spawn.rs`
- Git commit `25e7f6c`

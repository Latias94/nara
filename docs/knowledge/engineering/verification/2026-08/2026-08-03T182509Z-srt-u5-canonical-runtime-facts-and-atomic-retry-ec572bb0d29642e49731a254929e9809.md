---
type: "Verification Evidence"
title: "SRT-U5 canonical runtime facts and atomic Retry"
description: "Verifies one authored spatial authority, exact receipt and projectile ownership, atomic Retry preparation, authored presentation preservation, schema generation 4, and checkout-free product smoke."
timestamp: 2026-08-03T18:25:09Z
record_id: "ec572bb0d29642e49731a254929e9809"
tags: ["srt-u5", "verification", "reference-game", "retry", "scene", "spatial-authority"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f91e2e0"
verified_by: "Codex correctness and specification review plus serial focused gates"
---

# Verification

Commit `f91e2e0` closes SRT-U5 without accepting ADR 0089. The reference game now derives one run
generation from the exact admitted startup document and matching spawn receipt. Startup and Retry
share the same preparation path; mutable gameplay state is runtime-only; authored `Transform2d`
and completed `GlobalTransform2d` are the local and world spatial authorities.

Detailed review found that the first U5 candidate prepared and published hierarchy before the
product overlay was known to be lifecycle-free. The corrected Scene path prepares component apply,
the product overlay, hierarchy additions, identity, resource replacement, and retirement before
the first authority mutation. Its commit tail has no recoverable branch. A focused rejection test
proves that an overlay observer cannot change the old completed hierarchy generation.

# Result

SRT-U5 passed at `f91e2e0`.

- `WaveRunOwner` retains the exact current receipt and a canonical `SceneEntityId -> Entity` map.
  Authored gameplay, scoring, snapshots, presentation, and retirement require exact membership;
  unrelated entities with the same role and even a copied current-generation source marker are
  ignored. Runtime projectiles are reachable only through identity-bound owner tokens.
- The persistent `Player`, `Enemy`, and `Projectile` aggregates are tombstoned in schema generation
  4. Authored roles, initial health and velocity, wave timing, weapon configuration, transform,
  hierarchy, and Sprite remain in content; health, velocity, cooldown, damage, lifetime, and
  projectile identity are runtime-only.
- Player and enemy Sprite texture, region, sampler, tint, layer, and sort key remain authored.
  Desktop projection owns only enemy activation size and runtime projectile presentation. The
  authored weapon remains a real child and follows the completed transform graph in the same tick.
- The root startup activation owner is the sole Project Content lease holder. Product Retry retains
  only a weak `StartupSceneSourceView`, so clones cannot extend the document or budget charge after
  runtime retirement.
- The reference-game plugin declares the startup activation plugin dependency during pure planning.
  Empty persistent marker schemas are supported without placeholder fields, and the removed public
  registry error is covered by migration guidance.
- Repeated Retry keeps one runtime instance, replaces one scene identity generation, rejects
  lifecycle-ineligible candidates before old authority changes, restores canonical authored and
  runtime facts, and retires only current-generation projectiles.

# Evidence

- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked -j 1`: 64 passed.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --features desktop --locked -j 1`:
  99 passed.
- `cargo nextest run -p nara_hierarchy -p nara_scene -p nara_reflect -p nara_reflect_derive
  --locked -j 1`: 227 passed.
- Focused root package-policy, derive-dependency, startup-surface, and project-runtime suites with
  `runtime-2d serde`: 20 passed.
- Strict all-target reference-game Clippy with `desktop` and strict root library Clippy passed with
  only explicitly named pre-existing allowances.
- Root and reference-game `cargo fmt --all -- --check` plus `git diff --check` passed.
- The source headless product emitted one stable completed summary at tick 50 with score 300,
  player health 20, no remaining enemies, and four remaining projectiles.
- Release headless and desktop binaries were packaged at source revision
  `f91e2e0b7a5b0153ff665dc04c5d982600e5d9ca`. The transported no-checkout helper verified and ran
  the 18-file Windows candidate: archive SHA-256
  `427421eb52bf4449c7db4e4e4b44be03bcafb2ca82d25f57800abd4c3eb38f1c`, encoded size 13,869,679
  bytes, expanded size 39,160,405 bytes, stable headless summary, and completed formal desktop
  candidate smoke.
- This lane launched no concurrent Cargo commands. `tests/architecture_docs.rs` was intentionally
  not run or extended under the active plan.

# Follow-up

SRT-U6 is the sole active implementation unit. It must prove the real Editor
Edit -> Play -> Retry -> Stop journey, preserve an admitted unsaved scene edit through Retry,
verify saved reopen behavior, exercise the manual desktop journey, and classify every provisional
startup/replacement symbol as advanced, private, or removed.

The product-local decoder uses the same retained Weapon migration function registered in the
component registry; no second migration rule was introduced. A generic registry-backed system
decoder remains deferred until a second production consumer proves the public API. Stable asset-ID
Retry context, additive load, unload/travel, multiple active scenes, and general scene management
remain ADR 0089 triggers and are not claimed by this record.

# Citations

- `docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md`
- `docs/migrations/2026-07-engine-foundation.md#srt-u5-canonical-reference-game-runtime-facts-and-retry-ownership`
- `crates/nara_hierarchy/src/writer.rs`
- `crates/nara_scene/src/spawn.rs`
- `src/startup_scene.rs`
- `reference-game/schema/component-schema-v4.json`
- `reference-game/src/resources.rs`
- `reference-game/src/systems.rs`
- `reference-game/tests/first_wave.rs`
- `reference-game/tests/desktop_render.rs`
- Git commit `f91e2e0`

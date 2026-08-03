---
type: "Verification Evidence"
title: "SRT-U6 authoring and desktop Trial closure"
description: "Verifies the Editor Edit-to-Play-to-Retry-to-Stop-to-Reopen journey, ordinary desktop product entry, packaged candidate consumption, and the final advanced/private startup scene API classification."
timestamp: 2026-08-03T22:29:16Z
record_id: "b5c3434ed2da44ebae658bf7a487f4a8"
tags: ["srt-u6", "verification", "reference-game", "editor", "desktop", "public-api"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4f5fb6f1fd8b9543eb5ff1e4b8d96ed57d0b90ae"
verified_by: "Codex independent correctness, testing, maintainability, and project-standards review plus serial focused gates"
---

# Verification

Commit `4f5fb6f1fd8b9543eb5ff1e4b8d96ed57d0b90ae` closes the focused SRT-U6
implementation slice without accepting ADR 0089. The ordinary product recipe now proves one Editor
Edit -> Play -> Retry -> Stop -> Save -> Close -> Reopen journey, while the packaged desktop binary
uses the same product facade and reports only a product-owned successful exit as candidate success.

The provisional startup-scene surface is now classified rather than widened. Ordinary gameplay
preludes do not expose activation or replacement mechanics. Advanced embedding retains the
read-only startup activation parameter, its single ordering set, bounded product overlay writer,
resource marker, limits, and the focused replacement function. Raw `SceneSpawner::replace`,
candidate internals, retention ownership, and internal activation phases remain private.

# Review Disposition

A later report repeated four P1 findings from the pre-SRT-U5 snapshot. They do not apply at this
revision:

- Scene replacement prepares lifecycle-free product insertion, persistent components, hierarchy,
  identity, resources, and exact retirement before the first authority mutation.
- `WaveRunOwner` checks the exact current receipt and stable membership for authored actors; runtime
  projectiles are reached and retired only through identity-bound owner tokens.
- Player and enemy Sprite fields remain authored. Runtime presentation changes only activation size
  and product-owned projectile presentation.
- Schema generation 4 declares all three removed aggregate tombstones, and desktop tests use the
  current role components.

Independent final review found no P0 or P1 issue. Testing review identified one P2 gap in the
candidate timeout branch; a live ECS regression now proves an expired deadline returns a fallible
system error and cannot publish a normal exit request. Correctness, maintainability, and project
standards reviewers found no remaining issue.

# Product Journey Evidence

- `reference-game/tests/editor_journey.rs` edits the authored weapon x offset from `1.2` to `2.75`,
  starts through an ordinary recipe contribution, perturbs only the live runtime value, and proves
  Retry reconstructs `2.75` from the retained authoring source with a fresh scene instance and no
  old-instance member.
- The same test proves Play does not dirty authoring, Stop preserves the admitted edit, Save clears
  dirty state only through persistence authority, and Close/Reopen reads the saved `2.75` value.
- Earlier visible desktop acceptance in this session covered movement, overlapping input, focus
  release, Retry, and terminal behavior. The SRT-U6 delta does not change gameplay input, spatial,
  presentation, or render systems. This exact revision additionally passed the bounded packaged
  desktop candidate path; no new manual outcome is inferred from merely launching a visible window.
- The next candidate product slice is readable 2D authoring: first-party text/font data, stable UI
  ordering, and real HUD/menu/button use in the reference game. It requires a separate successor
  plan before code starts and does not admit a universal UI framework.

# Automated Evidence

- Focused Editor plus desktop product suite: 17 passed.
- Focused runtime-core public-surface and composition suite: 28 passed.
- Strict focused Clippy passed for the reference Editor/desktop targets and root runtime-core
  targets. Explicit allowances name only pre-existing `result_large_err`, `collapsible_if`,
  `double_must_use`, `too_many_arguments`, and `derivable_impls` findings outside this slice.
- Root and reference-game `cargo fmt --all -- --check` plus `git diff --check` passed.
- Release headless and desktop binaries were built serially at the exact source revision. The dev
  binaries correctly exceeded the fixed 32 MiB per-file package ceiling; the release binaries were
  19,232,768 and 19,835,904 bytes and required no budget change.
- The no-checkout Windows package contains 18 files. Archive SHA-256 is
  `ad49b87bc47b211ac3c69de350773883c89e68d57714c06ad2d2c6bd777732b3`, encoded size is
  13,864,637 bytes, and expanded size is 39,141,973 bytes. Its manifest binds source revision
  `4f5fb6f1fd8b9543eb5ff1e4b8d96ed57d0b90ae`; arbitrary-cwd consumption produced the stable
  headless summary and `desktop_candidate_smoke: ok` through the packaged `bin/desktop.exe`.
- No Cargo commands from this checkout overlapped. `tests/architecture_docs.rs` was intentionally
  not run or extended under the active plan.

# Decision

SRT-U6 is verified, but ADR 0089 remains `Proposed / partial`. This evidence proves one retained
startup scene, one exact Retry replacement, one Editor authoring journey, and one desktop product
entry. It does not prove asynchronous preparation, additive scenes, a public SceneManager, general
unload/travel, stable-asset database Retry context, multiple active instances, or bounded
scene-service retirement.

# Citations

- `docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md`
- `docs/architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md`
- `crates/nara_scene/src/spawn.rs`
- `src/startup_scene.rs`
- `tests/startup_scene_public_surface.rs`
- `tests/scene_product_public_surface.rs`
- `reference-game/tests/editor_journey.rs`
- `reference-game/src/bin/desktop.rs`
- Git commit `4f5fb6f`

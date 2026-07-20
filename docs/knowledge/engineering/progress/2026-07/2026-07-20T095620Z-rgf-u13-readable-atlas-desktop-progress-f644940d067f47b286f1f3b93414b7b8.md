---
type: "Work Progress"
title: "RGF-U13 readable atlas desktop correction"
description: "Commit 0accc9b replaces the opaque rectangle presentation with authored atlas regions and a readable 2D arena while retaining the final human Windows play check."
timestamp: 2026-07-20T09:56:20Z
record_id: "f644940d067f47b286f1f3b93414b7b8"
tags: ["rgf-u13", "reference-game", "desktop", "atlas", "playability"]
status: "automated-verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0accc9be9b59c33a931126caf973e9a4d8f295e2"
verified_by: "reference-game nextest,root U13 nextest,schedule contract,native surface smokes,example checks,focused clippy,fmt,independent review"
---

# Summary

The user-visible U13 correction replaces the prior three-rectangle presentation with the bundled
Kenney Tiny Dungeon atlas, a tiled arena, distinct player and enemy regions, and a directional
projectile. The game begins without a hidden desktop-only input gate. The shared committed player
state starts stationary, WASD supplies movement through the existing semantic command route, and
combat advances through the same authoritative fixed wave as headless.

This does not add a generic runtime atlas manager. Scene and Prefab data carry the exact persistent
Sprite image, region, sampler, tint, size, layer, and sort key needed before the first fixed tick;
the desktop projection updates runtime presentation only through the documented FixedUpdate
anchor. `repo-ref/tex-packer` remains a read-only candidate for a later offline importer after a
second real consumer proves the package and artifact contract.

# Verified State

- Independent reference-game all-feature gate: 70 passed.
- Root U13 Host, content, runtime, driver, and composition gate: 94 passed.
- Public schedule-extension contract: 7 passed. It rejected a temporary PostUpdate registration;
  the final implementation stays on the documented FixedUpdate/Capture anchor.
- Exact zero-tick extraction proves the player and enemy atlas UV rectangles, shared image handle,
  nearest sampling, and all 120 floor tiles before a fixed step.
- Both native surface-retirement smoke modes passed.
- The three required root desktop examples and the reference-game all-target/all-feature check
  passed.
- Strict Clippy passed for every changed target. The all-target invocation remains blocked only by
  unchanged warnings in `asset_task_flow.rs` and `image_asset_safety.rs`.
- Independent correctness, standards, testing, maintainability, performance, reliability, and
  adversarial review left no unresolved finding.
- The rebuilt Windows process opened a responsive `Nara Reference Wave` window.

# Open Gate

RGF-U13 remains active. Automation cannot replace the plan's human Windows playability evidence.
The user must confirm that the current window visibly reads as a 2D arena, WASD movement is
observable, combat progresses, Enter creates a fresh in-runtime wave after a terminal outcome, and
normal window close feels correct.

Until that confirmation, U14, U17, and U7 remain unadmitted. U15 is independently implemented
locally but still needs user authorization for a push or PR and disposable hosted Windows/Linux
evidence.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u13-complete-the-desktop-input-and-render-wave`
- `docs/knowledge/engineering/progress/2026-07/2026-07-20T044044Z-rgf-u13-physical-input-playability-correction-6ffe86e749164d568ae28e8bb9fe0dc4.md`
- `reference-game/scenes/startup.scene.json`
- `reference-game/prefabs/enemy.prefab.json`
- `reference-game/src/ui.rs`
- `reference-game/tests/desktop_render.rs`
- Commit `0accc9b`

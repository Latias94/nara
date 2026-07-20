---
type: "Verification Evidence"
title: "RGF-U13 desktop first playable final verification"
description: "Commit 5bc321d closes the desktop first playable after automated gates, resolved review findings, and human Windows confirmation."
timestamp: 2026-07-20T12:08:01Z
record_id: "88640a2745fa4e439483d35df4b9a219"
tags: ["rgf-u13", "reference-game", "desktop", "playability", "input"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5bc321d41aba59072a1f97ccc0473f91e0b2c161"
verified_by: "reference-game nextest, root U13 gates, native surface smokes, focused clippy, fmt, human Windows play"
---

# Verification

RGF-U13 projects the committed authoritative headless wave through the public desktop Host,
ordered physical input, one admitted window/target/view render transaction, atlas-backed sprite
and HUD presentation, semantic Retry, and bounded native shutdown. This record closes the final
manual gate against commit `5bc321d41aba59072a1f97ccc0473f91e0b2c161`; it does not claim a
packaged candidate, hosted CI, editor workflow, or public release.

# Result

Completed. The automated product path remained green after the final overlapping-input repair,
all independent review findings were resolved or dispositioned against the active-plan authority,
and the user confirmed the rebuilt Windows game behaves normally for the requested play check.

# Evidence

- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --all-features
  --test-threads=1` passed all 71 tests at `5bc321d`, including desktop flow/parity/render, the
  headless wave, raw-App baseline, project content, Retry, and desktop process cleanup.
- `cargo check --manifest-path reference-game/Cargo.toml --locked --all-targets --all-features`
  passed. Strict Clippy passed for the changed library, desktop binary, and `desktop_flow` target;
  the broader all-target invocation remained blocked only by pre-existing warnings in unchanged
  `asset_task_flow.rs` and `image_asset_safety.rs` tests.
- The earlier final U13 engine gate remains applicable because the commits after `0accc9b` changed
  only immutable engineering-memory records and reference-game input composition. That gate passed
  94 root Host/content/runtime/driver/composition tests, seven public schedule-extension tests,
  both native surface-retirement smoke modes, the three required desktop example checks, and the
  exact zero-fixed-tick atlas extraction proof.
- The atlas proof asserts the exact player and enemy source rectangles and all 120 arena floor
  sprites before a fixed step. This resolves the earlier whole-atlas first-frame and weak-count
  review concerns.
- The shared stationary player start is an admitted U13 playability tuning: the active plan permits
  committed shared scene-value tuning when raw-App, headless, and desktop baselines move together.
  Commit `0accc9b` updated those baselines and retained one authoritative simulation path.
- The final input regression observes the original failure before implementation and proves held
  `S+A` composes into normalized down-left motion, releasing `S` preserves left movement, and
  releasing the final key stops. The complete reference-game gate then proved no headless or Retry
  regression.
- In the rebuilt `Nara Reference Wave` Windows process, the user confirmed the 2D presentation,
  WASD including overlapping-key behavior, Enter Retry, and normal close behavior as working
  normally. This is human session evidence and is intentionally not represented as an automated
  portability claim.

# Follow-up

- Complete the U13 registration lineage and admit the next dependency-valid product work.
- RGF-U15 remains independently active until an authorized push or PR produces disposable hosted
  Windows/Linux evidence. RGF-U14 cannot close before that dependency is satisfied.
- Treat a reusable two-dimensional composite action as concrete authoring-friction evidence, not
  as a U13 blocker. Generalize it only through the next admitted input consumer or the U14
  bottleneck decision; keep authoritative movement intent in the game-command path.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u13-complete-the-desktop-input-and-render-wave`
- `docs/knowledge/engineering/verification/2026-07/2026-07-19T192736Z-rgf-u13-desktop-production-wave-automated-verification-9d99f3f9c3c2450ea98dfa0952a1ded3.md`
- `docs/knowledge/engineering/progress/2026-07/2026-07-20T095620Z-rgf-u13-readable-atlas-desktop-progress-f644940d067f47b286f1f3b93414b7b8.md`
- `reference-game/tests/desktop_flow.rs`
- `reference-game/tests/desktop_render.rs`
- Commits `0accc9b` and `5bc321d`

---
type: "Work Progress"
title: "RGF-U13 physical-input playability correction"
description: "Desktop profile cadence and production-shaped input-window regression landed after real Windows input exposed an imperceptibly short gameplay window."
timestamp: 2026-07-20T04:40:44Z
record_id: "6ffe86e749164d568ae28e8bb9fe0dc4"
tags: ["rgf-u13", "reference-game", "desktop", "input", "playability"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6c8d28b"
verified_by: "reference-game nextest,clippy,all-targets,Win32 message probe,pixel inspection"
---

# Summary

The first real Windows message probe contradicted the earlier automated-completion claim. Physical
Winit input and Enter Retry both worked, but after a 250 ms W press the 20 ms desktop fixed step
advanced the authoritative wave so quickly that the game was terminal before a person could
perceive or control the movement. U13 therefore remains active.

# Details

- Commit `6c8d28b` gives only the `desktop` project profile a 100 ms fixed timestep. The headless
  profile remains at 20 ms, and movement, combat, command ordering, success tick 49, defeat tick 4,
  and tick-level desktop/headless parity are unchanged.
- `desktop_flow` now applies the resolved profile time settings just as the product Host does. Its
  production-shaped regression drives 10 ms render frames around a 250 ms W press and asserts that
  the player moved visibly, stayed in the admitted camera range, and remained Running after 700 ms.
- The U26 `ProjectContentRevision` expectation changed because that revision intentionally includes
  project-settings lineage. The content digest, first-tick snapshot, plugin-plan fingerprint,
  schema fingerprint, and command digest remain unchanged.
- The complete independent desktop-enabled reference-game gate passed 69/69 with no skips. Focused
  strict Clippy, desktop `--all-targets` check, formatting, and diff checks passed.
- A post-fix Win32 `WM_KEYDOWN`/`WM_KEYUP` probe and pixel inspection showed a full-health waiting
  frame, a visibly displaced player with no terminal geometry after 700 ms, a later terminal frame,
  and a reset full-health waiting frame after Enter. This strengthens but does not replace the
  required human hand-feel check.

# Next Action

Have the user personally verify WASD response, the 100 ms cadence, terminal feedback, Enter Retry,
and normal close in the currently built Windows product. If the cadence feels playable, create a
new final U13 verification shard, complete registration `engine-foundation-contract-completion-codex-root`,
and activate U14. If it feels unacceptably coarse or still too fast, keep U13 active and correct the
desktop presentation/simulation pressure before recording completion.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u13-complete-the-desktop-input-and-render-wave`
- `docs/knowledge/engineering/verification/2026-07/2026-07-19T192736Z-rgf-u13-desktop-production-wave-automated-verification-9d99f3f9c3c2450ea98dfa0952a1ded3.md`
- `reference-game/nara.toml`
- `reference-game/tests/desktop_flow.rs`
- `reference-game/tests/manual_raw_app_baseline.rs`
- Commit `6c8d28b`

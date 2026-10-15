---
type: "Verification Evidence"
title: "Interpolation, native Editor display and migrated Reference workflows pass before full review"
description: "Verification Evidence for Interpolation, native Editor display and migrated Reference workflows pass before full review."
timestamp: 2026-10-15T19:38:36Z
record_id: "a4de05216dbf4ca286f3e1ef0e0510da"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "71349a2"
---

# Verification

- Transform/render/sprite libraries and real interpolation consumer:
  `330d6d13-56f8-412b-820e-c7bcb899d023`, 74/74 passed, 4.227s after 33.42s compilation.
- Render/Scene/egui libraries plus authoring decoder after the sampling policy and simplification:
  `a03786ce-89da-47b8-a4cc-4d710724528d`, 188/188 passed, 7.865s after 58.35s compilation.
- Affected root CI/module/public/Play/privacy targets:
  `dea1349e-d740-40ac-9820-e7cb9ee93b1b`, 40 run, 38 passed, two stale dependency expectations
  failed. Both repaired cases pass in `af8ecdc8-c6d3-4eab-be5c-6435c0db0ff9`, 2/2, 8.747s.
- Reference selected eight targets: `111a6905-b21a-44ed-b7ad-ba91cb5fac5d`, 38 run, 36 passed,
  two failed. The corrected save/close/new-session journey passes in
  `7e6ba9cf-e9d5-419c-91a8-5f4603ac7d68`; real host parity passes in
  `75520dee-18e3-4386-b232-9a4232d057fd`, 2/2, 2.011s.
- Actual `target/debug/editor_game_view_canary.exe`: exit 0 in 2.647s. Native Windows output
  reports runtime generation 2, input movement, pause/step, resized image 1120x869, 129 sprite
  instances, runtime UI and the authored UI move/create transaction with undo depth 2.
- Latest workspace all-feature/all-target check: session `34473`, passed in 1m13s. The following
  Clippy/fmt commands were still running at this capture and are not claimed here.

# Result

Fixed-step presentation is opt-in, interpolates local TRS before full affine composition, and
shares one projection between camera, sprite and tile. Default/current-tick, repeated extraction,
paused Step, authoring and new-entity reset behavior are verified. Both real motion examples
enable the policy. Game View exposes integer scaling through its ordinary panel, maps physical
pixels and chooses nearest target registration; the native default Fit path remains linear.

The completed native Editor canary exercises the integrated image/sample/window path and default
Fit behavior. The integer option has actual egui click/mapping coverage and compiled native
registration, but this canary does not click that option; no extra native nearest-pixel claim is made.

# Evidence

The root failures were stale expected feature/dependency sets. Locked independent fixtures were
updated with Cargo metadata while preserving previous registry pins; an initial generate-lockfile
attempt upgraded unrelated pins and was replaced only in these initially clean, task-owned locks.
All changes to the user's original index remain byte-identical. The private-field consumer and
actual opaque-prefab Play edits pass; obsolete source/AST owner inference was deleted.

The Reference journey tried to reload a closed document using Reopen. Existing persistence
behavior requires a live document revision, and the generic browser has a distinct Open path.
The trusted compiled fixture now physically closes the old Editor and opens a new session with
the same wave definition; saved Inspector values are still checked. No production check was relaxed.

Host parity initially timed out in its synthetic Windows environment. Finite subprocess probes
isolated the trigger to replacing USERPROFILE: empty cwd, HOME, XDG and AppData independently
work; replacing USERPROFILE alone hangs desktop initialization, including separate DX12/Vulkan
requests and fallback selection, while headless/Editor finish. Rust source inspection did not
establish the native driver/loader root cause. The fixture now keeps the real account profile and
continues isolating cwd and all application paths; all three product hosts publish the same envelope.

Disk pressure interrupted Reference linking before tests. Cargo's package-scoped clean of root
and Reference artifacts reclaimed 4.7 and 17.6 GiB; the selected suite then ran. Source files and
unrelated caches were preserved.

# Follow-up

Full code review, final static checks and evidence/ADR closure remain. The owned checkpoints are
`a3651c3` (render), `90b6cfb` (presentation Host) and `71349a2` (consumers/CI), after `8ba1ff9`
(authoring). Original overlapping Cargo/Host/SPL work stays in the integrated worktree. Neither
the Windows evidence nor a compile check establishes native Linux/macOS release support.

# Citations

- crates/nara_sprite_render/tests/presentation_interpolation.rs
- crates/nara_transform/src/presentation.rs
- crates/nara_tooling_egui/src/game_view.rs
- editor/src/platform/viewport.rs
- reference-game/tests/editor_journey.rs
- reference-game/tests/support/host_parity.rs
- tests/product_capabilities.rs
- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md

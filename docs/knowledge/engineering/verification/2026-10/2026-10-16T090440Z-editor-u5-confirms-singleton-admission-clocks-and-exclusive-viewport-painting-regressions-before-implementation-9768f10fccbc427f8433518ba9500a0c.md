---
type: "Verification Evidence"
title: "Editor U5 confirms singleton admission clocks and exclusive viewport painting regressions before implementation"
description: "Verification Evidence for Editor U5 confirms singleton admission clocks and exclusive viewport painting regressions before implementation."
timestamp: 2026-10-16T09:04:40Z
record_id: "9768f10fccbc427f8433518ba9500a0c"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "527a135"
---

# Verification

Proof-first U5 checkpoints on source `527a135`, before production changes in the respective owners.
Root was the only Cargo runner, using Rust 1.95, `CARGO_BUILD_JOBS=1` and
`CARGO_INCREMENTAL=0` in the shared checkout.

# Result

Three intended behavioral failures were observed. These are defect characterization, not a passing
U5 acceptance claim. U5 implementation remains active; U6-U8 have not passed.

# Evidence

- `cargo +1.95 nextest run -p nara_render_wgpu --all-features -E
  'test(editor_independent_producers_keep_their_own_admission_clock) |
  test(editor_host_transaction_accumulates_all_image_submissions)'`:
  run `85f233a2-c42c-4792-8307-500c42c61ab4`, two tests executed and failed.
  Activating producer B made a subsequent producer A packet fail with
  `StaleFrameGeneration`; per-image transaction resets produced one admission where two were expected.
- `cargo +1.95 nextest run -p nara-editor --lib -E
  'test(registered_scene_and_game_views_tessellate_both_presented_textures_in_one_frame)'`:
  run `f9e0fa05-03b0-4121-9dda-b3561d69ebae`, one test executed and failed.
  At 720 by 520 logical points, the actual tessellated frame sampled the Scene texture but omitted
  the distinct Game texture (`scene=true, game=false`). The test did not substitute a compile error
  or a synthetic layout flag for rendered output.

# Follow-up

U5 work is split between three exclusive owners: concrete GPU execution, frontend/panel composition,
and native Host/view/input integration. Root owns Cargo, formatting, review integration, memory
rollups and canonical commits. The frozen seams use explicit non-reused render-view identity,
one Host transaction clock, per-producer admission clocks, explicit image samples and a list of
exact per-image/window composites. Concurrent views replace the automatic Scene/Game pause handoff.

No native concurrency, detached-window, Dear ImGui adoption, Linux or macOS result is claimed here.

# Citations

- [Active plan](../../../../plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md), U5 and AE4.
- [Accepted ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md).
- [View-owner research](../../2026-10/2026-10-16T072527Z-editor-u5-view-owner-research-identifies-independent-admission-clocks-and-exact-multi-image-composites-9ab9c93b8d954dd5abb5bef8fe960f3b.md).

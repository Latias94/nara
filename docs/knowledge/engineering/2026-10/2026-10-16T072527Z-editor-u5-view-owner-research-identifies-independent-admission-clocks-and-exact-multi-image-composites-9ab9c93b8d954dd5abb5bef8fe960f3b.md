---
type: "Research"
title: "Editor U5 view owner research identifies independent admission clocks and exact multi-image composites"
description: "Read-only source findings for U5; no implementation or native multi-view acceptance claimed."
timestamp: 2026-10-16T07:25:27Z
record_id: "9ab9c93b8d954dd5abb5bef8fe960f3b"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "a99595d"
---

# Summary

Read-only explorer `editor_u5_view_owner_research` traced the concrete U5 owners on source `a99595d`.
Independent textures alone do not establish concurrent Scene/Game views: packet admission, cache clocks, sampling evidence and Host lifecycle are also singular.
This is implementation guidance for the existing plan, not a new decision or acceptance claim.

# Details

- `crates/nara_render_wgpu/src/backend.rs` owns global `source_generation` and `last_frame_index`; `activate_editor_source` and `admit_packet` currently let one producer invalidate another. Preserve the ordinary Runtime contract and put Editor per-view admission in its existing execution owner.
- `begin_frame_transaction` resets statistics/UI submission facts, and `begin_editor_submitter_frame` resets shared upload budgets/cache eviction using the producer frame index. A Host render-transaction clock must govern shared cache/budget age once per redraw; each producer retains its own stale-frame admission.
- `editor_execution.rs` has singular target, window-frame candidate/sample and outcome composite slots. Each exact composite receipt should continue proving one image/window join, while the candidate/sample/outcome containers hold all images actually rendered. Never issue a second image receipt merely because another texture was sampled.
- `platform.rs::commit_pending_game_start`, `reconcile_viewport_source` and `viewport.rs::EditorGamePresenter::commit_start` retire Scene/UI when Game starts. Remove that cross-view retirement and separate visibility from focus. Keep global retirement for project/GPU replacement and shutdown.
- Existing Scene preview and Game bridge are already independent producers. This finite slice does not require a general multi-document Runtime map or multiple embedded Game runtimes. Authoring Scene/UI may remain mutually exclusive within one authoring view.
- Preserve geometry-bound picking and the existing Game runtime/source/target/geometry/route lease. Focus changes and panel close release the corresponding held input without stopping a visible Game producer. Replace egui Rect at the geometry boundary with existing DisplayRect.
- The private device namespace currently retains Device/Queue only, while the backend owns Instance/Adapter too. U6 needs same-lineage retained clones through that private scoped seam; panel/tooling APIs receive no raw GPU handles and no second device is requested.
- Dear route preparation precedes primary acquire; it cannot live entirely inside the current post-acquire paint callback. A detached texture's secondary sample/present must not be certified with the primary window receipt. Missing upstream evidence would require a narrow adapter fix at U6.
- One device/submission owner does not mean one total queue submission. Independent image submissions followed by one window composite remain a valid finite design unless evidence requires deferred command-buffer aggregation.

# Next Action

After U4 is committed, begin U5 with per-view admission/reservation tests, then exact multi-image sampling/composites, Host reconciliation, focused input and a real Windows concurrent-view canary.
Compare independent source frame-1 submissions, cross-lane reservation activation, shared upload accounting, single-view retirement and unsampled-image rejection.
Retain existing ordinary Runtime tests and exact presented-frame tests.

# Citations

- `docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md` U5 and KTD6
- `docs/architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md`
- `crates/nara_render_wgpu/src/{editor_execution,backend,presented,texture}.rs`
- `editor/src/platform.rs`
- `editor/src/platform/{viewport,game_input}.rs`
- `src/project_host/runtime/editor/{presentation,preview,game_view}.rs`
- `repo-ref/dear-imgui-rs/backends/dear-imgui-wgpu/src/data.rs`
- `repo-ref/dear-imgui-rs/backends/dear-imgui-wgpu/src/renderer/multi_viewport.rs`

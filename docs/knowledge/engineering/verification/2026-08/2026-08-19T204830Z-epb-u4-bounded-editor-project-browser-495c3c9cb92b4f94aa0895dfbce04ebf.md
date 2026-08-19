---
type: "Verification Evidence"
title: "EPB-U4 bounded Editor Project Browser"
description: "Verifies UI-neutral source-index projection, bounded query and rendering work, exact frame-bound Refresh/Open ingress, and no filesystem authority in adapters."
timestamp: 2026-08-19T20:48:30Z
record_id: "495c3c9cb92b4f94aa0895dfbce04ebf"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6fb84d0"
verified_by: "codex-root"
---

# Verification

Verified implementation commit `cdd5892` and bounded-allocation correction `6fb84d0` against
EPB-U4 of the Editor Project Browser and Source Index v1 plan. The review covered the UI-neutral
browser contract, the Host-to-frame projection, frame-bound Refresh/Open dispatch, bounded query
and egui work, scan/query wake composition, and the absence of filesystem authority in Tooling and
Adapter code.

# Result

EPB-U4 passed. The Editor frame now carries an owned safe browser projection while the exact raw
source-index snapshot remains private to the accepted Host frame. Selection, expansion, and filters
stay local to the project session; only Refresh and Open enter the existing frame-bound action
batch. Scan progress and the final completed query slice both schedule a bounded follow-up redraw,
so `ControlFlow::Wait` cannot strand a new generation or completed row projection.

The implementation is split by authority: `project_browser.rs` owns public safe contracts,
`project_browser/state.rs` owns the mutable bounded query state, the Host-private module owns exact
source facts and admission, and the egui module owns presentation only. Simplification removed an
unused expanded-path view cache and an inert query-revision counter, and kept the Project Browser
panel crate-private. The correction keeps final query publication O(1) by moving a completed row
vector into an `Arc`, and projects already-sorted Host entries without resorting or reallocating
their retained relative paths.

# Evidence

- `cargo fmt --all` completed successfully.
- `cargo check -p nara-editor --tests` completed successfully.
- `cargo nextest run -p nara_tooling -E 'test(browser_query_is_deterministic_and_local_state_is_revision_keyed) | test(maximum_query_work_is_bounded_per_advance)' --test-threads=1` passed `2/2`.
- `cargo nextest run -p nara_tooling_egui -E 'test(project_browser_keeps_local_navigation_outside_the_host_batch) | test(project_browser_renders_all_states_and_caps_one_frame_of_rows)' --test-threads=1` passed `2/2`.
- `cargo nextest run -p nara --lib --features runtime-2d,serde,tooling -E 'test(project_browser_rejects_stale_frames_without_rebasing_the_open_request) | test(project_browser_rejects_unindexed_and_identity_replaced_sources_before_open)' --test-threads=1` passed `2/2`.
- Boundary searches found no `std::fs`, `read_dir`, `DirectoryCapability`, `FileCapability`, raw
  path, or native identity use in the Tooling/egui browser modules or the Host browser projection.
- `git diff --check` passed. Cargo reported that its global last-use metadata could not be saved
  because the system cache disk is full; compilation and tests used the existing workspace target
  on `F:` and completed successfully.

# Follow-up

EPB-U5 is the sole active unit. It must prove the real standalone Windows browse/filter/Open/Refresh
trajectory for one closure-external Scene and Prefab, last-good retention after a rescan failure,
and the absence of project Rust compilation before closing the plan. External Play, import/reimport,
watchers, source mutation, and extension hosting remain separate successors.

# Citations

- `docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md`
- `crates/nara_tooling/src/project_browser.rs`
- `crates/nara_tooling/src/project_browser/state.rs`
- `crates/nara_tooling_egui/src/project_browser.rs`
- `src/project_host/runtime/editor/project_browser.rs`
- `editor/src/platform.rs`
- commits `cdd5892` and `6fb84d0`

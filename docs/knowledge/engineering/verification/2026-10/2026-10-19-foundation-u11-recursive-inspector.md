---
type: Verification Evidence
title: Foundation U11 explicit recursive Inspector drafts
description: Shape-driven native controls commit complete root fields through existing document authority.
timestamp: 2026-10-19T09:35:00Z
record_id: c5f0bda9c9624816acae54759310d550
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract And Structure

The native Inspector consumes the shape already published by SceneInspectorFieldView. Private
value-draft and recursive-control modules support Optional Null/Some, closed records and scalar
List/Map children. Some/Add/Insert first opens an incomplete child draft; confirmation requires
explicit scalar values, complete record members and the admitted numeric domain. Empty strings,
keys and containers have explicit confirmation. No implicit zero/default record is invented.

The frontend applies one complete root value through the existing revisioned inspector action.
Apply/Cancel and unconfirmed children remain local until publication. Document owner validation
still controls revision, component shape, references and history. EntityRef/Opaque descendants do
not gain generic write controls. Revision/selection/document retirement clears drafts; unavailable
records remain lossless through Undo/Redo and Save/Reopen. Existing scalar/AssetRef controls remain.

# Verification

Windows, Rust 1.95.0, serialized Cargo with CARGO_BUILD_JOBS=1 and the root lockfile:

- `c987b415-1f7c-44b8-9d48-e5a1744cae82`: 147 ordinary Editor library/panel/generic-component tests
  passed, 16 native tests skipped. New cases include explicit constrained child creation, readonly
  reference descendants, real owned-Context controls in Scene and Prefab, and owner transactions
  rejecting invalid leaves, stale revisions and EntityRef writes while preserving unavailable data.
- Independent review found sibling List/Map Remove buttons shared an ImGui ID after the child
  scope ended. The helper now scopes the actual button by its structural path. The new owned-
  Context regression removes the second of two entries first and proves the first remains;
  it passed in the ordinary cohort above. Original workflow assertions were retained.
- Native run `8a069d68-50e9-4b1e-af38-0b3bafd34ed9`, with WGPU_BACKEND=dx12 and an adapter required:
  `native_recursive_inspector_drafts_round_trip_scene_and_prefab` passed through actual hidden
  native creation, owned Context input, WGPU submission/presentation, source Save/Reopen and cleanup.
  The three existing critical native cadence/input/view-reconstruction trajectories also passed.

That native selection passed five of six cases. The separate U13 panel test failed when it read
an old observation before the reopened painter executed; its corrected rerun is recorded under
U13. The U11 trajectory and three critical trajectories were executed and passed, not skipped.
No system pointer/keyboard injection, monitor scanout or other-platform qualification is claimed.

# Completion Boundary

U11's focused owner, public Editor consumer, real controls and native acceptance agree. The
final workspace gate and complete end-of-goal review remain separate requirements. No new
component-specific callback, frontend mutation authority or public widget framework was added.

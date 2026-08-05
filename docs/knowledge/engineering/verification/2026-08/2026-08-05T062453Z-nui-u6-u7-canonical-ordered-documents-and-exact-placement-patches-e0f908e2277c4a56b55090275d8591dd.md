---
type: "Verification Evidence"
title: "NUI-U6/U7 canonical ordered documents and exact placement patches"
description: "Verifies canonical ordered Scene and Prefab placement, bounded rejection of the removed parent shape, exact candidate-relative Patch inverses, tooling history, and persistence parity."
timestamp: 2026-08-05T06:24:53Z
record_id: "e0f908e2277c4a56b55090275d8591dd"
tags: ["nui-u6", "nui-u7", "ordered-placement", "scene-patch", "verification"]
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "53d6678c95c055d66021387ad09061041d82343e"
verified_by: "focused and integration nextest, all-target check, strict changed-crate Clippy, locked module/reference consumers, formatting, source search, and diff checks"
---

# Verification

NUI-U6 landed at `7c2c5ba3ecf3c5da4180658cf0471520660e1840`; NUI-U7 landed at
`53d6678c95c055d66021387ad09061041d82343e`.

- Scene and Prefab documents now publish one explicit ordered placement closure: document-owned
  `roots`, parent-owned `children`, and stable-ID-sorted entity records. Validation proves unique
  membership, reachable closure, acyclicity, bounded depth/work/bytes, and Scene/Prefab parity
  before publication.
- The removed record-owned `parent` shape is rejected within decode limits with an actionable
  legacy-shape diagnostic. Corrected first-party JSON/RON sources remain canonical version 1; no
  compatibility alias, order guess, or runtime auto-rewrite was introduced.
- Patch add, remove, restore, and move operations use `ScenePlacementOwner` plus stable
  first/last/before/after anchors. A batch resolves each operation against one sequential candidate,
  and the public apply path publishes only after every operation and full document validation pass.
- Move and remove require an exact source placement. Subtree deletion returns one `RestoreEntity`
  inverse containing the root, descendants, and prior placement, so delete/restore validation work
  is symmetric and exact undo/redo does not depend on a chain of intermediate operations.
- Prefab overrides apply ordered mutations before stable-ID namespacing. Inspector commands derive
  the current source placement and flow through the same Patch contract; Workspace records exactly
  one undo item per committed command. Host save/reopen preserves placement order independently of
  canonical entity-record sorting.

# Result

Passed for NUI-U6 and NUI-U7. ADR 0101 remains partial because ordered runtime hierarchy projection
belongs to NUI-U8 and UI consumption belongs to NUI-U13.

- `cargo check --locked --all-features --all-targets` passed.
- Focused `nara_scene`, `nara_hierarchy`, and `nara_tooling` verification passed 146 tests. The
  Scene JSON/RON format contract passed 31 tests, and the final Patch transaction rerun passed all
  22 tests after the internal error-path layout correction.
- Root authoring, Inspector, Editor persistence, and Workspace Play verification passed 42 tests.
  Project content, boundary, limits, Host boot, preflight, and capability verification passed 51
  tests with one unrelated trybuild case explicitly excluded.
- The locked module consumer passed its test. Reference-game authoring and Prefab startup passed
  all 8 tests.
- Strict `-D warnings` Clippy passed for all `nara_scene` and `nara_tooling` targets with all
  features and `--no-deps`. Formatting and staged diff checks passed.
- The broader strict workspace Clippy command remains blocked by Rust 1.97 baseline lints in
  unchanged `nara_app` and root Host code (`result_large_err`, `collapsible_if`, and
  `drop_non_drop`). The excluded trybuild case differs only in Rust 1.97 compiler wording for a
  missing associated item. Neither baseline was changed or hidden in these units.

# Evidence

- Ordered first/last/before/after insertion, same-owner reorder, cross-owner and root moves,
  candidate-relative anchor dependencies, missing/retired/self anchors, cycles, stale source
  placement, failure atomicity, exact subtree restore, deletion/recreation rejection, and equal
  delete/restore validation budgets are covered in `tests/scene_patch_transactions.rs`.
- Canonical document closure, legacy rejection, JSON/RON round trips, nested Patch/Prefab costs,
  and Prefab override order are covered by `crates/nara_scene/src/tests.rs` and
  `crates/nara_scene/tests/format_contract.rs`.
- Tooling history and persistence parity are covered by `tests/scene_authoring_session.rs`,
  `tests/scene_inspector.rs`, `tests/editor_persistence.rs`,
  `tests/workspace_play_runtime.rs`, and `src/project_host/runtime/tests.rs`.
- All first-party Rust callers use the ordered placement vocabulary. Searches find no remaining
  `ScenePatchOperation::Reparent` use or Scene/Prefab record-owned `parent` field.
- Concurrent user-owned changes in `crates/nara_ui/src/tests.rs`,
  `crates/nara_ui_render/src/tests.rs`, ADR 0101, and `docs/architecture/nara-foundation.md`
  remained unstaged and were not rewritten by U6/U7.

# Follow-up

Proceed to NUI-U8. Extend the hierarchy construction and mutation owners with ordered placement,
commit ordered batches atomically, and preserve the persistent root/sibling sequence through Scene
spawn, export, Prefab expansion, product replace/retire, and Retry without entity-allocation
fallback. Do not let the concurrent UI tests establish a second structural ordering authority.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u6-publish-canonical-ordered-sceneprefab-documents`
- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u7-add-exact-ordered-placement-patch-and-tooling-commands`
- `docs/architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md`
- `docs/architecture/adr/0026-editor-command-patch-and-undo-model.md`
- `docs/migrations/2026-07-engine-foundation.md#nui-u6-u7-1-canonical-ordered-documents-and-exact-placement-patches`
- `crates/nara_scene/src/document.rs`
- `crates/nara_scene/src/patch.rs`
- `crates/nara_scene/src/prefab.rs`
- `crates/nara_tooling/src/inspector.rs`
- `crates/nara_tooling/src/workspace.rs`
- `tests/scene_patch_transactions.rs`

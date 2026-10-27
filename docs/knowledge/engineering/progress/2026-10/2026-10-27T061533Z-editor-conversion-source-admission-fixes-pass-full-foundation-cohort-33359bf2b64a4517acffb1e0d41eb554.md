---
type: "Work Progress"
title: "Editor conversion source admission fixes pass full foundation cohort"
description: "Work Progress for Editor conversion source admission fixes pass full foundation cohort."
timestamp: 2026-10-27T06:15:33Z
record_id: "33359bf2b64a4517acffb1e0d41eb554"
producer_id: "codex-root"
run_id: "editor-prefab-conversion-2026-10-27"
---

# Summary

The full foundation Goal remains active. Current branch is `refactor/editor-prefab-conversion`
at base `c407ddf`; all current production changes are uncommitted. All worker and reviewer
agents used by this integration are closed. Root/Editor verification is still running; there
is no final native qualification or commit claim. Untracked `vendor/` remains untouched.

# Details

The Source preparation budget now carries the last loader allowance through localization,
complete document/classification copies, transaction data and retained format buffers.
Published resolver dependencies include their full retained classification, not only raw
documents. Conversion reuses immutable unchanged classification while rebasing preservation
placements and validating localized references.

Fermat's independent follow-up found two additional P2s after the first implementation:

- Publication-shape validation cloned a complete canonical document outside conversion work.
  The conversion path now borrows its ordered candidate and admits every retained JSON/RON
  buffer chunk before copying. Prefab validates its nominal format once, without a temporary
  Scene encoding. The old ordinary validation/encoding interface remains unchanged.
- Asset traversal used the remaining retained-output quota, making acceptance depend on entity
  order and breaking full readmission equivalence. Per-record traversal now uses its configured
  limits while aggregate reference retention remains separately bounded. Exact 64-node Scene
  and Prefab tests compare incremental/full classification and same-limit JSON/RON Reopen.

Fermat's final read-only review found no remaining concrete finding. Parfit independently
closed the Host findings after fixes: final Close polls conversion after physical task shutdown,
and review retention includes published/current/replacement pages and String-to-Arc scratch.
The Close regression deterministically releases a real worker after the first conversion poll,
drives actual task shutdown without conversion polling and requires terminal cleanup before
Close Complete. The frontend now allows cancellation during final validation.

The final full Reflect/Scene/Tooling cohort passed:

- Command: `cargo +1.95.0 nextest run -p nara_scene -p nara_reflect -p nara_tooling --all-features --locked --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail`.
- Run `9c471314-b3f1-4e66-a100-d90cb3f62ccd`: **983 passed, 4 skipped**, 59 binaries.
  Build 7.91s; execution 45.439s.
- The preceding run `03e1a279-1e28-46cb-ac0f-455d1209c49d` had 981 passes and two old
  traversal/retention-coupling assertions. They now check genuine adjacent aggregate output
  boundaries, including 56/57 bytes for three payload-relative property references.
- Earlier intermediate Root13 and combined62 runs are not final evidence. Combined62 had
  61 passes and an obsolete UI assertion requiring cancellation to be disabled during
  validation; that assertion now matches the implemented Host/UI cancellation contract.

Rust formatting ran after all writers froze. Cargo is strictly serial, Rust 1.95.0,
`CARGO_BUILD_JOBS=1`, root target reused. No Cargo is allowed from other lanes.

# Next Action

Observe live exec session **98321** to completion. It runs full Root/Editor all-feature library
nextest; log is `%TEMP%/nara-prefab-host-editor-full-tests.log`. Do not restart or overlap Cargo.

Then run the independent module-consumer tests `editor_prefab_conversion` and `prefab_conversion`
with `CARGO_TARGET_DIR` set to the root target. Execute ignored native
`native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab` with DX12,
plus affected retained Scene/Canvas controls. The native test validates actual UI input,
hierarchy, rendering inputs/composite receipts and Save/Reopen for both nominal kinds; it does
not claim framebuffer readback. Finish affected checks/Clippy, evidence/ledger/docs, precise local
commits and ff main only after coherent verification. No push or PR.

# Citations

- [Predecessor integration state](2026-10-27T053151Z-editor-complete-prefab-conversion-integration-and-pre-copy-budget-corrections-394c727d98ae4d55846b93f121fd416b.md)
- [Conversion plan](../../../../plans/2026-10-27-prefab-conversion-plan.md)
- [Source admission](../../../../../crates/nara_scene/src/authoring_decode/structure.rs)
- [Borrowed shape validation](../../../../../crates/nara_scene/src/format.rs)
- [Asset quota regressions](../../../../../crates/nara_scene/src/authoring_decode/asset_budget_tests.rs)
- [Host operation](../../../../../src/project_host/runtime/editor/prefab_conversion.rs)

---
type: "Verification Evidence"
title: "RGF-U17 honest editor product loop verification"
description: "Commit 0a87503 closes receipt-backed known-schema persistence, dirty close/reopen, Host-owned Play, safe-point runtime edit, and Apply Changes with reviewed regressions."
timestamp: 2026-07-21T05:24:52Z
record_id: "b9ddf86ba8984be9a8ffd7498f5354cc"
tags: ["rgf-u17", "editor", "persistence", "play", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0a87503"
verified_by: "Codex multi-persona review and independent validator"
---

# Verification

RGF-U17 was verified at `0a87503` against the active plan's known-schema editor Save, dirty-close,
reopen, scheduled Play, runtime-edit, Apply Changes, and bounded retirement contracts.

# Result

- `EditorProjectSession` owns project persistence and the single concrete Play lifecycle; tooling
  and egui retain only UI-neutral commands, views, models, and results.
- A saved checkpoint advances only from a matching atomic replacement receipt. A post-publication
  evidence failure enters sticky `PersistenceUncertain`, blocks blind retry, and clears only after
  explicit Reopen bytes are published into the workspace and committed as the new disk baseline.
- Dirty Close/Exit retains Save, Discard, or Cancel intent until persistence and runtime retirement
  finish. Normal multi-frame retirement stays `RetiringPlay`; terminal close failure cannot report
  successful Stop or begin Restart.
- Runtime edits require component and field `Edit` capability. Runtime edit and Apply Changes are
  mutually exclusive safe-point operations; Stop and Restart cancel pending export without later
  document mutation. Runtime Inspector structural removal is disabled.
- The formal review validated ten findings, rejected two unproven findings, and closed every
  validated P0/P1 plus the material testing gaps. The final review report is local run artifact
  `20260720-191015-f9d34159/report.md` under the Compound Engineering review directory.

# Evidence

- `cargo nextest run --locked -p nara_fs -p nara_tooling -p nara_tooling_egui --test-threads=1`:
  63 passed, 3 explicitly skipped.
- `cargo nextest run --locked -p nara --features tooling,runtime-2d,serde --lib --test
  editor_persistence --test scene_play_mode --test workspace_play_runtime --test-threads=1
  --no-fail-fast`: 57 passed.
- `cargo check --workspace --locked`: passed.
- Targeted strict Clippy passed with command-line allowances only for pre-existing unrelated
  `nara_app`, `nara_asset`, root ingest/test, and dead-code findings.
- `cargo fmt --all -- --check`: passed.
- A detached `HEAD + U17 staged index` checkout ran `cargo nextest run --locked -p nara --test
  architecture_docs --test-threads=1 --no-fail-fast`: 8 passed. Commit `10d9795` first made
  governance Markdown parsing independent of Windows CRLF checkout conversion.
- Workspace nextest executed all 922 tests: 920 passed; the capability expectation was corrected
  and reverified, while the remaining governance failure was the stale ADR 0076
  `ScenePlaySession` ledger anchor that this evidence closure replaces.
- Focused regressions cover post-name-switch uncertainty, failed Reopen state preservation,
  multi-frame retirement, terminal shutdown failure, pending-operation arbitration, Edit
  capability, result acknowledgement, and egui control gating.

# Follow-up

- ADR 0082 and ADR 0084 remain Proposed; RGF-U23 owns their independent decisions and compatibility
  result. U17 implementation evidence does not promote either proposal.
- ADR 0091 remains Proposed. U17 proves only the bounded known-schema, single-document receipt and
  explicit-reconcile subset; recovery journals, old-or-new multi-document publication, and general
  concurrent-writer admission remain unimplemented.
- Multi-document Exit must become a whole-workspace transaction before a public multi-document
  concrete Host path is exposed.

# Citations

- Active plan: `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u17-complete-honest-editor-save-reopen-and-play`
- Implementation: commits `3d23c40` and `0a87503`
- `src/project_host/runtime/editor.rs#EditorProjectSession`
- `src/project_host/persistence.rs#ScenePersistenceHost`
- `tests/editor_persistence.rs`
- `tests/workspace_play_runtime.rs`

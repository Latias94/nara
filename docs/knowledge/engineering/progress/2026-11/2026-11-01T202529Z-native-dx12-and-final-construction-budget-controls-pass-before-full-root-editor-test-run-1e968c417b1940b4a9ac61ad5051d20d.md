---
type: "Work Progress"
title: "Native DX12 and final construction budget controls pass before full Root Editor test run"
description: "Seven native trajectories, owning802 and final budget12 pass; correctness Clippy all-targets succeeds, full Root and Editor all-feature libraries are compiling."
timestamp: 2026-11-01T20:25:29Z
record_id: "1e968c417b1940b4a9ac61ad5051d20d"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-final-qualification-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
supersedes: "ac4dc7362e464029bb70551fd9d200b4"
---

# Summary

Full foundation Goal active. Complete authored Schema upgrade/source-authority/UI integration
remains uncommitted on refactor/foundation at ed21b230. All writers/reviewers are closed. Final
source is frozen; current full Root/Editor library tests must finish before another Cargo. The
remaining delivery work is their failures if any, minimal/fmt checks, honest final evidence/ledger
and precise local commit/ff main. Do not push/PR or mark the full foundation Goal complete.

# Details

Actual Windows/DX12 run `f6f3acea-17fc-44b0-9378-e19310f274b6` passes7/7,313 excluded,31.813s after
2m build. It covers three critical hidden-window Game/Scene lifecycle/input/presentation trajectories,
retained Scene navigation, UI Canvas priority/close, complete Prefab-to-local conversion with
Save/Reopen, and external stock Scene effects/picking/edit/Undo. DX12, nonfallback and required
adapter were explicit. It qualifies those concrete scopes, not Linux/macOS, monitor scanout,
future custom-drawn Inspector or a new graphical upgrade dialog.

Owner run `d703f405-6613-4dbd-b157-eb30db30fc04` passes802/802 across Scene/Tooling/UI full
all-feature library suites, zero skipped,33.826s. It includes initial five construction-limit
tests, actual UI creation templates and source-safe recipe controls. Independent budget review
then found two remaining P2s: declaration-free construction could coalesce9 repeated field writes
under the larger limit, and history::admit_input skipped precopy admission for mixed >8-field
transactions because of its old cheap-rejection assumption. Parent fixes both below; owning802
therefore precedes those two narrow changes and must not be called final-source full coverage.

Construction lower now uses ordinary8-count property lowering before coalescing when the draft
contains neither entity nor component declarations. A real mixed construction keeps the existing
total256 operation count and value budgets. History::admit_input now skips over-limit field arity
only for pure-field patches; all valid mixed inputs cross existing bounded history cost before
copying. No byte/node/history/replay cap changes. Existing raw mixed transactions with a valid
structure operation legitimately share256; no-op placement is not a separate authority issue.

New controls prove9 repeated declaration-free writes refuse for Scene/Prefab,8 can commit one
coalesced field and Undo, and an actually valid mixed AddComponent(60KiB template)+9 scalar writes
crosses the50,000 per-entry history admission before source copies. The initial test constructor
reversed total/entry limit arguments and returned None; parent corrects only its parameter order.
Run `3a8d73f1-8fed-4f26-b155-533bd9cd91cd` passes12/12 selected construction/property/structure/history
tests,541 excluded,0.594s. These results include both final P2 fixes and positive controls.

Mill `01a198bb-bfe4-7752-8d61-e5a2a7adf447` independently rereviewed both exact fixes and tests,
found no remaining assigned P1/P2, and is closed. Aristotle's test writer is also closed. All prior
scope-specific review limits remain as recorded, not one whole giant-diff review claim.

Correctness Clippy session85367 finishes0 in3m on final production/test source:
cargo +1.95.0 clippy -p nara -p nara-editor -p nara_scene -p nara_tooling -p nara_ui
-p nara_project -p nara_reflect --all-features --all-targets --locked -- -D clippy::correctness
It has ordinary existing/style/large-result warnings; do not call it warning-free. No broad
unsolicited warning cleanup was performed. TEMP/nara-authored-upgrade-final-correctness-clippy.log.

Current serial Cargo session44885 runs:
cargo +1.95.0 nextest run -p nara -p nara-editor --all-features --lib --locked
--test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
Log TEMP/nara-authored-upgrade-final-all-feature-libraries.log. It recompiles the combined root
all-feature dependency union, now at final Editor/root lib-test linking around04:25 local. It has
no reported error but no test result yet. CARGO_BUILD_JOBS1 and root target remain. Do not start
another Nara Cargo, kill unrelated Baozi Cargo, or infer success from sparse compile output.

Final Reference focused2, actual bundle producer/check, CLI/process22, public100, independent49,
Root392/Application35 and earlier Reflect/Project99 have their exact prior scopes in predecessor
records. The final UI template/source construction did not alter unrelated project generations;
current bundles preserve every non-UI owner exactly relative to HEAD. Source and all tests are
formatted; no further product edit followed the final budget controls or Clippy.

# Next Action

Poll44885 to actual terminal. Fix real regressions without restoring obsolete readers or bypassing
source authority. After final success, run root minimal and Scene no-default lib checks, fmt checks
for root and relevant independent workspaces, and memory validation. Reuse target/jobs1, serial.
Further broad reruns need changed behavior or unresolved failure, not documentation closure.

Create complete-unit Verification Evidence with exact counts/run IDs and limits; update the current
authored-upgrade plan, ADR implementation ledger and foundation/research links to that bounded
qualification. Do not rewrite immutable progress/history to erase failures or call the previous
Reference157/159 a single full pass. Record Windows interruption tier separately from power loss.
Inspect all348 intended tracked/new paths (count before final evidence), exclude vendor/repo-ref/
target, use exact pathspec staging, Conventional Commit and local ff main. User explicitly permits
local commits/merges. No push or PR. Source changes are this operation's authorized work, not user
pollution; never restore/reset/stash them. Remaining vendor stays untracked.

Full Goal retains separate CP2 extraction/root/reference gates, motion/execution, material
generations and editable Nara-drawn Editor work. CP2 source study is linked and not adopted.
Branches are only main/refactor/foundation. No permission question or external approval is pending.

# Citations

- [Predecessor production correction](2026-11-01T201206Z-complete-reference-ui-source-workflow-passes-after-construction-budget-scope-correction-ac4dc7362e464029bb70551fd9d200b4.md)
- [Complete unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- TEMP logs: nara-authored-upgrade-final-native-dx12-tests.log,
  nara-authored-upgrade-final-source-and-ui-owner-tests.log,
  nara-construction-final-budget-regressions.log,
  nara-authored-upgrade-final-correctness-clippy.log,
  nara-authored-upgrade-final-all-feature-libraries.log.

---
type: "Verification Evidence"
title: "Compiled Scene owners and reviewed transition corrections pass current public cohort"
description: "Final Reflect Scene Tooling UI1629 pass; Root native corrections20 pass; bootstrap render and cancelled-upgrade corrections are committed while integrated consumers remain open."
timestamp: 2026-11-02T05:34:11Z
record_id: "e8f7d20e3e2b49d5b086ce2100f1b9b2"
producer_id: "codex-foundation-main"
run_id: "compiled-scene-review-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "080852c"
supersedes: "08c9c66280314ff5904725f2ff2511f1"
---

# Verification

The complete foundation Goal remains active. This record qualifies the current owner/public
cohort and three independently committed corrections, not the whole uncommitted compiled unit,
CP2 extraction or the foundation. Main and refactor/foundation both point to080852c. Nothing
was pushed. The large compiled-production and source/export changes remain in the worktree.
All agents/writers are closed. Preserve vendor. Serial jobs1/root target and Rust1.95 apply.

# Result

| Check | Actual result and scope |
| --- | --- |
| Final Reflect, Scene, Tooling and UI libraries plus all public tests |1629/1629 passed,4 preexisting skipped;80 binaries; run9ac76e5b-e122-4f29-9dae-2511252af739. Build500s, execution77.430s. |
| Root seven earlier fixture failures, startup replacement, actual F2 retry/storage, F4 owner cases and hidden-window F3 native cases |20/20 passed, run1c72d53e-819e-4d55-bbda-79aa0fdb58f2;1890 unrelated excluded. Build301s, execution4.986s. |
| Earlier complete Root library run |1461/1468 passed,5 skipped; run95d47a99-f7b6-4986-82bf-6e58c44364c8. All seven failures are covered by the focused replay above. No claim that the earlier complete run was green. |
| F4 actual failing-project public caller |1/1 passed, d7f55fee-f77a-4139-8743-c64b75baa85b; distinctive compiler diagnostic retained, tool not launched, manifest/lockfile unchanged. |
| F3 original actual DX12 run |2/2 passed,8c6999f7-d29a-4039-aea8-155d8108ee54; subsequently hidden windows are covered by the final20-test replay. |
| Scoped independent reviews |No remaining assigned P1/P2 in the sealer, Native/Runtime/custody, F1 conversion and selected routing, F2-F4 corrections or new live export. Reviewers were read-only; their results are not independent execution evidence. |
| Docs and configuration |1260 local file links passed before this record; CI YAML and explicit render lanes parse. Whitespace check passes; root fmt ran after all writers froze. Remote CI not run. |

# Evidence

The final1629 run covers the actual compile/SceneCompilation names, private move-only compiled
storage, ordinary receiving checks before codecs, Runtime flat claims/cook, selected source
conversion and new live-export source snapshots. The formerly failing foreign-owner test now
checks the precise owner diagnostic and unchanged OwnerAdmission reason. Native/data export
both establish new context only from the lowered selected field. Independent8nodes/56bytes,
one-short node/byte and depth controls pass, as do same-definition recompile and JSON/RON tests.
F1 nested/masked/routed defaults, ordinary mixed AssetRef/EntityRef Reset and exact Undo/Redo
also pass. No default budget was raised and no incompatible receiver was relaxed.

The complete Root run's five processed-start failures were a raw Runtime fixture omitting its
actual Sprite Schema owner. It now encodes owner/version/value from the selected plan. Two
startup cases compiled under an unowned manual registry and received under the installed owned
provider. The fixture now uses that actual provider. Both failure groups pass in the focused
replay. Windows abort text followed the earlier assertion; no stack-overflow diagnosis was made.

Precise completed commits, each locally fast-forwarded to main:

- `349921f fix(tooling): retain failed bootstrap compiler diagnostics`. Two source/test files.
- `d6dccbc fix(render): revoke displaced runtime window interaction`. Shared successful-replacement
  invalidation, existing guard custody, native regressions and qualification instructions.
- `080852c fix(editor): isolate authored upgrade recovery attempts`. Fresh pre-admitted128-bit
  destination suffix, actual cancel/identical-proposal adoption, existing-destination refusal and
  the exact child-plan contract. The unrelated test-support import was left unstaged by applying
  only the reviewed storage-test hunk to the index, preserving working source throughout.

No main worktree switch, reset, restore, stash or vendor read was used. The first partial-staging
attempt normalized CRLF and failed its cached apply check without staging anything; the successful
attempt preserved patch bytes. No product source changed because of that failed command.

# Follow-up

First poll Cargo19841: it is running Root all-feature public integration tests, --test '*',
jobs1/test-threads1, log TEMP/nara-compiled-final-root-public-tests.log. It has reached individual
public binary linking; no result yet. Previous40734 is terminal0 (final1629). No open agent or
other owned Cargo remains. Other baozi builds may exist and must not be interrupted.

After Root public qualification, run the actual no-project-tooling lib-test configuration
(runtime-2d,serde,tooling), relevant Editor/public/native conversion/ordinary edit/retained Scene,
ReferenceGame and independent consumers, compile-fail docs, minimal and all-target correctness
Clippy. No need rerun already passing unaffected Root1468 when the focused replay closes its
specific failures and later changes do not invalidate them. Record and correct actual failures.
The separate CI consumer additions still need local command execution; parsing is not CI success.

The exact compiled/source unit has not yet been committed or claimed complete. Finish its
consumer/native gates and update current guides/ADR ledger with this actual scope, then precise
commit/local ff. Root/placement/extraction correspondence/publication/recovery remain unfinished;
the active Goal must not be narrowed to the current1629 tests. No push/PR.

# Citations

- [CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Foundation follow-up disposition](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [External review](../../../../audits/2026-11-02-refactor-follow-up-review.md)
- [Prior detailed integration state](../../progress/2026-11/2026-11-02T052010Z-follow-up-repairs-and-live-source-export-integration-before-final-compiled-qualification-08c9c66280314ff5904725f2ff2511f1.md)
- [Qualified F4 caller](2026-11-02T051018Z-failed-bootstrap-diagnostics-retained-through-the-real-project-tool-caller-446882fff4584df1a23d3930a6fda2dc.md)

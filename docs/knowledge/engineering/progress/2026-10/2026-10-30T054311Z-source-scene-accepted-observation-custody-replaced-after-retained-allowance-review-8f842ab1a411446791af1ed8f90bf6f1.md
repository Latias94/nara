---
type: "Work Progress"
title: "Source Scene accepted observation custody replaced after retained allowance review"
description: "Records predecessor native/build/game evidence and a later independent accepted-result retention finding with its move-only handoff replacement under qualification."
timestamp: 2026-10-30T05:43:11Z
record_id: "8f842ab1a411446791af1ed8f90bf6f1"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "3ee2da1"
---

# Summary

The complete foundation Goal remains active. Source Scene is a large uncommitted unit on
`refactor/foundation`; `main` remains `3ee2da1`. No push or PR. All writers are frozen. Cargo is
strictly serial with one job and root `target`. Preserve WIP and untracked `vendor/`.

# Details

After the earlier root/Editor/public results, actual qualification additionally produced:

| Predecessor cohort | Actual result | Run or log |
| --- | --- | --- |
| Eleven root integration binaries | 129 passed, 2 diagnostic assertion failures, 1 existing skipped | `40a72494-7dc4-4ea9-b68f-5b659c0bce75` |
| Corrected integration diagnostic regressions | 2 passed, 26 unselected | `2c309b79-84ac-4233-a3a5-8a2c1e096c56` |
| Actual Windows/DX12 critical three, retained Scene, Canvas and complete conversion | 6 passed, 280 unselected | `66d8aa82-577f-4503-bd47-5a13cc3577db` |
| Workspace all-feature/all-target check | passed; existing warnings | `nara-source-scene-workspace-check.log` |
| Minimal `project-workflow-cli,tooling` and `runtime-ui` library checks | passed; existing feature warnings | `nara-source-scene-minimal-source-check.log`, `nara-source-scene-minimal-ui-check.log` |
| Root + Editor all-feature/all-target correctness Clippy | passed; existing warnings | `nara-source-scene-clippy.log` |
| Reference Game all-feature ordinary tests | 179 passed, 0 skipped across 34 binaries | `bfa7c27c-5b63-4766-b148-7c47a87a68d6` |

The two integration corrections preserve strict rejection. Unknown unselected data reports the
specific `scene.unknown-component` and component identity; duplicate native UUIDs reject at the
shared canonical catalogue with `project.content.asset-identity-conflict`. No compatibility branch
or relaxed production check was introduced. The Reference Game compile/execution completed
normally; its source was already compiled before the next implementation edits.

## Later Independent Finding And Replacement

Sagan's narrow accepted-result review found a real P2 after those tests: worker completion refunded
all token bytes while the Editor still held complete selection copies, including unrelated open
documents. Accepted source/path/participant observations also allocated without a continuing Host
charge. Source-index charges do not cover new cloned ProjectLocation key strings. Previous green
native/build/game evidence therefore does not establish this final retention replacement.

The owning protocol now splits `AuthoringSourceCaptureInputs` into Host and worker retained bytes.
Admission checks their checked sum; worker snapshots cannot borrow Host allowance. Scene prepare
releases worker-only inputs after dropping them, transfers selected document charges and returns
the Host allowance in `AuthoringDocumentCapture::usage`. Atomic `replace_host_retained_bytes`
replaces only that portion; failure leaves source/output usage unchanged. Startup validation can
admit extra Host observations over the complete project usage without re-reading files. Source-only
callers have zero Host bytes; Prefab conversion keeps its conservative existing input custody.

Parent rewrites Editor state rather than preserving duplicate selections. `CaptureWork::Compose`
owns and moves the one complete offered selection into Candidate. `EditorSceneCapture` has no
duplicate selection field. Acceptance computes output cost before allocating, admits `S + O`,
moves only demanded selections, drops unused entries and vector storage, then settles to `O`.
`CapturedScene` retains root plus actual `CapturedSource` entries, not unrelated open documents.
Its validation is created after settlement. Startup observations are independently admitted.
Ordinary selection vectors reserve their concrete known count.

Sagan wrote Authoring APIs/tests, parent wrote Editor integration/test. Sagan is now read-only
reviewing the integration. Dirac independently reviews the API implementation. Both are active
but frozen for writes. New tests cover worker transfer, Host/worker non-substitution, exact/one-short
and overflow atomic replacement, startup validation retention and an actual Editor gated handoff
that retires unrelated inputs. A closure-lifetime build error was corrected with a moved borrowed
root capture; no tests ran in that failed build.

Current Cargo session `51298` runs 84 focused capture/Schema/Editor conversion tests, nextest ID
`736e3278-3e2a-48a0-a5ff-52b1830aafab`; at capture time it has started but no terminal result.
Log: `nara-source-scene-retention-tests.log` in TEMP. Do not start another Cargo until terminal.
Formatting passed before execution. No source commit/ff main or final qualification record yet.

# Next Action

Observe `51298`, fix actual failures and close independent reviews. Re-run affected root/Editor
and native scopes, final source feature/build/lint checks and the affected Reference Game Editor
tests against this handoff replacement. Earlier independent Source Scene 12 covers unchanged
Runtime content paths. Update owning plan/implementation ledger with final exact evidence, review
the complete diff, commit precisely and ff local main without remote publication. Keep the whole
Goal active; input/target delivery, render composition and custom-drawn Editor remain open.

# Citations

- `progress/2026-10/2026-10-30T044756Z-source-scene-owner-and-public-cancellation-observation-verification-checkpoint-a400ebcdfccc4c4d85d6b7cf00f02baa.md`
- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
- `docs/architecture/adr/0047-editor-workspace-and-scene-document-state.md`
- `docs/verification/native-editor-qualification.md`

---
type: "Work Progress"
title: "Source Scene closure qualification and global lookup boundary corrections in progress"
description: "Records executed Source Scene cohorts, physical-return/retry fixes and the discovered global lookup and overlapping-reader identity corrections before qualification."
timestamp: 2026-10-30T04:17:30Z
record_id: "322c0046b4b74b459d2ccde9613a899a"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "3ee2da1"
---

# Summary

The complete foundation Goal remains active. Source Scene work remains uncommitted and under
qualification on `refactor/foundation`; local `main` remains `3ee2da1`. No push or PR. Preserve
all concurrent working changes and untracked `vendor/`. User explicitly reiterates correct
ownership/reference contracts and replacement of obsolete code without historical compatibility.

# Details

The finite input token is now `AuthoringSourceCaptureInputs`, consumed by both real Scene and
Prefab-conversion consumers. Editor preadmits borrowed selection/snapshot costs before copying
the working set. Conversion currentness now borrows complete root/participant observations.
Source-ready Lagrange fixes cover finite production decode memory, exact compiler-result
transfer, Path self-metadata, unavailable Prefab selection and actual native metadata aliases.

Parent fixed Gauss's two Editor findings. No-root cancellation continues polling physical return,
then permits reclamation and clears deadlines. Failed preflight clears an obsolete full selection,
keeps a finite root/revision/Schema/persistence retry observation and respects the existing retry
deadline. Accepted captures keep actual dependency data after native publication. The new gated
no-root test passes. The retry integration test is still being corrected to use legal source
data and a policy allowing the separately reserved startup Image identity store.

Actual serial execution:

| Cohort | Result | Nextest run |
| --- | --- | --- |
| Root content/requests/Authoring capture/Editor capture/source Export | 119 passed, 3 failed, 895 unselected; 122 executed | `158e4fbe-d68c-4b4b-bdcb-cbdf4db2c474` |
| Three corrected review regressions | 1 passed, 2 failed, 1014 unselected | `22ec411b-b94a-4f5c-99e6-1dab62688c0c` |

The first cohort includes the five new production Source-admission regressions, corrected earlier
Runtime tests, Source Product requests, actual Editor dependency Save/Reopen and the relocated
cyclic-source Export. A private helper visibility build error was corrected before execution.
The second cohort confirms that unrelated metadata does not belong in exact captured file facts;
catalogue identity uniqueness still reobserves it independently. Its remaining stable recovery
failure is a real bug: a missing global UUID was observed through an Assets fallback scope, so a
later Prefabs source remained incorrectly unresolved during validation. Parent removes the
binding from `StableLookupObservation`: global UUID target observations and concrete reader/query
bindings are separate. An additional cross-reader recovery test is added; neither correction has
run yet.

Independent reviewer Dalton found another real P2: with overlapping roots and stable native
identity, Path-first loading wrongly compares the read alias to an outer-root sidecar alias,
while StableId-first hits the canonical cache and succeeds. Dalton is now the bounded fix writer
for `native_source.rs`, its signature call sites and `capture/tests/source_admission.rs`. It will
remove the redundant declaration-path parameter and validate actual configured scopes against
the complete canonical source, with both loading orders tested. Do not sample these files until
source-ready. Gauss re-reviews Editor; Lagrange independently reviews parent Runtime ownership.

ADRs 0047/0110 and current guides are being reconciled. They are not qualification evidence.
All Cargo commands are serial, one build job and root `target`; no current Cargo is running at
capture time. The 8 MiB retry policy first rejected startup's separate identity-store reservation;
its next source fixture violated single/aggregate string limits. The pending fixture uses five
legal 7 MiB documents, each eight 896 KiB values, under a 32 MiB capture policy. No production
limit is raised to hide a failure.

# Next Action

Receive Dalton's freeze and both independent reviews. Format after inspecting the final diff.
Run corrected focused regressions, independent public Source Scene 12 and affected full
Authoring/Editor/Schema/Prefab-conversion/source Export cohorts. Then run relevant feature/build,
correctness lint and actual native trajectories before precise commits and local ff main.
Input/target-build delivery, render composition and the future custom-drawn Editor remain open.

# Citations

- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
- `docs/architecture/adr/0047-editor-workspace-and-scene-document-state.md`
- `docs/architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md`
- `progress/2026-10/2026-10-30T034018Z-source-scene-actual-reader-closure-and-working-copy-integration-under-review-f469994dd6dd4a5f9536f8b8b8e8d83d.md`
- Temporary logs: `nara-source-scene-owner-tests.log`, `nara-source-scene-review-regressions.log`.

---
type: "Work Progress"
title: "Source Scene owner and public cancellation observation verification checkpoint"
description: "Records closed independent findings and executed root, Editor and public Source Scene evidence while integration/native qualification remains in progress."
timestamp: 2026-10-30T04:47:56Z
record_id: "a400ebcdfccc4c4d85d6b7cf00f02baa"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "3ee2da1"
---

# Summary

The complete foundation Goal remains active. Source Scene is still an uncommitted qualification
unit on `refactor/foundation`; `main` remains `3ee2da1`. All writers/reviewers are now frozen and
closed. No push/PR, no concurrent Cargo, one job and the root `target`. Preserve the WIP and
untracked `vendor/`. Current Cargo session `57555` is running the eleven root integration binaries
listed below; do not start another Cargo invocation until it returns.

# Details

Independent findings are closed in source, with actual owner tests:

- `StableLookupObservation` retains a global UUID/canonical target, without the old Prefab boolean
  or query reader binding. The catalogue uses a keyed map for deduplication and selected metadata
  iteration, avoiding repeated linear scans. Missing-to-found changes across readers now revoke
  the original observation. Query scope/kind and actual reader remain separately validated.
- Native metadata admission validates the complete canonical location through actual configured
  scopes, rather than requiring equality with the current narrow read alias. Authoring removes
  the redundant declaration-path parameter and tests Path-first/StableId-first plus narrow
  capability revalidation. Runtime delegates the same relation to its existing canonical index;
  startup, declared Path/Product, both native identity policies and outer/nested metadata aliases
  pass. Pure ProjectLocation Path remains metadata-free until a real lookup observes a sidecar.
- Cancellation return is independent of visible presentation. Hidden/no-root paths cancel and
  continue polling without admitting replacements. Frame models publish actual physical pending
  even with no extent, so the native scheduler continues maintenance. Gate tests assert public
  pending before return and false afterwards, cleared clocks and available reclamation.
- Failed preflight keeps a finite root/revision/Schema/persistence retry observation and clears
  obsolete full selections. The actual low-budget fixture uses five legal 7 MiB documents under
  a 32 MiB policy, without changing production limits. Root/candidate deep copies remain admitted
  at their actual owner; sharing an immutable root Arc does not copy document data.

Lagrange independently rereviewed Runtime and closed its canonical reader finding. Gauss closed
all Editor cancellation/retry/public observation findings. Dalton's independent native-order
finding is fixed and tested; it did not run Cargo. These are bounded correctness reviews, not a
claim that the complete foundation architecture is finished.

| Actual cohort | Result | Nextest run |
| --- | --- | --- |
| Root all-feature library | 1014 passed, 1 new fixture failure, 4 existing skipped | `68bbd445-69f8-4f1e-a03c-899f2f6a0337` |
| Corrected canonical native-alias combination | 1 passed, 1018 unselected | `e218c131-852a-4753-ad96-77ebd0729d4c` |
| Editor library after final hidden-frame model correction | 413 passed, 606 unselected | `d8a0faba-de19-4252-8415-f63d3694f36c` |
| Independent public Source Scene consumer | 12 passed, 0 skipped | `73a2ba0c-05d8-4d71-bac8-803632542ecd` |

The sole broad-run failure incorrectly expected a known Product and metadata receipt from a pure
ProjectLocation Path load without discovery. The focused correction preserves no eager scan and
all combination assertions. The later production change is limited to hidden physical-pending
frame observation, covered by the complete 413 Editor cohort. Earlier failed/fixture runs remain
in the preceding progress record. New source/decode, global recovery, mixed-order, low-budget and
physical-return cases pass in these actual scopes.

`57555` currently selects `project_scene_content`, `project_content_delivery`,
`project_content_boundary`, `project_content_limits`, `project_content_boot`,
`project_runtime_boot`, `project_workflow_check`, `product_data_components`,
`runtime_scene_activation`, `workspace_play_runtime` and `project_host_boundary` with all features.
Its log is `nara-source-scene-integration-tests.log` in TEMP. Earlier logs are
`nara-source-scene-root-lib-tests.log`, `nara-source-scene-native-alias-regression.log`,
`nara-source-scene-editor-tests.log` and `nara-source-scene-independent-tests.log`.

Rust workspace/independent formatting passed before these executions. Changed current Markdown
links pass (seven files); engineering memory validation returns zero with existing historical
warnings. ADRs 0047/0110 and guides are reconciled, but the implementation ledger/plan still need
the final qualified evidence. No final source commit or local ff main has occurred.

# Next Action

Finish the current integration cohort; repair any actual failure. Run workspace all-feature/
all-target check, relevant minimal source/tooling features and correctness Clippy serially.
Run actual Windows/DX12 critical three plus retained Scene navigation, Canvas priority and complete
Prefab conversion trajectories under the native qualification procedure; ordinary tests do not
replace that evidence. Qualify the affected Reference Game path. Review final diff, capture exact
verification/limits, make precise Conventional Commits and ff local main without pushing.
Input/target delivery, render composition and custom-drawn Editor remain separate open Goal work.

# Citations

- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
- `docs/verification/native-editor-qualification.md`
- `progress/2026-10/2026-10-30T041730Z-source-scene-closure-qualification-and-global-lookup-boundary-corrections-in-progress-322c0046b4b74b459d2ccde9613a899a.md`

---
type: "Work Progress"
title: "Fresh authored-upgrade recovery owner and mixed generation qualification"
description: "Headless recovery compiles and owner tests pass; actual process proof and cleanup-budget correction remain in progress."
timestamp: 2026-11-01T16:27:42Z
record_id: "48a5b7438aa44d56880dd7100a4afe70"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-recovery-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
git_branch: "refactor/foundation"
git_commit: "ed21b23051342587574be75da017da888270b597"
---

# Summary

The S2 implementation now has a public headless recovery owner, compiling without Tooling.
All authored-upgrade changes remain uncommitted on `refactor/foundation` at `ed21b230`.
The complete foundation Goal stays active. Neither these owner checks nor the already qualified
live publication complete fresh-process recovery, v2 Check/Play or the broader rebaseline.

# Details

`PreparedSourceUpgradeSchema` separates its live incumbent receipt from freshly admitted recovery
models and actual Pending/bundle custody. The same model, source-pair, successor-inventory, saved
project and exclusive Ready kernels serve both workflows. Recovery does not reconstruct an old
Schema receipt, Session, history or nominal basis. The shared bounded wire decoder replaces the
former live-only duplicate. `source_upgrade_policy.rs` now owns neutral complete-operation limits
and history effects; the old Editor-specific limit type is removed without an alias.

`AuthoredSchemaUpgradeRecovery::inspect` captures the referenced package and complete mixed
inventory, verifies stored reviewed-working/successor pairs and prepares publication without writes.
Borrowed Review names inspection-time Original/Successor/Unchanged separately from replacement
evidence. Confirmation revalidates that same set. Finite advancement retains original move-only
preparations and allowances on refusal; already-new members are observed without replacement.
Only actual final Ready publication returns a receipt. The private saved successor is disposed
when extracting that receipt; ordinary Open admits its own fresh generation afterwards.

Actual verification so far:

- Scene data-only recovery8 passed: `3380a811-0c74-4e64-a88a-7be9253c37be`.
- Mixed-inventory/source-publication18 passed: `32486e9f-7862-43cc-afba-e1111d3cf36b`.
- Root recovery-owner8 passed: `30146088-fca3-414c-bd04-6ec65aae822f`. They use actual Pending
  files, dirty working copies, alternate profiles and RON Prefab; cover all-old/mixed/all-new,
  same-byte replacement/third-content/membership refusal, final external edit and finite limits.
  This run precedes the subsequent cleanup-budget correction.
- CLI-only Root lib check passed in 5.42s. Root all-feature lib check passed in 2m12s; a new
  unused intermediate history reexport was removed afterwards.
- Initial owner test compilation found one missed neutral history import and two guard patterns
  accidentally placed inside `Some`; corrected without changing test acceptance.

Independent review found one P2: failed native cleanup retries could return before any metered
advance. The parent added positive work and bounded result/metadata scratch admission only when
cleanup is pending, before its actual call. Scratch is settled on either result; custody and
performed work persist. No new handle, path parse or body IO is charged. The reviewer is adding
Sources/Ready regressions and minimal test-only hooks; this correction is not yet qualified here.

The independent `authored-upgrade-recovery` workspace depends on renamed Nara with only
`project-workflow-cli`. Its CLI receives project/profile/finite limits, supports source/bundle/Ready
stop points and never executes conversion. Five process scenarios in the existing independent
EnemyStats consumer use a real public Session producer which exits after actual Pending, before
driving any source step; recovery is a separate executable. These files are source-ready but not
yet compiled/run. Companion build session `74163` is currently running serially in root target;
inspect its actual terminal result before another Cargo command. Log:
`TEMP/nara-recovery-companion-build.log`. Its first build generates its independent lockfile.

The distinct v2 product lane is implementing fixed data-owner recipes and an actual Rust runtime
consumer. Initial product fixtures must include all selected native builtin owners and choose
available/surviving-unavailable controls before upgrade. No post-publication bundle repair or
unknown-row deletion can count as consuming the actual upgrade. Existing `scene_runtime` World
materialization is insufficient for root Check/Player/Play evidence. A fixed old v1 product must
reject the same published v2 set with the exact selected-owner-divergent diagnostic.

# Next Action

Finish cleanup qualification, build/run the independent process scenarios, integrate the fixed v2
Check/Runtime/Play consumer and independently review actual code/results. Live partial-publication
retry/reconciliation still needs its own safe retained-owner route; fresh-only evidence does not
close it. Wait for test/product writers to freeze before diff review and `cargo fmt --all`. Keep
Cargo strictly serial with jobs1 and root target. No commit/ff main until the complete unit is
qualified; no push/PR and no vendor changes.

# Citations

- [Upgrade plan](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [Research reconciliation](../../../../research/2026-10-30-engine-direction-reconciliation.md)
- [Live-publication predecessor](2026-11-01T153359Z-live-authored-upgrade-publication-and-reversible-binding-qualification-e877554a8a254783aca406ff5211899e.md)
- Parent logs: `TEMP/nara-recovery-data-proof-tests.log`,
  `TEMP/nara-recovery-inventory-publication-tests.log`, `TEMP/nara-public-recovery-owner-tests.log`,
  `TEMP/nara-public-recovery-cli-check.log`, `TEMP/nara-public-recovery-root-check.log`.

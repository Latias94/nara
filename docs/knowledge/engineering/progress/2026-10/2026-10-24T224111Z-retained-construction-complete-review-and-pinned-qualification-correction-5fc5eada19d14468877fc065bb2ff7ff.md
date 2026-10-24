---
type: "Work Progress"
title: "Retained construction complete review and pinned qualification correction"
description: "Actual eight-lens CE receipt completed with one direct Session test gap; simplification applied and explicit Rust 1.95.0 qualification supersedes unsupported historical toolchain labels."
timestamp: 2026-10-24T22:41:11Z
record_id: "5fc5eada19d14468877fc065bb2ff7ff"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "7868d48072a893b5a9f176d93d4597d0a5ecb56a"
---

# Summary

Construction is still uncommitted on `7868d48`; the full foundation Goal remains active. Ordinary
creation's previous qualification is not substituted for this unit. The worktree and pre-existing
untracked `vendor/` are preserved; no remote publication or compatibility layer.

# Details

## Simplification

Five read-only passes completed and their handles are closed. Applied one deduplicated reuse/
quality finding: resolve original/staged records only for a vacant property candidate and reuse
its retained Schema/value on subsequent writes. Applied the efficiency finding that existing
sibling placement should borrow only its owner, not clone an entire sibling list: the original
patch owner query now accepts borrowed roots/records and is shared privately with construction;
resolved owners are cached in the existing map. The callback and extra macro discriminator are
removed. These preserve lookup/error order and do not establish a measured speedup.

Before that owner-only refactor, new Scene/Prefab existing/local Before/After characterization
passed on explicit Rust 1.95.0: `a4b67117-53f1-450a-a045-aa944aad5958`. Existing proof tests were
not deleted, merged or weakened. One redundant private module comment was removed; the short
two-stage local-ID byte-accounting explanation remains because naming alone does not explain
why `(1, 0)` is later charged again. Diagnostic helpers moved before the private test module.

## Actual Complete Review

The portable full CE invocation now completed successfully, unlike the earlier unavailable
ordinary-creation review. Receipt: run `nara-r2-ce-review-8rwh6_hq`, `status: complete`, verdict
`Ready with fixes`, eight structured reviewers, one fresh validator, no failed/degraded stage,
and complete cost metadata. Artifacts remain outside the repo in the same-named temporary
directory (`review.json`, `metadata.json`, reviewer/merge/validator inputs and results).

All eight reviewers and the merge, validator and report leaves are closed. The adversarial lens
ran as a Codex subagent under the user's restriction; no cross-model transfer or independence
bonus was claimed. No P0/P1 or supported runtime defect was found. The one confirmed P2 is a
test gap: Application filters a closing commit before direct Session ingress. Pending-intent
Application preparation does already reach the Session begin/prepare guards; the validator
explicitly narrowed the finding instead of claiming those were also untested.

The added `closing_session_rejects_retained_construction_at_owning_ingress` test uses existing
generic fixtures and direct Session calls, with Scene/Prefab crossed with pending CloseDocument,
pending Exit and first physical close. It retains a valid candidate and exact document revision,
then requires closing refusal/no receipt and unchanged source/history/selection/saved state and
bytes. It passes on explicit Rust 1.95.0: `9459907c-3a5e-454c-a81e-ae5ab594fedc`. The first attempt
failed compilation because the test lacked its `EditorCloseDecision` import; no production fix
was required. A bounded independent post-fix review is still live at this checkpoint.

## Toolchain Evidence Correction

The previous checkpoint's unqualified `cargo` invocations did not pin or record their compiler.
Current inspection observes default stable `rustc 1.99.0 (b940084d7 2026-09-28)`. Therefore the
earlier immutable record's blanket Rust 1.95.0 label is not supported and must not be used as
MSRV proof. Preserve that record as history with this explicit correction; the current benchmark
also states the caveat. Do not infer the exact historical compiler merely from today's default.

A supplemental unpinned/default batch passed workspace/all-target check, correctness Clippy,
portable wasm compilation and three native Windows trajectories (`688014fa-4e82-44f0-aca8-02bdb172fefe`).
That is not the final pinned qualification. The explicit `+1.95.0` rerun records
`rustc 1.95.0 (59807616e 2026-04-14)` without modifying the user's global toolchain. Its first
non-native batch passed Scene/Tooling 393, root/Editor 581, four independent public workflows,
one feature-off UI guard and three no-default creation tests. Final static/native/cost follow-up
is in flight after the test-only review correction.

# Next Action

First poll unified exec session `8877`, the current serial Rust 1.95.0 verification chain:
direct Session proof -> workspace all-target check -> correctness Clippy -> portable wasm check
-> three selected native trajectories -> retained/control cost comparison. Logs are outside
source as `%TEMP%/nara-r2-construction-direct-session.log` and `nara-r2-construction-pinned-*.log`.
Do not start another Cargo until this handle is terminal. Sessions 83561, 93937, 65392 and 95459
are terminal and must not be restarted. The compile failure in 95459 stopped before static gates.

Collect post-fix read-only agent `01a17012-73e3-7660-9051-3c73ac699639`, then close it. No other
agent remains open. Record final IDs, corrected pinned benchmark and source manifest; update
the mutable design/ledger/guide with only their qualified scope, render/validate memory, inspect
the complete diff and make the authorized precise local Conventional Commit. Then proceed to
the missing selection-independent component/root-value read -> computed edit workflow. Complete
R2, registered-panel hooks and the later foundation waves remain open.

# Citations

- [Prior non-native checkpoint](../../verification/2026-10/2026-10-24T215522Z-retained-construction-owner-public-and-lifecycle-regression-checkpoint-82b171c17ee24213934e375e976c3e0c.md)
- [Construction design](../../../../architecture/authoring-construction-design.md)
- [Cost report and toolchain caveat](../../../../benchmarks/2026-10-25-retained-construction.md)
- [Direct Session proof](../../../../../src/project_host/runtime/tests/closing_session.rs)
- [Next query seam](../../subagents/2026-10/2026-10-24T215152Z-r2-selection-independent-field-reading-seam-389b97a2cf8f4275abbae7eeaa84bc6d.md)

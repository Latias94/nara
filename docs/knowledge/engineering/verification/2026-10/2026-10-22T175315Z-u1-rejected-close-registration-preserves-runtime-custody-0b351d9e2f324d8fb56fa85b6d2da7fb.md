---
type: "Verification Evidence"
title: "U1 rejected close registration preserves runtime custody"
description: "Fresh failing custody regressions then 132 App tests pass; exact App ownership fix and ADR clarification."
timestamp: 2026-10-22T17:53:15Z
record_id: "0b351d9e2f324d8fb56fa85b6d2da7fb"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "90f67a2"
---

# Verification

Windows 11, default installed Rust 1.99 toolchain, source checkout based on `90f67a2` plus U1.
All Cargo ran serially with `CARGO_BUILD_JOBS=1`, reusing root target after the user's cleanup.
The affected methods and exact retained-ledger behavior were reviewed against existing close tests.

# Result

U1 repairs App's ownership transfer for wrong-phase and obligation-admission rejection without
changing activation errors or standalone ledger rejection. Rejected participants join the same
runtime close/retry drive; registration executes no cleanup callbacks. ADR 0084 and rustdoc
state that transfer explicitly. The existing duplicate participant-ID test remains passing.

# Evidence

- Red: `cargo nextest run --locked -p nara_app --test runtime_close -E
  'test(rejected_close_registration)' --test-threads=1 --no-fail-fast`, run
  `5eedf6e9-8e88-48a2-98b9-0a9dbc7d012e`: both new tests failed on original implementation.
  Build rejection observed one premature Drop; outside-Build owner was absent from close.
- Green: `cargo nextest run --locked -p nara_app --test-threads=1 --no-fail-fast`, run
  `0f410487-ea2f-4409-802f-62ee8334d490`: 132/132 passed across eight binaries. The new public
  tests cover undeclared/duplicate obligations, wrong phase, no early callback/Drop, incomplete
  retirement and retry. Duplicate obligation retains two actual protocol owners.
- Focused independent code review found no blocking implementation finding. Its two coverage
  observations were applied: assert zero Drops while incomplete and unchanged Begin count after
  retry. The final public runtime-close cohort passes 7/7, run
  `c67a1686-5508-4040-a347-b973d1c3fb98`; production code is unchanged from the 132-test run.
- `cargo fmt --all` completed; actual diff contains only intended App formatting/code changes.
  Final `cargo fmt --all -- --check` passes.
- Strict App all-target Clippy stopped on preexisting unedited `result_large_err`, `collapsible_if`
  and `type_complexity` sites. The same command with only those three command-line lint exemptions
  passes. No repository lint allowance or unrelated cleanup was added.
- Documentation checks cover the plan, origin correction and memory links. No native Editor or
  whole-engine/platform qualification is supplied by this App-only slice.

# Follow-up

U2-U11 remain in the active Goal. Retain partial ADR 0084/0117 coverage for other known gaps;
closing L01 does not claim spatial/asset correctness or thread-affine service support.

# Citations

- [U1 plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md#u1-retain-every-transferred-runtime-close-owner)
- [App registration](../../../../../crates/nara_app/src/lib.rs)
- [Public close regressions](../../../../../crates/nara_app/tests/runtime_close.rs)
- [ADR 0084](../../../../architecture/adr/0084-executable-runtime-ownership-and-isolation.md)

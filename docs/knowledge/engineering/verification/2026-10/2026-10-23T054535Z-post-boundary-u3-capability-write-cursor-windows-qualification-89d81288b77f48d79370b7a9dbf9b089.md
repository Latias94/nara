---
type: "Verification Evidence"
title: "Post-boundary U3 capability write cursor Windows qualification"
description: "Logical capability write position, exact-handle and lock controls qualified on Windows; other platforms unclaimed."
timestamp: 2026-10-23T05:45:35Z
record_id: "89d81288b77f48d79370b7a9dbf9b089"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "7cc3b6f"
---

# Verification

U3/R3 of the post-boundary plan, on top of `7cc3b6f`. The implementation in the same commit owns
the exact source delta; the baseline hash is not a claim that the fix existed before that delta.

The worker added mixed-reader/writer regressions before its implementation but ran no checks.
No fresh red execution is claimed for this unit. The earlier PB3 audit retains its original scope.

## Result

- `cargo +1.95.0 nextest run -p nara_app -p nara_fs --locked --test-threads=2` passed:
  211 tests across 11 binaries, with 3 pre-existing privileged Windows filesystem cases ignored.
- Native Windows exact-handle tests passed: continuing writes after partial reads/seeks/reborrows,
  multiple readers, lock/release, rename plus name replacement, and external growth.
- Offset helper tests cover short writes, no progress/error and signed-range precheck. Their injected
  results do not qualify real OS failure behavior.
- Independent read-only Codex review of U3/U4 found no blocking implementation defect. It explicitly
  distinguished native tests from helper simulation and made no Linux/macOS claim.
- `cargo +1.95.0 clippy -p nara_fs --all-targets --locked -- -D clippy::correctness
  -D clippy::suspicious` passed with three existing advisory warnings outside the cursor repair.
  Package formatting and scoped `git diff --check` passed before commit.

## Limits

Linux/macOS execution, performance and native I/O fault injection were not run. The change does not
strengthen content snapshot, external-writer exclusion, lock or durability guarantees. Workspace
integration, persistent consumers and formatting are recorded separately when executed.

## Sources

- [Plan](../../../../plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [Public contract tests](../../../../../crates/nara_fs/tests/filesystem_contract.rs)

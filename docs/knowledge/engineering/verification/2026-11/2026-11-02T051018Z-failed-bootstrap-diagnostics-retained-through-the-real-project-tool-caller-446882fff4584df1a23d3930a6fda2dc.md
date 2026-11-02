---
type: "Verification Evidence"
title: "Failed bootstrap diagnostics retained through the real project-tool caller"
description: "Bootstrap preserves bounded compiler and protocol failures and refuses executable launch; owner tests and a real compile_error consumer qualify the correction."
timestamp: 2026-11-02T05:10:18Z
record_id: "446882fff4584df1a23d3930a6fda2dc"
producer_id: "codex-foundation-main"
run_id: "follow-up-bootstrap-20261102"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/foundation"
git_commit: "349921f"
supersedes: "84710703b83743ad9ebc067159d8b714"
---

# Verification

Commit349921f corrects F4 from the [follow-up review](../../../../audits/2026-11-02-refactor-follow-up-review.md).
The bootstrap now consumes Cargo JSON before classifying process success and retains the
decoder's bounded diagnostics even when push or finish rejects. It returns the existing
CargoBuildFailure::Compilation with the actual status and protocol error. Caller-facing Display
includes the actionable entries and loss counts. The exact executable selection and child cleanup
owners remain unchanged. This is scoped qualification, not complete foundation or compiled-Scene
qualification. Other WIP remained present throughout the checks.

# Result

| Check | Actual result |
| --- | --- |
| Root/Editor all-feature library and test-target typecheck | Passed,67.07s; includes the new bootstrap error mapping and owner tests. |
| Bootstrap owner cases in the full Root library run | All five new build_tests and existing completion/custody controls passed; run95d47a99-f7b6-4986-82bf-6e58c44364c8. The complete run was1461/1468; seven unrelated compiled-Scene fixture failures were not counted as passing. |
| Real failing-project bootstrap consumer |1/1 passed, run d7f55fee-f77a-4139-8743-c64b75baa85b; native Windows/MSVC Rust1.95. |
| Independent read-only review | No assigned P1/P2 in bootstrap/decoder/caller scope; no independent test execution claimed. |
| Commit preparation | Precise two-file staged diff and whitespace check passed; local main fast-forwarded to349921f. Nothing pushed. |

# Evidence

The real consumer creates a normal dependency-free headless package with a distinctive
compile_error, prepares its lockfile, then invokes the public ProjectToolRequest::Check path.
It requires the diagnostic in both the structured error and caller-readable Display, preserves
the unsuccessful status and Cargo BuildFailed protocol result, and checks that the launch marker
was never created. Manifest and lockfile remain byte-identical. Its build reuses the repository
target and jobs1; it does not imitate Cargo output for this production check.

The five focused owner cases independently cover a failed compilation, a rejected JSON push,
finish rejection after a successful process, refusal of an exact artifact from an unsuccessful
process, and bounded diagnostic truncation/omission. Merely moving the process-status check after
finish would still lose diagnostics on those decoder failures, so the error retains the decoder's
observations regardless of where selection terminates.

The real command was:

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run --manifest-path tests/fixtures/cargo-build-consumer/Cargo.toml --locked --test bootstrap --run-ignored only -E 'test(bootstrap_compile_failure_reaches_the_caller_without_launching_the_tool)' --test-threads=1
```

The consumer built in122.02s and executed in0.527s. TEMP logs remain local diagnostics and are not
committed. The new regression is explicitly ignored because it runs actual serial Cargo; ordinary
workspace nextest alone does not establish this caller result.

# Follow-up

The complete foundation Goal stays active. Closed compiled storage, contextual source conversion,
the same-window render correction, cancelled-upgrade retry and their aggregate final qualification
remain separate work. This record does not promote complete CP2 extraction, editable Nara UI,
motion/execution or the broader render proposal. Preserve all existing WIP and vendor.

# Citations

- [Bootstrap owner](../../../../../src/project_tooling/bootstrap.rs)
- [Real caller regression](../../../../../tests/fixtures/cargo-build-consumer/tests/bootstrap.rs)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Integrated predecessor state](../../progress/2026-11/2026-11-02T044348Z-complete-compile-interface-and-follow-up-owner-transition-corrections-in-integration-84710703b83743ad9ebc067159d8b714.md)

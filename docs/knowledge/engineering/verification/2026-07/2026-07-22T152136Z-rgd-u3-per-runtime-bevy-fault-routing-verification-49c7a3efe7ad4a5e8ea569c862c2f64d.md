---
type: "Verification Evidence"
title: "RGD-U3 per-runtime Bevy fault routing verification"
description: "Verifies bounded per-runtime Bevy failure attribution, reservation-before-transfer, truthful route retirement, and overlapping runtime execution."
timestamp: 2026-07-22T15:21:36Z
record_id: "49c7a3efe7ad4a5e8ea569c862c2f64d"
tags: ["rgd-u3", "runtime", "fault-routing", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6c6813848ea6335ef0a3eb40c16a9e6bbfa9ce39"
verified_by: "Focused nextest, strict Clippy with documented allowances, workspace/reference-game checks, source boundary audit, and independent multi-lens review"
---

# Verification

RGD-U3 was verified against implementation commit
`6c6813848ea6335ef0a3eb40c16a9e6bbfa9ce39`. Managed runtimes now reserve a
bounded, private Bevy error-handler route before transferring App or retirement ownership. The
route remains bound to that runtime through admission, startup, publication, close-incomplete, and
process quarantine, and cannot be reused until executor and handler scopes are quiescent.

# Result

- `RUNTIME_SCHEDULE_AUTHORITY`, `ACTIVE_RUNTIME_REPORTER`, and the public
  `RuntimeFaultKind::ScheduleAuthority` contract were removed. Independent runtime schedules can
  overlap without sharing reporter attribution.
- `RuntimeAdmissionReservation::try_acquire` is the public pre-transfer capacity boundary.
  Saturation does not take the caller's App or obligation ledger; every failure after admission
  begins owns its route and explicit retirement path.
- System, run-condition, default command, and observer errors use runtime-specific static
  trampolines. Explicit command error handlers remain caller-owned and do not create sticky runtime
  faults.
- Route reuse is protected by slot epochs, an outer executor in-flight guard, handler in-flight
  accounting, and `Active -> Retiring -> Quiescent` transitions. Close-incomplete and quarantined
  owners continue to consume capacity truthfully.
- App observers are materialized only when raw execution or managed admission establishes the final
  error authority. Once raw execution begins, later managed admission fails closed.
- Independent review found six actionable issues: pre-bound observer bypass, admission-unwind route
  release, missing failure-retention assertions, incomplete overlap coverage, undocumented deferred
  observer semantics, and leakage of the private pool size. All six were independently validated,
  fixed, and re-verified; no P0/P1 remains.

# Evidence

- `cargo nextest run --locked -p nara_app --test-threads=1`: 71 passed, run
  `cbc273fc-9c96-48dd-a0fe-affb79e169f8`.
- `cargo nextest run --locked -p nara --test runtime_instance --test
  runtime_driver_boundary --test-threads=1`: 75 passed, run
  `10d003b6-e40b-4998-9be4-1574fbbca0ac`. The suite includes eight repeated rounds of true overlap
  among System, RunCondition, Command, Observer, and healthy runtimes.
- `cargo nextest run --locked -p nara --lib --features runtime-2d,serde
  route_capacity_rejection_keeps_the_same_project_start_attempt_retryable --test-threads=1`: one
  focused Project Host retry test passed, run `05a1874e-58d3-4a25-847d-0ecc7daecc53`.
- `cargo clippy -p nara_app --locked --all-targets` and `cargo clippy -p nara --locked
  --all-targets --all-features` passed with every warning denied except the documented existing or
  ownership-contract lints: `result_large_err`, `collapsible_if`, `needless_return`,
  `double_must_use`, `too_many_arguments`, `derivable_impls`, and `drop_non_drop`.
- `cargo check --workspace --locked`: passed for all workspace packages.
- `cargo check --manifest-path reference-game/Cargo.toml --locked --all-targets`: passed.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test-threads=1`: 50
  passed, run `b4a610f6-a218-4663-b90a-8ac506ea3ed2`.
- Source searches found the deleted global authority names only in the negative boundary test and
  found no removed `RuntimeCandidate::admit`/`admit_with` call. `admit_reserved` remains private.
- `git diff --cached --check` passed before the implementation commit; the staged scope contained
  exactly 26 U3 implementation, migration-callsite, example, and regression-test files.
- `architecture_docs` was intentionally not run: the active plan assigns that governance gate to
  U1 and U7, not U3.

# Follow-up

1. Activate RGD-U4 and prove fresh mutable runtime-session reconstruction across sequential and
   overlapping generations while preserving the immutable U2 registry authority.
2. Add test-owned service-session identity and concrete Wgpu instance/device/cache namespace
   evidence without introducing a generic Runtime or backend provider interface.
3. Preserve the U3 route through every U4 replacement and close-incomplete path; replacement must
   wait for truthful retirement.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u3-route-bevy-fallible-execution-per-runtime`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/migrations/2026-07-engine-foundation.md`
- `AGENTS.md`
- Commit `6c6813848ea6335ef0a3eb40c16a9e6bbfa9ce39`

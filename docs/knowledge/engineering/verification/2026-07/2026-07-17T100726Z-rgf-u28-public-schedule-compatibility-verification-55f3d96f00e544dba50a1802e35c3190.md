---
type: "Verification Evidence"
title: "RGF-U28 public schedule compatibility verification"
description: "Commit c24b38a closes the four public schedule anchors and seal-time deferred compatibility contract."
timestamp: 2026-07-17T10:07:26Z
record_id: "55f3d96f00e544dba50a1802e35c3190"
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "c24b38a"
verified_by: "focused, workspace nextest, workspace check, rustdoc, fmt, and static boundary audits"
---

# Verification

RGF-U28 was verified against commit `c24b38a` on
`refactor/engine-foundation-contracts`. The reviewed scope is the exact public inventory
`CoreStage::FixedUpdate`, `FixedUpdateSet::Simulate`, `GameplayCommandSet::Consume`, and
`GameplayCommandSet::Capture`, plus App seal-time compatibility enforcement and facade-safe ECS
derives for external extensions.

# Result

Passed. The implementation publishes no additional first-playable schedule anchor, keeps
`nara_app` independent from `nara_gameplay` and `bevy_app`, and leaves custom schedules available
without exposing raw mutable access to engine-owned schedules.

# Evidence

- `cargo nextest run --locked -p nara_app --test schedule_compatibility --test-threads=1`: 4 passed.
- Focused `nara_app` library and schedule tests: 40 passed.
- `cargo nextest run --locked -p nara_gameplay --lib --test-threads=1`: 29 passed.
- `cargo nextest run --locked -p nara --test schedule_extension_contract --test-threads=1`: 7 passed.
- Reference-game public-surface verification: 1 passed.
- Derive dependency fixtures: 3 passed; public-prelude focused verification: 1 passed; the
  independent renamed-`nara_ecs` fixture also passed its locked check.
- `cargo nextest run --workspace --locked --test-threads=1` with one build job: 801 passed, 3
  skipped, no failures.
- `cargo check --workspace --locked`, `cargo doc --locked -p nara_app --no-deps`,
  `cargo fmt --all -- --check`, and `git diff --check`: passed.
- Static dependency and surface audits found no `nara_app -> nara_gameplay`, `bevy_app`, scheduler
  wrapper, string schedule registry, global stage DSL, or internal-crate fixture dependency.
- Adversarial review findings were resolved for independent fixture lockfiles, alias/macro/module/
  include/path/raw-identifier/aggregate-value conformance bypasses, and raw built-in `Schedule`
  replacement before the final gates.

# Follow-up

RGF-U28 closure does not close the active plan. Select the next admitted critical-path unit from
RGF-U12 and RGF-U29 before proceeding toward RGF-U26, RGF-U24, RGF-U25, and RGF-U6.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u28-prove-public-semantic-scheduleset-anchor-compatibility`
- `docs/architecture/adr/0003-own-app-plugin-and-schedule-lifecycle.md`
- `docs/migrations/2026-07-engine-foundation.md#rgf-u28-1-public-semantic-schedule-anchors-and-seal-validation`
- `crates/nara_app/tests/schedule_compatibility.rs`
- `tests/schedule_extension_contract.rs`
- `tests/fixtures/schedule-extension/renamed-root/`
- Commit `c24b38a`

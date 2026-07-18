---
type: "Verification Evidence"
title: "RGF-U26 manual ownership counterfactual verification"
description: "Commit a2d695d freezes the minimal pre-Host success and three-failure ownership baseline without a policy engine."
timestamp: 2026-07-18T17:44:14Z
record_id: "9730fc03912d4eaa874e5384771b0fe8"
tags: ["rgf-u26", "verification", "reference-game", "ownership"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "a2d695d"
verified_by: "focused nextest,reference-game nextest,all-target check,architecture docs,clippy,fmt,independent review"
---

# Verification

RGF-U26 was verified against commit `a2d695d` on
`refactor/engine-foundation-contracts`. The reviewed subject is the committed manual raw-App
reference-game path from authorized project content through one guarded scene materialization and
fixed tick, plus the three failure cuts required by R36 and AE21.

# Result

Passed. U26 now preserves a small, executable pre-Host counterfactual without introducing a local
policy engine, dependency closure, evidence envelope, candidate/runtime owner, or Host abstraction.

- The success cut loads the committed manifest, scene, prefab, image, plugin plan, and command
  fixture; materializes through U29's real guarded apply; captures the frozen first-tick result; and
  joins every configured task worker before App drop.
- The pre-owner cut loads a dedicated authorized directory with no `nara.toml`, returns
  `ProjectContent(OpenManifest)`, creates no App, and requires no retirement owner.
- The late-hook cut rejects with `scene.persistent-apply-ineligible` before entity allocation,
  invokes no hook, publishes no scene/runtime, and retires App-owned work completely.
- The stalled-task cut first reaches the same authoritative tick, then observes bounded task-close
  timeout while `App::World` retains custody. It releases the task, retries through the same close
  owner, proves every worker joined, shuts down plugins, and only then drops App.
- The compact ownership record explicitly leaves process-reaper custody to lower-level
  `nara_tasks` coverage instead of claiming that a task-closure exit signal proves worker join.

This unit changes test behavior and evidence only. It adds no public or persistent production
contract, so no ADR implementation-ledger or foundation boundary changed.

# Evidence

- Proof-first red: the focused test failed to compile because
  `run_manual_raw_app_pre_owner_failure`, `run_manual_raw_app_incomplete_retirement`, and
  `ManualRetirementCustody` did not exist. The final focused gate passed 4/4.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test manual_raw_app_baseline --test-threads=1`:
  4 passed, no failures.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test-threads=1`: 26
  passed across 12 binaries, no failures.
- `cargo check --manifest-path reference-game/Cargo.toml --locked --all-targets`: passed.
- `cargo nextest run --locked -p nara --test architecture_docs --test-threads=1`: 5 passed, no
  failures, including repository-relative Markdown links and ledger invariants.
- Focused strict Clippy for `manual_raw_app_baseline` passed. The command allowed only the existing
  reference-game `result_large_err` / `double_must_use` findings and the pre-existing shared-fixture
  `let_and_return` finding; no U26 warning remained.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Independent ownership review found one P1 in the first attempt: a task-closure exit signal did not
  prove process-reaper custody or worker join. The implementation and document were changed to
  retain `App::World` custody and prove retry completion from joined-worker counts. Follow-up review
  found no remaining P0/P1; both residual P2s were also closed.
- The existing lower-level
  `dropping_an_unfinished_close_owner_retains_worker_handles_until_exit` task test passed 1/1 and
  remains the owner of process-reaper fallback evidence. U26 does not generalize that result.

# Follow-up

RGF-U26 closes only the pre-Host comparison point. RGF-U24 must now run its concrete ordinary
product action over the same success task and three failure classes, preserve authoritative output,
diagnostics, visibility, and custody, and keep candidate/admission/publication/retirement concepts
outside the five-category ordinary caller surface.

The two unrelated Cargo lockfile edits remained unstaged and are not part of commit `a2d695d`.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u26-freeze-the-manual-raw-app-ownership-counterfactual`
- `docs/benchmarks/runtime-ownership-baseline.md`
- `reference-game/tests/manual_raw_app_baseline.rs`
- `reference-game/tests/support/manual_raw_app_boot.rs`
- `crates/nara_tasks/src/tests/close.rs`
- Commit `a2d695d`

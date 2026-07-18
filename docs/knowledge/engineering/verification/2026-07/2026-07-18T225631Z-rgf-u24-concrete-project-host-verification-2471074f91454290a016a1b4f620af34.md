---
type: "Verification Evidence"
title: "RGF-U24 concrete project Host verification"
description: "Commit 5ddbf18 closes the bounded headless product action, atomic runtime publication, and U26 ownership reversal proof."
timestamp: 2026-07-18T22:56:31Z
record_id: "2471074f91454290a016a1b4f620af34"
tags: ["rgf-u24", "verification", "project-host", "runtime", "reference-game"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5ddbf18"
verified_by: "focused nextest,reference-game nextest,workspace nextest,workspace check,all-target check,clippy,fmt,independent review"
---

# Verification

RGF-U24 was verified against commit `5ddbf18` on
`refactor/engine-foundation-contracts`. The reviewed scope is the concrete root-owned headless
product action from an authorized project root through manifest ingest, plan/content binding,
unpublished runtime construction, guarded scene materialization, startup, atomic publication, one
or more exact fixed ticks, outcome capture, and bounded retirement.

# Result

Passed. A normal headless caller now uses `HeadlessRunIntent`, `HeadlessRun`, semantic command
submissions, `HeadlessRunOutcome`, and structured diagnostics without owning App, candidate,
publication, or retirement choreography.

- A private `ProjectHost` binds only matching `RuntimePlan` and `ProjectContentSnapshot` lineage and
  schema fingerprints, creates one start epoch and obligation ledger, and keeps every acquired owner
  under that ledger before the next fallible action.
- Plugin preparation and closed App commit share the same `PluginPlan` implementation as the
  code-first path. Partial task-pool construction returns a pollable owner and enters the runtime
  ledger instead of synchronously waiting or escaping into process fallback.
- Registry verification, U29 guarded persistent apply, scene materialization, semantic command
  submission, and startup complete while the runtime remains unpublished.
- `ReadyRuntimeCandidate::publish_into` performs the final fault check, owner transfer, slot write,
  and publication-state transition under the reporter lock. Rejected publication returns a typed,
  retryable retirement owner; a slot cannot be replaced after publication or consumption.
- Close-time faults, delayed plugin shutdown failures, startup unwind, and incomplete task
  retirement remain visible. `RetirementIncomplete` retains Host custody and a later bounded drive
  retries only unfinished work.
- The reference-game product path and committed U26 manual path use the same resolved plugin plan,
  authorized fixture, semantic command, exact tick, and authoritative outcome. Their pre-owner,
  late persistent-hook, and incomplete-retirement cuts preserve the required diagnostic,
  visibility, and custody distinctions.
- Public boundary tests keep ordinary callers inside the five allowed concept categories. The
  lower-level `RuntimeCandidate`, publication slot, start attempt, and Host state machine remain
  advanced or private implementation surfaces.

This is implementation evidence for the RGF-U24 slice. ADR 0082 and ADR 0084 remain Proposed until
RGF-U23 independently evaluates them and their compatibility. U24 does not freeze a universal
Host/factory trait, process scope graph, Editor ownership model, desktop driver shape, or final
public spelling for every product action.

# Evidence

- U24 package gate: 271 passed across `nara_app`, `nara_project`, `nara_asset`, `nara_scene`,
  `nara_gameplay`, and `nara_tasks`.
- U24 root gate with `runtime-2d,serde`: 106 passed across the root library,
  `project_composition`, `runtime_instance`, `project_runtime_boot`, and
  `project_host_boundary`.
- U24 reference-game gate: 11 passed across `manual_raw_app_baseline`,
  `project_manifest_ingest`, and `runtime_drive`.
- `cargo nextest run --workspace --locked --test-threads=1 --no-fail-fast`: 859 passed, with 3
  declared conditional skips and no failures, against commit `5ddbf18`.
- `cargo check --workspace --locked` and
  `cargo check --manifest-path reference-game/Cargo.toml --locked --all-targets`: passed.
- Focused strict Clippy passed for root U24 code after allowing only documented pre-existing
  `result_large_err`, `collapsible_if`, `double_must_use`, `too_many_arguments`,
  `derivable_impls`, and unrelated image-test `needless_return` findings. U24-owned large-enum and
  duplicate-`must_use` findings were fixed with boxed state ownership and attribute cleanup.
- `cargo fmt --all -- --check`, `git diff --check`, and staged-diff checks passed.
- Independent correctness and product review found no remaining code P0/P1. Earlier P1 findings for
  partial task-owner escape, synchronous spawn-failure wait, close-time fault loss, plan mismatch,
  weak cleanup evidence, flattened diagnostics, and unstructured CLI authority failure were fixed
  and reverified. The final review's sole P1 was stale architecture documentation; this closure
  updates the foundation, design harness, migration guide, and implementation ledger while keeping
  ADR 0082/0084 Proposed.

# Residual Risk

- The current headless CLI retries incomplete cleanup with `yield_now`; a longer-lived product loop
  should use a paced wake strategy before this becomes a sustained production workload.
- Reporter-lock linearization has deterministic race tests, but no test forces the exact two-thread
  publication/fault lock interleaving. Add one if this internal publication seam gains another
  producer.
- The private Host state machine still contains a substantial diagnostic mapping section, and the
  ordinary-caller boundary combines lexical checks with an external compile fixture. Split or
  strengthen these only when U6/U17 growth creates concrete pressure; neither is a U24 correctness
  blocker.

# Follow-up

RGF-U6 is now admitted. It must extend this same public headless action into the complete
authoritative wave, stable snapshot, terminal outcome, and CLI contract. It may repair the engine
only from a failing public regression and must not reopen candidate/publication choreography in
the reference game.

The two unrelated Cargo lockfile edits and the untracked ownership-data directory remained
unstaged and are not part of commit `5ddbf18`.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u24-compose-the-concrete-root-host-start-transaction`
- `src/project_host/runtime.rs`
- `src/project_host/runtime/action.rs`
- `crates/nara_app/src/runtime.rs`
- `crates/nara_tasks/src/runtime.rs`
- `tests/project_runtime_boot.rs`
- `tests/project_host_boundary.rs`
- `tests/runtime_instance.rs`
- `reference-game/tests/runtime_drive.rs`
- `reference-game/tests/manual_raw_app_baseline.rs`
- Commit `5ddbf18`

---
type: "Verification Evidence"
title: "RGF-U23 runtime and Host decision verification"
description: "Verifies the independent ADR 0084 and ADR 0082 verdicts, bounded Trial compatibility, U20 block, trust scope, governance checks, and exact staged-index result."
timestamp: 2026-07-21T13:11:16Z
record_id: "0451acd3af664983a3fc2ebecbcb6f24"
tags: ["rgf-u23", "runtime", "host", "compatibility", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "7f45328023118a2f95529fc09944f30a849b08f9"
verified_by: "Independent Runtime, Host, compatibility, and post-write specification reviews"
---

# Verification

RGF-U23 was decided against baseline `f7e5ee283e06ff156224b0f11fcc1df0c31284a3`
and recorded by commit `7f45328023118a2f95529fc09944f30a849b08f9`. Independent Runtime and
Host reviews were completed before a separate compatibility review; the pair result did not
overwrite either independent verdict.

# Result

- ADR 0084 remains Proposed. Ten success metrics pass; driver parity, external driver authority,
  and complete runtime isolation remain insufficient. Process-global drive contention and the
  plan/World behavior-registry split are explicit implementation blockers.
- ADR 0082 remains Proposed. Its current concrete Host metrics pass in the admitted authority scope,
  but its required compatible Accepted executable-runtime dependency is absent.
- The pair is conceptually compatible and may continue only as a bounded Trial for already-compiled,
  Host-trusted code-first and RGF paths. It is not current architecture authority and does not
  authorize native code from project data.
- U20 remains blocked until both required product authorities are Accepted or replaced by compatible
  Accepted successors and the combined evidence is rerun.
- No universal Host, service registry, factory, or Runner SPI was admitted, and no existing success
  metric was reduced to fit the implementation.

# Evidence

- Three read-only reviewers independently covered ADR 0084, ADR 0082, and pair compatibility at the
  baseline revision. Their results agree on `Remain Proposed`, bounded-Trial compatibility, and the
  U20 block while retaining separate reasons.
- Post-write specification review found one P1: token-only governance checks could miss deletion or
  drift of exact evidence and metric rows. The final test parses four tables, rejects duplicate
  keys, and compares the complete 11-revision, 13-Runtime-metric, 10-Host-metric, and five-scenario
  maps. The reviewer confirmed the finding closed with no remaining issue.
- `cargo nextest run --locked -p nara --test architecture_docs --test-threads=1
  --no-fail-fast`: 9 passed in the active worktree.
- A detached `HEAD + exact staged index` worktree ran the same architecture gate: 9 passed. The
  temporary worktree was path-validated and removed by Git after completion.
- `cargo nextest run --locked -p nara --features tooling,runtime-2d,serde --test runtime_instance
  --test runtime_driver_boundary --test project_runtime_boot --test project_host_boundary --test
  editor_persistence --test workspace_play_runtime --test-threads=1 --no-fail-fast`: 97 passed.
- The first combined dependency run completed 96 of 97 and aborted
  `dirty_close_discard_stops_play_before_removal_without_writing_disk` after its bounded helper did
  not complete under concurrent review load. The exact test then passed twice, a 20-run stress loop
  passed every attempt, the complete `editor_persistence` target passed 10 of 10, and the final
  single-thread dependency matrix passed 97 of 97. No reproducible product fault was inferred from
  the isolated failure.
- `cargo fmt --all -- --check`, `git diff --cached --check`, and engineering-memory validation
  passed. Memory validation retained only pre-existing legacy-record and stale-rollup warnings.

# Follow-up

1. Replace the plan/World duplicate registry behavior authority with one immutable exact authority
   or binding receipt, including same-catalog/different-codec rejection before scene mutation.
2. Remove destructive process-global drive contention by making fault routing execution-scoped or
   rejecting a proven singleton capability before publication.
3. Prove independent owner overlap behavior and complete service/backend/identity reconstruction.
4. Run one reference-game command stream through the real headless, desktop, and Editor Hosts.
5. Add the required renamed-dependency external Platform/Runner candidate, then repeat independent
   Runtime and pair review before reconsidering ADR 0082 or U20.

# Citations

- Decision matrix:
  `docs/knowledge/engineering/decisions/2026-07/2026-07-21T112729Z-rgf-u23-runtime-and-host-independent-decision-matrix-a5b3266847924dfc93667c72c8929550.md`
- `docs/architecture/adr/0082-process-host-authority-and-runtime-construction-topology.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u23-decide-runtime-and-host-proposals-independently-then-reconcile`
- Commit `7f45328023118a2f95529fc09944f30a849b08f9`

---
type: "Verification Evidence"
title: "RPR-U5 current product-readiness Redirect"
description: "Records bounded current-revision product observations, exact cross-platform candidates, missing required evidence, and the resulting Redirect without a reusable evidence framework."
timestamp: 2026-08-01T03:22:22Z
record_id: "c064db2329544a5da5c02a6eb8183db5"
tags: ["rpr-u5", "product-readiness", "redirect", "decision-local"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-01-001-refactor-product-readiness-delivery-reset-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "74b15a3194d961e62fcc3b8ec3e6a163fbe61596"
verified_by: "codex-root"
---

# Verification

The measured executable revision was
`74b15a3194d961e62fcc3b8ec3e6a163fbe61596` on branch
`refactor/engine-foundation-contracts`. The checkout was clean before and after the
decision-local observations. Local measurements used Windows 11 Pro `10.0.26200` on
`x86_64`, an Intel Core i9-13900KF, Rust/Cargo `1.97.1`, release binaries, one Cargo
build job, and explicit per-process time bounds. They do not constitute a reusable
collector or benchmark protocol.

The GitHub Actions candidate prerequisite completed successfully at the same executable
revision: [run 30680998417](https://github.com/Latias94/nara/actions/runs/30680998417).
It built and consumed the real `headless` and `desktop --candidate-smoke` binaries on
Windows and Linux from no-checkout consumer jobs. This proves bounded candidate transport
and product smoke only; it is not manual desktop-playability, frame-time, memory, GPU, or
public-coverage evidence.

The data-edit observation alternated the persisted `reference_game.Player` `hit-points`
value between `20` and `21` in `reference-game/scenes/startup.scene.json`. Each sample
started after the edited file was flushed, ended when the formal release `headless`
product exited, and checked that its terminal output reflected the edited value. The
temporary renamed-root consumer was copied from the independently locked fixture, had no
workspace inheritance, private crate, patch, or repository-private helper, and used only
the public recipe and `HeadlessRun` surface. Its module task added a runtime-only recipe
plugin; its configuration task replaced a typed recipe configuration and checked the
changed runtime outcome.

# Result

The result is **Redirect**. Seven current product measures have bounded passing
populations, but thirteen required first-playable measures and the additional R8
render-packet-cost measure have no honest current population. The sole hard-stop
`gameplay.headless_wave_success` passes, so there is no evidence for `Stop`.

| Metric | Current population | Outcome | Observation |
| --- | ---: | --- | --- |
| `build.cold_ns` | 0 | Missing | No fresh isolated cold-build population. |
| `build.incremental_ns` | 0 | Missing | No fresh warm-build population. |
| `frame.p99_ns` | 0 | Missing | Candidate smoke is not a public frame-time observation. |
| `gameplay.desktop_playable_success` | 0 | Missing | No bounded manual desktop author journey. |
| `gameplay.headless_wave_success` | 10 | Pass | Every formal data-edit product run completed successfully. |
| `iteration.body.p50_ns` | 0 | Missing | No current body-edit-to-result population. |
| `iteration.body.p95_ns` | 0 | Missing | No current body-edit-to-result population. |
| `iteration.data.p50_ns` | 10 | Pass | Nearest-rank P50: `121,738,200 ns` (target: at most `2,000,000,000 ns`). |
| `iteration.data.p95_ns` | 10 | Pass | Nearest-rank P95: `151,325,500 ns` (target: at most `5,000,000,000 ns`). |
| `iteration.structural.p50_ns` | 0 | Missing | No current structural-edit-to-result population. |
| `iteration.structural.p95_ns` | 0 | Missing | No current structural-edit-to-result population. |
| `journey.clean_to_desktop_playable_ns` | 0 | Missing | No clean, bounded, manual desktop journey. |
| `journey.clean_to_headless_wave_ns` | 0 | Missing | No fresh clean-to-headless population. |
| `module.add.success` | 3 | Pass | Three ordinary external-author tasks completed with result `1`. |
| `module.add.time_ns` | 3 | Pass | Nearest-rank P95: `16,258,844,500 ns` (target: at most `900,000,000,000 ns`). |
| `public.production.coverage_basis_points` | 0 | Missing | No executed public production-call denominator and population. |
| `runtime.gpu_resource_bytes` | 0 | Missing | No public normal-product backend-owned population. |
| `runtime.memory_bytes` | 0 | Missing | No bounded normal-product process-memory population. |
| `slot.configure.success` | 3 | Pass | Three typed recipe-configuration replacement tasks completed with result `1`. |
| `slot.configure.time_ns` | 3 | Pass | Nearest-rank P95: `13,990,289,700 ns` (target: at most `300,000,000,000 ns`). |

R8 also requires render-packet cost. The current product has no normal public observation
for that fact, so it remains **Missing** rather than being substituted with a source probe
or a private render hook.

# Evidence

Data-edit sample values, in nanoseconds, were:

```text
151325500, 128160700, 126352600, 108450700, 133493400,
124466000, 104021000, 121738200, 115893000, 109282000
```

The three module-add values were:

```text
16258844500, 11877737400, 15636251400
```

The three typed-configuration values were:

```text
13990289700, 11966908800, 9045256700
```

All data-edit product runs were bounded to five seconds. The external author-task runs
were bounded to 180 seconds. A command failure, timeout, or output mismatch was not
admitted as a passing sample. Historical U9 data was not carried forward to fill any
missing current population.

Two independent read-only reviews examined the decision without being asked to produce
`Publish`:

- The product review admitted only the seven measures above, required every remaining
  current measure to remain missing, and concluded `Redirect` rather than `Stop`.
- The correctness review found no current P0/P1 that changes the decision. It confirmed
  the exact cross-platform candidate identity and that candidate smoke cannot stand in for
  human desktop or telemetry evidence.

The historical `f098876` collector review found a real defect in the retired
`measure_first_playable.py`: an extremely large finite timeout could raise outside its
expected error path and still leave a collected-looking manifest. The collector and its
transport/ingest/release chain are absent from the active tree, so this is a limitation of
historical evidence rather than a current executable defect. This U5 decision neither
relies on nor revives that collector.

# Follow-up

No publication workflow, tag, Release, approval chain, or successor pre-release plan is
activated by this record. Any later attempt to reach `Publish` needs a new product plan
and fresh observations at its own exact executable revision. It must first supply real
author-facing desktop evidence and normal product owners for frame, process-memory,
GPU-resource, render-packet, and executed-public-coverage facts; it must not recreate the
retired JSONL/manifest transport framework merely to re-evaluate this Redirect.

# Citations

- `docs/plans/2026-08-01-001-refactor-product-readiness-delivery-reset-plan.md`
- `docs/benchmarks/data/protocol/v1/reference-game-first-playable.json`
- `tests/measurement_policy.rs`
- `reference-game/README.md`
- `reference-game/src/bin/desktop.rs`
- `tests/fixtures/runtime-runner/renamed-root/`
- `.github/workflows/reference-game-candidate.yml`
- `https://github.com/Latias94/nara/actions/runs/30680998417`

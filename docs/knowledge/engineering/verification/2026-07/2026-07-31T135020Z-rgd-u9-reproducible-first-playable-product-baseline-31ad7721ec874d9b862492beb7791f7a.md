---
type: "Verification Evidence"
title: "RGD-U9 reproducible first-playable product baseline"
description: "Closes U9 at f098876 with committed raw samples, a reproducible collector, and a Rust-policy Redirect verdict."
timestamp: 2026-07-31T13:50:20Z
record_id: "31ad7721ec874d9b862492beb7791f7a"
tags: ["rgd-u9", "measurement", "reference-game", "redirect", "completed"]
status: "completed"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f09887600d2161144b920b6e2618fc8151dad4fa"
verified_by: "codex-root"
supersedes: "406733392389447c879f6de86245151e"
---

# Verification

The committed one-shot collector executed from and byte-matched source
revision `f09887600d2161144b920b6e2618fc8151dad4fa`, after U8's exact-revision
hosted matrix completed successfully. It created a detached clean worktree,
isolated Cargo, home, and temporary roots outside the repository, used one
Cargo build job, restored every controlled edit, and left the source checkout
unchanged.

The successful collection ran from `2026-07-31T13:18:27Z` through
`2026-07-31T13:40:19Z`. The helper's independent `verify` command accepted the
committed raw transport at the same source revision. Diagnostic logs are
non-canonical and were not imported.

# Result

RGD-U9 closes with `Redirect`:

- 74 raw observations form complete populations for 10 of 20 required
  first-playable metrics.
- 9 complete metrics pass their frozen targets.
- `iteration.data.p95_ns` is a complete non-hard-stop failure: 8.323 seconds
  against a 5-second maximum.
- 10 required desktop, telemetry, and ordinary-author metrics remain missing.
- The only hard-stop metric, `gameplay.headless_wave_success`, equals `1` and
  passes.

The committed Rust decision policy maps missing required evidence and failed
non-hard-stop evidence to `Redirect`; it maps only a failed complete hard-stop
metric to `Stop`. Python did not aggregate the population or choose the
verdict.

# Evidence

- Source revision:
  `f09887600d2161144b920b6e2618fc8151dad4fa`.
- U8 prerequisite: GitHub Actions run `30629508555`, six of six jobs
  successful at that exact revision.
- Collector SHA-256:
  `51f1b0b7bb5738548540d0f391e0bc052317d3b1242d80bd9a0bb9f0f7a956b7`.
- Protocol JSON SHA-256:
  `e55fe4205584b801e3e3301e391b23e86b2f9ded1e04e4936c92436da1c988c2`.
- Raw JSONL SHA-256:
  `040a9779de7c4f4d915f101626a832618e9e9d98b679e57bd7ba651782de3f80`.
- Run-manifest SHA-256:
  `ca321c8b6ed037215d30e2bbaa615e037e770700f6e12e79dcd8cf5dc1f590f5`.
- Independent PowerShell recalculation used the protocol's one-based
  nearest-rank rule and reproduced every population, sample count, aggregate,
  target comparison, and the 9-pass/1-fail/10-missing classification recorded
  in the baseline.
- Focused `measurement_helpers` and `measurement_policy` nextest suites passed
  26 of 26 tests. They cover collector source binding, bounded execution and
  transport, environment and population admission, aggregation, and
  `Continue`/`Redirect`/`Stop` precedence.

An initial non-canonical invocation failed before producing a product sample
because a normal PowerShell `PATH` selected Scoop's GNU `link.exe`. Running
from the installed Visual Studio x64 development environment corrected the
host prerequisite without modifying the collector or measured source.

# Follow-up

The baseline exposes a real data-edit tail as well as missing desktop and
ordinary-author evidence. Those are product bottlenecks, not justification for
growing the collector into a benchmark, provenance, approval, or telemetry
framework. Freeze the U9 helper unless another product consumer proves a
smaller shared action is needed.

RGD-U10 is now the next delivery unit. RGD-U11 remains blocked until corrected
Windows/Linux candidates close U10 at an admitted revision.

# Citations

- `docs/benchmarks/reference-game-first-playable-baseline.md`
- `docs/benchmarks/data/runs/v1/rgd-u9-f098876/run-manifest.json`
- `docs/benchmarks/data/runs/v1/rgd-u9-f098876/raw-samples.jsonl`
- `reference-game/tools/measure_first_playable.py`
- `tests/measurement_helpers.rs`
- `tests/measurement_policy.rs`
- `tests/support/first_playable_evidence.rs`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u9-record-the-first-playable-product-baseline`
- Verification record `38f71939f1994eb39c2ac44d6632f008`

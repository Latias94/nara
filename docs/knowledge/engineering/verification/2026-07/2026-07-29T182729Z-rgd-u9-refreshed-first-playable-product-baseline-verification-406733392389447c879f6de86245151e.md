---
type: "Verification Evidence"
title: "RGD-U9 refreshed first-playable product baseline verification"
description: "Records the current isolated Windows first-playable population, deterministic Redirect verdict, missing product evidence, and non-claims."
timestamp: 2026-07-29T18:27:29Z
record_id: "406733392389447c879f6de86245151e"
tags: ["rgd-u9", "rgf-u14", "first-playable", "measurement", "windows", "redirect", "completed", "refresh"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "b2ddb5b0ea6ea9b98e213619dccf65a5326b1505"
verified_by: "codex-root"
---

# Verification

RGD-U9 re-executed the carried RGF-U14 first-playable protocol after the final current-revision
RGD-U8 hosted Windows/Linux prerequisite closed. The measured subject was clean detached revision
`b2ddb5b0ea6ea9b98e213619dccf65a5326b1505`; its changes from hosted executable revision
`ef8f300889086cfa1241c45c19bfc8d4edf8ffb3` are evidence and engineering-memory documents only.

The admitted automatic run used Rust/Cargo 1.97.1 on Windows 11. Cargo used one build job, offline
resolution, five distinct empty cold targets, one retained warm target, and an isolated Cargo home
containing a byte-and-file-count-verified copy of the prior isolated crates.io registry. Writable
home, profile, temporary, and AppData paths resolved under the external collection root. The active
checkout was never the measurement subject, and both checkouts were clean after collection.

# Result

The collector produced 75 zero-exit raw records under one environment fingerprint. Independent
recalculation verified contiguous per-metric indices, no failure references, the manifest-bound
raw digest, and the protocol's one-based nearest-rank aggregation. Evidence review rejected the
single public-coverage record because its static test did not implement the declared denominator,
executed-call ratio, or both-terminal-path boundary; the remaining 74 records are admitted.

All 10 admitted required metrics passed their frozen targets:

| Metric | Aggregate |
| --- | ---: |
| Cold build P95 | 204.677 s |
| Incremental build P95 | 17.024 s |
| Clean-to-headless-wave P95 | 205.422 s |
| Headless wave success | 1 |
| Body edit P50 / P95 | 7.829 s / 22.469 s |
| Data edit P50 / P95 | 0.069 s / 0.078 s |
| Structural edit P50 / P95 | 7.713 s / 9.257 s |

Ten required metrics have no admissible population: desktop playable success, clean desktop
journey, frame P99, process memory, backend-owned GPU resource bytes, module-add time/success,
public production coverage, and slot-configuration time/success. The deterministic suite verdict
is therefore **Redirect**. No complete hard-stop metric failed, so the result is not `Stop`;
incomplete evidence cannot produce `Continue`.

# Evidence

- Protocol BLAKE3:
  `82986505c1074833e87c05feae3189e985165bc04adb7bfd9769724c4452cc03`.
- Measurement-plan JSON SHA-256:
  `a41c43a9873820de79f1ee1aef63e8393127a94abe70eb85f78fc3f1102f4121`.
- One-run collector SHA-256:
  `c70aea68f493ca3874e4beb25ad78b4183407d0e200c925b5951e286fe899cf4`.
- Raw JSONL SHA-256:
  `076b6057413375b74d49f0c5bed1abfea08cbddba6841e278ee56f21aca82515`.
- Run-manifest SHA-256:
  `7b8d587af8eb3a9f7cd0b4484a32a20edd3d441c62be4866d6b049b4b59ef527`.
- Preflight-failure record SHA-256:
  `d298057804996303b5b50249ec2df431bd5e897ad3f419af1ec752147a714fa6`.
- Environment fingerprint:
  `23c62c56777a214c57686aa7ee4513e8c52be6912584a64537160485e660ce6e`.
- Collection window: `2026-07-29T18:05:05Z` through `2026-07-29T18:23:17Z`.
- Base terminal summary digest:
  `77819d5cac39ab3e25d9e9537e09995480e96d0e9488515cb480c7d50dba5169`.
- Focused U9 helper/policy gate: 21/21 passed with one build job and one test thread.
- Architecture-governance gate: 9/9 passed after replacing ADR 0081's stale reference to the
  removed rustc-diagnostic snapshot with the current compile-time negative trait assertion.
- Hosted prerequisite: GitHub Actions run
  [`30462379022`](https://github.com/Latias94/nara/actions/runs/30462379022), six of six
  root/reference-game/module-consumer cells successful.

The first current-refresh preflight was rejected before Cargo because the one-run collector retained
the prior CRLF data-edit anchor while the canonical scene file is LF. The external collector changed
only that exact anchor; repository and detached-subject bytes remained unchanged. The committed
baseline retains that failure classification and the complete normalized numeric populations. It
also retains the rejected coverage record inside the raw digest while excluding it from the
admitted metric count and verdict input.

# Follow-up

RGD-U9 and carried RGF-U14 are complete again with a `Redirect` verdict and named product
bottlenecks. The next evidence work must define an executed public-call coverage tracer, a bounded
desktop pressure tracer, and real module-add and window-slot author tasks before any first-playable
`Continue` claim.

RGD-U10 current-revision candidate execution remains separately admissible but requires a fresh
one-shot candidate-dispatch authorization. This record grants no dispatch, tag, environment,
Release, or publication authority. Any later Rust, Cargo, policy-test, protocol, workflow, or
reference-game executable change invalidates the applicable U8/U9 evidence before a later delivery
decision.

# Citations

- `docs/benchmarks/reference-game-first-playable-baseline.md`
- `docs/benchmarks/reference-game-first-playable-protocol.md`
- `docs/benchmarks/data/protocol/v1/reference-game-first-playable.json`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u9-record-the-first-playable-product-baseline`
- `docs/knowledge/engineering/verification/2026-07/2026-07-29T163302Z-rgd-u8-final-hosted-three-workspace-ci-refresh-b3883a881dda4296b19b5490153dc3fc.md`
- `tests/measurement_policy.rs`
- `tests/measurement_helpers.rs`

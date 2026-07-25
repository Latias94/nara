---
type: "Verification Evidence"
title: "RGD-U9 first-playable product baseline verification"
description: "Records the isolated Windows first-playable automatic population, deterministic Redirect verdict, missing product evidence, and non-claims."
timestamp: 2026-07-25T16:34:53Z
record_id: "ae82fc256c2145f69a239bf8f6924aa2"
tags: ["rgd-u9", "rgf-u14", "first-playable", "measurement", "windows", "redirect", "completed"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "c477f7de542ba171828d4b7d5f23f505c2c7c1cd"
verified_by: "codex-root"
---

# Verification

RGD-U9 executed the carried RGF-U14 first-playable protocol after RGD-U8 completed its final
hosted Windows/Linux prerequisite. The measured subject was clean detached revision
`c477f7de542ba171828d4b7d5f23f505c2c7c1cd`; its changes from the hosted executable revision are
evidence and engineering-memory documents only.

The admitted automatic run used Rust/Cargo 1.97.1 on Windows 11. Cargo used one build job, offline
resolution, distinct empty cold targets, one retained warm target, and an isolated Cargo home
populated only with a copied crates.io registry. Writable home, profile, temporary, and AppData
environment paths resolved under the external collection root. The active checkout was never the
measurement subject, and the detached worktree was clean at completion.

# Result

The collector produced 75 zero-exit raw records under one environment fingerprint. Independent
recalculation verified contiguous per-metric indices, no failure references, the manifest-bound
raw digest, and the protocol's one-based nearest-rank aggregation. Evidence review rejected the
single public-coverage record because its static test did not implement the declared denominator,
executed-call ratio, or both-terminal-path boundary; the remaining 74 records are admitted.

All 10 admitted required metrics passed their frozen targets:

| Metric | Aggregate |
| --- | ---: |
| Cold build P95 | 154.578 s |
| Incremental build P95 | 6.801 s |
| Clean-to-headless-wave P95 | 151.411 s |
| Headless wave success | 1 |
| Body edit P50 / P95 | 5.020 s / 6.962 s |
| Data edit P50 / P95 | 0.056 s / 0.067 s |
| Structural edit P50 / P95 | 5.616 s / 7.935 s |

Ten required metrics have no admissible population: desktop playable success, clean desktop
journey, frame P99, process memory, backend-owned GPU resource bytes, module-add time/success,
public production coverage, and slot-configuration time/success. The deterministic suite verdict
is therefore **Redirect**. No complete hard-stop metric failed, so the result is not `Stop`;
incomplete evidence cannot produce `Continue`.

# Evidence

- Protocol BLAKE3:
  `82986505c1074833e87c05feae3189e985165bc04adb7bfd9769724c4452cc03`.
- Measurement-plan JSON SHA-256:
  `e874993c85132c7bcc65b0102a5958c4f1e4323002e897de2b6bae4ee80915ac`.
- One-run collector SHA-256:
  `b6b3a3b5a8ce0f4f33170585cb838e7f828ed5fccbf1a684346a641a11f4a00b`.
- Raw JSONL SHA-256:
  `9a4175fda556672c3717757556252bd86e64d70851a6928ff2a2dcf49c59a50b`.
- Run-manifest SHA-256:
  `d2e39f23dca21858e5f52765baebda0adf63db7829e11ab3138bb84de1958ad4`.
- Environment fingerprint:
  `0952921828e09c731554a11538d7a2ba62191e935c718726af97082d1500c30f`.
- Collection window: `2026-07-25T15:45:21Z` through `2026-07-25T15:59:43Z`.
- Base terminal summary digest:
  `77819d5cac39ab3e25d9e9537e09995480e96d0e9488515cb480c7d50dba5169`.
- Hosted prerequisite: GitHub Actions run
  [`30154962046`](https://github.com/Latias94/nara/actions/runs/30154962046), six of six
  root/reference-game/module-consumer cells successful.

An earlier complete automatic attempt was rejected because it exposed the shared user Cargo home
and Cargo updated its global-cache tracker. No value from that population contributes to this
record. The committed baseline retains the complete failure classification and exact normalized
numeric populations. It also retains the rejected coverage record inside the raw artifact while
excluding it from the admitted metric count and verdict input.

# Follow-up

RGD-U9 and carried RGF-U14 are complete with a `Redirect` verdict and named product bottlenecks.
The next evidence work must define an executed public-call coverage tracer, a bounded desktop
pressure tracer, and real module-add and window-slot author tasks before any first-playable
`Continue` claim.

RGD-U10 remains independently admissible but requires its own hosted candidate-dispatch
authorization. This record grants no dispatch, tag, environment, Release, or publication
authority. Any later Rust, Cargo, policy-test, protocol, workflow, or reference-game executable
change invalidates the applicable U8/U9 evidence before a later delivery decision.

# Citations

- `docs/benchmarks/reference-game-first-playable-baseline.md`
- `docs/benchmarks/reference-game-first-playable-protocol.md`
- `docs/benchmarks/data/protocol/v1/reference-game-first-playable.json`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u9-record-the-first-playable-product-baseline`
- `docs/knowledge/engineering/verification/2026-07/2026-07-25T115419Z-rgd-u8-final-hosted-three-workspace-ci-verification-0ee1fb9b871a4716ae3d0c533fbdd044.md`
- `tests/measurement_policy.rs`
- `tests/measurement_helpers.rs`

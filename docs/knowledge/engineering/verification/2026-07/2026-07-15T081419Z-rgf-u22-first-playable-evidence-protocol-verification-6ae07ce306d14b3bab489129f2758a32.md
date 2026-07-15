---
type: "Verification Evidence"
title: "RGF-U22 first-playable evidence protocol verification"
description: "Verified the pre-target decision protocol, independent source attestations, trusted evidence envelope, Git revision admission, and ownership cohort gate."
timestamp: 2026-07-15T08:14:19Z
record_id: "6ae07ce306d14b3bab489129f2758a32"
tags: ["rgf-u22", "verification", "evidence", "measurement", "provenance", "security"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
pre_commit_head: "dedf7cb"
verified_by: "cargo-nextest,cargo-check,cargo-fmt,cargo-clippy,independent-review"
---

# Verification

RGF-U22 freezes one pre-target first-playable decision protocol before U4, U5, U12, U24, or U25
implementation. The canonical protocol binds semantic subjects, independently reviewed product
constraints, measurement/context identity, raw-sample aggregation, environment equivalence,
source invalidation, evidence-envelope limits, and Continue/Redirect/Stop outcomes. It remains
test-only policy and does not introduce a benchmark runner, production evidence API, runtime
diagnostics bridge, or target result.

# Result

The U22 contract is ready for an isolated commit. The canonical protocol is 50,376 bytes with
BLAKE3 `82986505c1074833e87c05feae3189e985165bc04adb7bfd9769724c4452cc03`.
The reviewed product-budget source is 39,621 bytes with BLAKE3
`fe7bf66ad90fef9838ede7acc7c7849849611c4f4b32b66793e2906b781ca0ce` and remains bound to
pre-target source revision `09695166d16d4c9411c53fe68a86ebed177bcda7`.

The evidence path now proves:

- two independent approved attestations with no U4-or-later target results observed;
- encoded, shape, record, field, string, identity, digest, path, and publication limit+1 rejection;
- an opaque clean-Git admission bound to the explicit repository root, exact HEAD,
  ancestor/merge-base proof, and complete NUL-delimited change manifest;
- legal UTF-8 Git paths remain distinct from the narrow evidence identifier grammar, while unknown
  or unrepresentable paths invalidate all reuse;
- ownership decisions require the exact U26 metric denominator, one U26 baseline, one trusted U24
  candidate digest, and matching correctness, fault, lifecycle, baseline, and reviewer digests;
- generic aggregation and decision entry points reject the ownership suite; and
- the frozen lifecycle starts at `candidate`, makes `stopped` terminal, and proves both start and
  termination reachability for every declared state.

# Evidence

## Automated gates

- The exact U22 gate passed 32/32, including the clean-checkout LF attribute contract:
  `cargo nextest run --locked -p nara --features serde --test measurement_policy --test
  evidence_envelope --test-threads=1` (final run `5a76a3b1-8566-4c94-922d-6fdef6556ebc`).
- The final workspace gate passed 685/685 with three declared conditional skips:
  `cargo nextest run --workspace --locked --test-threads=1` (run
  `f816dc2a-486c-494f-b33f-21d96c686e0c`).
- `cargo check --workspace --locked` passed.
- Root no-default, default, every coarse single-feature ceiling, and
  `--all-features --all-targets` checks passed with one build job.
- `cargo fmt --all -- --check` and `git diff --check` passed; Git emitted only existing line-ending
  conversion warnings.
- Targeted strict Clippy initially stopped on three pre-existing `result_large_err` findings in
  `src/project_host.rs`. Re-running the U22 targets with only that lint allowed passed with all other
  warnings denied:
  `cargo clippy -p nara --features serde --test measurement_policy --test evidence_envelope
  --no-deps -- -D warnings -A clippy::result_large_err`.

## Independent review

- Performance measurement review `u22_review_performance_v4` approved the 39,621-byte source at
  `2026-07-15T07:25:18Z`; its evidence file is 669 bytes with BLAKE3
  `060c6e934a66fd4b6357d1b5476d7ab5d695b6e01c9ee091153d3353211ba10e`.
- Protocol provenance review `u22_review_provenance_v2` approved the same source at
  `2026-07-15T07:30:33Z`; its evidence file is 664 bytes with BLAKE3
  `cdd40415f4a7905d3e262ef247052add859d696963d1a76ee9d2465e2cbe4a9c`.
- The final Rust contract review approved the stable admission, aggregation, ownership, lifecycle,
  and evaluator snapshot with no remaining P0/P1/P2 findings.

# Non-Claims And Follow-Up

- U22 contains no target result and does not prove Nara meets any range. U14, U25, and U20 own the
  later decisions without changing this protocol version.
- U26 must freeze and independently review the manual counterfactual before U24 implementation.
- U14/U20 still own real collector processes, archive acquisition, temporary-root containment, and
  restricted raw-log retention.
- Cross-revision admission currently requires UTF-8 paths for selective mapping. Any path that
  cannot be represented safely forces full invalidation rather than stale reuse.
- The next serial implementation unit is RGF-U4. No additional U22 metrics or evidence framework
  work is admitted before that product-composition step.

# Citations

- `docs/benchmarks/reference-game-first-playable-protocol.md`
- `docs/benchmarks/data/protocol/v1/reference-game-first-playable.json`
- `docs/benchmarks/data/sources/v1/first-playable-product-budgets.json`
- `docs/benchmarks/data/sources/v1/first-playable-product-budget-review.json`
- `tests/measurement_policy.rs`
- `tests/evidence_envelope.rs`
- `tests/support/first_playable_evidence.rs`
- ADRs 0048, 0049, and 0068.

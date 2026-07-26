---
type: "Verification Evidence"
title: "RGD delivery evidence invalidation after workflow hardening"
description: "Records why the hosted U8 and candidate U10 verdicts remain historical but no longer certify the hardened delivery revision, and corrects three immutable U10 citation anchors."
timestamp: 2026-07-26T12:05:33Z
record_id: "755db565363243deb24cb34f5a08d008"
tags: ["rgd-u8", "rgd-u9", "rgd-u10", "workflow", "invalidation", "correction"]
status: "blocked-pending-rerun"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "50f61408203bc21fe1fdbea988f7235679e37974"
verified_by: "codex-root"
supersedes: "a3bd2daed35e430abb30103ab86b9bdb"
---

# Verification

The protected U8 and U10 runs at
`6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f` remain truthful historical
evidence. They no longer certify the current delivery revision because four
qualifying local repairs landed afterward:

- `1fba33c` adds the previously omitted architecture-governance suite to the
  hosted root job and repairs two real governance-test failures.
- `a76123b` makes candidate and evidence-ingest authorizations single-attempt,
  rejects reruns explicitly, and installs the complete Linux X11/Vulkan smoke
  profile in both release stages.
- `45b8fcd` removes the impossible future publisher revision from the ancestor
  approval while retaining the reviewed workflow digest and trusted runtime
  publisher identity.
- `50f6140` pins the corrected verifier and approval schema to the already
  existing `45b8fcd` commit, its Git blobs, and exact SHA-256 values.

The active plan's R14, U8 test scenarios, and evidence-invalidation rules say
that later policy-test or workflow changes reopen the hosted verdict. The U10
record likewise requires a new candidate when workflow inputs change.

# Result

- **U8:** reopened. Run `30197255438` remains evidence that all six cells passed
  at `6914785`, but a new six-cell hosted run must cover the final hardened
  revision after all remaining U11 local preparation lands.
- **U9:** retained as a historical `Redirect` baseline only. Its protocol uses
  `current_revision_only` evidence and classifies `.github/workflows/` under
  delivery-automation invalidation, so it cannot supply final U11 evidence for
  the current source.
- **U10:** reopened for final candidate production. Run `30197927459` and
  artifact IDs `8630813710`/`8630881470` still prove the old candidate contract,
  but U11 must consume candidates produced by the final single-attempt workflow
  after the renewed U8 gate.
- **U11/U12:** local preparation may continue. No evidence-ingest dispatch,
  approval, tag, environment approval, Release mutation, or publication is
  authorized by this record.

# Evidence

- Focused local governance and workflow policy suites passed after the repairs:
  architecture/CI `22/22`, candidate/evidence/release policy `16/16`, release
  verifier `7/7`, and corrected verifier pin coverage `10/10`.
- The old U8 and U10 records cite exact protected run identities and remain
  immutable rather than being rewritten.
- Three immutable shards cite the nonexistent fragment
  `#u10-build-and-consume-standalone-windowslinux-candidates`:
  `2026-07-25T181843Z-rgd-u8-final-revision-hosted-ci-refresh-ac11b989e59c4df4b2db8804ceee3362.md`,
  `2026-07-26T023033Z-rgd-u10-candidate-capacity-diagnosis-and-local-budget-repair-914d3c5e90934617b4778cb2c85aa431.md`,
  and
  `2026-07-26T030317Z-rgd-u8-final-hosted-ci-after-candidate-capacity-repair-73cf3d9e548743eb9462aec8acbcfec1.md`.
  Their intended canonical target is
  `#u10-build-and-consume-standalone-release-candidates`. This correction
  record carries the mapping without mutating those shards.

# Follow-up

Finish U11's missing local product-metric and author-journey preparation. Then
run the final hosted sequence in dependency order with separate one-shot
authorizations: U8, fresh U9 evidence, final U10 candidates, and only afterward
U11 evidence ingest. Any later qualifying source, policy, or workflow change
repeats the same invalidation analysis.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#evidence-invalidation-map`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u8-close-hosted-three-workspace-ci`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-release-candidates`
- `docs/benchmarks/data/protocol/v1/reference-game-first-playable.json`
- `.github/workflows/ci.yml`
- `.github/workflows/reference-game-candidate.yml`
- `.github/workflows/reference-game-evidence-ingest.yml`
- `.github/workflows/reference-game-release.yml`
- Commits `1fba33c`, `a76123b`, `45b8fcd`, and `50f6140`
- GitHub Actions runs `30197255438` and `30197927459`

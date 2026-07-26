---
type: "Verification Evidence"
title: "RGD-U12 release verifier pin follows candidate layout"
description: "Records the reviewed ancestor, blob, and SHA-256 binding that keeps release validation aligned with the repaired candidate layout."
timestamp: 2026-07-26T02:50:29Z
record_id: "651c8156185f48a080d52cd1c0b68247"
tags: ["rgd-u10", "rgd-u12", "release-policy", "packaging", "verified"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "c3cd2930237d6cb41d3b010be7ebf302c12a660c"
verified_by: "codex-root"
---

# Verification

The first RGD-U10 capacity repair changed the candidate layout from `64 MiB`
to `80 MiB`. An independent security review found that the immutable-release
verifier still fetched the prior layout snapshot, so a later release would
correctly fail closed while verifying a valid repaired candidate.

# Result

Commit `c3cd2930237d6cb41d3b010be7ebf302c12a660c` advances the verifier's
reviewed source revision to ancestor
`476548f091a51c33110a3dca98edeee976876373` and updates the layout's Git blob
and SHA-256 pins together. The verifier's existing `fileAt` path still fetches
the content from that fixed revision and rejects any blob or digest mismatch
before transport preflight or release authority is reached.

The release policy now mutation-tests all three fixed identity values:
reviewed revision, package-layout blob, and package-layout SHA-256.

# Evidence

- The reviewed ancestor check passed for
  `476548f091a51c33110a3dca98edeee976876373` into
  `c3cd2930237d6cb41d3b010be7ebf302c12a660c`.
- At the reviewed revision, `reference-game/packaging/package-layout-v1.json`
  has blob `803460ad1d2ea2a2e3c36d193c72b09a636f9ef9` and SHA-256
  `95d53fc19a2108276199b8def11061c4650ef550c058d29ae42f55161ddb82bf`.
- `cargo nextest run --locked -p nara --test artifact_package_policy --test
  ci_policy --test release_workflow_policy --build-jobs 1 --test-threads 1`
  passed 22/22 tests after the pin update.
- `actionlint .github/workflows/reference-game-release.yml` passed.
- Two independent security reviews found no remaining P0, P1, or P2 issue in
  the revision/blob/digest binding. Neither review executed Cargo or candidate
  code.

# Follow-up

This closes the local release-verifier alignment repair only. Push the commit
chain and obtain ordinary hosted CI for the resulting revision. The failed
candidate dispatch remains consumed; a new one-shot authorization is still
required before RGD-U10 may be re-dispatched. No release, tag, approval, or
publication action occurred here.

# Citations

- `.github/workflows/reference-game-release.yml`
- `tests/release_workflow_policy.rs`
- `reference-game/packaging/package-layout-v1.json`
- Commit `476548f091a51c33110a3dca98edeee976876373`
- Commit `c3cd2930237d6cb41d3b010be7ebf302c12a660c`

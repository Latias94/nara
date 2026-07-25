---
type: "Verification Evidence"
title: "RGD-U12 local immutable pre-release preparation verification"
description: "Verifies the locally reviewable immutable-release workflow, policy, and credential boundaries without claiming a hosted release or publication."
timestamp: 2026-07-25T01:49:44Z
record_id: "5186939a5dec4103bfa1c702061af655"
tags: ["rgd-u12", "release", "immutable", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "58bbf6a3652a2f730b865dad2cfd4818f0bfc622"
verified_by: "Focused nextest policy suite, actionlint, static credential-boundary audit, Python/Node syntax checks, and staged-scope review"
---

# Scope

This evidence verifies only RGD-U12's locally admissible preparation at commit
`58bbf6a3652a2f730b865dad2cfd4818f0bfc622`: a manually dispatched immutable
pre-release workflow, bounded credential-free verification, separated draft/finalize writers,
and policy/documentation coverage. It does not close U12 or claim a hosted workflow run, tag,
draft release, published release, immutable repository setting, or public artifact.

# Implemented Contract

- `reference-game-release.yml` is manually dispatched, protected-main scoped, run-attempt gated,
  and tag-concurrent without cancellation.
- The credential-free verifier fetches the reviewed helper and schemas by exact source revision,
  Git blob ID, and SHA-256. It validates bounded approval, trusted-input, candidate, and
  publication-manifest records before any release mutation is admitted.
- Candidate artifacts cross the candidate-fetch and verifier boundaries as transport bundles.
  Credential-bearing write jobs stream only manifest-bound archive bytes after exact
  name/size/digest checks; they never checkout, extract, or execute candidate or repository
  helper bytes.
- Draft upload and final publication are distinct write-capable jobs with separate protected
  environments. Finalization consumes only bounded manifest, draft-release, and authorization
  identities, then rechecks immutable-release policy immediately before publication.
- Draft smoke uses a separately scoped read credential and sanitized platform environments.
  Public smoke uses anonymous release and asset downloads with no token, secret, or Authorization
  header, then verifies digest/size before running the pinned smoke helper.
- The local preparation guide records environment/token prerequisites, failure/new-version
  handling, and the fact that no hosted or public claim exists.

# Verification

- `cargo nextest run --locked -p nara --test ci_policy --test artifact_package_policy --test
  release_verification --test release_workflow_policy --build-jobs 1 --test-threads=1`: passed,
  27/27.
- `actionlint .github/workflows/reference-game-release.yml`: passed.
- A local static audit confirmed exactly two write-capable jobs (`draft-upload` and
  `release-finalize`); neither checks out repository content or executes candidate/helper bytes.
  The verifier's GitHub Script token is explicitly empty, while public smoke has no token,
  secret, or Authorization header.
- All inline GitHub Script bodies passed Node syntax checking; all inline Python bodies compiled.
  The workflow parsed as YAML with the intended nine jobs.
- `git diff --cached --check` and an exact staged-path audit passed before the commit. The commit
  contains exactly the U12 workflow, policy tests, and release-preparation documentation; it
  excludes concurrent architecture, strategy, memory, and Cargo changes.
- `architecture_docs` was intentionally not run under the user's instruction. This local U12
  slice does not alter architecture-governance authority.

# Review Corrections

Focused review corrected early workflow issues around immutable-release policy authorization,
transport-bound archive identity, finalizer smoke-receipt identity checks, helper/schema pinning,
and static policy coverage. The committed workflow keeps the required immutable-release policy
read separate from release-mutation credentials and treats any post-tag failure as a new-version
condition.

# Remaining Boundary

RGD-U12 local preparation is complete, but publication remains blocked. U8 must first close the
final hosted Windows/Linux matrix on the integrated revision. U11 must then produce a valid
`Publish` decision bound to final candidate identities and digests. Only after separate explicit
authorization for protected tag creation, draft-upload environment approval, release-finalize
environment approval, and Release mutation may a dispatch begin. No local result authorizes any
of those operations, and no tag, release, or public artifact is recorded here.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u12-publish-the-evidence-approved-immutable-github-pre-release`
- `docs/benchmarks/reference-game-release-preparation.md`
- Commit `58bbf6a3652a2f730b865dad2cfd4818f0bfc622`

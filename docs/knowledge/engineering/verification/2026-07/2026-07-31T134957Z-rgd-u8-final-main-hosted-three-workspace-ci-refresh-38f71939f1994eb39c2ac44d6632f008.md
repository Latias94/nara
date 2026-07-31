---
type: "Verification Evidence"
title: "RGD-U8 final main hosted three-workspace CI refresh"
description: "Closes the reopened U8 matrix at the merged main revision f098876 with six successful Windows/Linux jobs."
timestamp: 2026-07-31T13:49:57Z
record_id: "38f71939f1994eb39c2ac44d6632f008"
tags: ["rgd-u8", "ci", "hosted", "windows", "linux", "completed"]
status: "completed"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f09887600d2161144b920b6e2618fc8151dad4fa"
verified_by: "codex-root"
supersedes: "b3883a881dda4296b19b5490153dc3fc"
---

# Verification

The merged `main` revision
`f09887600d2161144b920b6e2618fc8151dad4fa` triggered ordinary GitHub Actions
CI run
[`30629508555`](https://github.com/Latias94/nara/actions/runs/30629508555).
GitHub reported workflow `CI`, event `push`, branch `main`, attempt `1`, exact
head SHA `f09887600d2161144b920b6e2618fc8151dad4fa`, and final conclusion
`success`. The run started at `2026-07-31T12:08:17Z` and completed at
`2026-07-31T13:08:55Z`.

# Result

All six independently visible required jobs completed successfully:

| Workspace | Platform | Job ID | Result |
| --- | --- | ---: | --- |
| root | Ubuntu | `91152348226` | success |
| root | Windows | `91152348329` | success |
| reference-game | Ubuntu | `91152348494` | success |
| reference-game | Windows | `91152348759` | success |
| module-consumer | Ubuntu | `91152349784` | success |
| module-consumer | Windows | `91152349794` | success |

No required job was skipped, neutral, cancelled, or evaluated at another
revision. Both root jobs ran the separately visible architecture-governance
test before the root workspace suite. This closes the reopened RGD-U8 contract
without making a packaging, candidate, baseline, or publication claim.

# Evidence

- `gh run view 30629508555 --json ...` reported `completed/success`, event
  `push`, branch `main`, attempt `1`, and the exact source SHA above.
- The root, reference-game, and module-consumer job pairs each exercised their
  independent lockfiles on Windows and Ubuntu.
- The run was triggered only after the measurement-helper, candidate,
  publisher, policy, and governance preparation had landed in the integrated
  revision.
- Local branch `refactor/engine-foundation-contracts` was fast-forwarded to the
  merged `origin/main` commit before dependent U9 collection began, and the
  worktree was clean at collection admission.

# Follow-up

RGD-U8 is complete at the cited executable revision. The committed U9
collector may measure this exact revision. Later Rust, Cargo, policy-test,
protocol, workflow, or reference-game executable changes reopen affected
delivery evidence; evidence-only records do not.

This record authorizes no candidate dispatch, evidence ingest, approval, tag,
environment action, Release mutation, or publication.

# Citations

- `.github/workflows/ci.yml`
- `tests/ci_policy.rs`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u8-close-hosted-three-workspace-ci`
- `docs/knowledge/engineering/verification/2026-07/2026-07-30T101248Z-rgd-delivery-evidence-correction-after-formal-product-review-4cf635ad00a744f59b2999d2cbeae8be.md`
- GitHub Actions run `30629508555`
- Commit `f09887600d2161144b920b6e2618fc8151dad4fa`

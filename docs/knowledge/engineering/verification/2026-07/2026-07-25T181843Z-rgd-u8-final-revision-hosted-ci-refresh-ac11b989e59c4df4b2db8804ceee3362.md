---
type: "Verification Evidence"
title: "RGD-U8 final-revision hosted CI refresh"
description: "Re-establishes the final hosted Windows/Linux matrix after the immutable-release approval-order repair."
timestamp: 2026-07-25T18:18:43Z
record_id: "ac11b989e59c4df4b2db8804ceee3362"
tags: ["rgd-u8", "rgd-u12", "ci", "hosted", "release-policy", "windows", "linux", "completed"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f9c7105d106b464d833ecfb7de3ba1c6c4dfdcc3"
verified_by: "codex-root"
supersedes: "0ee1fb9b871a4716ae3d0c533fbdd044"
---

# Verification

Commit `f9c7105d106b464d833ecfb7de3ba1c6c4dfdcc3` repaired the immutable-release
approval order by making `candidate-fetch` depend on `immutable-policy`. Candidate transport
staging therefore starts only after the protected policy job succeeds instead of creating
one-day intermediary artifacts while approval is still pending.

The release-policy validator now rejects missing, non-string, and duplicate `needs` entries.
Independent review validated both the missing mutation coverage and the prior
`filter_map`/`BTreeSet` false-acceptance path before the fixes were committed.

# Result

GitHub Actions run
[`30169033727`](https://github.com/Latias94/nara/actions/runs/30169033727) completed successfully
for all six required cells:

| Workspace | Platform | Job ID | Result |
| --- | --- | ---: | --- |
| root | Ubuntu | `89706820221` | success |
| root | Windows | `89706820260` | success |
| reference-game | Ubuntu | `89706820245` | success |
| reference-game | Windows | `89706820256` | success |
| module-consumer | Ubuntu | `89706820264` | success |
| module-consumer | Windows | `89706820267` | success |

This run re-establishes RGD-U8's final-revision hosted CI evidence after the release workflow and
policy-test change. It does not execute or complete RGD-U10, RGD-U11, or RGD-U12 publication.

# Evidence

- The workflow identity was `CI`, event `push`, head
  `f9c7105d106b464d833ecfb7de3ba1c6c4dfdcc3`, with terminal conclusion `success`.
- The root Windows and Ubuntu jobs both completed `Test CI and public dependency boundaries`;
  neither matrix cell was skipped, neutral, or cancelled.
- Before commit, `cargo nextest run --locked -p nara --test release_workflow_policy --test
  ci_policy --build-jobs 1 --test-threads=1` passed 16/16 tests.
- `cargo fmt --package nara`, `git diff --check`, and
  `actionlint .github/workflows/reference-game-release.yml` passed.
- Six focused reviewer roles reported no unresolved finding after two independently validated P1
  findings were fixed. The remaining external risks are the live GitHub Environment protection
  settings, token scope, and hosted approval/rejection/cancellation behavior.

# Follow-up

RGD-U10 remains the next evidence-producing unit. Its protected
`Reference Game Candidate` workflow has not been dispatched and still requires one-shot explicit
authorization. Ordinary push authorization and this CI run do not authorize that dispatch, any
environment approval, tag, Release mutation, or publication.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u8-close-hosted-three-workspace-ci`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-windowslinux-candidates`
- `.github/workflows/reference-game-release.yml`
- `tests/release_workflow_policy.rs`
- Commit `f9c7105d106b464d833ecfb7de3ba1c6c4dfdcc3`

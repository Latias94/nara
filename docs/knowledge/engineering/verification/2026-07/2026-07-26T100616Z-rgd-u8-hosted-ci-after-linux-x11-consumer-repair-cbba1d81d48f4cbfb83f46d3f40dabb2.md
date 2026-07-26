---
type: "Verification Evidence"
title: "RGD-U8 hosted CI after Linux X11 consumer repair"
description: "Re-establishes the exact six-cell hosted CI baseline after the Linux X11 candidate-consumer workflow repair."
timestamp: 2026-07-26T10:06:16Z
record_id: "cbba1d81d48f4cbfb83f46d3f40dabb2"
tags: ["rgd-u8", "rgd-u10", "ci", "hosted", "windows", "linux", "completed"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f"
verified_by: "codex-root"
supersedes: "73cf3d9e548743eb9462aec8acbcfec1"
---

# Verification

The Linux X11 candidate-consumer workflow and policy repair at `aa9b564` invalidated
the preceding RGD-U8 hosted verdict. A user-authorized push then published the exact
integrated descendant `6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f` to `origin/main`
and triggered ordinary GitHub Actions CI run
[`30197255438`](https://github.com/Latias94/nara/actions/runs/30197255438).
The run used the `push` event and reported the same exact head SHA.

# Result

All six required jobs completed successfully:

| Workspace | Platform | Job ID | Result |
| --- | --- | ---: | --- |
| root | Ubuntu | `89780863725` | success |
| root | Windows | `89780863687` | success |
| reference-game | Ubuntu | `89780863674` | success |
| reference-game | Windows | `89780863706` | success |
| module-consumer | Ubuntu | `89780863720` | success |
| module-consumer | Windows | `89780863733` | success |

No required job was skipped, neutral, cancelled, or evaluated against another
revision. This re-establishes the RGD-U8 hosted baseline after the X11 consumer
repair without making a standalone-candidate claim.

# Evidence

- `gh run view 30197255438 --repo Latias94/nara` reported overall
  `completed/success`, exact head SHA
  `6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f`, and six
  `completed/success` jobs.
- `gh run list --repo Latias94/nara --commit 6914785...` reported exactly the
  ordinary `CI` workflow for this revision. No protected `Reference Game
  Candidate` workflow was implicitly dispatched.
- Local `HEAD`, `origin/main`, and the hosted run head were equal when the result
  was recorded, and the worktree contained no concurrent change.

# Follow-up

RGD-U8 is again complete at the cited executable revision, and the previously
completed RGD-U9 baseline remains valid. RGD-U10 is still incomplete: the prior
one-shot candidate authorization was consumed by failed run `30186343288`, and
this push authorization covered only the ordinary CI observation. The next
evidence-producing action requires a new explicit one-shot authorization for the
protected `Reference Game Candidate` workflow. This record authorizes no
environment approval, tag, Release mutation, or publication.

# Citations

- `.github/workflows/ci.yml`
- `.github/workflows/reference-game-candidate.yml`
- `tests/ci_policy.rs`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u8-close-hosted-three-workspace-ci`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-release-candidates`
- GitHub Actions run `30197255438`
- Commit `6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f`

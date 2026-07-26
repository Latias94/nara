---
type: "Verification Evidence"
title: "RGD-U8 final hosted CI after candidate capacity repair"
description: "Re-establishes the six-cell hosted CI baseline after the candidate capacity and release-verifier pin repairs."
timestamp: 2026-07-26T03:03:17Z
record_id: "73cf3d9e548743eb9462aec8acbcfec1"
tags: ["rgd-u8", "rgd-u10", "rgd-u12", "ci", "hosted", "windows", "linux", "completed"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "26009e4dc3294eafbf19b35915436b30e13f47e0"
verified_by: "codex-root"
supersedes: "ac11b989e59c4df4b2db8804ceee3362"
---

# Verification

After the Linux candidate expanded-byte repair and the immutable-release
verifier's corresponding reviewed-layout pin update, the ordinary hosted CI
matrix ran from commit `26009e4dc3294eafbf19b35915436b30e13f47e0`.

# Result

GitHub Actions run
[`30185273807`](https://github.com/Latias94/nara/actions/runs/30185273807)
completed successfully for all six required cells:

| Workspace | Platform | Job ID | Result |
| --- | --- | ---: | --- |
| root | Ubuntu | `89748689726` | success |
| root | Windows | `89748689747` | success |
| reference-game | Ubuntu | `89748689724` | success |
| reference-game | Windows | `89748689725` | success |
| module-consumer | Ubuntu | `89748689749` | success |
| module-consumer | Windows | `89748689774` | success |

This re-establishes RGD-U8 at the repaired final revision. It does not
complete RGD-U10: no new candidate workflow was dispatched by this push.

# Evidence

- The CI event was a protected-branch `push`, with head
  `26009e4dc3294eafbf19b35915436b30e13f47e0`, created at
  `2026-07-26T02:53:34Z` and completed successfully at
  `2026-07-26T03:02:10Z`.
- Root Windows completed both the workspace check and the public dependency
  boundary test. Both reference-game cells completed their package and public
  surface checks; both module-consumer cells completed their direct
  scene-module surface checks.
- Before the push, the focused local package/CI/release-policy suite passed
  22/22 and `actionlint .github/workflows/reference-game-release.yml` passed.
- Three focused security reviews found no remaining P0, P1, or P2 issue in
  the reviewed ancestor, layout blob, and SHA-256 release-verifier binding.

# Follow-up

The original RGD-U10 dispatch was consumed by a Linux package-capacity
failure. The repaired source now needs a new explicit one-shot authorization
to dispatch `Reference Game Candidate`; no ordinary push, CI result, approval,
tag, Release, or publication authority implies that authorization.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u8-close-hosted-three-workspace-ci`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-windowslinux-candidates`
- `.github/workflows/ci.yml`
- `.github/workflows/reference-game-candidate.yml`
- GitHub Actions run `30185273807`
- Commit `26009e4dc3294eafbf19b35915436b30e13f47e0`

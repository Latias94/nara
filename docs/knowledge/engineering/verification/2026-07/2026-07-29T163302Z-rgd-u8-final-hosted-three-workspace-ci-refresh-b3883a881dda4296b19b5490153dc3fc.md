---
type: "Verification Evidence"
title: "RGD-U8 final hosted three-workspace CI refresh"
description: "Closes the final-revision Windows/Linux root, reference-game, and module-consumer CI matrix at ef8f300."
timestamp: 2026-07-29T16:33:02Z
record_id: "b3883a881dda4296b19b5490153dc3fc"
tags: ["rgd-u8", "ci", "hosted", "windows", "linux", "completed"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ef8f300889086cfa1241c45c19bfc8d4edf8ffb3"
verified_by: "codex-root"
supersedes: "cbba1d81d48f4cbfb83f46d3f40dabb2"
---

# Verification

The final executable revision
`ef8f300889086cfa1241c45c19bfc8d4edf8ffb3` was published to `origin/main`
and triggered ordinary GitHub Actions CI run
[`30462379022`](https://github.com/Latias94/nara/actions/runs/30462379022).
The run used the `push` event, reported that exact head SHA, and completed with
`success`. This is the first final-revision run after the Linux strict-filesystem,
canonical-fixture, platform-layout budget, and software-GPU Host repairs exposed by
the preceding provisional runs.

# Result

All six required jobs completed successfully:

| Workspace | Platform | Job ID | Result |
| --- | --- | ---: | --- |
| root | Ubuntu | `90611472883` | success |
| root | Windows | `90611472761` | success |
| reference-game | Ubuntu | `90611472884` | success |
| reference-game | Windows | `90611472812` | success |
| module-consumer | Ubuntu | `90611472722` | success |
| module-consumer | Windows | `90611472846` | success |

No required job was skipped, neutral, cancelled, or evaluated against another
revision. The root jobs exercised the default and all-feature workspaces; the
reference-game jobs exercised their default and all-feature lockfile; the module
consumer jobs checked and tested its independent renamed dependency surface. This
closes RGD-U8 without making a baseline, candidate, packaging, or publication claim.

# Evidence

- `gh run view 30462379022 --repo Latias94/nara` reported
  `completed/success`, the exact source SHA above, and six
  `completed/success` jobs.
- `gh run list --repo Latias94/nara --commit ef8f300...` reported exactly one
  ordinary `CI` workflow for this revision. No protected candidate, evidence-ingest,
  tag, draft, release, or publication workflow was dispatched.
- Local `HEAD`, `refs/heads/main`, `refs/remotes/origin/main`, and the hosted run
  head were equal when the result was recorded, and the worktree was clean.
- This record resolves the RGD-U8 slice of the hardened-delivery invalidation
  recorded by `755db565363243deb24cb34f5a08d008`; its U9 and U10 conclusions remain
  open until their own fresh evidence exists.

# Follow-up

RGD-U8 is complete at the cited executable revision. The baseline previously
recorded at `c477f7de` remains truthful historical evidence but cannot certify the
current source after later Rust, Cargo, policy, and workflow changes. RGD-U9 must
therefore collect a fresh first-playable baseline before RGD-U10 or RGD-U11 can use
current delivery evidence. Any later Rust, Cargo, policy-test, protocol, workflow,
or reference-game executable change reopens the affected evidence according to the
active plan.

This record authorizes no protected candidate dispatch, evidence ingest, approval
commit, tag, environment approval, Release mutation, or publication.

# Citations

- `.github/workflows/ci.yml`
- `tests/ci_policy.rs`
- `docs/architecture/adr/implementation-status.md`
- `docs/architecture/nara-foundation.md`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u8-close-hosted-three-workspace-ci`
- `docs/knowledge/engineering/verification/2026-07/2026-07-26T120533Z-rgd-delivery-evidence-invalidation-after-workflow-hardening-755db565363243deb24cb34f5a08d008.md`
- GitHub Actions run `30462379022`
- Commit `ef8f300889086cfa1241c45c19bfc8d4edf8ffb3`

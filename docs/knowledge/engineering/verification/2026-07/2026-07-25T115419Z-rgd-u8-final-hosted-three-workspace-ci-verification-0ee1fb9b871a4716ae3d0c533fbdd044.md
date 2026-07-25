---
type: "Verification Evidence"
title: "RGD-U8 final hosted three-workspace CI verification"
description: "Records the final user-authorized GitHub Actions matrix for root, reference-game, and module-consumer on Windows and Linux."
timestamp: 2026-07-25T11:54:19Z
record_id: "0ee1fb9b871a4716ae3d0c533fbdd044"
tags: ["rgd-u8", "rgf-u15", "ci", "hosted", "windows", "linux", "completed"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "1e60291fd9ce890b0ddfd04cce427c02c4c9c4a5"
verified_by: "codex-root"
---

# Verification

RGD-U8 carried RGF-U15's locally prepared three-workspace workflow into one final hosted matrix
at executable revision `1e60291fd9ce890b0ddfd04cce427c02c4c9c4a5`. The run was triggered by a
separately user-authorized direct push to `main`; its `push` event, branch, head SHA, six job
identities, and terminal conclusions were read back from GitHub Actions.

The hosted repairs changed only `.gitattributes` and `tests/ci_policy.rs`. They did not change the
U2-U6 runtime, Host, registry, or gameplay evidence surfaces, so they did not invalidate the U7
Runtime/Host decision. The final run includes every policy-test and byte-stability repair.

# Result

GitHub Actions run
[`30154962046`](https://github.com/Latias94/nara/actions/runs/30154962046) completed successfully:

| Workspace | Platform | Job ID | Started (UTC) | Completed (UTC) | Result |
| --- | --- | ---: | --- | --- | --- |
| root | Ubuntu | `89671373007` | `2026-07-25T10:43:10Z` | `2026-07-25T10:48:12Z` | success |
| root | Windows | `89671373026` | `2026-07-25T10:43:08Z` | `2026-07-25T10:51:42Z` | success |
| reference-game | Ubuntu | `89671373046` | `2026-07-25T10:43:09Z` | `2026-07-25T10:45:41Z` | success |
| reference-game | Windows | `89671373017` | `2026-07-25T10:43:08Z` | `2026-07-25T10:47:56Z` | success |
| module-consumer | Ubuntu | `89671373047` | `2026-07-25T10:43:09Z` | `2026-07-25T10:45:21Z` | success |
| module-consumer | Windows | `89671373038` | `2026-07-25T10:43:08Z` | `2026-07-25T10:47:42Z` | success |

All cells completed rather than being skipped, neutral, or cancelled. The Windows root job ran the
complete 38-test CI and public-boundary suite after its locked workspace check; the other jobs ran
their locked workspace checks and dedicated public-surface tests.

# Evidence

- The final workflow identity is `CI`, event `push`, branch `main`, head
  `1e60291fd9ce890b0ddfd04cce427c02c4c9c4a5`, created at `2026-07-25T10:43:05Z` and completed at
  `2026-07-25T10:51:43Z`.
- Provisional run `30150995380` exposed CRLF-sensitive workflow policy mutation at `4c70f41`;
  `6f8fccc` fixed workflow byte stability.
- Provisional run `30152722806` then exposed the same mutation-helper assumption for a Windows
  Cargo manifest; `41ada06` added a platform-independent CRLF regression and normalized mutation
  inputs.
- Provisional run `30154559550` reached release verification and exposed CRLF conversion of
  canonical approval fixtures; `1e60291` fixed the LF contract for every JSON fixture without
  weakening canonical or digest validation.
- Before the final push, the exact root hosted boundary command passed locally with 38/38 tests,
  and the focused release-verification suite passed 7/7 tests. The final hosted run, rather than
  either local result, owns the cross-platform verdict.
- GitHub's branch-protection endpoint reported that `main` is not currently protected. This record
  therefore proves only the separately authorized hosted CI matrix; it does not claim protected
  branch governance, candidate trust, publication authority, or release readiness.

# Follow-up

RGD-U8 and the carried RGF-U15 hosted-CI lane are complete at the cited revision. RGD-U9 and
RGD-U10 may now enter their independently authorized evidence-producing phases. This authorization
does not carry into a candidate dispatch, evidence-ingest dispatch, tag, environment approval,
Release mutation, or publication.

Any later Rust, Cargo, policy-test, or workflow change reopens U8. Later immutable evidence and
registration records do not reopen it while those executable inputs remain unchanged.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u8-close-hosted-three-workspace-ci`
- `.github/workflows/ci.yml`
- `tests/ci_policy.rs`
- `docs/knowledge/engineering/verification/2026-07/2026-07-23T081241Z-rgd-u7-runtime-host-authority-verification-38a8bf4d48614a829bdd6388f02c9446.md`
- `docs/knowledge/engineering/registry/2026-07/2026-07-21T141717Z-reference-game-foundation-rgf-u15-codex-root-0b5ca03232b44fd49c99ae872feef69f.md`
- Commits `6f8fccc153607d6f2f52244e7f85148c070dbacb`,
  `41ada06c2697865d9e6233fbd2ffb3bd53a24e77`, and
  `1e60291fd9ce890b0ddfd04cce427c02c4c9c4a5`

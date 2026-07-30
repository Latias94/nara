---
type: "Verification Evidence"
title: "RGD-U10 refreshed standalone candidate completion verification"
description: "Closes immutable checkout-free Windows and Linux candidates at fafc949 with exact hosted identities, bounded contents, and successful headless/desktop consumption."
timestamp: 2026-07-30T03:44:18Z
record_id: "ecd70107315740eab6c580f18eca4dd0"
tags: ["rgd-u10", "candidate", "hosted", "linux", "windows", "packaging", "completed", "refresh"]
status: "completed"
producer_id: "codex-root"
run_id: "019f5096-ee46-7571-a208-be491cc72786"
source_session: "019f5096-ee46-7571-a208-be491cc72786"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "fafc9497f7101f0c271751f2ea3dea85b3eb9101"
verified_by: "codex-root"
supersedes: "a3bd2daed35e430abb30103ab86b9bdb"
---

# Verification

A fresh explicit one-shot authorization dispatched the protected
`Reference Game Candidate` workflow against remote `main` at
`fafc9497f7101f0c271751f2ea3dea85b3eb9101`. GitHub Actions run
[`30510353046`](https://github.com/Latias94/nara/actions/runs/30510353046)
reported repository `Latias94/nara`, workflow path
`.github/workflows/reference-game-candidate.yml`, `workflow_dispatch`, branch
`main`, attempt `1`, the exact expected head SHA, actor and triggering actor
`Latias94`, and overall `completed/success`.

The source workflow has Git blob
`e13589b6659850056e99d0e93bb2635a7abc387c`. The default branch was protected
before dispatch and remained protected when evidence was collected. Its
administrator-enforced rule denies force pushes and deletion without requiring
a pull request, status check, or alternate source revision.

| Job | Job ID | Result |
| --- | ---: | --- |
| Candidate build (linux-x86_64) | `90768890390` | success |
| Candidate build (windows-x86_64) | `90768890335` | success |
| Candidate consumer (linux-x86_64) | `90771902220` | success |
| Candidate consumer (windows-x86_64) | `90771902201` | success |

# Result

Both candidate archives and their independently retained transport artifacts
have exact identities:

| Field | Linux | Windows |
| --- | --- | --- |
| Candidate archive | `nara-reference-game-linux-x86_64.zip` | `nara-reference-game-windows-x86_64.zip` |
| Archive bytes | `23,724,434` | `17,444,232` |
| Archive SHA-256 | `c347e005e105ea5bebe85391ec6a476ec63bf80cbbda095040a5477215331a40` | `cf6eaad99d674b4678fb225b744b02b73b0bc1ef9a4f2c91b74ec5c9dc22878e` |
| Expanded bytes | `75,566,806` | `49,596,998` |
| Manifest payload files | `18` | `18` |
| Transport artifact ID | `8747060311` | `8747209952` |
| Transport bytes | `23,801,362` | `17,523,068` |
| Transport SHA-256 | `e1161ab38f916df1b35b7ad06137c0b3300014181f1002e6ff4c8ec065b4c6bc` | `2b5ce0459712123db08c03a1f20cab0c2d620104ceea2d15ddc62501721528da` |
| Retained until | `2026-08-13T03:20:36Z` | `2026-08-13T03:29:43Z` |

Each consumer downloaded the exact run/attempt-scoped artifact without a
repository checkout, matched GitHub's transport digest, verified the bundle
before extraction, and executed the candidate from a disposable random work
root and home. Both reported version `0.1.0`, source revision
`fafc9497f7101f0c271751f2ea3dea85b3eb9101`, headless summary schema
`nara-reference-game.wave-summary-v1`, and `desktop_probe: completed`. Linux
used Xvfb, the explicit X11 client-library profile, and Mesa Vulkan software
fallback; Windows used the declared fallback adapter environment.

# Evidence

- The GitHub run completed at `2026-07-30T03:31:11Z`; both build jobs and both
  no-checkout consumer jobs succeeded. The rerun-rejection job was skipped on
  attempt `1`, as required.
- GitHub's artifact API bound both artifacts to run `30510353046`, branch
  `main`, repository ID `1294975702`, and the exact source revision. It reported
  the IDs, transfer sizes, SHA-256 digests, and expiry timestamps above.
- The Linux consumer independently reported transport digest
  `e116...c6bc`, candidate digest `c347...1a40`, 18 payload files,
  `75,566,806` expanded bytes, successful headless execution, and completed
  desktop probing after installing the X11/Vulkan software profile.
- The Windows consumer independently reported transport digest
  `2b5c...28da`, candidate digest `cf6e...2878`, 18 payload files,
  `49,596,998` expanded bytes, successful headless execution, and completed
  desktop probing.
- A separate local evidence collection downloaded each retained transport into
  an isolated temporary directory. The transport-owned pinned verifier passed
  `bundle-verify` for both platforms, and independent SHA-256 calculations
  matched both candidate receipts and bundle manifests exactly.
- The workflow grants only `contents: read`; candidate consumers have no
  checkout, Rust toolchain installation, secret, OIDC, write permission, or
  publication step. Artifact names bind platform, run ID, and run attempt, and
  retention is 14 days.

# Follow-up

RGD-U10 and the carried RGF-U7 candidate unit are complete at the cited source
revision. RGD-U8 remains green and RGD-U9 remains complete with its measured
`Redirect` verdict. The candidate artifacts are ephemeral; expiry or any
executable, policy-test, or workflow drift requires a newly authorized U8/U10
refresh rather than an inferred replacement.

The pinned download action emitted GitHub's Node 20 deprecation warning while
the runner forced it onto Node 24. The transfer and digest checks succeeded, so
this is workflow-maintenance evidence rather than a candidate failure. Changing
the action or workflow now would invalidate the current delivery evidence.

This record authorizes no RGD-U11 evidence-ingest dispatch, approval commit,
tag, environment approval, Release mutation, or publication.

# Citations

- `.github/workflows/reference-game-candidate.yml`
- `reference-game/packaging/package-layout-v1.json`
- `reference-game/tools/package.py`
- `reference-game/tools/smoke_artifact.py`
- `tests/artifact_package_policy.rs`
- `tests/ci_policy.rs`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-release-candidates`
- GitHub Actions run `30510353046`
- Candidate artifact IDs `8747060311` and `8747209952`
- Commit `fafc9497f7101f0c271751f2ea3dea85b3eb9101`

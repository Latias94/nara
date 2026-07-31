---
type: "Verification Evidence"
title: "RGD-U10 corrected standalone candidate completion"
description: "Closes U10 with checkout-free Windows/Linux candidates that run the packaged desktop and headless product entries."
timestamp: 2026-07-31T15:40:41Z
record_id: "8fde293bbf06472d827d24304ecc2b40"
tags: ["rgd-u10", "candidate", "packaging", "windows", "linux", "completed"]
status: "completed"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "05a856bb0407e3b3b7b75e460dadd08c3c822841"
verified_by: "codex-root"
supersedes: "4cf635ad00a744f59b2999d2cbeae8be"
---

# Verification

GitHub Actions workflow `Reference Game Candidate` run
[`30641757833`](https://github.com/Latias94/nara/actions/runs/30641757833)
executed from `main` at exact source revision
`05a856bb0407e3b3b7b75e460dadd08c3c822841`. GitHub reported event
`workflow_dispatch`, attempt `1`, and final conclusion `success`. The run
started at `2026-07-31T15:10:39Z` and completed at
`2026-07-31T15:34:14Z`.

The build jobs checked out source with no persisted credentials, built the
release headless and desktop products, created bounded packages, bundled the
candidate with pinned verification inputs, and uploaded two 14-day artifacts.
The consumer jobs performed no repository checkout and installed no Rust
toolchain. They downloaded the exact same-run artifact, verified it before
extraction, created unique temporary home/cwd/tmp state, and ran both the
packaged `bin/headless` and `bin/desktop` entries.

# Result

All four required jobs completed successfully:

| Role | Platform | Job ID | Result |
| --- | --- | ---: | --- |
| build | Linux x86_64 | `91193194948` | success |
| build | Windows x86_64 | `91193194999` | success |
| consumer | Linux x86_64 | `91198541339` | success |
| consumer | Windows x86_64 | `91198541447` | success |

The rerun-rejection job `91193196052` was correctly skipped because this was
the first workflow attempt. The platform-inapplicable smoke steps were also
correctly skipped; no required build, verification, extraction, headless, or
desktop step was skipped.

This closes RGD-U10. It proves package construction, bounded transport,
checkout-free consumption, randomized process state, and real product-entry
smoke on both supported platforms. It grants no approval, tag, environment,
Release, or publication authority.

# Evidence

| Platform | Artifact ID | GitHub transport bytes | GitHub artifact digest | Expires |
| --- | ---: | ---: | --- | --- |
| Linux x86_64 | `8798066405` | `23,846,157` | `sha256:8e96293e219a89de6dc85916de69678817e2a3360550cd7a1eb099f00a479dfb` | 2026-08-14T15:22:46Z |
| Windows x86_64 | `8798354125` | `17,557,754` | `sha256:98cb1ea6d42693025749e0c87d9095526cdbadb4fbfff4d739cc286735e23221` | 2026-08-14T15:32:51Z |

Independent post-run download and `bundle-verify` checks reproduced the
candidate receipts:

| Platform | Candidate bytes | Expanded bytes | Files | Candidate SHA-256 |
| --- | ---: | ---: | ---: | --- |
| Linux x86_64 | `23,767,030` | `75,702,711` | 18 | `8559150fb0680c5a4c389ace33059b9e7ec23d6ab6b4a8754d2f0793961d28ce` |
| Windows x86_64 | `17,476,651` | `49,680,226` | 18 | `1e3bb58054fdffce30118ba91948cdddf353d3970026392ca7a969189f334572` |

Both receipts use schema `nara.reference-game.candidate-package-v1`, format
version `1`, product version `0.1.0`, and source revision
`05a856bb0407e3b3b7b75e460dadd08c3c822841`. The transport manifests bind the
archive, receipt, package layout, packager, and smoke helper by path, size, and
SHA-256.

The Linux consumer installed the explicit X11 and Mesa Vulkan software
profile, launched the real desktop product through `xvfb-run`, and requested
the fallback adapter. The Windows consumer launched the packaged desktop
product directly with the fallback adapter. Both completed the bounded
`--candidate-smoke` marker and the stable headless wave summary.

# Follow-up

RGD-U8, U9, and U10 now form one admitted dependency chain. RGD-U11 may enter
its source/candidate/clean-room audit, but no evidence-ingest or publication
mutation should run until that audit reconciles the current simplified
evidence strategy and clears every P0/P1 finding.

The GitHub Actions Node 20 deprecation annotation applies to the pinned
`actions/download-artifact` runtime. GitHub forced that action onto Node 24 and
the step succeeded. Treat the annotation as a future action-pin maintenance
item, not a failure of these candidate bytes.

# Citations

- `.github/workflows/reference-game-candidate.yml`
- `reference-game/tools/package.py`
- `reference-game/tools/smoke_artifact.py`
- `reference-game/packaging/package-layout-v1.json`
- `tests/artifact_package_policy.rs`
- `tests/ci_policy.rs`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-release-candidates`
- Verification records `38f71939f1994eb39c2ac44d6632f008` and
  `31ad7721ec874d9b862492beb7791f7a`
- GitHub Actions run `30641757833`
- Commit `05a856bb0407e3b3b7b75e460dadd08c3c822841`

---
type: "Verification Evidence"
title: "RGD-U10 standalone candidate completion verification"
description: "Verifies immutable checkout-free Windows and Linux candidates, exact hosted identities, bounded contents, and successful headless/desktop consumption."
timestamp: 2026-07-26T10:45:13Z
record_id: "a3bd2daed35e430abb30103ab86b9bdb"
tags: ["rgd-u10", "candidate", "hosted", "linux", "windows", "packaging", "completed"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f"
verified_by: "codex-root"
supersedes: "4e1b673441774fd99f3637586cb2df4f"
---

# Verification

A new explicit one-shot authorization dispatched the protected `Reference Game
Candidate` workflow against remote `main` at
`6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f`. GitHub Actions run
[`30197927459`](https://github.com/Latias94/nara/actions/runs/30197927459)
reported `workflow_dispatch`, the exact expected head SHA, and overall
`completed/success`.

| Job | Job ID | Result |
| --- | ---: | --- |
| Candidate build (linux-x86_64) | `89782687782` | success |
| Candidate build (windows-x86_64) | `89782687791` | success |
| Candidate consumer (linux-x86_64) | `89784309721` | success |
| Candidate consumer (windows-x86_64) | `89784309665` | success |

# Result

Both candidate archives and their independently uploaded transport artifacts
have exact retained identities:

| Field | Linux | Windows |
| --- | --- | --- |
| Candidate archive | `nara-reference-game-linux-x86_64.zip` | `nara-reference-game-windows-x86_64.zip` |
| Archive bytes | `23,167,150` | `16,936,245` |
| Archive SHA-256 | `056c059efa8abb5e0fa559478517ae251765d1804df268366fd6241de3bd432c` | `39392414010f88494db27c23060679056219cc2359cb2f4faf26884ca5063b3f` |
| Expanded bytes | `73,922,503` | `48,166,327` |
| Manifest payload files | `17` | `17` |
| Transport artifact ID | `8630813710` | `8630881470` |
| Transport bytes | `23,244,039` | `17,015,041` |
| Transport SHA-256 | `fbad188ad61e85e5863061a13ddc7e7177666482107044da3306196d9d5b77c8` | `eb7507d091dae62cf78d6ad619bc4cb5842d0b04c68644880c16f13bdbd867b9` |
| Retained until | `2026-08-09T10:27:51Z` | `2026-08-09T10:35:01Z` |

Each consumer downloaded the exact run/attempt-scoped transport without a
repository checkout, matched the GitHub transport digest, verified the bundle
before extraction, and then executed the candidate under a fresh randomized
working directory and home. Both reported version `0.1.0`, source revision
`6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f`, headless summary schema
`nara-reference-game.wave-summary-v1`, and `desktop_probe: completed`. The Linux
consumer used Xvfb, the explicit X11 client-library profile, and the Mesa Vulkan
fallback; the Windows consumer used the declared fallback adapter environment.

# Evidence

- The candidate archive bytes independently matched `candidate/receipt.json`,
  `bundle-manifest.json`, and a local SHA-256 calculation for both platforms.
- Each embedded package manifest enumerated 17 payload files. The archive also
  contained its generated manifest entry. The payload included `README.md`,
  `CONTROLS.md`, `LICENSE-MIT`, `LICENSE-APACHE`, the Kenney asset license,
  headless and desktop binaries, the desktop render probe, project settings,
  startup scene, prefab, textures and metadata, and both component-schema
  fixtures.
- Linux verification reported archive SHA-256 `056c...432c`, 17 payload files,
  `73,922,503` expanded bytes, successful headless execution, and completed
  desktop render probing after installing the exact X11/Vulkan software profile.
- Windows verification reported archive SHA-256 `3939...3b3f`, 17 payload files,
  `48,166,327` expanded bytes, successful headless execution, and completed
  desktop render probing.
- The workflow grants only `contents: read`; consumer jobs have no checkout,
  toolchain installation, secret, OIDC, write permission, or publication step.
  Artifact names bind platform, run ID, and run attempt, and retention is 14 days.

# Follow-up

RGD-U10 and the carried standalone-candidate unit are complete at the cited
revision. RGD-U8 remains green and RGD-U9 remains complete. The exact candidate
artifacts are ephemeral and must be consumed before their recorded expiration;
expiry or executable drift requires a newly authorized candidate run and new
identities rather than an inferred replacement.

The pinned upload action emitted GitHub's Node 20 deprecation warning while the
runner forced it onto Node 24; uploads completed successfully. This is a future
workflow-maintenance signal, not a failure of the retained candidates, and any
repair that changes executable or workflow inputs must follow the plan's
invalidation rules.

RGD-U11 remains separately gated. This evidence authorizes no evidence-ingest
dispatch, approval record, tag, environment approval, Release mutation, or
publication.

# Citations

- `.github/workflows/reference-game-candidate.yml`
- `reference-game/packaging/package-layout-v1.json`
- `reference-game/tools/package.py`
- `reference-game/tools/smoke_artifact.py`
- `tests/artifact_package_policy.rs`
- `tests/ci_policy.rs`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-release-candidates`
- GitHub Actions run `30197927459`
- Candidate artifact IDs `8630813710` and `8630881470`
- Commit `6914785eb39bd2c71ec3c7fa75a6ec89f1d1289f`

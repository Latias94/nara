---
type: "Verification Evidence"
title: "RGD-U10 candidate capacity diagnosis and local budget repair"
description: "Records the failed protected candidate run, measured Linux package capacity, and local bounded repair while preserving the required hosted rerun gate."
timestamp: 2026-07-26T02:30:33Z
record_id: "914d3c5e90934617b4778cb2c85aa431"
tags: ["rgd-u10", "candidate", "packaging", "hosted", "linux", "windows", "partial"]
status: "partial"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d9cc9f284c4b46db24ae0489eeae2b7f38264215"
verified_by: "codex-root"
---

# Verification

The one-shot protected `Reference Game Candidate` dispatch ran on
`d9cc9f284c4b46db24ae0489eeae2b7f38264215` as GitHub Actions run
[`30183495975`](https://github.com/Latias94/nara/actions/runs/30183495975).
It exposed a Linux candidate-package capacity mismatch before any candidate
consumer smoke executed.

# Result

The Linux build job `89744049633` completed both locked release builds, then
failed at `Create bounded candidate` because its staging tree exceeded the
then-current `67,108,864`-byte expanded limit. The Windows build job
`89744049612` completed packaging, bundle construction, and upload; its
archive contained `48,169,378` expanded bytes. The matrix failure skipped the
candidate-consumer job, so this evidence does not complete RGD-U10.

The layout now uses an `83,886,080`-byte (`80 MiB`) expanded limit. It remains
a bounded aggregate below the three `32 MiB` per-file maxima, retains the
`32 MiB` encoded archive limit, and leaves `9,957,401` bytes of headroom over
the measured Linux staging tree.

# Evidence

- A Linux Rust `1.95.0` release rebuild with one Cargo build job measured
  `headless` at `16,406,944` bytes, `desktop` at `28,632,008` bytes, and
  `desktop_render_probe` at `28,835,456` bytes. The fixed documentation and
  project files added `54,271` bytes, for `73,928,679` expanded candidate
  bytes across 17 staged files.
- The repaired local package preflight succeeded without executing candidate
  code. Its ZIP archive was `23,160,464` bytes with SHA-256
  `02f03f364e1964bebc7a3276a5057276d8ff1efa3761f311fecb4e4c37187b2d`;
  archive verification and no-checkout transport verification both reported
  the same identity, file count, and expanded-byte total.
- `cargo nextest run --locked -p nara --test artifact_package_policy --test
  ci_policy --test release_workflow_policy --build-jobs 1 --test-threads 1`
  passed 22/22 tests after the layout change.

# Follow-up

Commit and push the bounded layout repair, then require a fresh successful
ordinary hosted CI run for that commit. The prior dispatch authorization was
consumed by run `30183495975`; a separate new one-shot authorization is
required before re-dispatching the protected candidate workflow. Only a
successful hosted Linux and Windows candidate plus no-checkout consumer run
can complete RGD-U10.

# Citations

- `reference-game/packaging/package-layout-v1.json`
- `reference-game/tools/package.py`
- `reference-game/tools/smoke_artifact.py`
- `.github/workflows/reference-game-candidate.yml`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-windowslinux-candidates`
- GitHub Actions run `30183495975`

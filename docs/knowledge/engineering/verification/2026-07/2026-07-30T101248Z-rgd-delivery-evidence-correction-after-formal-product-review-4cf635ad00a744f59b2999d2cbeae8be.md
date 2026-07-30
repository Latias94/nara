---
type: "Verification Evidence"
title: "RGD delivery evidence correction after formal product review"
description: "Reconciles U8/U9/U10 claims at the reviewed delivery revision, invalidates the probe-only U10 completion, and blocks U11 pending refreshed evidence."
timestamp: 2026-07-30T10:12:48Z
record_id: "4cf635ad00a744f59b2999d2cbeae8be"
tags: ["rgd-u8", "rgd-u9", "rgd-u10", "rgd-u11", "correction", "invalidation"]
status: "blocked-pending-rerun"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8b263c425c39fbf3d083b748c3c5fe10dc140b42"
verified_by: "codex-root"
supersedes: ["755db565363243deb24cb34f5a08d008", "ecd70107315740eab6c580f18eca4dd0"]
---

# Verification

Formal product review found that the standing repository text, hosted gates,
and immutable evidence did not justify the mutable authority views which
claimed RGD-U10 complete and RGD-U11 ready. In particular:

- repository-controlled text was incorrectly described as a durable source of
  platform authority;
- hosted CI omitted the `architecture_docs` governance suite;
- the historical U9 archive did not contain a reproducible committed collector
  and its semantic raw copy was incomplete;
- the recorded U10 consumer executed a Winit render probe rather than the
  packaged `bin/desktop` product entry, and used fixed state paths; and
- release consumers pinned an older smoke helper, while filesystem error
  contract changes lacked an explicit migration record.

Commits `e670748`, `fac5e3c`, `835ff33`, and `8b263c4` repair those repository
contracts. They do not retroactively turn the historical hosted observations
into current completion evidence.

# Result

The reviewed delivery state is:

- **RGD-U8 reopened.** Workflow, policy-test, helper, and governance inputs
  changed after hosted run `30462379022`; that run remains historical evidence
  only.
- **RGD-U9 reopened.** The exact surviving four-file transport payload is now
  archived and verified, including all 75 JSONL records, but the historical
  collector was uncommitted and host-specific. Its `Redirect` verdict remains
  a historical baseline, not a current U9 completion usable by U11.
- **RGD-U10 reopened.** Run `30510353046` remains valid evidence for package
  construction, headless execution, and a Winit/render probe at `fafc949`, but
  it did not execute the formal packaged desktop entry from a genuinely
  isolated state root. Record `ecd70107315740eab6c580f18eca4dd0` is therefore
  corrected by this record rather than treated as current completion.
- **RGD-U11 blocked.** No evidence-ingest dispatch, approval, tag, environment
  action, or publication is admitted until refreshed U8, reproducible U9, and
  formal U10 evidence close in dependency order.

Platform credentials and environment approvals remain external, scoped,
revocable control-plane authority. Repository text can constrain execution but
cannot grant that authority to itself.

# Evidence

- Root architecture, artifact-policy, CI-policy, measurement-policy, and
  release-policy suites passed after the corrections. The focused root runs
  covered 39 governance/policy tests, 11 measurement-helper tests, 8 artifact
  package-policy tests, and 3 release-workflow policy tests.
- `nara_fs` filesystem-contract tests passed 22 tests with 3
  platform-specific skips; `nara_image` passed 63 tests; the reference-game
  desktop binary passed 13 tests.
- `cargo check --workspace --locked --all-targets` and reference-game
  `cargo check --all-features --all-targets` passed with one Cargo build job.
- The historical U9 verifier now pins the exact four logical files, source
  revision, collector state, and negative U9/U11 claims, and proves each
  semantic file equals its preserved UTF-8 transport original after newline
  normalization.
- The formal smoke helper is pinned at commit
  `835ff33fc4ca7e6293b59cc2642d66f8e239bb9a`, Git blob
  `c2d7b544efd5c472881943e311a32fd2baba5899`, and SHA-256
  `a42edd4bc6d65253cc54a290603c6e4bf0ec7f01182ecdd06782d14b87776586`.
  Draft and public release consumers require the formal desktop completion
  marker rather than the old probe marker.

# Follow-up

Run a fresh hosted U8 matrix at the corrected revision. Then commit and execute
a portable, parameterized U9 collector and preserve its raw replay inputs. Only
after those close may a fresh U10 candidate run execute the packaged desktop
entry through unique, path-independent state roots. Re-evaluate U11 only after
all three predecessor records cite the same admitted dependency chain.

# Citations

- `.github/workflows/ci.yml`
- `.github/workflows/reference-game-candidate.yml`
- `.github/workflows/reference-game-release.yml`
- `reference-game/tools/smoke_artifact.py`
- `docs/benchmarks/data/runs/v1/rgd-u9-b2ddb5b/`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md`
- `docs/architecture/adr/implementation-status.md`
- GitHub Actions runs `30462379022` and `30510353046`
- Commits `e670748`, `fac5e3c`, `835ff33`, and `8b263c4`

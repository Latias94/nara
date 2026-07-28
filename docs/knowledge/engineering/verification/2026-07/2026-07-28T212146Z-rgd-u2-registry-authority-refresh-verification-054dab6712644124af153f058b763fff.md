---
type: "Verification Evidence"
title: "RGD-U2 registry authority refresh verification"
description: "Refreshes frozen component behavior authority evidence after closing the public ECS resource and direct-App fault-reporting bypasses."
timestamp: 2026-07-28T21:21:46Z
record_id: "054dab6712644124af153f058b763fff"
tags: ["rgd-u2", "registry", "runtime", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "23196978608a8c257b6f1b7dca7858752e896c37"
verified_by: "Serial nextest, all-target checks, strict targeted Clippy, source boundary audit, and independent review"
supersedes: "84188e9196d242078d5e32c6368f7ca6"
---

# Verification

The refreshed RGD-U2 contract was verified against implementation commit
`b4d105cbf6312cb4006d8b06b0170f8cfdc1a8ec` and isolated-fixture lock refresh
`23196978608a8c257b6f1b7dca7858752e896c37`. The executable component registry is no longer a
public ECS resource, and both direct and managed execution now reject a changed registry authority
at schedule boundaries.

# Result

- `ComponentRegistry` remains usable as a standalone build/freeze/read value but no longer
  implements `Resource`; a compile-fail fixture freezes that public negative contract.
- An `App` owns the executable registry through a private resource. Schema plugins have one
  controlled registration operation, and runtime/Host consumers receive one immutable read view.
- Code-first and file-backed Apps freeze one exact snapshot and instance token. Replacement,
  removal/reinsertion, and same-snapshot rewrapping reject during build, finish, direct execution,
  custom schedules, exact stepping, or managed runtime safe points as applicable.
- Direct frame and custom-schedule failures are now observable through
  `AppRunError::DirectRuntime` and `AppScheduleRunError::Runtime`; a runner cannot suppress a sticky
  authority fault or replace the entire `App` instance unnoticed.
- `ProjectContentSnapshot` retains only its reviewed World-independent content surface. Its public
  wrapper, fields, methods, trait implementations, and loader return path are exact allowlists.
- All built-in schema owners, the project Host/Editor, the reference game, and clean-room consumers
  use the replacement API. Static search found no production direct `ComponentRegistry` resource
  access or managed-only validator symbol.
- Independent correctness, testing, performance, and simplicity review findings were resolved;
  no P0/P1 remained before the final serial verification.

# Evidence

- `cargo nextest run --locked -p nara --test schedule_extension_contract -E
  'test(renamed_root_fixture_observes_the_public_anchor_contract)' --test-threads=1`: 1 passed.
- `cargo nextest run --workspace --locked -E 'not binary(architecture_docs)'
  --test-threads=1`: 1,083 passed, 3 skipped. The documentation-test binary was deliberately
  excluded per the repository owner's direction; no green claim is made for that binary.
- `cargo check --workspace --locked --all-targets`: passed.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --all-features
  --test-threads=1`: 97 passed.
- `cargo check --manifest-path reference-game/Cargo.toml --locked --all-targets --all-features`:
  passed.
- Strict targeted Clippy passed for all changed engine crates and for root `nara` with all features
  and targets; only the explicitly named pre-existing lint classes were allowed.
- `cargo fmt --all -- --check`, reference-game format check, and `git diff --check`: passed.
- The complete workspace run exercised the `ComponentRegistry: !Resource` trybuild fixture,
  direct/managed authority regressions, project-content boundary guard, root Host/Editor paths, and
  clean-room locked consumers.

# Follow-up

1. Re-run the independent RGD-U7 Runtime decision, then the Host/combined compatibility review,
   against this refreshed executable authority before refreshing hosted or candidate evidence.
2. Preserve the private registry owner and direct/managed sticky-fault behavior through that review;
   future APIs may expose immutable facts, not public replacement authority.
3. Keep dependency correction deferred until the active plan's four evidence gates pass:
   production-edge classification, a hierarchy/2D propagation consumer slice, the
   `nara_reflect -> nara_asset` deletion test, and direct plus renamed-root consumer verification.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u2-publish-one-frozen-component-behavior-authority`
- `docs/architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/migrations/2026-07-engine-foundation.md#rgd-u2-1-private-executable-registry-authority`
- `AGENTS.md`
- Commit `b4d105cbf6312cb4006d8b06b0170f8cfdc1a8ec`
- Commit `23196978608a8c257b6f1b7dca7858752e896c37`

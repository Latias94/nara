---
type: "Verification Evidence"
title: "RGD-U11 schedule, registry, reference-game, and CI hardening"
description: "Verifies four local pre-publication corrections and records the exact authority and delivery evidence invalidated by their new revision."
timestamp: 2026-07-27T09:14:37Z
record_id: "3508e830c12045c1871db1855ae51a6e"
tags: ["rgd-u11", "schedule", "component-registry", "reference-game", "ci", "invalidation"]
status: "verified-local"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "a7599490ba1fe8e18d01ad172db61d426db99649"
verified_by: "codex-root"
---

# Verification

The reviewed commit sequence through
`a7599490ba1fe8e18d01ad172db61d426db99649` closes four local
pre-publication findings:

- `78300531c23317665da5b042e5ba4963107c3122` publishes
  `CoreStage::Cleanup` as the narrow frame-end schedule anchor, validates its
  automatic deferred insertion and final flush at seal, covers paused
  execution, and removes the reference-game Host parity probe's dependency on
  the private `FixedUpdateSet::Finalize`.
- `e67dce37a44e7b121fc66d2745e2e052a46c4813` makes registry-resource
  insert/discard tracking intrinsic to `ComponentRegistry`. A preinstalled
  registry and a remove/reinsert of the same object can no longer bypass the
  frozen runtime authority revision.
- `bb0ab56fcfa35a3d7cfff5345a5b958b66130c10` removes the Enemy prefab's
  implicit outer Player target. Enemy schema v1 migrates to v2 with a
  tombstoned target field, the committed v1/v2/v3 catalogs remain loadable,
  and runtime pursuit resolves the unique Player role inside the same scene
  instance.
- `a7599490ba1fe8e18d01ad172db61d426db99649` makes Hosted CI compile every
  root target, run the supported feature/example matrix, execute complete
  default and all-feature root/reference-game nextest suites, and compile/test
  the direct module consumer. Policy tests bind the exact commands.

# Evidence Invalidation

This local success does not preserve the prior delivery verdicts:

- The registry authority correction changes RGD-U2 evidence. RGD-U2 and the
  dependent RGD-U7 Runtime/Host decision review must be refreshed before a
  final Hosted gate can cite the new revision.
- The Rust, policy-test, and workflow changes reopen RGD-U8 Hosted
  certification.
- The reference-game gameplay, schema, startup content, and package bytes
  make the earlier RGD-U9 baseline and RGD-U10 standalone candidates
  historical evidence only.
- No protected Candidate, evidence-ingest, tag, draft, finalize, or Release
  stage is authorized. RGD-U12 remains ineligible.

# Result

- **Local correction status:** verified and committed.
- **Remaining U11 source gates:** optional-owner lineage, unforgeable
  persistence receipts, bounded terminal asset reload, and paused-input
  transition retention.
- **Delivery state:** awaiting the remaining corrections, refreshed U2/U7
  review, final-revision U8 Hosted CI, and new U9/U10 evidence in dependency
  order.
- **Non-claims:** this record does not complete U11, Hosted CI, a final
  candidate, a Publish decision, or any public release.

# Evidence

- Root CI policy: 26 passed.
- Schedule-extension contract: 7 passed.
- Image-limit and derive-fixture regressions: 5 passed.
- Reference-game public-surface and raw-App baselines: 2 passed.
- Reference-game default suite: 60 passed.
- Reference-game all-feature desktop suite under forced fallback: 96 passed.
- Root default workspace suite: 1,031 passed, 3 skipped.
- Root all-feature workspace suite: 1,264 passed, 4 skipped.
- Direct module-consumer suite: 1 passed.
- Architecture authority and ledger structure: 9 passed.
- Root and reference-game all-target checks, all 18 root feature/example
  checks, formatting checks, and `git diff --check` passed.
- Strict changed-scope Clippy passed for `nara_app`, `nara_reflect`, the root
  all-target/all-feature product, and the reference-game all-target/all-feature
  product with only the repository's documented unrelated baseline allowances.
- Independent final correctness review reported no remaining P0/P1 finding.

# Review Follow-ups

The following P2 items do not weaken the verified corrections: a migration
missing the removed Enemy target is classified as an invalid field rather than
a missing field; the feature-command list is duplicated between workflow and
policy; and the Linux X11 dependency profile is repeated in delivery
workflows. A full workspace dependency-direction allowlist also remains a
separate compatibility-policy follow-up.

# Follow-up

Resolve OQ-044 before implementing optional-owner lineage, then close the
persistence, asset-reload, and paused-input corrections. Refresh RGD-U2 and
RGD-U7 before requesting a separately authorized final RGD-U8 Hosted run; only
then regenerate the RGD-U9 baseline and RGD-U10 candidates.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u11-complete-pre-publication-successor-and-candidate-evidence`
- `docs/architecture/adr/0003-own-app-plugin-and-schedule-lifecycle.md`
- `docs/architecture/adr/0011-component-schema-ids-and-migrations.md`
- `docs/architecture/adr/0014-testing-ci-and-compatibility-policy.md`
- `docs/architecture/adr/0055-feature-matrix-boundary-checks-and-compatibility-fixtures.md`
- `docs/architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md`
- `.github/workflows/ci.yml`
- `crates/nara_app/src/lib.rs`
- `crates/nara_reflect/src/plugin.rs`
- `reference-game/src/components.rs`
- `tests/ci_policy.rs`
- Commit `a7599490ba1fe8e18d01ad172db61d426db99649`

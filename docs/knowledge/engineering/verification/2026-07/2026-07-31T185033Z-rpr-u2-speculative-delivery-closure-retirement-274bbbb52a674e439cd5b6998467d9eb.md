---
type: "Verification Evidence"
title: "RPR-U2 speculative delivery closure retirement"
description: "Retires the unused evidence and release supply chain while preserving bounded checkout-free candidate verification."
timestamp: 2026-07-31T18:50:33Z
record_id: "274bbbb52a674e439cd5b6998467d9eb"
tags: ["rpr-u2", "delivery", "simplification", "candidate"]
status: "completed"
producer_id: "codex-root"
related_plan: "docs/plans/2026-08-01-001-refactor-product-readiness-delivery-reset-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "b41e30f59433c8d992592b12b43d31eaf14f6733"
verified_by: "focused nextest 22 passed; actionlint; focused strict clippy"
---

# Verification

- `cargo nextest run --locked -p nara --features runtime-core,serde --test ci_policy --test
  measurement_policy --test artifact_package_policy --test architecture_docs --test-threads=1`
  passed all 22 tests.
- `actionlint .github/workflows/ci.yml .github/workflows/reference-game-candidate.yml` passed.
- Focused strict Clippy passed for `ci_policy` and `measurement_policy` after allowing only the
  repository's pre-existing `result_large_err`, `collapsible_if`, `derivable_impls`,
  `double_must_use`, and `too_many_arguments` findings in unchanged dependencies.
- `cargo fmt --all` and `git diff --check` passed. Repository search found no active consumer of the
  deleted collector, ingest, approval, or release closure outside this plan's retirement inventory
  and ADR 0099's explicit prohibition.
- Independent correctness and testing reviews found no P0. Their two P1 findings were fixed before
  the commit: the no-checkout consumer now rejects source acquisition or rebuilding, and the exact
  U9 protocol, run manifest, and raw JSONL bytes are bound by code-owned BLAKE3 digests.

# Result

RPR-U2 is complete at `b41e30f59433c8d992592b12b43d31eaf14f6733`.

The unit removed 19,610 lines of delivery-only machinery: the evidence-ingest and release
workflows, six specialized Python collectors/verifiers, approval and normalized-envelope schemas,
release fixtures, generalized evidence helpers, and their policy interpreters. The historical U9
`Redirect` remains reproducible through its committed metric catalog, exact raw population, and
small Rust decision oracle; the original collector remains available at its evidence-bound Git
revision instead of being maintained on the active product path.

The retained candidate workflow still builds real headless and desktop products on Windows and
Linux, packages them, and executes both from a no-checkout consumer. Off-main dispatches and reruns
now produce explicit failed jobs. Only the candidate packaging and smoke scripts remain active for
this product action.

# Evidence

- Implementation commit: `b41e30f59433c8d992592b12b43d31eaf14f6733`.
- Retained workflows: `.github/workflows/ci.yml` and
  `.github/workflows/reference-game-candidate.yml`.
- Retained product tools: `reference-game/tools/package.py` and
  `reference-game/tools/smoke_artifact.py`.
- Compact historical decision oracle: `tests/measurement_policy.rs`.
- Candidate semantic policy and black-box package tests: `tests/ci_policy.rs` and
  `tests/artifact_package_policy.rs`.
- Decision boundary: `docs/architecture/adr/0099-decision-local-product-evidence-and-publication-admission.md`.

# Follow-up

Execute RPR-U3. Add one pure replayable `ProductRecipe`, one typed schema-owning contribution, and
small ordinary headless/desktop run facades while keeping raw Host lifecycle assembly on the
advanced embedding surface. Do not add another evidence, package-manager, provider, or release
framework.

# Citations

- `docs/plans/2026-08-01-001-refactor-product-readiness-delivery-reset-plan.md`
- `docs/benchmarks/reference-game-first-playable-baseline.md`
- `docs/architecture/adr/implementation-status.md`

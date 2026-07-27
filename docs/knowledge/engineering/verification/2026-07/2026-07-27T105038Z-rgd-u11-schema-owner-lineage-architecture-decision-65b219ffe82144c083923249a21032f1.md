---
type: "Verification Evidence"
title: "RGD-U11 schema owner-lineage architecture decision"
description: "Records independent review and governance verification for Accepted ADR 0098 while leaving implementation and delivery evidence open."
timestamp: 2026-07-27T10:50:38Z
record_id: "65b219ffe82144c083923249a21032f1"
tags: ["rgd-u11", "adr-0098", "schema-lineage", "decision"]
status: "verified"
producer_id: "codex-root"
run_id: "019f5096-ee46-7571-a208-be491cc72786"
source_session: "019f5096-ee46-7571-a208-be491cc72786"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5baaa6ac5b8ce0cef00db2c276c625da2f2e504f"
verified_by: "independent-correctness-review"
---

# Verification

Accepted ADR 0098 was checked against the RGD-U11 optional-owner lineage defect, Accepted ADRs
0081 and 0095, the current `ComponentRegistry` and provider shape, and the strict ADR 0090
fail-closed boundary.

# Result

The architecture decision is accepted at commit
`5baaa6ac5b8ce0cef00db2c276c625da2f2e504f`. Its implementation remains `not-started`: no owner
record, owner-aware composition, typed composition/executable fingerprint, provider-candidate
merge, or reference-product tracer is claimed by this record.

# Evidence

- The first independent correctness review found three P1 gaps: aggregate mutation after a failed
  provider callback, inability to reserve claims from a known inactive owner, and conflated
  semantic/executable identity. It also found an ambiguous Direct App versus file-backed pointer
  identity claim.
- The revised decision requires private owner-local registry candidates, pure-by-contract bounded
  head sources for every trusted known definition, four distinct fingerprint domains, stable
  owner-contribution receipts, and pointer identity only within one managed Host.
- The second independent correctness review reported no remaining P0/P1. Its four P2 clarifications
  were incorporated: publication binds owner receipts with executable equivalence, failure
  atomicity is tested through observable structure rather than memory bytes, exact definitions
  deduplicate before distinct-owner rejection, and the trusted head loader is not described as a
  sandbox.
- The first focused governance run failed only because the four alternatives lacked the repository
  `Option A` through `Option D` heading convention. After correcting those headings,
  `cargo nextest run --locked -p nara --test architecture_docs --test-threads=1` passed 9 of 9
  tests in nextest run `9443ef59-6a16-4d66-8795-b4d54ae54871`.
- `git diff --check` passed before the decision commit.

# Follow-up

Implement the ADR 0098 tracer test-first. It must prove `A+B -> A -> A+B`, real owner-local
deletion/migration, inactive-owner claim reservation, callback error/unwind atomicity, stable
definition deduplication, distinct semantic and executable fingerprints, complete Runtime
bindings, and unchanged strict Scene/Prefab rejection for omitted owners. Then update the
implementation ledger from current repository evidence; this decision record alone cannot close
the RGD-U11 source gate.

# Citations

- `docs/architecture/adr/0098-schema-owner-lineage-and-active-runtime-composition.md`
- `docs/architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md`
- `docs/architecture/adr/0095-plugin-owned-specialized-domains-and-project-configuration.md`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md`
- Commit `5baaa6ac5b8ce0cef00db2c276c625da2f2e504f`

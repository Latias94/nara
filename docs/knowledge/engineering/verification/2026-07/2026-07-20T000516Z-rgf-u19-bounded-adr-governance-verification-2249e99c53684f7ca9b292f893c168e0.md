---
type: "Verification Evidence"
title: "RGF-U19 bounded ADR governance verification"
description: "Commit 347015e derives governance membership dynamically and rejects bounded authority, relationship, evidence, and draft-role drift."
timestamp: 2026-07-20T00:05:16Z
record_id: "2249e99c53684f7ca9b292f893c168e0"
tags: ["rgf-u19", "verification", "architecture", "adr", "governance"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "347015e8d9fd5d529b9dd5482ceaa02086f4615a"
verified_by: "focused nextest,workspace check,clippy,fmt,diff review"
---

# Verification

RGF-U19 was reviewed at implementation commit `347015e` on
`refactor/engine-foundation-contracts`. The unit replaces row-presence-only checks with one
test-only governance snapshot while keeping unrelated historical prose outside the gate.

# Result

RGF-U19 passed its implementation and review gates.

- Repository-derived membership is exactly 79 ADR files, 79 catalogue entries, and 79 ledger
  rows. No current count is embedded in the test.
- Decision and implementation vocabularies are independent; `Trial` is rejected, supersession
  states stay aligned, and Accepted non-implemented rows retain a gap and admission trigger.
- The sole active plan and its predecessor retain reciprocal supersession and sole authority
  pointers. RGF-touched ADR relations are reciprocal across Markdown line wrapping.
- Changed ledger anchors resolve to repository paths and symbols. The check exposed and repaired
  three stale `ImageImportBudgetHost` anchors after that type moved from `limits.rs` to
  `budget.rs`.
- ADRs 0045 and 0081 expose the same non-empty canonical-v1 capability vocabulary. Proposed ADRs
  may retain bounded implementation evidence but cannot be cited as current product or foundation
  authority.
- Every top-level non-normative architecture harness, appendix, guide, draft, or matrix is indexed.
  The render extension harness now names the RGF closure architecture handoff as its rebaseline
  activation owner.

# Evidence

- `cargo nextest run --locked -p nara --test architecture_docs --test-threads=1
  --no-fail-fast` -> 7 passed.
- Negative snapshot mutations reject missing, duplicate, extra, stale, or misdirected membership;
  unknown states; incomplete evidence; broken plan/ADR reciprocity; missing anchors; divergent
  capabilities; unqualified Proposed authority; and unclassified or unowned drafts.
- `cargo check --workspace --locked` passed.
- `cargo clippy -p nara --test architecture_docs --locked -- -D warnings` passed with only the
  repository's documented pre-existing allowances for `nara_app`, `nara_asset`, and
  `nara_reflect`.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Final review tightened ledger link-target identity, superseded decision/implementation
  alignment, and multiline implemented-slice parsing before the last focused rerun.

# Scope Boundary

This gate does not implement a generic atomic-admission parser, crawl unrelated historical prose,
or decide Proposed ADRs 0082/0084. U23 retains the independent runtime/Host decision matrix, and
U7 retains hosted CI and release-candidate verification.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u19-enforce-bounded-adr-governance-validation`
- `tests/architecture_docs.rs`
- `docs/architecture/README.md`
- `docs/architecture/adr/README.md`
- `docs/architecture/adr/implementation-status.md`
- Commit `347015e`

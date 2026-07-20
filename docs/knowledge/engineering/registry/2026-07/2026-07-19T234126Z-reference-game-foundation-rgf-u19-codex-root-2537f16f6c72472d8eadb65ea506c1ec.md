---
type: "Work Registration"
title: "Reference-game-driven foundation refactor: RGF-U19"
description: "Activate bounded ADR catalogue, ledger, relationship, authority, and draft-governance validation."
timestamp: 2026-07-19T23:41:26Z
record_id: "2537f16f6c72472d8eadb65ea506c1ec"
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f5f7724"
registration_id: "reference-game-foundation-rgf-u19-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
---

# Scope

RGF-U19 dynamic ADR/catalogue/ledger set equality, status separation, active-plan reciprocity,
changed-row anchors, canonical capability agreement, Proposed-authority exclusion, and top-level
architecture document classification.

# Current Claim

The unit is active from code/document revision `f5f7724`. The current architecture test passes but
cannot detect a missing ledger row, extra catalogue entry, or several other plan-named governance
failures.

# Latest Links


# Handoff

Build a structured test-only governance snapshot and negative mutations without hard-coding the
current ADR count or crawling unrelated historical prose. Reconcile only defects exposed by the
bounded U19 contract.

# Citations

- docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u19-enforce-bounded-adr-governance-validation
- tests/architecture_docs.rs

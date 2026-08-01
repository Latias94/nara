---
type: "Verification Evidence"
title: "RGS-U1 spatial authority activation"
description: "Verifies activation of the focused runtime hierarchy and completed 2D transform product slice without reviving retired product-evidence infrastructure."
timestamp: 2026-08-01T07:20:32Z
record_id: "c4ec02d7fb954a00b5ee25e148c7e3c4"
tags: ["rgs-u1", "spatial-authority", "hierarchy", "2d-transform"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4c27c7edf6558d55c8fce07db3fcfb410cb0e954"
verified_by: "codex-root and direct product, scope, coherence, feasibility, and adversarial reviewers"
---

# Verification

- Activated `docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md`
  as the only implementation-ready plan and marked its RPR predecessor as superseded with a
  reciprocal pointer.
- Added Accepted ADR 0100 for only the dedicated non-linked runtime hierarchy ownership boundary,
  one-way persistent lowering, distinct local/global 2D authority, and completed-consumer
  invariant. ADR 0085 remains Proposed for persistent order, reparenting, visibility, prefab, UI,
  physics, and 3D semantics.
- Updated the architecture map, ADR catalogue, implementation ledger, foundation ownership map,
  active work registration, and derived engineering-memory rollups.
- Used direct Codex product, scope, coherence, feasibility, and adversarial reviewers. Their
  accepted corrections removed an unneeded general runtime reparent API and premature affected-
  closure optimization, documented the raw Bevy mutation limit and camera affine boundary, and
  kept Retry reconstruction and runtime-derived ownership private to the reference game instead of
  accepting an engine scene-session API. The final scope pass also restored the foundation diagram
  to the actual current crate graph and left `nara_hierarchy` visibly marked as a not-started target.

# Result

RGS-U1 is active and documentation-complete. ADR 0100 is intentionally `accepted/not-started`;
runtime implementation begins in RGS-U2. The recorded RPR-U5 `Redirect` remains immutable, and no
collector, evidence transport, approval protocol, release workflow, general hierarchy provider,
or 3D contract was reintroduced.

# Evidence

- Direct authority audit: exactly one active plan, reciprocal plan supersession, one ADR 0100
  catalogue entry, one ledger row, contiguous R1-R23 and AE1-AE16 identifiers, and unchanged
  historical RPR-U5 evidence.
- Engineering-memory validation: passed; existing legacy-record warnings remain informational.
- Engineering-memory render and render-check: passed for `current-state.md` and `log.md`.
- Authority link targets: eight required plan, ADR, and verification paths exist.
- Whitespace checks: tracked `git diff --check` passed; all four new records passed explicit
  untracked-file whitespace checks.
- No Cargo command and no `tests/architecture_docs.rs` run were required for this documentation-
  only unit.

# Follow-up

Implement RGS-U2 from the dedicated hierarchy ownership boundary. Characterize existing scene
replacement, lifecycle-hook, and UI behavior before moving code. Preserve the plan's explicit
limits: no general runtime reparent API, no hierarchy-owned lifetime, no inferred component-type
sweep on Retry, no engine scene-session or candidate-initialization port, and no persistent ordering
or 3D expansion.

# Citations

- `docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md`
- `docs/architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md`
- `docs/architecture/adr/0085-hierarchy-transform-and-visibility-semantics.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/knowledge/engineering/registry/2026-08/2026-08-01T072620Z-engine-foundation-contract-completion-codex-root-510bd4496c5b48fb9b411b5d06b10ea2.md`

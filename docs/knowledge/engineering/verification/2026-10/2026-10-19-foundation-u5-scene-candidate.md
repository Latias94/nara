---
type: Verification Evidence
title: Foundation U5 associated Scene materialization candidates
description: Move-only expanded input binds immutable Scene data and provenance before allocation or replacement.
timestamp: 2026-10-19T09:04:00Z
record_id: 410d3e36bd574f768bd76f096a4f4788
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract

`ExpandedSceneCandidate` retains an immutable document and its source map together. Prefab
expansion is the only constructor accepting projected provenance. The ordinary plain constructor
builds local provenance itself; unresolved Prefabs still reject during admission. Public consumers
cannot replace either half or clone a candidate out of a retained-source borrow.

Spawn and product replacement accept the associated candidate. Provenance admission precedes
entity allocation and product configuration. Prepared component publication installs admitted
`SceneEntitySource` facts before hierarchy and persistent identity retirement; there is no second
fallible provenance pass after replacement has committed.

Project content and startup ownership retain an `Arc` to the candidate while preserving sharing of
the immutable document allocation. Scene retention accounts for that additional owner. The public
view lends the candidate without exposing an independently retainable document allocation.

# Serial Verification

Windows, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, repository target and lockfile:

- Scene all-features run `e12adcb9-b4ff-4522-875b-17137c8d0c93`: 194 passed.
- Focused candidate regressions `3ffd0c2c-d562-4e98-98b9-b0fc99eb9587`: four passed, covering
  malformed provenance before allocation/replacement, rejection by a source-component observer
  without retiring old identity/provenance, and unresolved plain-Prefab rejection.
- Root library plus `project_content_boot` and `project_runtime_boot`, with
  `--no-default-features --features tooling,runtime-2d,serde`: 138 passed. This includes retained
  document lifetime/sharing, prepared start failures, immutable content, startup materialization
  and ordinary saved-source runtime boot.
- Root public boundary run `ce1fe22a-6ede-46b8-869c-0c94cf18824b`: all four selected tests passed.
  The Scene trybuild trajectory admits the advanced surface and rejects all five ordinary/private
  construction routes, including construction of the candidate's private fields. The other three
  cases exercise the root and renamed-root custom-schedule consumers for U7.

Two initial new assertions incorrectly assumed that `World::new()` contained zero entities and
that every entity carried Scene source data. They were corrected to compare the preexisting entity
set and inspect only admitted source-bearing entities. Those failures are test-fixture corrections,
not evidence of an additional production defect.

An independent adversarial review found no actionable public route to forge an associated pair.
Structural validation alone cannot prove an arbitrary externally substituted same-key map; the
private expansion constructor and move-only public value prevent that substitution.

# Remaining Evidence

Candidate rustdoc examples, independent Scene module consumers and Reference Game expansion
consumers still need their post-migration runs. The final
workspace gate also remains pending. This record does not yet close U5.

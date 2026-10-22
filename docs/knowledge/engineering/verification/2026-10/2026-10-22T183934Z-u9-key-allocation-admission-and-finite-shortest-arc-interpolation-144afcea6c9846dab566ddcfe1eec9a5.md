---
type: "Verification Evidence"
title: "U9 key allocation admission and finite shortest-arc interpolation"
description: "Original allocation and numeric regressions reproduced; shared key visitor and endpoint-oriented interpolation repaired."
timestamp: 2026-10-22T18:39:34Z
record_id: "144afcea6c9846dab566ddcfe1eec9a5"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "feb273d"
---

# Verification

Windows 11, Rust 1.99. Cargo ran serially with one build job and reused the root target.
These are U9 results before the U4 spatial snapshot replacement.

# Result

Borrowed map keys pass both per-string and aggregate budgets before Nara allocates the duplicate
detection copy. Owned parser strings transfer without another copy and are charged once. Syntax
adapter allocations remain outside this shared visitor's guarantee, as clarified in ADR 0049.

Rotation interpolation preserves exact endpoints and equal rotations. Other endpoints are reduced
through the same f32 `sin_cos` orientation used by their matrices, then interpolated on the shortest
arc in f64. This avoids both overflowing subtraction and inaccurate huge-angle remainder reduction.

# Evidence

- Red allocation run `8264aa02-8e6c-47a9-a294-fe8b3a37fb3a` observes a 1,048,576-byte allocation
  before rejecting the borrowed 1 MiB key on the original visitor.
- Green Core full serde cohort: 16/16 pass, final run `9a06563f-fe0d-482f-bbbd-af8ddbbf6726`.
  The allocation observer starts after input/parser construction and checks both individual and
  aggregate rejection before large copies, plus accepted-copy and escaped/owned/duplicate paths.
- Red numerical run `47e2b06e-e639-4e20-b7fc-ccc0c16c0471`: finite extreme endpoints produced a
  non-finite matrix at alpha 0.25. An initial wide subtraction fix passed finite tests, but independent
  review found an inaccurate extreme-angle shortest arc. The adopted trig reduction resolves that
  finding; the reviewer confirmed no remaining blocker without running Cargo.
- Initial Transform full serde cohort passed 19/19 (5 ignored measurement tests), run
  `7b922993-11eb-48db-a222-eef054b36a5d`. After the final trig correction, both numerical tests
  pass in `94d81111-adbb-4f89-a64a-3379f7e04c08`; this adds direction-bisector assertions for
  extreme endpoints rather than checking finiteness alone.
- Core serde all-target strict Clippy passes. Transform Clippy reached existing dependency lint
  failures in unedited Asset artifact/reload/storage code (`chunks_exact_to_as_chunks`,
  `double_must_use`, `derivable_impls`, `too_many_arguments`), so it is not reported as passing.
- Independent review found no key-visitor or allocator-delegation blocker. The thread-local
  allocation observer delegates original layouts/pointers to System and has no TLS destructor.

# Follow-up

U4 must preserve the corrected interpolation function and rerun its numerical tests after its
history/projection changes. U3-U8/U10-U11 remain active; this evidence does not close the Goal.

# Citations

- [Plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
- [Shared visitor](../../../../../crates/nara_core/src/serde_shape.rs)
- [Allocation regression](../../../../../crates/nara_core/tests/shape_allocations.rs)
- [Interpolation](../../../../../crates/nara_transform/src/presentation.rs)
- [ADR 0049](../../../../architecture/adr/0049-untrusted-project-input-and-parse-budget-policy.md)

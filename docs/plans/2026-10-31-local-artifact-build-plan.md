# Local Artifact Build And Recipe Authority Plan

Date: 2026-10-31. Status: in progress; full R6b outcome remains unqualified.
Baseline: `96f4519`. Parent: [foundation R6/R7](2026-10-23-foundation-architecture-rebaseline-plan.md#r6-resource-facets-durable-continuity-and-local-artifact-store).
Design: [local artifact authority](../architecture/local-artifact-build-design.md).
Imported finding: CP5.

## Complete Outcome

The existing independent compound importer and a real target-sensitive consumer execute two
clean builds and verified reuse. Auxiliary/settings/target and actual processed-input changes
produce current complete groups. Cache deletion preserves authored Product identity. Interrupted
publication/reopen never exposes a half group or marks a stale predecessor current. Admitted
Runtime packs and escaped resources retain their original data independently of cache lifetime.

## Ordered Work

1. Reconcile recipe/content/group semantics and the concrete execution seam against pinned
   Bevy/Godot. Update affected Accepted ADRs before replacement; keep the rest of Proposed0087
   outside the bounded adopted slice.
2. Replace source import provenance with actual `ImportRecipe`/`ImportRecipeDigest`. Remove
   synthetic artifact records/paths and unused policy fields. Migrate real Image/state/reload/
   render/prepared-pixel consumers without compatibility aliases and qualify their workflows.
3. Implement finite executing recipe capture: explicit selected versioned definitions, effective
   normalized settings, actual raw/presence and processed-input observations, consumed target
   facts and independent output content digests. Preserve source-only settings ergonomics.
4. Implement local generated recipe lookup, complete immutable group manifests and CAS using
   existing filesystem publication/cleanup/durability authority. Validate hits against current
   inputs and complete output membership; distinguish hit, miss, invalid, failed and stale.
5. Run actual independent compound and target-sensitive build/Runtime consumers, including
   optional/deleted/changed inputs, member removal, same-recipe nondeterminism, every publication
   interruption boundary, fresh reopen, original Runtime custody and exact allocation limits.
6. Independently review; run serial owning/consumer nextest, affected minimal/all-target/native
   checks as needed, record actual scope and remaining gaps, commit coherent qualified units
   and locally fast-forward main. Keep the full foundation Goal active.

## Gates

The source provenance correction is a prerequisite, not evidence of durable reuse. A manifest
constructor test is not build execution. A hash of an unused target label is not a target-sensitive
cook. A source hash is not a processed-input stamp. Per-member publication is not whole-group
success, and sync receipts must not be widened into unsupported crash durability.

Reuse root target, CARGO_BUILD_JOBS=1 and one Cargo at a time. Preserve vendor and all concurrent
work; reference checkouts are read-only. No push/PR, migration guide or legacy-format reader.

## Qualified Prerequisites

The source recipe replacement and pure CPU Image cook are implemented in the
[recorded scope](../knowledge/engineering/verification/2026-10/2026-10-30T201802Z-source-import-recipes-and-target-sensitive-cpu-image-cook-prerequisite-qualification-462c0179d20b44fd8545d4c135e814f9.md).
Owner328, downstream219, Root1039, independent22 and two selected nativeDX12 trajectories pass,
with minimal/Clippy/format and type-isolation doctest evidence. No existing exact budget threshold
was widened. Continue steps3-5 through actual tracked compound/processed Image builds and durable
publication; this checkpoint does not close the complete outcome or full foundation Goal.

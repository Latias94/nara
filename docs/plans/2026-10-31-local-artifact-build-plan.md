# Local Artifact Build And Recipe Authority Plan

Date: 2026-10-31. Status: finite local-build outcome qualified; target executable/package delivery remains separate.
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
was widened. This prerequisite checkpoint does not itself close the complete outcome or full
foundation Goal.

## Qualified Local Build Outcome

The [complete local-build qualification](../knowledge/engineering/verification/2026-10/2026-10-30T231610Z-tracked-local-artifact-builds-and-complete-group-publication-qualification-d3fcd08e15e04d45812ae09d7c81e944.md)
records typed tracked execution and local complete-group reuse/publication under ADR 0103's
bounded refinement. Actual compound and processed Image consumers cover the complete outcome
above, including fresh-process reuse, interruptions and original Runtime custody. Root/Asset1252,
final independent57, store20, minimal features and workspace/strict-consumer Clippy pass within
their recorded source scope. No existing threshold was widened. General graph scheduling,
target executable selection and packaged Player delivery remain separate; the full foundation
Goal stays active.

## Dependent Source Refresh Successor (November 13)

The foundation R6/C6 successor connects actual successful source-build observations to live
typed CPU consumers. The qualified store remains the immutable complete-group owner; no watcher,
universal asset scheduler, second Runtime or alternate command/task queue is introduced. Editor
UI and physical mixed-DPI work remain paused. Only precise local commits are in scope.

The complete independent TextBundle trajectory must change required `suffix.txt` and optional
`build-suffix.txt` through existing semantic source-change input, observe typed output without
an explicit rebuild or manual graph edit, and retain stable Product identity. Cover input
create/change/delete, dependency A-to-B replacement, first-load failure followed by input creation,
warm group reuse, failure preserving last-good, superseded completion and no unrelated
importer/decoder execution. Escaped immutable Runtime bases keep their original bytes.

Initial source inspection finds that `build_source` records successful present/absent input
observations but discards attempted observations on failure. `AssetSourcePlugin` currently
routes a concrete Image consumer, and its historical dependency graph accepts source UUID
relations rather than raw auxiliary paths. The current Source/Processed collections are immutable
Scene-content owners; republishing a build group alone cannot mutate them. The live receiving
owner and its typed publication boundary must therefore be selected against this complete
consumer before a reusable public refresh API is frozen.

One prerequisite applies whichever receiving adapter is selected: a failed tracked read cannot
produce a successful closed recipe if trusted domain code ignores its returned error. Such a
path has no admitted present/absent snapshot. The original read error remains primary; retain
the attempted canonical path for retry observation, separately from any last-good group.
Establish a real public build regression before changing this boundary. Do not downgrade the
complete successor to a graph helper or failure-observation constructor.

### Selected Receiving Owner

Extend the existing `AssetSourcePlugin` / Tasks / `Assets<T>` execution path. An explicitly
selected root source-build adapter executes the actual domain and codec against the original
store. Keep typed registration and request generations in Asset; keep Source Scene collections
and processed Runtime bases under their existing immutable publication owners. This is the
selected design direction, not an implemented automatic-refresh API.

A shared typed wrapper must retain the product, verified encoded group and original decoded
retention reservation together. Expose a borrowed product and a cloneable complete owner;
allowing a bare product `Arc` to escape would release its budget before its data. A Product
cannot simultaneously claim the ordinary value type and the wrapped type in `AssetServer`.

Canonical raw-path subscriptions need a complete last-successful set and a separate bounded
latest-failed-attempt retry set. Auxiliary paths do not require artificial source UUIDs. Only
current typed adoption may replace the successful set; a stale task updates neither set.
Warm reuse restores the actual verified inputs plus primary/metadata selection facts without
executing the domain. Failed execution keeps last-good and may replace its retry set, while
admission rejection without execution cannot erase recovery observations. Before adoption,
revalidate new inputs that were not subscribed when the task began, including a missing input
created during the attempt. Successful A-to-B replacement removes obsolete A subscriptions.

Reconcile ADRs 0007, 0037 and 0103 when implementing this boundary. Reject a parallel live
`SourceBuildHost` value dictionary, publishing subscriptions from artifact-store success alone,
or widening Image-specific recovery into a universal protocol. The pinned Bevy processor
(`03ed5a27c92715c8fa2188f31f04e454ec39942f`) supports reverse-edge replacement and missing-path
retention as reference mechanisms; its failed-processing state does not itself prove Nara's
separate last-good/retry contract.

### Ignored Tracked Read Failure Qualification

The original [build executor](../../src/asset_build/execute.rs) now retains the first tracked
read failure outside the domain's generic return value and rejects before encoding or
publication. The independent TextBundle regression first fails against the previous executor
(run `44cccf8f-7a22-4e96-a56a-828e3e14c43e`). It then verifies the exact source-byte budget
error, zero publication-callback calls, and a normal complete rebuild with the actual input
digest after correcting the dependency.

On Windows x64 and Rust 1.95.0, the final locked/offline release `durable_build` suite passes
20/20 with no skips (run `bcc86bb7-900b-4b5a-a7b5-abebf348b483`). It retains optional
absence/presence/deletion, required dependencies, warm and fresh-process reuse, complete-group
interruption, original Runtime custody and publication-uncertainty controls. Root and independent
workspace formatting checks pass. The root production-library correctness Clippy gate passes
with `--no-default-features --features project-workflow,tooling`; existing general Clippy
warnings remain. Local document links and whitespace checks pass. A bounded independent
correctness review finds no P1/P2 issue in this change. All Cargo runs reuse the root target
with one build job and no overlap.

This prerequisite adds no retry-observation store or automatic typed source-refresh path.
The selected receiving owner and complete successor workflow above remain to be implemented.

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

### Dependent Refresh Implementation And Review

The successor candidate extends original Asset source resolution, generation-bound requests,
Tasks phases and typed `Assets<T>` adoption. It adds no parallel live value store, watcher or
task queue. The root adapter selects one compiled build family and retains its original source
capability, store and immutable output owner; the pure domain/codec facets remain unchanged.
The [public workflow](../guides/project-asset-domains.md#live-typed-source-refresh) is exercised
by the independent TextBundle consumer, with separate owner regressions for exact budgets,
principal removal and same-generation state/slot mutation.

The first affected Asset/Image run passes 319/319 with one existing ignored case
(`nara-dependent-refresh-asset-image-first-20261113.log`). The first independent live-refresh
and durable-build run passes 24/24 with no skips (`14e10791-b466-4658-bde8-cffde55b804d`). These
are intermediate source scopes, not final qualification after review corrections.

Independent review identifies two lower routing defects. The generic queue-capacity preflight
initially skipped deletion observations for known published sources. A loaded-value regression
fails against it with the deleted source still usable (run
`8c05ea80-d2ec-474b-b378-5ff9ebc37656`). The resolver now observes invalidation and supersedes
known attempts even when cleanup cannot be queued, without claiming a fresh unknown ID or
entering Loading. Type conflicts still reject before these mutations.

The original watcher also normalizes metadata sidecar notifications to a principal path. Exact
raw subscriptions initially missed that provenance and could notify an unrelated source reader
(red run `7ca8270b-4e66-46ec-8806-aceeb232e931`). Semantic changes now retain independent raw
source/metadata origin flags through coalescing; the resolver queries only the represented raw
inputs. Sidecar removal preserves its existing principal invalidation semantics. New watcher
translator-to-resolver consumers test both Published and Retry sets and keep Image's original
principal behavior. The initial failing watcher assertions also exposed a test cleanup abort;
the fixture must retire its original Runtime before resuming any assertion panic.

Root review additionally requires that a partial observation followed by global work/retained
admission refusal cannot erase a previous retry set. Dedicated actual worker regressions hold
the original store's allowance after selection reads or before the first auxiliary trace entry;
they must prove refusal without autonomous retry and recovery from a later input-only event.
Final qualification below must supersede the intermediate results and include all corrections.

Those two regressions fail against the initial adopter with the real missing `c.txt` retry
subscription removed (`c0644510-6c54-44c4-9ed4-36ad8dda667b`: 13/15 pass, two fail). The corrected
adopter preserves retry on global allocation/count refusal and checks that actual replay/domain
input execution began before replacing a failed-attempt set. The producer retains failure
classification and exact attempted paths; a nonempty selection prefix is not treated as complete
execution. Failed I/O/source-byte reads alone may be re-probed under their original bound;
persistent refusal does not create an autonomous retry loop.

The shared request also binds its original generation resource at issuance. A replacement owner
cannot revive an old completion by recreating the numeric counter. Different source kinds may
share a product type with independent authority and per-ID claims; the initial global type
exclusivity was unnecessary. Root and Image controls exercise old success/failure/removal and
new-owner recovery. Successful metadata adoption acknowledges its exact dirty observation.
The independent successor also checks a successful new-input decode whose only change event
was consumed before that path was subscribed.

Final lower Asset, Watch and Image qualification passes 366/366 with one existing measurement
case skipped (`e6cc3811-0d80-412e-8f8e-3af3b065c7e7`). It includes the complete Image scope/recovery
suite, original and new issuer rejection controls, raw-path subscription limits/refunds, source
notification coalescing and actual watcher-translation-to-resolver consumers. Two fixture
corrections retain their original assertions: Image request setup uses public typed publication
instead of a crate-private state setter, and the de-duplication fixture subscribes to the actual
sidecar raw path rather than its normalized principal. Final bounded independent review reports
no remaining P1/P2 in the corrected boundaries. Root/public qualification remains separate below.

### Final Dependent Refresh Qualification

All commands use Rust 1.95.0 on Windows x64, `--locked --offline --release`, one build job and
the original root target. No concurrent Cargo, clean, compiler/dependency upgrade or remote
publication is part of this unit.

| Executed scope | Actual result |
| --- | --- |
| Asset, Watch and Image library tests, all features | 366/366; one existing task-facility measurement ignored. Run `e6cc3811-0d80-412e-8f8e-3af3b065c7e7`. |
| Root `asset_build`, `runtime_content` and `cooked_content` tests with `project-workflow,tooling` | 111/111; 1,326 unrelated tests filtered. Run `b24920b3-e510-4d80-b0a7-9ca24098effd`. |
| Complete independently locked asset-domain consumer package | 95/95 with no skips. Run `7266af8c-d577-4980-a191-c25e11ee6473`. Includes six live-refresh trajectories and original source/processed/retained-base/durable consumers. |

The root regressions prove both repaired budget-refusal paths, original generation-owner
replacement, same-generation slot/state mutation, principal removal and original decoded
custody. The independent consumer edits actual required/optional inputs, replaces A with B,
retains last-good while retrying a newly missing input, restores subscriptions on warm reuse,
and rejects a superseded successful decode. Both failed-input creation and successful-input
change races consume their sole early source event before subscription and still reach current
content through the original queue. Metadata-only warm adoption clears the original dirty
observation without executing the build. Original and successor groups feed actual retained
Runtime delivery; escaped values and bases keep their own text after source/cache removal.

These are source-authoring/development typed CPU workflows and bounded storage/authority
checks. The watcher tests exercise real translation and existing runtime watcher ownership;
they do not qualify every native filesystem or a new watching backend. No GPU/native Editor,
physical mixed-DPI, numeric performance budget, general processed-input scheduler, streaming,
automatic immutable Scene/base replacement or full R6/R11/R12 completion is claimed. The
complete foundation Goal remains active. Final static checks and local commits follow this
functional qualification; documentation closure does not require rerunning these tests.

The locked/offline release workspace all-target/all-feature check passes, as do the independent
asset-domain consumer's all-target Clippy gate with `-D warnings` and the owning Asset/Watch/Image/
root all-target/all-feature gate with `-D clippy::correctness`. Existing general root and
dependency warnings remain. Two newly added test-only Clippy suggestions are simplified without
changing assertions, then the lower all-target/all-feature correctness gate passes again.
Pure Asset, serde-only Asset, minimal root runtime-core and portable `wasm32-unknown-unknown`
runtime-core production-library checks pass; Wasm is compile-only. These checks make no Rust
1.100 or foreign-native execution claim. Root and independent formatting, local document links
and final whitespace/staged-scope checks complete delivery. No new dependency or lockfile
change belongs to this unit; pre-existing UI/platform working-tree edits remain separate.

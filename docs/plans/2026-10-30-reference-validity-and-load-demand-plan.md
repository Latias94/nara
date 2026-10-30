# Reference Validity And Current Load Demand Plan

Date: 2026-10-30. Status: selected design; implementation not started.
Parent: [foundation R6/R7](2026-10-23-foundation-architecture-rebaseline-plan.md#r7-target-build-and-base-source-free-delivery).
Design: [reference validity and demand](../architecture/reference-validity-and-load-demand-design.md).
Baseline: `8e175db`. Imported finding: CP3; CP4 backing/residency is separate.

## Complete Outcome

Three saved/processed Scenes link A -> B -> C through declared Deferred Door targets. They share
an atlas; B also has a distinct heavy Image/domain resource. A has Immediate CPU Scene D. Strict
saved-source Check/Export and actual processed deep validation check all mandatory targets and
their resources. Ordinary A preparation retains its own Immediate data/resources without decoding
or publishing B-only resources. Actual Door requests B/C later produce Ready then explicit
commit, while cancellation/failure preserve current state. D can be borrowed as data but is not
a travel root.

The selected change is a breaking owning replacement. No old required-only visitor, unit AssetRef
shape reader, demand-erasing reference helper or alternate eager Runtime branch remains after
qualification. Do not add compatibility aliases, a generic loader service or a migration guide.

## Ordered Work

1. Capture the pinned Bevy/Godot comparison and reconcile ADR0007/0033/0110/0122 plus the relevant
   Schema/reference/Authoring owners. Separate resolved targets from strict payload validation.
   Review exact-set admission, compound target proof and dirty-working-copy boundaries before
   implementation; keep original eager-capture availability intact.
2. Implement the shared Immediate/Deferred vocabulary and explicit recursive Schema leaf,
   fingerprint, derive declaration and all actual reference inventories. Preserve defaults,
   nested paths, property values, unavailable overlays and bounded pre-allocation traversal.
   Migrate canonical v1 schema artifacts through their real producer; no legacy readers.
3. Replace the codec required-only inventory and catalogue admission. Aggregate duplicate actual
   references with Immediate precedence. Reject missing, extra mandatory and wrong-role edges
   before dependency traversal. Reference requires present membership; Soft retains its meaning.
4. Rewrite current processed preparation around verified actual demand, retaining each actual
   decoded owner once and preserving typed/native publication. Add explicit deep validation
   through the same selected compilers/codecs with finite disposal; expose the result truthfully.
   Preserve TileSet/Image's independent native validation even when a custom codec underreports.
   Use the design's canonical priority over currently pending verified work, not the displaced
   global family-first decode order. Prove X revealing earlier-definition Y with reversed recipe
   insertion, exact/one-short overlap budgets, one decode and shared typed/native acquisition.
5. Split source preparation and saved-source validation/delivery observations. Keep deferred target
   resolution narrow, strict compound membership real, all mandatory delivery nodes reachable and
   source receipt revalidation exact. Migrate Editor capture so only Immediate dirty Scenes enter
   preview participants; preserve separate binding and deep-validation observations.
6. Complete the independent public source/processed workflow, real Check/Export and process/native
   consumers as affected. Remove superseded mechanisms, independently review, run serial focused
   nextest plus relevant minimal/all-target checks, update evidence/ledger/guides, commit coherent
   qualified units and locally fast-forward main. Overall foundation Goal remains active.

## Required Failure And Lifetime Evidence

- Same Product appears Immediate and Deferred in one nested/default/overlay-effective payload:
  one Required aggregate, no duplicate decode and exact final reference semantics.
- Missing Reference membership, omitted/extra mandatory edge or wrong demand role rejects before
  following it. Soft never satisfies a present payload reference. Unrelated Soft metadata cannot
  trigger work or become a validated target.
- A source target resolves but B payload is malformed: ordinary A preparation does not claim B's
  payload valid; strict Check/Export and actual B request reject. The current A instance survives.
- Missing auxiliary Product fails real strict import/decode membership; generic metadata never
  supplies fake proof. Compound and TileSet/native independent admission remain covered.
- Dirty Immediate D is captured with exact working-copy/revision/Schema facts. Dirty Deferred B
  does not become A's current data/native participant. Explicit deep working-copy validation,
  if exposed, revokes when B changes; saved-source Check remains distinct.
- Diamonds/cycles remain finite, aggregate budgets precede allocation, cancellation returns
  actual decoder/task owners, and escaped currently demanded values retain original custody.
- Actual public startup and later requests observe exact native/domain membership and cross-thread,
  fixture-isolated decode counts. `acquire_startup_scene` alone cannot prove native preparation.

## Verification Method

Extend the existing independent `source_scene_references` and `processed_scene_references`
consumers rather than inventing fake loaders. Use shared-atlas and stock Image/TileSet codecs,
an observed real codec and one actual CPU Scene read. First execute a bounded red oracle against
the baseline where possible, then keep exact assertions against the selected production path.
Numerical acceptance is zero B-only decode/publication during A preparation, one selected member
decode per preparation, finite graph visits and the existing owner limits. No performance gain
or allocator isolation is implied.

Use one Cargo at a time, CARGO_BUILD_JOBS=1, root target and focused nextest. Subsequent public/API,
source Export, Reference and selected native evidence follows affected behavior. Documentation
closure does not require another unrelated full product cycle. Store actual results and limits
in engineering memory, not a mutable execution table in this plan.

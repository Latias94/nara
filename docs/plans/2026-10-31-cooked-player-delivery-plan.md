# Cooked Content And Captured Player Delivery

Date: 2026-10-31. Status: implemented and qualified for the finite Windows native headless
single-Player/base-pack consumer. Broader target/platform delivery remains open.
Parent: [foundation R7](2026-10-23-foundation-architecture-rebaseline-plan.md#r7-target-build-and-base-source-free-delivery).
Prerequisite: [explicit Cargo selection](2026-10-31-explicit-cargo-build-selection-plan.md).
Broader direction: Proposed [ADR 0088](../architecture/adr/0088-target-build-cook-package-and-runtime-content-catalog.md).

The [qualification](../knowledge/engineering/verification/2026-10/2026-10-31T050529Z-cooked-content-assembly-captured-player-binding-and-relocatable-source-free-delivery-qualification-816fbb7867fb4c3288a2b2b34f27e1ff.md)
records final106 owner tests, actual independent cooked delivery, original custom/foreign and
seven-case processed regressions, independent reviews and explicit native/budget/freshness limits.
The [guide](../guides/cooked-player-delivery.md) documents the production interfaces. This closes
the selected finite outcome without completing all R7 or the full foundation Goal.

## Complete Consumer

An independent ordinary workspace uses the same game recipe in a selected Host cooker and a
custom-named native Player. Production owners cook and publish one relocatable directory with
that captured Player, a complete processed-content pack and a bounded delivery manifest. The
Player runs from an unrelated working directory after authoring source, metadata and build cache
are removed and Cargo/rustc are unavailable. It exercises global physical Input, two Scene travels,
a Deferred destination, shared Image/TileSet content and an auxiliary compound Product.

Keep source-carrying Export available. Target compilation, package publication, native execution
qualification, Runtime Ready and executable last-good remain distinct results. A foreign artifact
continues to be build-only until a suitable runner separately qualifies it.

## Actual Gaps And Reusable Owners

The [explicit build consumer](../../tests/fixtures/cargo-build-consumer/README.md) already captures
custom Host/Player bytes and runs seven processed Input cases. Its
[shared fixture](../../tests/fixtures/runtime-runner/renamed-root/tests/support/processed_input_fixture.rs)
still assembles the catalog and supplies expectation arguments from the test process. Replace
those responsibilities with production owners in this unit.

- `VerifiedArtifactGroup` already retains complete exact member bytes, Product mappings, formats
  and dependencies. A valid old group does not prove that it is still the current cook selection.
- `RuntimeSceneCandidate::cook` and the Input Runtime codec already own their concrete encoding.
  They do not need to become cases in a new erased universal cooker.
- `RuntimeContentCatalogDefinition`, base admission and deep validation already check target,
  configuration, recipe, Schema, codecs and members. The missing production assembler must supply
  their complete inputs rather than create another parser or loader.
- The existing v1 pack writes exact admitted content. Its writer does not own directory staging,
  sync, atomic publication or failed-cleanup custody; the package publication owner must do that.
- Existing filesystem directory publication, Scene commit/residency, native runner and Editor
  generation/Ready owners retain their responsibilities.

## Selected Implementation Direction

First reconcile the bounded package contract with Accepted ADRs 0007/0033/0103/0110. Adopt only
the consumed slice of Proposed 0088. Its older dependency wording must follow the current rule:
Required and Reference edges both impose complete delivery obligations; Immediate/Deferred demand
controls current materialization. Soft alone permits absence. Deferred destinations must not be
stripped because their current resources are not prepared.

The request explicitly names Cargo selection/tools, content configuration and target, startup and
travel roots, addressable roots, concrete cook definitions and effective settings. Cargo profile
and content profile are separate fields. The code inventory and content observations also remain
separate: bind both to this request without requiring equal path sets or claiming a complete
workspace snapshot.

Prepare the complete cooked catalog and retain its exact members before building the Player.
Consume original verified artifact bytes rather than reopening mutable CAS paths. Track current
source/metadata/auxiliary and actual processed-input facts with the existing local-build fences.
Concrete target policy must affect actual output, such as the existing CPU Image transform.
Deep validation refuses a transform that invalidates atlas or Scene-cell constraints unless a
separately selected policy preserves those constraints.

For the first consumer, generate one finite Rust bootstrap input containing the expected catalog
digest/length, content target/profile and fixed relative pack path. The Player explicitly compiles
this input and uses its own compiled recipe for compatibility. Include the generated file in the
Cargo declared input inventory. The package binder checks its exact digest against the current
content preparation and captured provenance. Old Player A cannot bind catalog B even when their
startup Product or display label is unchanged.

```text
verified source/groups -> cooked catalog + exact members
                      -> generated Player expectation input
                      -> captured explicit Cargo Player
                      -> exact code/content binding
                      -> complete directory publication
                      -> separate native execution qualification
```

The catalog does not include the final executable digest, avoiding a hash cycle. A final delivery
manifest/receipt records executable, catalog and pack digests, selected build facts and provenance.
The manifest describes delivery; it is not a self-authorizing trust root. The Player never derives
its expected catalog digest from the catalog being admitted. A later reusable prebuilt-Player
mode would need its own independent expectation/trust contract; it is outside this first workflow.

Use a finite layout such as `player.exe`, `content.pack`, `package.json`, with the platform's real
executable name. The Player locates the pack relative to its executable, opens the original pack
and uses the existing processed Host startup. No runtime Cargo, importer, source manifest or cache
fallback is permitted. Additional native runtime libraries, if required, are explicit finite
members; never scan `target/` to guess them.

One package owner retains executable/content/preparation bytes and publication capabilities.
Account for overlapping groups, base, validation scratch, executable, pack writing and candidate
directory work under explicit limits. Reuse existing rejected/orphaned/unresolved cleanup and
durability outcomes. Partial pack writes never become published success. Preserved pack backing
retains its existing authenticated-availability guarantee; it is not silently upgraded to the
complete captured backing's stronger availability after file deletion or mutation.

## Ordered Work

1. Reconcile the consumed Accepted contracts and compare pinned Bevy/Godot. Select finite request,
   catalog assembly, generated expectation and exact package-binding facts before public APIs.
2. Implement concrete cooked-content preparation through existing tracked build/Scene/Input
   owners and complete catalog/deep validation. Refuse unsupported facets explicitly.
3. Generate the bounded expectation input, build the selected Player through the qualified Cargo
   owner and reject code/content/input mismatches before directory publication.
4. Stage, sync and atomically publish the finite package through existing filesystem authority.
   Retain cleanup custody and separate delivery evidence from native qualification.
5. Run the independent relocatable source-free consumer, targeted failure/retention/budget cases
   and affected source Export/Runtime/native regressions. Independently review, record actual
   limits, commit coherent replacements and locally fast-forward main. Keep the full Goal active.

## Acceptance And Limits

- Two clean content builds produce equal canonical catalog/member/pack digests. No Rust executable
  byte reproducibility is required. Every captured executable retains its own exact digest.
- Relocation and unrelated CWD succeed with only declared package members and normal OS runtime
  libraries. Startup, physical Input, failed travel and successful travel use production paths.
- Before publication, deep-validate the complete candidate and reject code A/content B, stale
  groups/bootstrap input, wrong target/profile/Schema/codec, duplicate Products, missing/corrupted
  candidate members and unsupported facets. Startup rejects catalog/target/extent and demanded
  content failures before Startup effects. A later corrupt Deferred payload rejects its acquisition
  and preserves current Scene/resources. Original-file backing does not promise perpetual availability.
- Deep-validation member coverage must equal the delivered member set. Extra compound siblings
  require actual roots/dependencies or reject; opening a valid catalog cannot substitute for
  decoding every candidate payload. Verify the original staged pack against the expected exact
  pack bytes. Define malformed-producer fixtures as pre-publication rejection, and inject later
  Deferred corruption only after complete publication to exercise demand-time failure.
- Bind the canonical generated-input path and digest/length from actual Cargo provenance and
  compare the whole requested selection. Trusted Player inclusion/use remains explicit; no binary
  inspection or Rust include-analysis proof is introduced. Keep original Host content observations
  and selection fences distinct from code inventory, including metadata and auxiliary reads.
- Interrupt cook, Cargo, staging and publication. No partial package is success; existing package,
  Runtime Scene/resources and executable last-good remain under their original owners.
- Retain base/acquisition A while building B and deleting cache. Escaped resources and subsequent
  acquisition obey A's original backing guarantee. Test exact/one-short allowances at new overlaps.
- Preserve the seven existing processed Input process cases. Qualify desktop presentation and any
  foreign runner separately. Headless execution alone does not prove server stripping; a claimed
  server mode needs its own selected recipe, member audit and executable capability evidence.

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` separates processed loading from processor
execution in [asset composition](../../repo-ref/bevy/crates/bevy_asset/src/lib.rs). Pinned Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` separates executable template preparation from content
export in [PC export](../../repo-ref/godot/editor/export/editor_export_platform_pc.cpp), and selects
target imported variants in [platform export](../../repo-ref/godot/editor/export/editor_export_platform.cpp).
These are responsibility references, not Nara execution or platform evidence. No new resolver,
universal cooker/build graph, remote service, patch/mount system, compatibility reader or migration
guide is selected. Reuse root target and serial Cargo; preserve vendor, concurrent work and refs.

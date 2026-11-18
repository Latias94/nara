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

## Request-Owned Generated Rust Input Successor

Remove the shared-source bootstrap write used by Two-Level and the independent delivery fixtures.
The original Cargo request owns one bounded immutable generated Rust source and its canonical
logical name. The original command supplies its reserved environment value; requests without
that input explicitly remove ambient values. The trusted build script declares
`rerun-if-env-changed`, writes only Cargo's `OUT_DIR`, and enables its cooked branch only for a
selected input. Stale generated files in source or `OUT_DIR` do not activate an ordinary Player.

Keep ordinary source-file observations separate from the request-generated name/digest in
provenance. Package binding requires the actual injected generated fact, removing acceptance of
a shared source-file observation as bootstrap authority. Preserve the original content/code
fences, exact executable capture, target-directory lease, native cleanup and whole-directory
publication. This is one concrete generated-input slot, not a generic environment API, source
copier, Cargo dependency resolver or additional process owner. Record bounded parent work and
encoded input bytes through the existing Cargo owner; child/ambient work keeps its existing scope.

Prepare distinct A/B expectations before either native build, hold both immutable requests and
build serially in the same workspace/target. Verify the actual captured code and generated facts,
including rerun after a changed selection, no-input refusal with stale Cargo outputs, cross-bind
and installed cross-pack rejection. Relocate and execute a real source-free Player. Migrate the
three Two-Level runtime selections and independent renamed-engine consumers without restoring a
source-write fallback. Keep this prerequisite distinct from the later shared starter/Two-Level
cooked entry and its new external-game acceptance.

### Generated Input Implementation And Qualification

Implementation: `ebc50da`, qualified on November 18, 2026 for the Windows Rust 1.95.0 workflows
recorded below. The shared starter/external-game cooked entry remains the next delivery consumer.

`CargoBuildRequest::with_generated_rust_input` selects one canonical logical name and owns an
exact NUL-free UTF-8 source copy. The reserved invocation environment is removed for ordinary
requests and set from that owned copy for generated requests. No global environment is modified.
The 16 KiB transport ceiling is separate from total input bytes, input count and parent work
allowances. Current Player bootstrap constants contain only the bounded catalog digest/length,
target/profile and fixed pack name; their existing encoded representation is preserved.

The source has no temporary-file or dependency-routing owner. Cargo retains the existing target
lease and process custody. Build scripts use `rerun-if-env-changed` and write only `OUT_DIR`;
the three Two-Level wrappers include the resulting module and retain explicit ordinary-build refusal.
`GeneratedPlayerBootstrap::name` replaces its old physical-path API. Captured provenance and
the unpublished canonical v1 manifest record a separate generated fact and
`generated_input_name`; source-file matching is removed from bootstrap binding. Metadata digest
still represents actual declaration-only Cargo metadata. Trusted Cargo configuration must not
define or override `NARA_CARGO_GENERATED_RUST_SOURCE`; this remains a trusted compiler/build-script
contract rather than Rust inclusion, hermetic-build or hostile-config proof.

The owning Cargo/package tests pass 17/17, with 946 unrelated cases excluded, in
`378cb41e-0727-4e7b-9a05-57fa06df8d77`. Controls cover request byte independence, no-source-file
effects, command-local environment selection/removal, single-slot/name/transport rejection,
exact and one-short budgets, distinct generated-versus-file provenance and actual binder refusal
when only a matching ordinary file fact exists. Original target lease and failed-child cleanup
controls remain intact. The combined 600-file/16 KiB source case admits the full overlap before
file-fact allocation and rejects one short at that same preallocation point.

Independent review found and corrected the initially late combined-work check, a test-only empty
diagnostics construction error, three tuple-versus-object digest assertions and missing actual
Two-Level stale-output refusal coverage. The first owner run also exposed an invalid test tool
choice: nextest's multiply linked executable is correctly rejected by the original tool identity
owner, so the fixture now discovers the installed toolchain normally. These corrections do not
relax native build, source/current-content or cleanup contracts. Final independent/native
execution evidence follows below.

Independent real delivery passes 2/2 in `344cedaa-d039-41c4-a5e7-f0c3d0dac834`. The conventional
renamed-engine workspace prepares A/B requests before either build, then builds A/B/A with one
selection, workspace and shared target. The captured source-file facts stay equal while generated
facts and actual B code/content differ. Three original captures are published, relocated and run
after source/cache/tool removal. A and B each reject the other's installed pack; a subsequent
ordinary request refuses despite stale Cargo-generated output. The independent Palette cooker
and source-free Material-only Image consumer also pass through the same generated-input slot.

The initial two-consumer run `ecfc1fd0-ccbb-40ae-841c-0789207a8246` passes the Material consumer
and reaches cross-pack refusal in the A/B/A path. Its newly added assertion incorrectly expects
a catalog-digest error. The two profile strings produce different encoded catalog lengths, so
the original reader correctly rejects the pack header first. The fixed negative control checks
that actual length difference and requires exact `InvalidPackHeader` refusal in both directions.
All original valid gameplay, Deferred corruption and native cleanup controls remain intact.

The strengthened A/B/A consumer passes 1/1 in `d297efc8-4842-4dd1-ac78-2ba4f5682dc6` with a
parent-provided `compile_error!` value in the reserved environment. The original request replaces
that value for generated builds and clears it for the ordinary no-input build. This is actual
command/`OUT_DIR` behavior, beyond inspection of a constructed `Command`; Cargo configuration
override remains outside this trusted-environment guarantee. The separate Palette result above
does not need another execution cycle after this outer-test-only strengthening.

Two-Level's affected release cohort passes 36/36, with six native cases excluded, in
`eae2a5c4-a77d-4869-aa63-3e0c572d65e7`. It compiles all changed package-test helpers and covers
source agreement, actual profile preparation, content/code fences, wrong inputs and bounded pack
corruption. The existing headless/desktop/audio real package trajectories are qualified separately
after execution; this CPU result does not establish their native behavior.

The first three-Player native run, `9c27ff55-790e-4347-9fe6-32fc20cf87ea`, reaches actual audio
execution after its release build but fails the original exact-foreground-window guard during
physical input. The observed foreground belongs to another process. Keep that result as a failed
native prerequisite, not an audio or generated-input success; preserve the original assertion for
the separate rerun. Its desktop case later also fails the same original foreground guard after
multiple physical-input steps; it is not a successful desktop qualification.
The same run's headless package case passes, including the actual no-input
rebuild and refusal with old `OUT_DIR` bytes still present, followed by normal relocated gameplay,
checkpoint/fresh Restore and damaged-content controls. Later isolated results follow below.

The isolated physical rerun, `ad6f2e88-ebfb-4471-9722-11718a53291c`, fails both cases. Desktop again
loses the exact foreground to another process. Audio instead reaches its 30-second script bound
at the first Walk, with no held movement, one unsolicited travel and one Retry. The retained
diagnostic does not establish the event that changed those facts. Read-only inspection confirms
the observed foreign foreground PIDs belong to Edge and Explorer; the fixture does not restore
foreground before its original Player finishes. The first run reaches audio's later physical
test only after its complete scripted callback/PCM trajectory succeeds. These facts separate the
two failed native attempts; they do not turn either aggregate into a passing qualification.
Replay the retained source-free sealed worker before paying for another full Cook/build cycle,
and preserve its original foreground, gameplay, PCM and physical-cleanup assertions.

The retained audio sealed worker then passes 1/1 in 5.64 seconds without another Cargo invocation
or changed Player bytes. Its original script admits seven effects (six complete, one cancelled)
and verifies the captured PCM. A second original process performs physical D/E input and owned
window close, admitting two effects, and both prepublication corruption/mismatch controls reject.
Every original child records empty containment, stopped readers, drained output, destroyed window
and no termination intervention. The source/cache remain unavailable. This replay qualifies that
captured worker trajectory; the subsequent complete Cook/package rerun is recorded separately
below. No production input cancellation is weakened or bypassed.

Source inspection explains the stalled script's possible interruption: ordinary input is active
after initial reconciliation, and Travel or Retry correctly clears game-owned held movement. The
finite verifier sends Walk once and retains that dispatched state. An outside transition can
therefore leave it waiting for a pickup until its original diagnostic bound. The captured failure
does not identify the sender or exact command order. An idle desktop remains a prerequisite of
this live-input script; the observation does not justify changing gameplay cancellation or
silently reissuing a command to make the assertion pass.

The complete desktop package trajectory passes 1/1 in
`8fcf7748-25f7-4f10-9521-a7dfe1d908e6` (97.130 seconds). It recooks and publishes the actual
Player, relocates the complete installation and removes source/cache before the sealed worker.
Both hardware physical-keyboard sessions, authored region/material/TileSet and collision
controls, Save/fresh Restore, twenty further travels, software-DX12 first presentation, invalid
initial save refusal and all three finite scripted controls retain their original assertions.
All seven original windows/processes and pumps retire without termination intervention. This
passing rerun supersedes the desktop foreground failures for the current implementation; it
does not erase those failed-attempt diagnostics or qualify other platforms/performance.

The complete audio package trajectory subsequently passes 1/1 in
`f1ac0e74-023a-41b3-a446-e3c05d617c72` (84.516 seconds), including a fresh production Cook,
three-file publication/relocation and source/cache removal before the sealed worker. The actual
hardware-DX12/WASAPI script, full callback/PCM oracle, physical keyboard/owned-close process and
both invalid-package controls all pass through their original native owners. This is the full
outer delivery test, separately from the earlier retained-worker replay. Together with the
headless and desktop results above, it closes the selected three-Player migration on Windows
Rust 1.95.0. Physical human audibility, foreign execution, numerical performance and the shared
starter/external-game cooked entry remain unqualified or open as previously recorded.

Final correctness Clippy passes for the owning production library, the changed sample library,
cooker and package-test targets, and the independent consumer's complete target set. Root,
sample and independent-consumer formatting checks pass. Added relative document links and
whitespace checks pass; the fifteen paused Editor/Dialogue integration files retain their original
bytes. These checks and the actual native results above qualify the current implementation;
documentation closure does not require another unchanged product acceptance cycle.

Cargo's documented build-script output and environment dependency rules justify this seam.
`OUT_DIR` can retain old files across invocations, so activation follows the explicit request,
never file presence. Cargo configuration can supply/override environment values, which is why
the reserved-key constraint is explicit instead of an unsupported sandbox claim.

- [Cargo build-script outputs and change detection](https://doc.rust-lang.org/cargo/reference/build-scripts.html#outputs-of-the-build-script)
- [Cargo environment configuration](https://doc.rust-lang.org/cargo/reference/config.html#env)

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
this input from Cargo's `OUT_DIR` and uses its own compiled recipe for compatibility. Select the
generated source on the original request independently of ordinary file inputs. The package binder checks its exact digest against the current
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
- Bind the canonical generated-input name and digest/length from its distinct Cargo provenance and
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

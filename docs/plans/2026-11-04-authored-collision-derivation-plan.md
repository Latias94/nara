# Authored collision derivation

## Contract and scope

This section retains the initial translation-only contract. The
[fixed-input successor](#fixed-spatial-inputs-and-transformed-collision-successor) replaces its
shape semantics and owns the current transformed-collision workflow.

Complete the generated-collision workflow in the foundation plan's R10 using the independent
motion workspace. ADR0095 owns the plugin-specific domain; ADR0117 owns persistent configuration
versus native reconciliation; Scene owns source, occurrence and structured Prefab provenance.
This qualifies one concrete Rapier authoring integration, not a production backend selection.

One persistent `CollisionDefinition` describes a translation-only box: center, positive bounded
half width/height and fixed/dynamic behavior. Register its exact native codec through the ordinary
Schema inventory. Durable Scene/Prefab data contains only that definition, never Rapier handles,
runtime Entity IDs, solver caches or debug output.

A private domain owner derives real Rapier bodies/colliders after ordinary Scene materialization.
Its reconciliation input is the complete selected set of live Scene receipts and their accepted
authoring revisions. Resolve those receipts in the actual World and copy typed `SceneEntitySource`
provenance; do not parse expanded entity IDs or invent another persistent identity service.
Attribute body, collider and debug roles to source, occurrence, component/property and revision.

Validate the complete input before native mutation. Unsupported geometry, missing/stale source
membership and duplicate input refuse with structured diagnostics. Invalid authoring patches keep
their old document revision; a bypassed invalid native value must also refuse derivation. Retire
removed or changed native pairs before publishing their replacements. Preserve unchanged pairs
and solver state for unchanged live entities. The existing public Scene synchronization replaces
that occurrence's activation; this consumer derives fresh pairs for the replacement rather than
infer native continuity from equal source IDs. Synchronize only dirty selected sources, preserving
other occurrences' entities and dynamics. An in-place changed definition also rebuilds its pair.
This finite authoring policy does not promise seamless physical editing or native rollback.

Debug geometry is an immutable numeric capture derived from the actual current collider/body,
not a second mutable shape authority. Retained old captures keep their exact data; native handles
from removed or replaced outputs no longer belong to the current solver. Complete unload and
managed shutdown retire all owned bodies/colliders. A retained debug capture cannot resurrect them.

## Alternatives

- A codec that creates native bodies conflates persistent decoding with lifecycle effects and
  prevents data-only editing. Keep codec preparation free of solver mutation.
- Private post-spawn derivation reuses the admitted native binding and current Scene membership.
  Select it for this consumer, which has no measured startup/cook/delivery pressure.
- A domain cook remains conditional on a concrete delivery or startup requirement. Do not create
  a generic baker, portable Physics provider, writer ACL or second Scene coordinator.

## Complete consumer and verification

1. Build and materialize a Scene/Prefab through public Schema/Scene interfaces; derive actual
   Rapier body/collider/debug output and observe real dynamic motion/contact through fixed steps.
2. Edit dimensions through the ordinary authoring patch, reconcile and check the native shape,
   debug vertices, revision and original output retirement. Invalid edits preserve the old state.
3. Duplicate a definition and instantiate the same source at another occurrence. Prove distinct
   generated membership and structured attribution, including a Prefab projection.
4. Remove a component/entity and unload one occurrence. Verify exact remaining membership and
   preservation of unrelated bodies. Hold an old debug capture across edit/removal/close.
5. Undo/Redo and Save/Reopen the authored definition, bind it through the exact original codec,
   and reconstruct output without any serialized native handle. Reject a missing native binding.
6. Exercise managed close, invalid native input and stale/foreign receipts. Run focused serial
   nextest, fixture formatting and correctness Clippy; independently review before local commit.

Reuse Rust 1.95.0, root `target` and `CARGO_BUILD_JOBS=1`. No concurrent Cargo. Record only actual
qualification in the owning ADR/ledger and one verification shard. Full R10, affine authoring,
physical audio devices and broader foundation work remain separate.

## Qualification

Implemented in the independent motion workspace without production engine edits. The final
default and feature-unified cohorts pass 30/30 and 31/31, including eight authored collision
controls. Actual collider/debug observations accompany public edit/history/disk-Reopen, and the
complete selected Entity set accompanies every structural removal/unload control. A late-invalid
mixed batch retains all native state; repaired same-Entity retry rebuilds only its own pair.
Independent native-ownership and testing reviews close with no remaining P1/P2. The
[verification](../knowledge/engineering/verification/2026-11/2026-11-03T170523Z-authored-collision-definitions-retain-exact-source-attribution-through-real-rapier-derivation-and-retirement-b550d8d09ee34b72adfa1d18262eec6a.md)
retains exact commands, the initial integration failures, review corrections and support limits.
This closes this finite collision unit, not full R10 or the foundation rebaseline.

## Fixed Spatial Inputs And Transformed Collision Successor

Status: implemented and locally qualified after `6ee25b6`. The existing independent domain now
covers a complete parented fixed-box and root dynamic-body workflow. Editor/Dialogue work remains paused. This
does not select a production Physics backend or a persistent local-affine representation.

The original domain interprets its center in world coordinates and does not consume Transform
completion. That finite translation-only qualification cannot establish transformed geometry or
solver writeback. Replace that interpretation in the fixture: authored center and box extents
are local shape data; the original completed spatial input supplies the effective affine frame.
Every selected collider needs an actual Transform participant. No identity/local fallback or
physics-owned hierarchy traversal is admitted.

Physics needs a cut after Prepare's deferred mutations and original Hierarchy completion but
before Simulate. Add an opt-in fixed-input role to the existing Transform owner, with current
World/runtime/tick/frame validation and immutable captured sources. It must not overwrite the
existing Historical, completed Fixed or Presented cuts, or turn a historical value into a
current input. Reuse the original hierarchy validator and affine construction; do not create
another relation, propagation algorithm or public automatic phase framework. Installing input
consumers repeatedly must not duplicate publication. Unselected worlds retain their current
schedule work. Failure withholds this role rather than exposing old payload bytes.

Fixed boxes consume the full affine input, including rotated nonuniform ancestors, shear and
reflection, as a finite native convex shape. Degenerate/nonfinite or unadmittable geometry
refuses the complete batch before native mutation. Dynamic bodies retain ADR0097's initial
root/unit-scale subset; parented, scaled or sheared dynamic writeback refuses explicitly. The
domain converts native root poses back to the original local Transform before fixed Resolve.
This is the same solver/domain owner, not a generic transform synchronization service. External
accepted pose/shape edits rebuild only the affected native pair; domain-owned pose writeback
does not reset dynamics every tick.

Acceptance must exercise both admitted automatic profiles with real Rapier contacts and native
geometry: same-tick parent/shape edits, a non-spatial direct-parent boundary, all four reflection
sign combinations, a genuine sheared fixed shape, root rotation/writeback, zero/multiple ticks,
Pause/exact Step, source edit/Undo/Redo/Save/Reopen, unchanged unrelated bodies and retained old
debug output. Validate complete late-invalid batches and missing/stale spatial inputs before
changing any native pair, then repair the same original operation and close through the managed
owner. Preserve existing provenance, occurrence, Schema and source-revision checks.

Independent numeric oracles derive corners and solved poses without using uploaded/native
geometry as their expected values. Use an absolute `1e-4` plus relative `1e-5` coordinate budget
for the bounded test scenes; exact persistent/history/provenance assertions stay exact. Record
any tighter domain admission bounds and conditioning refusals before calling them supported.
This is a correctness budget, not a runtime cost or performance claim.

Inspect pinned Bevy/Godot and installed Rapier source, reconcile ADR0095/0097/0100 and guides,
review independently, run focused shared Transform/Hierarchy plus independent motion tests and
an affected actual native history trajectory. Cargo stays serial with Rust1.95, jobs1 and the
original target. Do not claim parented dynamic-body support, non-TRS local authoring, arbitrary
physics rollback, foreign native execution or completion of full R4c/R10/Foundation.

### Source-Grounded Boundary And Fixture Replacement

Pinned Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`,
`crates/bevy_transform/src/components/global_transform.rs`, keeps complete affine composition
but documents nondegenerate/no-shear assumptions on decomposition; `to_isometry` discards scale.
Its propagation multiplies affine values. This is Transform evidence, not a built-in Bevy
Physics integration or a Nara support claim.

Pinned Godot `7a37606a433c0e7735525037cdf774f043146a21`,
`modules/godot_physics_2d/godot_body_2d.cpp::set_state`, retains affine input for static/kinematic
bodies but orthonormalizes dynamic bodies and reconstructs rotation/translation on integration.
`scene/2d/physics/rigid_body_2d.cpp::_sync_body_state` writes global pose through Node2D's parent
inverse. Node2D can retain skew; Nara's local TRS cannot inherit that capability by analogy.
The Godot local-scale warning is not a complete parent/shear admission check. Its rectangle SAT
also cannot establish arbitrary-shear support merely because Transform2D accepts the value.

Installed Rapier2d 0.33.0 and Parry2d 0.28.0 expose numeric `convex_hull` construction; native
poses retain only rotation/translation. The fixed-box conversion therefore uses a native origin
at `A(center)` and four relative points `linear(A) * corner`. Check construction and retention
of all four transformed corners before retiring any predecessor. Dynamic body pose corresponds
to the root entity's Transform, with center as collider-local offset. Rotation remains locked
in this independent fixture; initial root rotation still participates in geometry and writeback.
These installed versioned sources, not their dirty-marked packaging VCS metadata, define the
API evidence used by the actual build.

The fixture replaces its unreleased world-center component with canonical v1
`fixture.motion.LocalBox`; there is no alias or reader that silently reinterprets old data.
Every selected entity acquires an actual persistent Transform. Its ordinary Scene source
synchronization only materializes/unloads data; native admission occurs at the next explicit
fixed-input boundary. The tests may run an explicitly named admission-only tick to inspect
unchanged native state before solving. This advances the Runtime clock but does not hide a
native physics step in source sync. Whole native batch preservation is distinct from rollback
of an already accepted Scene replacement.

Independent native-domain review found a pre-solve writeback gap: admission checked captured
root identity, but a live reparent between admission and solve could retain identical local
values and source metadata. Writing the native root pose into that new child would add its new
parent transform. Pre-solve validation now also checks live structural root membership for every
dynamic writeback target, before advancing any body. A frozen spatial input remains readable;
it is not mutation permission for a changed live target. The regression must reparent through
the original HierarchyWriter, prove whole-batch no-step/no-writeback and revoked current debug,
then repair the root, readmit and solve the unconsumed input exactly once. The complete consumer
qualification below executes this regression.

### Initial Shared Qualification

The shared production-library check passes on Windows with Rust1.95 in release mode. Owning
Hierarchy/Transform run `6eff5c7e-3a3b-431c-9e10-f86bbb7d6720` passes 86/86, leaving five
existing cases unselected. Three new public integration tests execute deferred Prepare edits,
same-tick reparent and non-spatial boundaries, frozen input versus later native-style writeback,
historical/completed/presented separation, repeated registration, zero/multiple ticks,
Pause/exact Step and opt-out work. Three internal adversarial tests cover identity/phase/time,
failure/recovery and cache/statistics behavior. Original hierarchy, projection, interpolation,
same-change-tick and native-value-copy controls remain in the cohort.

Independent shared source review reports no confirmed P1/P2; it did not execute tests. This is
the shared prerequisite's actual evidence, not the complete transformed-collision qualification.
The only lock change so far adds the concrete physics author's Hierarchy dependency edge in
the motion fixture; the independently locked render consumer retains its unchanged closure.
Real solver, migrated source/history tests and native history continuation remain pending.

The migrated real motion/history consumer now reads its current parent translation through
`FixedInputSpatialSnapshot2d` before MotionIntent, asserting the actual input phase and tick.
Its original late capture/prepared refusal, clocks, exact Step, selective history reset and
submission/output assertions remain. On RTX4090/DX12 with explicit hardware selection and
the Parallel profile, run `21130516-fc34-44f8-92d9-924c9590dd15` passes 1/1, with six other
history tests outside the filter. This proves an actual native continuation of the shared input
role, separately from the still-pending authored transformed collision CPU/native-solver cohort.

Consumer review found three qualification weaknesses and the tests now close them: synchronous
callbacks use immediate result observation rather than an unbounded channel wait; moving ticks
compare completed Fixed globals against that same tick's post-solve locals and compare Presented
immediately after the multi-tick drive; paused exact Step checks actual body descent and one-step
gravity velocity change while retaining the native pair, current solved stamp and writeback.
The later settled contact cannot mask missing moving-frame publication or skipped exact solving.
These corrected controls execute in the Parallel-enabled qualification below.

The first full independent motion run `46bab127-e3a7-4dc9-b31a-b6762805b44c` executes 46 tests:
38 pass and eight test processes fail. All eight fail at the same migrated fixture setup,
before transformed collision admission: it tries to materialize a Scene during configuration
under an installed but unfinished managed Registry, correctly receiving
`scene.prepared-definition-changed`. The failure processes exit with `0xc0000409`; this does
not independently diagnose a general unwind bug. The ordinary full-affine contacts, moving
root writeback and exact-step positive trajectories pass in this initial run.

Correct the fixture through the same original owner. The existing preloaded Registry plugin
now exposes a direct snapshot constructor for code-first compositions, alongside its existing
plan-local delayed preparation adapter. Install the exact admitted original snapshot, including
Transform provider receipts, and materialize through StartupStage::Scene after Finish. Assert
that the Runtime's admitted snapshot is the exact original value. This preserves the repaired
native-codec candidate proof, rather than reconstructing a different registry, forging admission
or weakening Scene's managed-authority check. The new constructor reuses the original build,
Finish, authority and retirement implementation; it introduces no new Registry owner.

The second complete run `c404bc30-fb74-4113-a4c7-0948a56a5b58` passes 45/46. Its sole refusal is
inside the missing-codec regression: a provider callback receives an initially empty owner-local
native candidate, not a builder already populated from the separate source catalog. Freezing
that empty candidate succeeds and cannot prove missing-binding refusal. The corrected callback
first registers the actual local-box schema, then takes/freezes/retrieves that exact failed
candidate, checks `MissingNativeBinding` and unchanged catalog, and installs its original codec.
The aggregate still registers only actual providers. It does not mix raw registration into
provider composition or replace the candidate's authority. Code-first Scene admission controls
also use the public ready-snapshot constructor rather than the plan-local delayed adapter.

Final Parallel-enabled motion run `51586e4d-1671-4d7a-a8e1-8454439498f8` passes 46/46 with no
skips on Windows/Rust1.95 release. It retains all original comparison, managed, prediction/PCM
and eight authored-collision tests, adding ten transformed-collision tests. Each new transformed
workflow runs Serial and Parallel against the same two-worker Host pool. Actual Rapier contact,
full-affine corners, moving-root writeback, immediate fixed/presented consistency, paused exact
Step, late live reparent refusal/repair, whole-batch preservation and source/history/Reopen all
pass. This is CPU/native-solver evidence, separate from the hardware GPU history result above.
Independent shared, domain, testing and public-constructor reviews close without remaining
P1/P2 findings.

The default-feature motion run `44928bbb-e62a-4bbe-934a-bcb0097e82d7` also passes 40/40 with no
skips. Registry owner run `779da72e-4608-403d-985f-2566fa297678` passes 15/15, leaving 172
unrelated library tests outside its filter. Public Scene admission run
`cfe85a91-88f4-41db-825a-718b2b4851ec` passes 4/4 through the direct public constructor.
Existing rejection during configuration, preloaded provider callback suppression, exact
installation identity, undeclared-provider refusal and managed replacement/fault controls remain
intact. The Scene controls preserve installed-but-unadmitted refusal and the ordinary admitted
data path.

DX12 software run `5cce798c-ed9f-48db-9464-2f4448047c75` passes the actual joint Material/history
trajectory 1/1 with the Parallel profile and explicitly requested Microsoft Basic Render Driver.
It observes four cuts, fourteen temporal publications and four late stock outputs, retaining
exact old pixels after producer retirement and the original World-drop/cleanup assertions.
There are no cleanup failures. This software correctness path complements the separately
selected RTX4090 hardware history run; neither substitutes for the other's adapter prerequisite.
The real motion consumer now always reads the fixed-input role in both trajectories.

Final owning correctness Clippy passes in release/locked/offline mode for Hierarchy, Transform
and Reflect with all targets/features; the independent motion consumer with
`production-parallel` and all targets; and the independent render `history`/`joint_history`
targets with that profile feature. Existing non-correctness warnings remain. Production
Hierarchy/Transform/Reflect libraries also compile for stable `wasm32-unknown-unknown`; this is
compile-only evidence, not browser execution or native Rapier support on Wasm. Cargo operations
remain serial with one build job and the original root target; no dependency version changes,
extra build tree or clean is used.

Root, independent motion and independent render format checks pass. Added local document links
and fragments, TOML/lock syntax and ordinary Git whitespace checks pass. The only lock edit
adds the already used Hierarchy dependency to the concrete author's optional feature closure;
the render consumer lock remains unchanged. Existing CI already runs the Parallel motion suite
and software joint-history target, so the new cases join those jobs without a new verification
script. No remote CI execution or publication is claimed.

The qualified scope remains fixed affine local boxes and actual rigid root/unit-scale dynamic
poses in one independent domain, with rotation locked in the native owner. It does not select
a production Physics backend, support parented/scaled dynamic writeback, choose persistent
local-affine authoring, restore arbitrary native side effects, measure performance budgets,
qualify other native platforms or close full R4c/R10/Foundation. Editor/Dialogue work remains
paused and outside this change.

Local implementation commits are `79cd09e` (direct admitted Registry installation), `d934b24`
(opt-in fixed spatial input and native history consumer), and `c5b75ba` (transformed authored
collision). `SpatialSnapshotPhase::FixedInput` requires exhaustive readers to handle the new
role. The concrete fixture replaces its old component identity with canonical v1
`fixture.motion.LocalBox` and removes the old `step` entry in favor of `step_and_writeback`;
it provides no compatibility alias or reinterpretation of previous source bytes.

## Authored Collision And Visible Runtime Successor

Status: implemented at `1532cdf`/`5197e64` and locally qualified. Join the qualified affine
collision owner and the ordinary Sprite/stock GPU path through one complete editable
platform and falling-box workflow. At unit entry, authored collision tests observe real solver
geometry and writeback; the existing GPU
motion consumer uses another translation-only owner and draws its kinematic actor. Neither
alone proves the authored collision body's visible result. The ordinary games keep their own
bounded movement/hit rules; this workflow does not replace them or select an official backend.

Compose the existing `motion-physics::authored::CollisionWorld`, public Schema/Scene, original
fixed-input helper, Transform and ordinary Sprite capture in the independent render-policy
consumer. Do not copy collision construction, hierarchy traversal, native solve or writeback.
The game-owned visual adapter relates local-box center/extents to Sprite anchor/size on the same
original entity. Include nonzero centers, a sheared parented platform and a dynamic rigid root.
Native fixed geometry, post-solve local values, fixed/presented cuts and actual output pixels
must describe the same original workflow. Rendering still consumes Presented; interpolated
moving pictures are intentionally distinct from the latest solved pose and need their own
previous/current/alpha oracle. Do not disable interpolation to hide that distinction.

Use public Scene patches to change the platform and reset the drop, reject an invalid source
edit without effects, exercise Undo/Redo and exact Save/Reopen, and observe a distinguishable
new native contact and picture. Retain an original unprepared CPU input and published output
across the source replacement, later ticks, original Runtime/solver close and producer retirement.
First realize the old input after its original World is destroyed; old output pixels remain
exact after later work. Pause/exact Step must advance the real body once without fabricating a
Render capture; the following ordinary Extract supplies the next actual input.

Expected geometry and pixel coverage derive independently from authored local numbers and
observed native root poses, not uploaded matrices, extracted vertices or screenshot copies.
Require exact opaque colors and alpha; any edge rounding policy must be explicit, bounded and
independently challenged. Prove real contact with the intended platform and an independent
rest-height calculation. Use original Runtime/native/Host retirement on success and assertion
unwind, explicit hardware/software selection, owning CPU controls and an affected existing
history control. Qualify only the actual profiles/platforms executed. No new GPU, input,
Physics provider, spatial or presentation authority, registry shard or performance claim follows.

### Initial Visible-Consumer Integration

The new public consumer compiles on the independently locked workspace with the Parallel feature.
It keeps original Schema/Scene materialization, the same `CollisionWorld`, genuine fixed-input
admission, original Transform completion and stock capture. Source synchronization runs in the
original ordinary `First` stage, including paused maintenance. An explicitly driven exact tick
still creates no Render capture; a later ordinary Extract does. Pending source edits require that
explicit maintenance before an exact tick, rather than temporarily resuming the game or adding
another Runtime mutation port.

The first integration exposes a real time mismatch in the concrete domain. Its native world keeps
Rapier's default 1/60-second duration even when the App selects another fixed timestep. Red run
`fc7a1250-b958-4c5d-9f91-819bf3f4b78d` fails 0/1: the actual 8ms free-fall velocity is
`-0.16350001`, while this tick requires `-0.078480005`. The Runtime and native owner retire before
the failing numeric assertion. This is a newly exercised custom-timestep defect in the fixture,
not an upstream Rapier fault or a claim that the earlier default-duration tests failed.

The original admission now captures the actual positive finite `FixedTime::delta` with its input
stamp and revalidates the exact Duration before native effects. Only then does it set the native
integration duration and consume the solve. It does not infer duration from frame delta or current
policy, add a second clock, change the original fixed owner or replay a tick. Focused run
`225accad-4ee7-4bcf-9b64-95560fa5bd36` passes 1/1, covering 8ms/32ms and paused exact Step under
both admitted profiles. Complete Parallel-enabled motion run
`34dc4fe4-deaa-40c3-b29b-b5e7b1cd750b` passes 48/48 with no skips. Its second new control swaps
in genuine changed-duration and zero-delta clock values after admission while retaining a valid
original spatial input, proves whole native-state preservation/current-debug revocation, restores
the original clock/receipts and successfully consumes that original 8ms cut once. Independent time
review finds no P1/P2; final integrated/native evidence remains separate.

Consumer review strengthens two initial acceptance weaknesses before native execution. A small
Presented-versus-Fixed matrix difference can cover identical pixel centers, so the real trajectory
must select a bounded moving sample whose interpolated and current-only allowed palettes have
a disjoint pixel. The actual readback must match interpolation and reject the current-only oracle.
Paused source-edit and Undo/Redo maintenance frames must validate their new source pose and
Presented geometry before exact Step, while native debug still retains the previous unconsumed
physics cut. Later successful steps cannot substitute for those intermediate assertions.

The first manifest enabled the facade's broad `serde` feature merely to read a Scene file, adding
unneeded UI/font/layout dependency closure. The consumer instead enables `nara_scene/serde`
directly and keeps the original facade feature selection. Its first ordinary test build is
intentionally stopped before execution to apply that dependency correction. This is not passing
CPU/native evidence. Final lock review and tests must cover the narrower actual composition.

The second genuine consumer also exposes duplicated Schema ownership. Both wrappers register
the identical `CollisionDefinition` and native callback, but the first render wrapper assigns
them another owner/provider lineage. That can run in separate Apps yet cannot validate the
original exact receipts and can conflict in a known-owner composition. The domain now exports
`COLLISION_SCHEMA_PROVIDER_ID` and `COLLISION_SCHEMA_PROVIDER`, retaining the original
`fixture.motion.collision-authoring`, `fixture.motion.collision-authoring.schema` and
`fixture.motion.collision-native` v1 identities. Both consumers register that same original
definition; their diagnostic Plugin IDs, Scene selection, schedules, drawing and close integration
remain consumer-owned. The original motion registry validates the shared provider snapshot, and
the missing-codec/repaired-binding test keeps its deliberately distinct binding and original
candidate. This is reuse of the concrete domain, not a new common Physics provider or permission
for two solvers to write the same entities.

The initial narrowed consumer run `00ab4c99-936a-48d3-98bd-ae782a16cb1e` passes 9/9, leaving two
native cases unselected. It executes eight independent math/negative controls and the actual
Runtime's paused source-edit/maintenance/exact-Step/Undo/Redo path before any GPU owner exists.
Its release build takes 10m39s separately from 0.244s test execution. This checkpoint precedes the
canonical provider move and final restored dependency edges; final qualification must use those
actual sources. Four incidental resolver-selected Windows dependency edges are restored to their
previous allowed versions after inspecting their actual declared ranges. Only required Scene
serialization, temporary-file and domain-authoring dependencies remain; no unrelated package
version changes are selected.

After the canonical-provider move and restored dependency edges, ordinary consumer run
`3b831fff-a53a-4c07-ac8d-c70ec2984bbf` again passes 9/9, with two native cases unselected. The
first explicit DX12 software run `1d4c1397-af13-4046-9849-407bf32f48fd` passes the actual
acceptance-panic cleanup control and original joint Material/history trajectory, but the full
new workflow fails at disk Reopen. Its fixture uses direct serde struct serialization instead
of Scene's actual file envelope; the ordinary candidate parser correctly rejects the naked
`placements` field. Use `SceneDocument::to_json_string`, retain actual disk bytes and the same
candidate/Schema publication checks, and remove the unnecessary direct JSON dependency. No
parser relaxation or format compatibility path is added. The earlier readback mismatch printed
by that trajectory is the intentional current-only negative oracle, caught before continuing;
it is distinct from the final Reopen failure. Source edit/contact/pixel progress in a failed
trajectory is not complete native qualification.

### Visible Runtime Native Qualification

The corrected complete DX12 software run `31a15cf8-a24c-4879-9fb0-7ed3ccfba677` passes 3/3:
the full authored collision workflow, its actual acceptance-panic cleanup control and the
unchanged joint Material/history trajectory. It explicitly uses Microsoft Basic Render Driver
with forced fallback. RTX4090/DX12 hardware run `9c202b2f-30b3-41f8-a9d4-86c47b36ce6a` then
passes both new native tests 2/2 with forced fallback disabled. The unrelated ordinary CPU case
is outside each ignored-only selection. These runs use Windows x64, Rust1.95, the Parallel
profile, locked offline release, the original target, one build job and serial nextest.

Both full trajectories publish eleven original images. Each image compares all 128x80 pixels
against source-field geometry and actual native-pose endpoints, independently reconstructed in
f64. Opaque alpha, body/platform/clear colors remain exact. Although the oracle admits at most
eight tightly bounded geometric-edge candidates, both actual runs report zero candidates and
zero alternatives. The real moving readback rejects the current-only expectation outside every
edge allowance while matching the interpolated picture; its caught negative panic is expected
evidence rather than a native failure.

Real contacts and independent rest heights pass before and after the source platform becomes
reflected/sheared with a different height. Ordinary patches, invalid-patch preservation,
Undo/Redo and actual enveloped disk Save/Reopen retain exact source values. The same collider
entity supplies ordinary Sprite geometry through its local center/anchor mapping. An original
unprepared CPU input first realizes after its original World and native bodies are destroyed;
published old outputs remain exact through later work and producer retirement. A fresh Runtime
from the reopened source reaches the same settled image. Original prefix, World-drop,
zero-native-body and zero-Host-resource/metadata/fault checks pass. The injected acceptance
panic occurs only after a real publication, then original Runtime/solver/producers/Host retire
before its original payload is checked. These are offscreen execution/readback results, not
native-window display, scanout, a Physics-on-GPU or performance measurement claim.

The final canonical-provider motion cohorts pass 48/48 in Parallel-enabled run
`01eb6036-4c71-4c71-a292-8c630c34d508` and 42/42 in default run
`3ad1beec-c495-42e6-9055-c8c0aba108e1`, both without skips. These results follow the export of
the original provider and preserve missing-codec candidate repair plus all original motion,
prediction/PCM, source/history, transformed-contact and managed retirement controls. The first
canonical integration compile had one missing `PersistentComponentProvider` import in the
consumer's source-record encoder; restoring the required trait import fixes that mechanical
error without altering codec behavior or test assertions.

Final adapter inspection strengthens the native stream trajectory after that passing checkpoint.
The original version creates a producer for each image, which proves immutable captures and
retirement but does not challenge continued target/source updates through one ordinary live
stream. Retain one original producer across the running Runtime, source edits and Undo/Redo;
create separate streams only for late old-input replay and the reopened fresh Runtime. Require
exactly three producers for the eleven publications and retain every original pixel, prefix,
history and cleanup assertion. The earlier native pass remains its producer-per-image scope;
the strengthened continuous-stream source needs its own affected native rerun.

Default-feature ordinary visible-consumer run `f97e2d73-0c31-4f66-bfeb-f3bcba3ddaff` passes
9/9 with two native cases unselected. It executes the same real Runtime paused edit/history/
exact-Step and independent math controls under Serial, separately from the Parallel 9/9 run.
The 10m49s release build is recorded separately from 0.322s test execution. No throughput
conclusion follows from compiler time or these short correctness cohorts.

### Final Continuous-Stream Qualification

The strengthened final source passes the complete same-stream workflow and acceptance-panic
control in these explicit native selections:

| Profile / adapter | Result | Nextest run |
| --- | --- | --- |
| Parallel / DX12 Microsoft Basic Render Driver, including original joint history | 3/3; one ordinary CPU case outside ignored selection | `93537b0a-41d0-4f4e-9cff-d27d73e97ab8` |
| Parallel / DX12 RTX4090 | 2/2; one ordinary CPU case outside ignored selection | `04a69f79-4228-4204-9614-5f79a448a685` |
| Serial / DX12 RTX4090 | 2/2; one ordinary CPU case outside ignored selection | `559b07a5-6fda-4ff9-8ac7-b37b118a94fa` |

Every full trajectory records eleven publications through exactly three producers: nine
successive original-Runtime frames share one producer, late original input replay uses a second,
and the reopened fresh Runtime uses a third. All pixel, actual contact, source/history, exact
Step, original-prefix and retirement assertions remain unchanged. All three runs report zero
edge candidates and alternatives. The independent bounded adapter review finds no P1/P2 in
continued stream/target routing or its success/panic cleanup. The earlier per-image producer
results remain historical checkpoints, not substitutes for this stronger execution.

Release/locked/offline correctness Clippy passes for the entire independent motion consumer with
`production-parallel` and all targets, and for all targets of the independent render-policy
workspace with that feature. Existing non-correctness warnings remain visible, including
target-specific unused test helpers. The normal/default, owning and actual GPU controls above
are separate from compiler checks; no unrelated whole-Editor or unchanged framework matrix is
rerun. Seven research/implementation/review agents close at the end of the unit. CI adds the
two ignored authored-collision cases to the existing explicit DX12 software job; ordinary CPU
cases already run in the independent render consumer job. Local configuration changes do not
claim remote CI execution or publication.

Run these from the root with Rust1.95, one build job and the original absolute root target:

```text
cargo +1.95.0 nextest run --manifest-path tests/fixtures/motion-execution/Cargo.toml --locked --offline --release -p motion-execution-consumer --features production-parallel --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --locked --offline --release --features production-parallel --test authored_collision_math --test authored_collision --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --locked --offline --release --features production-parallel --test authored_collision --test joint_history --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail --success-output immediate
```

The last command requires explicit `WGPU_BACKEND=dx12` and `NARA_WGPU_FORCE_FALLBACK=1` for the
software cohort; use `0` for the requested hardware and omit `production-parallel` for Serial.
A missing/wrong adapter fails. These are offscreen Windows execution and retained-output
correctness results. They do not qualify native window presentation, physical controller ingress,
foreign execution, bit-exact replay, dynamic parent/scale support, another shape family, official
backend selection, numerical performance budgets or all R4c/R10/Foundation. Editor/Dialogue
product work remains paused.

After the final mechanical simplification of direct returns and exact color membership checks,
Parallel CPU run `4b4d037f-baf1-4a2f-a3e6-6e1cc9341108` passes 9/9 and the affected DX12
software rerun `2f0ad7ff-5508-4073-8d06-6bef2086a92c` passes both authored native controls 2/2.
The original same-stream eleven-publication/three-producer facts and zero edge alternatives
remain. Final render all-target correctness Clippy passes again. Root and both independent
workspaces pass format checks; added local links/fragments, TOML/locks, CI YAML syntax and Git
whitespace checks pass. The actual lock retains every original package version/source and all
previous dependency edges, adding only required authoring/Scene serialization/temporary-file
closure. There are no UI/font/layout additions.

Local commits `1532cdf` (original domain Schema/time) and `5197e64` (visible consumer/CI) retain
their coherent ownership and qualification scopes. There is no remote publication, main/branch
movement or change to paused UI/Dialogue work. The selected visible-runtime successor is closed;
the full Foundation Goal and remaining R4c/R10/R12 requirements stay active.

## Swept Character Movement Through The Actual Game

Status: shared package/fixture reuse committed at `d76aced`, actual game integration at `86eef47`;
source, native, delivery, actual rebuild/adoption and final checks pass. This selected unit is complete.
Runtime capability remains the priority. Replace the game's direct coordinate update and boundary
clamp with native shape sweep/slide against authored static walls, through a reusable concrete
Rapier module. Finish source gameplay, Pause/Step, scene replacement, exact reconstruction,
native presentation and source-free delivery before another cost experiment. Editor/UI/Dialogue
work remains paused.

### Selected Ownership And Interface

Add `nara_rapier2d` as a directly consumed optional integration package, without adding it to
Nara's default product or defining a portable Physics backend. It owns immutable static native
query scenes, the native top-down character preset and affine box construction. Expose the
underlying Rapier query interface for independently authored queries and controllers. Reuse
Rapier0.33.0/Parry0.28.0; do not implement another sweep, broadphase or slide algorithm. Extract
the existing fixture's fixed affine construction into this package and delete the displaced
construction there. The fixture retains its original Schema lineage, finite bounds, dynamic
solver/world, diagnostics and writeback contract.

The game owns its durable `CollisionBox`, Scene roles, room bounds, desired velocity, player
Transform, clock and progress. The concrete package has no World/Entity mapping, scheduler,
native window, background worker, mutable transform authority or generic persistence service.
Static query scenes compile from admitted root Scene data during content preparation and refresh
once through the original collision-only pipeline. They are immutable thereafter; a replacement
level brings another original scene. Actor shape data is prepared when its authored definition
or restored rotation/scale changes. Moves perform only current numeric input checks and the
native query. Full Scene, shape and provenance validation do not repeat on each slide or tick.

The original game plugin writes the returned effective translation once, then evaluates pickups
from that actual position. Marker creation keeps its existing pre-move position. Animation still
follows held intent while blocked. Existing fixed time supplies both desired displacement and
the native duration; no extra simulation runs during paused content preparation or restoration.
This root-only game does not reinterpret a stale FixedInput cut after same-tick Retry. Static
geometry comes from the admitted immutable root data, and the original game remains the actor's
pose writer. This does not add a hierarchy traversal or change Transform completion.

Keep the existing persistable player-center range `x=+-280`, `y=+-150`. Limit the desired endpoint
to that range before the native sweep. If a finite native slide would finish outside this game
domain, retain the original admitted pose rather than clamp the native result or degrade the game.
This closes the small restored actor counterexample: scale0.5 at `(279,80)` must stop normally and
remain capturable under held Right. Default18x18 geometry still stops at the actual wall margin
before this center limit. This is a game-owned movement restriction, not a second collision solver.

Four visible outer walls replace the old clamp, with inner surfaces at `x=+-289`, `y=+-159` for
the 18x18 explorer. One visible interior wall differs between Meadow and Cavern, making stale
collision scenes observable. Preserve the six original objects, IDs, positions and all gameplay
facts; the default room now has eleven authored members. The catalog remains six members: two
Scenes, three Images and the Sound Bank. Migrate exact member/pixel assumptions deliberately,
including rectangular sprites and Tilemap witness regions. Do not mechanically replace every
literal six or weaken score, marker, source/receipt, lifetime or clear-pixel assertions.

### Placement, Restoration And Qualification

Rapier's character penetration recovery is empty in this installed version. Admission must
check spawn/checkpoint/restored poses against the prepared query scene. Reject overlapping or
unrepresentable poses before committing their placement. Keep `ResumeState`'s bounded wire grammar;
finite zero scale can remain format-valid while a collision operation refuses degenerate geometry.
The selected controller retains no velocity, gravity, moving-platform, contact-history or solver
state between moves. Reconstruct query geometry and actor shape from source plus existing TRS;
do not serialize native handles/BVH or add an arbitrary solver-state recovery claim. New dynamic
or historical behavior must reopen this eligibility policy.

Pinned Godot `7a37606a433c0e7735525037cdf774f043146a21`, CharacterBody2D floating movement and
PhysicsBody2D motion tests, confirms native motion results and bounded sliding. Installed Rapier
`control/character_controller.rs`, `pipeline/query_pipeline.rs` and `collision_pipeline.rs` own
the selected sweep, overlap and collision-only refresh APIs. Their packaged VCS metadata is dirty;
the installed0.33.0/Parry0.28.0 source is the API evidence. Native slope/ground defaults are not
top-down semantics: select sliding without snap/autostep and qualify all movement directions.
Shape validity belongs at construction/change; body/query freshness and current numeric inputs
belong at operation admission. Trust unchanged native geometry and traversal internals.

Qualification covers free movement, wall stop, projected diagonal slide, corner stop and a thin
wall crossed by a large requested displacement whose endpoint is unobstructed. Compare actual
native results and visible wall/player pixels. Complete Pause/exact Step, travel/Retry, invalid
placement preservation, contact-state fresh reconstruction and continued state, retained original
scene lifetime, real Rust body/structure adoption and relocated source-free play. Preserve prior
animation/audio/checkpoint/refusal/cleanup controls and keep speed-discrimination probes in a
genuinely unobstructed corridor. Run Cargo serially with Rust1.95, jobs1, root target, release
locked/offline nextest; do not run a backend/profile/workload Cartesian product or benchmarks.
Record actual results and remaining scope here and in the existing ADR/ledger/guides. Full
R4c/R10/R12/Foundation, dynamic Physics production selection and foreign execution remain open.

### Initial Character Integration Checks

The initial owning package run `ee0a449a-3fbc-4776-b34f-3151368681f1` passes15/15 without skips.
Independent review subsequently corrects a construction-only precision gap: expected offset and
world corners now use the original f32 inputs promoted to f64, independently of the rounded native
geometry. A large-center/translation cancellation cannot make both sides of the budget comparison
lose the same residual. Final owner run `ce2f83eb-0907-49f0-b613-7c229f61a75a` passes16/16, including
that exact counterexample. No new geometry work is added to ordinary movement.

Game review identifies the small restored actor/center-domain mismatch described above; the game
now limits the requested endpoint and treats a finite out-of-domain native result as blocked. Source
and target-room placement refusals have separate public consumers. The initial library/build checks
also expose an invalid assumption that built-in `Transform2d` implements the derived provider trait.
Content preparation instead reads complete finite TRS from the already-admitted current-version
record, extending the previous translation-only inspection without a second native codec authority.
After the import correction, the actual release headless executable builds, and its original
`--nara-project-tool 1 schema-write` and `schema-check` operations succeed. The authoring bundle is
generated through the real recipe with game binding revision2 and `CollisionBox`; it is not edited
by hand. Required native dependencies are added without removing/upgrading original packages or
rewiring their existing Windows dependency edges. Native/delivery acceptance is pending.

The first actual source-game run `e9fc8163-096c-4f67-a256-3276908d4086` passes32/32 without skips:
six collision workflows, the target-Cavern placement refusal, five prior animation workflows,
seven prior gameplay/save/travel controls and thirteen exact-resume controls. It covers blocked
held animation, diagonal sliding/rounding, actual room bounds, different Cavern wall data and Retry,
contact-state fresh reconstruction/continued canonical state, mirrored/nonuniform/rotated shapes,
same-room overlap/zero-scale early refusal and the small-actor persistent-domain boundary. The
different-room rejection preserves the fresh candidate's original Scene/Progress/player before
target publication. These are real public headless games, not native-window or packaged execution.

Independent motion run `796ef49e-dda3-4354-a9b5-e3d4e0093227` passes42/42 with its original locked
dependencies and default managed execution. The fixed branch now consumes the shared affine box;
its native body receives the original world pose and its attached collider receives an identity local
pose. Existing bounds, Schema/provider lineage, dynamic rigid-pose writeback, contacts, source/history
and prediction controls remain. No separate native solver or transform ownership is added. The
render consumer lock gains only this package and its dependency edge; its renderer is unchanged.

### Source And Native Character Qualification

The first selected desktop/cost/cook/Player/audio build exposes two test-integration omissions:
the hand-built `Content` baseline fixture needs a valid collision scene, and the audio module's
include alias must export `levels` for the shared member-count constant. Both are corrected without
changing production behavior or removing their original assertions. Its ordinary run then passes
324/326, with two copies of one new workload assertion failing. Scene declarations are canonicalized
by ID; authored root ordering belongs to `placements`. The corrected test requires the complete
canonical declaration vector and the complete original root-placement vector separately. No Scene
ordering or loader change is made to satisfy the test.

Final ordinary run `b49a1de2-85ab-4c79-911a-c923cd266fbd` passes **326/326**, with thirty explicit
native/build cases skipped for separate selection. The closure is
`desktop,cost-measurement,cook,cooked-player,audio-cpal` with ordinary development defaults. It
includes both copies of the workload correction, the independent rectangular-wall/atlas pixel
oracle, source and cooked content, audio CPU controls, exact lifecycle and prior game behavior.
The optional scene-work observation feature is not selected; no new workload timing is measured.

Actual Windows/DX12 hardware reconstruction passes1/1 at
`c686900d-947e-4952-a1f0-0c71d18ab30d`. The original exact Step and keyboard reconciliation owners
drive contact at Cavern's `x=4.99`, continued blocked intent/clock/animation and W+D sliding above
the baffle. Existing complete-state restoration, release gap, nonfirst atlas frame and three original
native retirement checks remain. After both game Worlds and native targets retire, ten retained
input replays include the original six animation samples, original/restored equality and the two new
collision samples. The new GPU comparisons require visible player and rectangular-wall pixels.
This is independent retained-input rendering, not swapchain readback or scanout measurement.
Relocated source-free physical delivery and real Rust-edit adoption remain pending.

The first relocated delivery builds and starts the real shipping Player but fails only its new wall
evidence expectation: a JSON array mixed one f32 and three inferred f64 color literals. Expected
values now use explicit f32 for every channel, preserving exact comparison with the admitted
runtime `Color`. No tolerance or production color change is introduced. Corrected actual delivery
run `a2429d2d-0d17-45ca-8665-8e58bcd6e0e9` passes1/1 (one private worker is deliberately selected
by its parent). Its original feature-closure control excludes development, tooling, watcher and UI.
The source/cache is physically removed from the relocated installation. Both normal interactive
processes, the explicit software first-presentation process, scripted controls and original refusal
paths retain their cleanup and progress assertions.

Matched physical D/W input reports Meadow contact `(-15.01010036468506,0)` and Cavern contact
`(4.989899635314941,0)`, against expected `-15.01` and `4.99`. A 350ms matched hold preserves each
contact while actual ticks/presentations advance. W+D then slides and clears each wall; released
positions are approximately `(-3.69639,45.33760)` and `(16.30361,45.33760)`. Scores/markers stay
unchanged through these excursions. Save/Restore, four atlas frames, twenty further physical travels,
complete eleven-member receipt/shape/TRS/Sprite evidence and unchanged stored checkpoint bytes
pass. The catalog remains six members and the original image residency bounds remain. Native
retirement reports preserve destroyed-window, empty-containment, stopped-reader and drained-output
facts without forced kill. The first cold build duration is not a Runtime performance measurement.

Actual source/headless rebuild adoption passes1/1 at
`c6e5e329-1a3c-405a-bd43-bd6dd8263def`. The original production Host performs real Rust body and
private-state structure edits, actual Cargo builds and fresh process reconstruction. Probe gameplay
is discarded before the fresh adopter restores the original basis. Changed speed executes after
adoption; compile-error, wrong probe, source drift, exact nominal tickets, cancellation and separately
pending predecessor retirement remain covered. The deliberate compiler failure is its original
negative control. New direct package paths are rewritten only in the owned temporary Cargo copy.
This retains a complete game development workflow; it adds no code-patching service or desktop
adoption claim. Native physical/window and source-free acceptance above supply their separate scopes.

The selected source-free desktop and reconstruction paths qualify Windows/DX12 hardware, with an
explicit software first-presentation control. No foreign-platform execution, arbitrary dynamic solver
history, moving platforms, gravity, dynamic collision authoring, new shape family, numeric performance
budget or full R4c/R10/R12/Foundation completion follows. Editor/UI/Dialogue work remains paused.

Final all-target correctness Clippy passes for `nara_rapier2d`, the original motion-physics authoring
selection and the game's tested desktop/cost/cook/Player/audio closure. Existing non-correctness
warnings remain; no warning-free claim is made. Root, sample and independent motion workspaces
pass format checks. Added Markdown links/fragments, manifests/locks, Scene/schema JSON and ordinary
Git whitespace checks pass. All twelve scoped implementation/review agents are closed. The fifteen
paused UI/Dialogue files remain byte-identical to their original receipt and are excluded from commits.
Verification uses Windows11/Rust1.95, serial jobs1, the original target and locked offline release builds.
The meaningful runtime feature is delivered before further micro-optimization; no new benchmark,
compact encoding, universal backend, worker or lifecycle authority is added. Commits remain local.

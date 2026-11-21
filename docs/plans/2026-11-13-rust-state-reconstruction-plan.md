# Rust State Capture, Rebuild And Reconstruction

Date: 2026-11-13. Updated: 2026-11-21. Status: qualified for the shipped Windows source/headless
workflow and explicit source/desktop reconstruction/adoption through the finite development Host.
The packaged Server continuation also passes Windows and Linux WSL2 release execution. Continuous
Editor Play integration and complete R12 remain open. Owner: the foundation rebaseline R12 runtime
lane.
Editor/UI/Dialogue product work remains paused. Physical mixed-DPI testing is separately
authorized and has bounded [Runtime drawing/picking qualification](2026-11-14-durable-material-and-shader-plan.md#presented-drawing-and-picking-successor).
The qualified functional successor joins the existing desktop development Host to the actual
[audible game with explicit effect reset](2026-11-03-joint-motion-execution-plan.md#audible-development-with-explicit-effect-reset).
It is not another cost-attribution or representation experiment.

## Complete Outcome

In the independently compiled Two-Level Game, change an actual Rust function body and then a
Rust state structure, build each selected executable through the existing Cargo adapter, restore
the same meaningful playable situation in a fresh Runtime, and continue gameplay under the new
implementation. Retain the original paused game and its exact captured executable until the
candidate passes the declared state and continued-execution oracle. Capture, compile, startup,
compatibility and restore failures must leave a usable original game. A checksum helper,
progress-save reload, unchanged executable restart or successful startup alone cannot complete
this unit.

The owning contracts are [ADR0024](../architecture/adr/0024-determinism-fixed-update-and-replay-policy.md),
[ADR0076](../architecture/adr/0076-play-runtime-debug-control-and-observation.md),
[ADR0093](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md),
[ADR0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
and [ADR0116](../architecture/adr/0116-standalone-supervised-editor-play.md). The existing
[game plan](2026-10-20-persistent-two-level-game-plan.md) owns progress saves and their migrations.

## Source Findings And Selected Ownership

At `493837d`, Two-Level Game saves progress and reconstructs the player at an authored anchor.
It does not restore the live player position, held semantic movement intent or game time.
Movement has no independent velocity state: it derives direction from held intent and uses the
fixed delta. The exact state inventory must follow that actual model rather than claim a saved
physics velocity that the game does not own.

`HeadlessRun` prepares and drives the original private `ProjectHost`, captures an outcome and
then stops it. Public managed Runtime control already exists below that Host, but the public
source-product path cannot retain a running or paused game between caller operations. The first
implementation prerequisite is a persistent `HeadlessSession` facade over that same Host. It
reuses the existing source admission, Runtime publication, Pause/Step/Resume controls, gameplay
queue and physical cleanup. It introduces no second Runtime, driver, task facility or queue.

The first end-to-end process consumer belongs to the game executable and its test Host. It may
use an explicit finite request/reply protocol to hold its own source Runtime while another
captured executable is built and checked. Process creation, captured executable installation,
timeouts and physical retirement remain in the executable/test Host. The game library owns
typed state, validation and reconstruction. This does not change the existing supervised desktop
one-READY protocol. Any later integration with supervised Editor Play must amend ADR0116 and
use its actual generation and containment owners.

The selected game path keeps the ordinary product recipe identical between original and candidate.
A fresh paused candidate admits one finite resume command through its original gameplay queue;
the first exact transaction starts preparation, and paused TaskUpdate maintenance completes the
existing Scene request/commit and derived reconstruction. Capture checks the actual App
configuration as well as Schema/content facts. The game explicitly classifies its shipped
headless services; extra plugins, source watchers, scripted verification and unsupported native
services do not silently become checkpointable. Nonzero time remainder/debt and unsupported time
scale refuse the first format rather than being omitted.

The concrete `continuity` binary uses an explicitly supplied trusted Host-owned temporary control
directory. Bounded versioned request/reply files carry the original session/generation and strictly
increasing operation ID, separately from stdout/stderr diagnostics and project source. One
outstanding request, finite message/operation/lifetime limits and explicit Close keep this a game
consumer, not a new process framework. The actual-build test Host owns the child, captured bytes,
request deadlines and fallback kill/wait; a clean-close reply requires physical Runtime retirement.

## Implementation Order

1. Retain one source Headless Host across explicit start, frame drive, Pause, exact fixed step,
   semantic-command admission, paused observation and physical Close. Startup and execution
   failure keep their original cleanup owner. Closed sessions cannot restart. Public independent
   consumers must prove continued execution, exact stepping, input refusal and teardown.
2. Add a separate bounded game-owned resume format. Preserve existing progress-save semantics
   and v1 migration. Inventory complete progress, active semantic level/instance, live player
   transform, held semantic intent, logical game time and any counters that affect later play.
   Preserve a separate fresh Runtime/App clock; do not forge old Runtime generations or rewind
   frame-internal FixedTime fields. Declare disposable presentation and verification state.
3. Capture only after stable paused `AppFrameCompleted`. Reject active travel/reconstruction,
   pending local/future commands, poisoned/quarantined command state and outstanding authoritative
   services. Include explicit game format/compatibility, Schema and admitted content identities.
   Rebuild native and Scene membership from semantic IDs; serialize no ECS entity, authority
   token, file/task/GPU handle or old input connection.
4. Construct and validate fresh candidates. Reconcile content/build compatibility explicitly:
   compatible gameplay-code changes may differ in executable digest, while Schema/content or
   unsupported state-shape changes fail closed or use a named game migration. Check exact state
   before any continued simulation, then exercise future movement, collection, marker identity
   and travel. Retain the original until the candidate is accepted.
5. Use the existing `CargoBuildRequest` with the actual changed `.rs` files in its input inventory.
   Perform real body and structural edits, actual builds, immutable executable capture and fresh
   process execution. Preserve last-good bytes across compile/link failure. Bind protocol replies
   to the original operation, process/build generation, capture and candidate; reject stale
   replies and timeout without promoting a candidate.

The real-build fixture works in its own copied game source tree. A body edit changes the movement
law so continued execution can distinguish the candidate from the retained original. A structural
edit groups held movement intent into a different private Rust representation while preserving
the declared semantic format. Exact restored state must match before continuing under either
candidate. An unsupported semantic format or compatibility change is a separate rejection case;
the fixture must not silently discard authoritative state or modify the user's source checkout.

## Game State Inventory For The Successor

| Actual owner | Capture and reconstruction policy |
| --- | --- |
| `Progress::checkpoint` | Preserve the complete validated progress, semantic level/instance, collected IDs, marker records and next-marker ID. Existing save format/migrations remain separate. |
| Player `Transform2d` | Preserve finite game-owned local transform in the active level, including the non-anchor position; validate against the game's actual supported spatial range. Rebind the semantic player ID in the fresh Scene receipt. |
| `Session::held` | Preserve four semantic movement intentions. Do not restore old input routes, OS keys, controller connection tokens or queue source authority. The new implementation derives its movement from this restored intention. |
| Game time | Introduce game-owned logical time for exact game continuity. Preserve it across reconstruction, while exposing a fresh Runtime fixed clock independently. Never rewrite Runtime frame flags, generations or arbitrary FixedTime memory. |
| Session counters | Inventory and preserve reported travel/restart/save/restore counts when included in the semantic oracle. Capture refuses error/degraded/reconstruction states; no implicit retry or anchor reset substitutes for restoration. |
| `Travel::pending`, local and authoritative command queues | Capture refuses pending travel, unconsumed local commands, future commands, active/quarantined batches or incomplete acknowledgement. New-runtime commands use its fresh queue/time/source authority; the capture does not revive old keys. |
| Current Scene, dynamic marker tokens and sprite visibility | Rebuild from validated semantic IDs/progress through the existing Scene transaction and marker owner. Prove fresh runtime identities and matching derived values, not equal ECS handles. |
| Verification driver and SaveSlot | Automated script cursor is not an interactive-game resume format. The finite process workflow uses ordinary semantic commands. The first format refuses an open/locked SaveSlot, whose concurrent candidate ownership has no selected policy. Storage retains its original explicit close owner; no live file handle or pending publication is captured. |

The persisted compatibility record must include the admitted Schema and content closure relevant
to every restored level, not only a source pathname or the startup Scene. Executable provenance is
tracked separately so an explicitly compatible body/private-structure edit can change code while
unsupported content or schema changes fail before replacing the original. The exact bounded DTO,
canonical encoding and validation errors belong to the game's `resume` module. They establish
this game's semantic format; no general engine checkpoint Interface is selected.

Per-level compatibility projects the original admitted manifest and runtime-content digest;
Schema and actual App configuration remain separate mandatory comparisons. Startup admission
additionally records an authoring Schema-bundle attestation that ordinary travel does not include.
The full `closure_plan_digest` therefore remains an original admission observation and is not a
cross-path level identity. Initial public tests exposed that mistaken comparison (6/8 passed,
run `9b06f78b-fe80-4964-8c0f-e2657b4baab6`); the game format removes that redundant field rather
than changing the content owner's digest meaning. A dedicated regression restores initial Meadow,
first Cavern and returned Meadow through the same complete candidate path.

## Processed Server Continuation Successor

Status: implemented at `c488d72` with process qualification at `196f003`. Windows x64 and Linux
x86-64 WSL2 package execution qualified. Use the ordinary packaged Two-Level dedicated Server as
the consumer. Capture at the original completed paused frame and restore in a separately
started candidate. Preserve the game's selected progress, pose, held intent, counters,
scheduling phase and still-pending semantic commands from the Server's external stream. Keep
source-level stamps for source reconstruction. A processed capture observes the exact committed
Scene Product and its catalog digest under one Host-owned source view. The product must match
the selected game level. The capture also binds the existing App and Schema fingerprints.

Observe pending intent through the original gameplay queue. Form a game-owned bounded value
using offsets from the acknowledged boundary and retain order within a tick. Reconstruct the
semantic commands through ordinary admission against the candidate's fresh fixed watermark.
The old queue counters and Runtime generation stay process-local. Eligibility still requires
closed rooms, no open checkpoint slot, no unresolved Scene work or unsupported native services,
and a healthy queue without an active or quarantined batch. An unknown pending producer or an
unrepresented effect must refuse capture.

Use the existing `--serve` control stream for pause, capture and explicit drives. A fresh
`--serve --paused --resume-state` startup loads the game's bounded continuation format, applies
its original Scene transaction and time policy, and compares a fresh canonical capture before
Ready. The control client can retain and explicitly retire a paused incumbent without executing
its transferred pending work. This is a local handoff workflow. The command-ingress, Scene,
Runtime and physical process owners keep their existing responsibilities.

Compare separately launched processes from the ordinary relocated Cook package across a
non-anchor pose, checkpoint progress, held movement, nonzero remainder and preserved debt,
pending commands at different relative ticks, and continued execution. Prove fresh queue
watermarks, exact canonical equivalence, a refused incompatible candidate with the incumbent
still usable, and original process cleanup. Target the selected Windows and Linux headless
deliveries. Keep the regular paced service, progress checkpoint and source reconstruction
controls in the affected regression scope.

Bevy's fixed clock accumulates overstep in the time owner, and Godot's main timer derives
physics steps from its own accumulated time. Their Scene loading recreates instances from
admitted Scene content. Nara uses its existing time and Scene owners and the game's explicit
continuation inventory to cross a process boundary.
[Bevy fixed time](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_time/src/fixed.rs) · [Bevy resolved Scene](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_scene/src/resolved_scene.rs) · [Godot timer](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/main/main_timer_sync.cpp) · [Godot Scene instantiation](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/resources/packed_scene.cpp)

### Processed Continuation Qualification

The Windows Rust 1.95.0 release Server run `0a6db4fb-40c0-446d-92a3-f5a85b9eec2b` passes the
ordinary Cooker, relocation and source-free processes. The incumbent reaches a non-anchor Cavern
instance after collecting three pickups and authoring three markers. It captures at local tick
53 with 20 ms of preserved fixed debt, 7 ms of remainder, and three pending commands at offsets
2, 4 and 8. A candidate with the wrong catalog digest refuses before Ready. An independently
launched candidate from the admitted installation starts its local fixed history at tick 1.
Its new paused capture matches all 1,747 canonical bytes. Both processes follow the same six
elapsed frames and converge after the first two pending commands. The incumbent retires with the
last command still pending. The candidate executes that command once at its relative boundary,
resumes paced operation, accepts new input and closes. The process owner verifies complete native
retirement for all sessions. The previous live Server, checkpoint and mismatched-pack controls
remain part of this execution. [Packaged continuation test](../../samples/two-level-game/tests/support/server_delivery/continuation.rs)

The first source/processed format suite passes 44/44 (`b9443c91-89ae-45e7-af13-6bad09511905`).
The Windows source reconstruction, task return, gameplay, replay, authored content, collision
and Server control selection passes 232/232 (`ad26d952-613d-4303-a038-5537159e6c16`).

Independent review found that a Cook with the same game's startup and travel roots swapped could
capture the committed Cavern as Meadow, that an unrelated recipe could panic during capture or
restore, and that a nested positional command could enter the version-one format. The game now
compares the hosted Product with the level before state capture or reconstruction. It refuses a
missing dedicated game recipe before reading game-owned state. The processed command payload
requires a named object, while source and desktop command readers keep their existing grammar.
The follow-up Windows release run passes 194/194 (`ab47d1cc-2f06-4ae2-8caa-f9e066a855ac`). Its
Cook controls start both mismatched sessions at tick zero and prove that the typed refusals leave
the original committed Scene instance in place. The related Gameplay and independent renamed
HeadlessSession suites pass 58/58 and 12/12 (`7274497e-035b-41d3-99e9-93a0b3c2beaa` and
`e987408a-4951-4326-aea9-6482cf051eab`).

After the review corrections, the Windows production Cooker and relocated headless Player and
Server pass 2/2 (`0cc96951-e541-4331-9f56-909f0b896e5f`). Both start with no source or Cook
cache. The Server keeps the original live control and checkpoint trajectories and repeats the
full compatible continuation and incompatible-candidate refusal.

Ubuntu 26.04 on WSL2 passes the same Rust 1.95.0 release delivery cases 2/2 on ext4
(`5e67915b-32d7-4c7f-9e53-2e5db441b897`). Both cases use the locked Cooker and their ordinary
relocated processes. Each Server handoff compares the exact captured bytes and closes its own
native owners. Verification covers process handoff within one compatible package on each tested
platform. Cross-machine continuation, networking, live SaveSlot transfer and arbitrary plugin
state remain open.

The minimal `cooked-server` release library passes 54/54 after the review corrections
(`c277cb1f-47c0-4a9f-bd28-dff8cc77502d`). Correctness Clippy passes for the generated shipping
Server and the affected sample library, Cook and packaged test targets. Root and sample
formatting and added document links pass.

The shared source reconstruction path also passes its original Windows DX12 hardware case
(`0d0a288c-178d-43af-a343-6a031d402bbc`, 1/1). It checks fresh Runtime and native owners,
the captured player and animation pixels, reconciled keyboard holds, and complete window,
surface and physical event-loop retirement.

## Qualification

- Run Cargo serially, with one build job and the existing root `target`; retain the qualified
  Rust 1.95 baseline. Add explicit 1.100 evidence when it covers this actual workflow rather than
  reinterpret the earlier four-library compile as complete product acceptance.
- Exercise source and processed Headless lifecycle consumers where the new facade is supported.
  Test startup failure, initial pause, full-frame deferred work, exact fixed stepping, accepted
  and refused semantic commands, runtime fault, terminal AppExit, pending close and repeat Close.
- Use the actual game for full state capture/reconstruction and continued-state comparisons.
  Include non-anchor position, held movement, at least one collected pickup and dynamic marker,
  a second level/semantic instance and fresh Runtime/Scene identities.
- Exercise real body/structural compilation and candidate execution, build/capture/restore failure,
  incompatible Schema/content/state and stale replies. Inspect process retirement and retained
  original continuation together. Do not replace these assertions with startup READY.
- Independently review Host lifecycle, game inventory/compatibility and process adoption. Run
  relevant focused nextest, owning checks/Clippy, formatting and document checks. Record actual
  distributions before making latency or memory claims.

## Current Evidence And Limits

The retained Headless lifecycle prerequisite is implemented. It adds `HeadlessSession` over the
existing private `ProjectHost`, with source and processed constructors, explicit start/frame/
Pause/Step/Resume, read-only observation and paused capture, original gameplay-queue admission and
polled physical Close. The Host distinguishes typed control refusal from execution failure.
Normal command/control refusal leaves the original game usable, and post-admission fault retains
the actual command result. AppExit refuses further execution and input while preserving final
observation and normal close. A session cannot restart after failure or close.

Independent review found three actual errors in the initial facade: exact fixed transaction was
incorrectly treated as a complete App-frame boundary; simulation-not-ready refusal permanently
disabled the incumbent; and AppExit did not prevent later driving. The fixes run one complete
paused maintenance frame after each successful exact tick, preserve the existing control rejection
reason and retain terminal exit. The independent regressions exercise the exact sequences, and a
bounded rereview found no remaining P1/P2. The first all-feature compilation was deliberately
cancelled before test execution to apply those corrections; it supplies no acceptance result.

Executed on Windows x64 with Rust 1.95.0, release/locked/offline, one build job, serial Cargo and
the existing root `target`:

| Scope | Actual result |
| --- | --- |
| Production Runtime libraries | Minimal source-product/serde/tooling compile and root all-feature production-library compile pass. |
| Original processed base after source removal | 1/1, run `97a336f4-cd28-42df-8b19-889d066b1e64`; retains the exact base, prepared Image asset and Runtime generation through pause, step, resume and physical close. |
| Affected root Host and public boot | Final 111/111, run `f0914c31-840c-4f5c-a657-5ee4641d1193`; 90 Host and 21 public boot cases, 551 unrelated cases filtered. It repeats the earlier `357a7452-a983-4552-89af-b154a458f99b` result after final typed Pause-refusal propagation and includes the processed-base case above. |
| Independently locked renamed consumer | Final 12/12, no skips, run `418c7075-563d-45bc-9821-d37c1fca6e62`; full-frame/Last/deferred effects, direct initial-paused step, recoverable readiness refusal, commands, source custody, original/candidate isolation, AppExit, actual frame-execution failure and pending/completed-error Close. The earlier 11/11 run `e1f058d1-8eb2-4b3b-8944-89be352e4039` preceded the added execution-fault regression. |

The final root run and independent run include the final typed Pause-refusal correction. Root
correctness Clippy, independent test-target Clippy with `-D warnings`, and root/independent fmt
checks pass. The final independent suite adds an actual frame-execution failure to separately
exercise diagnostic and physical cleanup custody. Its 12/12 run and repeated strict Clippy pass
include that case. Existing feature-specific unused/dead-code warnings remain and are not new
runtime failures. Changed-document links and whitespace checks pass. The existing
root runtime-runner contract executes the renamed package's tests, so no new CI entry or verifier
script is needed. That prerequisite qualified CPU lifecycle only; the game/process successor has
separate evidence below.

### Game Reconstruction And Actual Rust Builds

Implementation revisions: source-content stamps `0762e9c`, game state/reconstruction `57bc267`,
and finite process/actual-build consumer `1ae8f59`.

The source/headless game now owns `ResumeState`, separate from its progress-save format and v1
migration. It captures complete progress across semantic level instances, the active player's
finite local transform, four held movement intentions, logical game time, reported counters and
compatibility. The bounded versioned encoding contains no World, Scene, command, file, task or
native authority. The fresh candidate keeps its own Runtime tick and reconstructs through one
command in its original queue, original Scene preparation/commit and paused maintenance frames.

`ProjectSceneContentStamp` copies the original admitted Schema/manifest/runtime-content/full-plan
observations without retaining source content or budget. Prepared and committed saved-source
Scenes expose that stamp. Direct, processed and Editor working-copy Scenes return `None`, so an
unsaved projection cannot attest original disk bytes. Game compatibility selects per-level
manifest/runtime-content digests plus global Schema and actual App configuration; it does not
reinterpret the admission-specific full-plan digest.

The finite `continuity` binary retains one ordinary source `HeadlessSession`. The actual-build
fixture copies the game into an exclusive verified temporary directory, uses public Cargo build
selection/capture and installs immutable executable bytes. It performs an actual movement-body
edit from 160 to 80 units per second, then moves private held-state storage into `MovementIntent`.
The semantic DTO and ordinary product recipe stay unchanged. Independently created native
processes restore exact state before continuing; the original process retains its old movement
law after Cargo output has changed twice.
The outer qualification binary uses release mode; the public adapter selects the inner game's
`dev` profile and explicit `x86_64-pc-windows-msvc` target in the existing root `target`. Compiler,
lockfile and offline policy remain fixed across A/B/C.

| Scope | Actual result |
| --- | --- |
| Affected Engine content/Scene owners | 60/60, run `087175c1-d399-4fd6-b963-ee3f4b47210e`; 790 unrelated tests filtered. Covers original admitted stamp facts and budget release, frozen prepared/committed source through disk changes, Source/Direct/processed/working-copy distinction and the existing Scene request/retirement controls. |
| Complete ordinary sample package | 65/65, run `075f5cb0-134b-4173-aae9-14badc53bace`; the nested-build test is the one explicit skip. Includes bounded format/save migration, all nine public resume workflows, ordinary travel/recovery and binary timeout/close regressions. |
| Actual Rust build and native-process workflow | 1/1, run `26250a18-aad4-41da-b158-7bfd39f52ea2`; the ignored test is explicitly selected. Actual A/B/C executables, meaningful three-instance capture, body/private-structure edits, exact state, continued movement/pickups/marker identity/travel/Retry, build/startup/configuration/Schema/content rejection, stale requests/replies, immutable incumbent bytes and physical Close all pass. |

The first actual-build run `df956b44-4b08-40ed-9884-ad5567fcff99` reached the intentional
`compile_error!` case, then failed its test oracle: the public adapter correctly returned both
non-success exit and `Some(CargoBuildError::BuildFailed)`, while the fixture incorrectly required
no protocol error. The corrected assertion requires that exact failure variant and the distinctive
structured compiler diagnostic. It changes no build-owner behavior or game-state assertion. The
successful successor takes 44.970 seconds in this warm uncontrolled run; this includes several
builds and process trajectories, and is not an edit-to-result distribution or performance budget.

Independent game/content and adversarial process reviews are closed with no remaining P1/P2.
The content review removed the startup/travel full-plan mismatch described above and corrected
the test's cross-World identity oracle to compare `WorldIdentityDomainId` together with the
World-local Scene number. The adversarial review corrected three issues: Close now retries the
same retained owner after timeout; a failed post-admission operation fences successor execution;
and a Host observing child exit rechecks the already published final reply before reporting an
early-exit error. Actual binary regressions exercise the first two; the complete process workflow
retains exact Close and request/reply identity checks.

The final root all-feature production-library check passes. Root all-feature library Clippy
and the sample's no-default-feature all-target Clippy pass with `-D clippy::correctness`;
root and sample `cargo fmt --all -- --check` also pass. This is a correctness gate, not a
warning-free claim: existing feature-specific warnings and advisory error/enum-size and style
warnings remain. Changed-document links/headings and whitespace checks pass. Existing CI already
compiles and executes the ordinary sample package; the real nested build remains an explicit
serial qualification command in its README. No unrelated Editor/native/component matrix is
repeated for this CPU and process workflow.

### Remaining Scope

This unit supports the shipped source/headless recipe on Windows x64 with Rust 1.95.0. It refuses
open SaveSlots, unknown plugin services, pending authoritative tasks/commands/travel, unsupported
Runtime time policy and direct/processed content rather than omitting their state. It does not
implement general service migration, arbitrary World snapshots, historical tick-log replay,
dynamic-library replacement or a generic process supervisor. Existing supervised desktop Play
retains ADR0116's one-READY protocol; a production UI action or automatic process adoption needs
that owner's integration and qualification. Native UI/GPU, Linux/macOS execution and latency or
memory distributions remain unqualified. Editor UI and physical mixed-DPI work stay paused.

Existing R6 automatic source refresh and R10 input/replay evidence remain under their owning
plans. The complete R12 and foundation Goal remain active; this source/headless rebuild/restore
evidence does not substitute for those broader production and platform outcomes.

## Shared Native Process Custody Successor

Status: implemented and qualified on Windows x64 as the supervised reconstruction prerequisite;
post-start desktop adoption and Linux execution qualification remain open.

Source inspection at `98c68e9` finds two real consumers with different physical guarantees. The
Editor already owns bounded native process execution, Windows Job containment, exact executable
leases, diagnostic/protocol output and completion-gated reader retirement. The game reconstruction
Host directly owns `std::process::Child`; its previous acceptance proves Runtime Close and direct
child reap, without the Editor's process-tree guarantee. Move the existing native implementation
to a UI-independent `project_tooling::process` module and use it from both consumers. Do not copy
the platform implementation or introduce a process-provider trait, scheduler or second supervisor.

The shared module owns only a concrete native process and its physical resources: bounded
command/environment selection, exact executable-handle launch, an explicitly requested inherited
bootstrap endpoint, bounded output, termination and polled cleanup. A cleanup receipt requires
the original direct child, containment and output readers to finish. Drop remains a fail-safe.
Wake callbacks carry no lifecycle authority. Editor-specific build/generation preparation and
the asynchronous spawn worker remain with the existing Editor owner; game capture, compatibility,
request identity and reconstruction remain with the game. The Editor's manifest lease retains
its original platform guarantees for the child lifetime; Windows write denial is separate from
Linux descriptor custody and cooperative immutable-store policy.

Implement and qualify the complete prerequisite through both existing consumers:

1. Move the existing platform/output/log/wake code behind the shared concrete Interface. Preserve
   original Windows suspended exact-image/Job/handle-list admission and Linux descriptor/process-
   group behavior, with unsupported-platform refusal. Keep build stdout's cumulative protocol
   allowance separate from diagnostic-tail truncation. Remove the replaced Editor implementation.
2. Migrate the actual game A/B/C Host to that owner, retaining the original semantic and executable
   assertions. Bind launch to its existing immutable executable installation, carry explicit
   bounded arguments and environment, and expose process IDs only as observations. Normal Close
   requires both the game reply and the shared physical cleanup receipt; fallback termination
   remains separately observed. A rejected or timed-out candidate cannot retire the incumbent.
3. Exercise an independent public consumer without Editor/UI dependencies, and the existing
   Editor process/build/generation/handshake/retirement controls. Include descendant containment,
   inherited-handle isolation, noisy diagnostics versus exact protocol stdout, wake failure,
   late spawn/cancellation and retained cleanup retries where affected. Run the real game edit/
   build/restore trajectory through the shared owner, not a substitute helper-only trajectory.
4. Update the owning ADR and current guide with actual guarantees, results and platform scope.
   Keep physical native tests separate from UI tests; mixed-DPI and Editor UI remain paused.

This extraction does not extend `desktop-supervised-play/v1`. Its bootstrap still closes after
one exact first-present READY, Restart still revalidates the current saved source before retiring
the incumbent, and runtime Pause/Step remain unavailable for that placement. A later explicit
post-start contract must own capture/restore/result correlation, simultaneous incumbent/candidate
custody, qualification side effects, one adopted execution target and exact predecessor retirement.
The shared process consumer does not prove those later adoption semantics or make desktop native
services checkpointable. Complete R12 and the foundation Goal remain active.

### Shared Process Qualification

Implementation revisions: shared native execution and Editor adapters `de16b01`, followed by
the real game Host, independent public consumer and CI selection `e40d3a9`.

The native implementation, bounded output queue, log ring and wake move from the Editor rather
than remaining as duplicated backends. Editor adapters retain generation revalidation in the
original background spawn worker, preserve its typed generation failure, explicitly request the
one-shot bootstrap, and transfer the original restrictive manifest lease into the shared owner.
The game retains all A/B/C semantic/build assertions while replacing its direct child and compiler
probe supervision. Its normal Close now checks the game reply and the original native cleanup
receipt, including output drain; diagnostics use a separate bounded memory tail.

Independent Editor adapter and native/consumer reviews are closed. The latter found one P2 in
the new independent test guard: an unresolved native cleanup could be followed by work-directory
deletion. One guard now owns both resources, retries the same process and releases the directory
only after a real receipt. If the final finite retry cannot complete, the isolated failing test
retains the owner/directory until process exit. The bounded rereview closes that finding; no
remaining P1/P2 is reported in the selected scope.

| Scope | Actual result |
| --- | --- |
| Root process/output/log/wake | 31/31, run `bb9c7c9e-9e15-41e9-86a5-3081168c73fd`; 850 unrelated tests filtered. Windows x64, Rust 1.95.0, release/locked/offline, shared root `target`, one build job. |
| Independently locked renamed process consumer | 3/3, run `b0966bba-75a8-4c47-94f5-b84cf37e4f8d`; two substantive public workflows plus their ordinary no-op helper. Complete 512 KiB binary stdout survives delayed polling and 1 MiB diagnostic overflow; exact capability launch observes sealed environment, EOF stdin, original PID and actual exit/containment/readers/output cleanup. |
| Original Editor supervised consumers | 77/77, run `6076c58f-fb4d-4596-8deb-b5ba7e518a2c`; 148 unrelated tests filtered. Includes build/generation/handshake, stale and cancelled work, exact source/image custody, actual descendant cleanup, inherited-handle isolation and physical retirement retry. The exact native generation test also observes Windows manifest write denial after launch and release after completed cleanup plus owner drop. |
| Actual game rebuild/reconstruction through the shared owner | 1/1, run `32c14786-90dc-420f-8e66-d521f48d071b`. The original A/B/C Rust edits, complete state comparison, failure/stale-message controls, incumbent continuation and game Close assertions remain; every native product now consumes exact executable/directory handles and completes the shared physical cleanup. |

The later [source-free desktop Player](2026-10-20-persistent-two-level-game-plan.md#desktop-implementation-and-initial-controls)
strengthens the cleanup evidence without adding another process owner. A receipt now separately
records `termination_requested()`, including automatic residual-tree termination after a normal
direct-child exit. Empty containment plus `force_killed=false` alone did not establish absence of
an earlier Windows Job termination request. The owner skips termination for already empty
containment; consumers requiring natural retirement check the new fact and their own game Close.
Current35/35 root controls distinguish natural trees, residual descendants and explicit requests;
`b92fd2e` adds a fixed250ms natural-drain observation before residual-tree termination, preserving
the original deadlines and truthful intervention fact. The linked successor owns the complete
desktop package qualification and remaining platform scope.

Root `project-tooling` and the no-default-feature Editor production-library checks pass. A whole-
Editor source scan finds the default frontend's native lifecycle tests also observe wake pending
state. The shared wake now exposes that same read-only query in production, explicitly without
completion authority; default-frontend test compilation is checked separately from UI execution.
The shared-owner game run takes 105.111 seconds after its outer test program is compiled. It
includes inner Cargo work and multiple actual process trajectories; differing cached compilation
inputs prevent comparing it with the earlier direct-child run as a process or edit-latency
regression. No P50/P95, memory or runtime performance budget is established.

Final default-frontend Editor test-target compilation passes, including the native lifecycle
callers of `wake.pending()`; no native UI test is executed. Root `project-tooling` library,
no-default-feature Editor library and the game's actual-build Host pass Clippy with
`-D clippy::correctness`. The independent process test target separately passes strict
`-D warnings`. Root/sample/independent formatting, affected TOML/CI YAML, document links/headings
and whitespace checks pass. Existing advisory warnings remain outside the selected correctness
gate. The independent normal dependency tree excludes `nara-editor`, `winit`, `wgpu` and
`dear-imgui*` packages; no Editor/UI runtime is required by this consumer.

The existing Windows/Linux module-consumer CI job now selects this ordinary public process
target without enabling the ignored nested-build/cooked-delivery matrix. Configured CI is not
remote execution evidence. Linux and other native execution remain outside this local Windows
result. The full foundation Goal stays active, with post-start candidate adoption, desktop
service classification and iteration-cost measurement still under their original owners.

## Game-Owned Development Host And Adoption

Status: implemented; Windows source/headless acceptance is recorded below. This successor delivers
a concrete development Host and command-line consumer; desktop Play retains its separate lifecycle
and one-shot READY contract.

The existing actual-build test manually retains A/B/C processes and compares their game states.
It does not implement a production command route, candidate ticket or adoption operation. Move
the game's finite process client and packet definition into its ordinary library, using the same
shared native-process owner. Add one concrete `DevelopmentHost` and a finite `develop` CLI. Keep
the original low-level A/B/C oracle as a separate regression; new tests must call the production
Host and demonstrate the actual command target changing. No universal Runtime serializer,
process-provider interface, scheduler or Editor UI feature is selected.

The caller supplies an explicit source project, trusted temporary parent, captured initial
executable and later explicit Cargo requests. Only this game's native headless continuity binary
and supported service inventory qualify. The Host does not edit source. It captures its incumbent
at a stable paused frame, compares canonical complete state with the caller's expected initial
probe basis, builds the actual changed source and restores an isolated probe. Each finite supplied
command step must match the complete expected canonical state, including logical time, held
intent, progress, counters and compatibility. Observation fields alone are insufficient, and
floating-point equality cannot substitute for canonical encoding's signed-zero fidelity.

A passing probe is closed through both the game and native owner before another child starts.
The Host then launches a fresh adopter from the same immutable captured bytes and restores the
original capture. This prevents probe movement, pickup, marker or travel state from leaking into
the adoption point; it does not roll back external native-code or build-script effects. Capture
continues to refuse open save storage and unclassified authoritative services.

Preparation returns a nominal, privately constructed ticket for the exact retained candidate.
The Host remains the sole ordinary command router to the incumbent; a foreign, consumed or
cancelled ticket cannot select or retire another candidate. If the incumbent changes after
preparation, adoption refuses and leaves the candidate available for explicit cancellation.
Before the route exchange, revalidate both complete states and the candidate's source envelope.
After the exchange, all ordinary commands target the new active process. Predecessor cleanup is
a separate retained outcome: pending cleanup cannot be relabelled as a failed adoption or cause
rollback. The new active game may continue, while another rebuild waits for original cleanup.

The source envelope is concrete to this game: the manifest/lock/project files and finite complete
`src`, `assets`, `scenes`, `prefabs` and `schema` trees, plus every declared Cargo input. Capture
the prospective edited source before building, then revalidate after building, before the fresh
adopter and immediately before adoption. Verify the original project directory object and the
complete inventory, including Cavern before it is visited. This is trusted-local per-file
revalidation, not an atomic filesystem snapshot, arbitrary Rust dependency discovery or protection
against hostile concurrent writers. Old executable provenance remains distinct from prospective
build inputs; an intentional Rust edit must not be rejected merely because the old code differs.

The finite Host bounds rebuild attempts, probe steps, captured executable bytes, source work,
protocol messages and native cleanup drives. Preserve the current synchronous Cargo adapter's
explicit contract: cancellation is cooperative between phases and cannot interrupt a blocked
compiler/build-script call. Do not claim a hard compilation deadline or latency distribution from
these bounds. Broader asynchronous supervision and desktop process adoption need their own
integration under the existing owners, rather than a detached timeout thread in the game.

Qualify the complete public backend with actual Rust body and private-state edits, wrong/stale
probe basis, compile/restore/probe failure, source drift, cancellation, foreign/consumed tickets,
probe-state disposal, sole-route adoption and exact predecessor retirement. Exercise the ordinary
CLI with real children and structured results. Retain physical cleanup custody on startup,
protocol, deadline or Close failure. Run focused serial nextest, owning builds/Clippy, format and
document checks; record actual local platform/toolchain evidence and remaining limits below.

### Development Host Qualification

Implementation revisions: production Host, source/process custody and actual adoption tests
`09dc48b`; ordinary JSON CLI, portable source inventory and guide `0b8c7d7`.

The game library now owns one `DevelopmentHost`, exact nominal `PreparedRebuild` tickets, a finite
process client and a concrete source envelope over the existing native/Cargo owners. The protocol
moves out of the continuity binary into the game library; the original A/B/C test consumes that
same protocol without losing its independent process-level assertions. The only root API addition
is `CargoBuildRequest::workspace()`, returning its existing canonical admitted workspace.

The ordinary [`develop` consumer](../../samples/two-level-game/src/bin/develop.rs) uses that Host,
retains its exact candidate ticket and emits bounded newline JSON. It never creates process
authority from a supplied generation or PID. Its
[guide](../../samples/two-level-game/README.md#sourceheadless-development-host) describes explicit
paths, source trust, operations and independent adoption/retirement results. Responses expose
original generation, game Close, fallback termination and actual native exit/containment/readers/
output facts; physical cleanup completion is separate from graceful game Close.

Three independent final reviews cover Host orchestration, process/protocol custody and source/CLI
correctness. Concrete findings are corrected: each retained child receives a fresh cleanup budget
instead of inheriting a deadline spent on earlier owners; initial capture mismatch uses the
Capture stage; already-reaped probe/adopter PIDs need not differ; and no-argument CLI commands use
Serde struct variants so unknown generation/candidate fields are actually refused. All findings
close on bounded rereview. The production adoption test retains the exact complete-state oracle,
including signed zero, and explicitly uses the new active game while predecessor retirement is
still Pending. Its final Close retains and retires both the predecessor and active game.

| Scope | Actual result |
| --- | --- |
| Final ordinary sample | 78/78, two actual-build tests intentionally skipped, run `0bb03432-e31e-487c-bb46-aae999ec3572`. Includes nine Windows source-envelope tests, Host construction/lifecycle controls, actual CLI parsing rejection of unknown selectors and the real nested continuity source inventory. Test execution is 10.799 seconds; the first ordinary release build separately took 4m22s. |
| Production Host with actual Rust edits and adoption | 1/1, run `c88cf9e6-c348-440d-9862-7927ba92d782`, 75.911 seconds including nested Cargo work. Actual body and private-structure edits, meaningful two-level state, typed compilation diagnostics, wrong probe, cancellation, source drift, foreign/consumed tickets, probe-state disposal, sole-route adoption, Pending-time continued execution/rebuild refusal and original physical cleanup all pass. |
| Original actual A/B/C process regression after protocol migration | 1/1, run `e59d7959-77e1-40ab-9232-26eab07382c8`, 52.552 seconds. Original code/state/executable/failure/stale-message assertions remain. |
| Actual ordinary CLI | 3/3 finite subprocess trajectories using the release `develop` binary and actual inner native Cargo builds: prepare/adopt/Pending-time Step/rebuild refusal/retry/Close; a prepared candidate followed by an unknown generation selector; and Observe followed by EOF. The malformed request exits unsuccessfully without adopting; all three complete clean game/native retirement. Five handles opened while their exact native children are live are observed signaled after cleanup, independently of PID reuse. Passing temporary storage is removed only afterward. |

The first production acceptance run `bafb83e2-0760-49d0-bc49-68177cfbb814` stops on a stale test
expectation for the initial mismatch stage after review correction. Updating that assertion to
Capture resolves it; neither canonical comparison nor incumbent-preservation assertions are
removed. The succeeding full production trajectory above includes every original assertion.

The first actual CLI smoke refuses startup at source inventory before admitting a child: Windows
`Path::join` produced native backslashes for the logical `RelativePath` parser. The CLI now carries
validated portable relative spellings through its finite inventory and resolves them only at the
native filesystem boundary; it does not replace separators inside arbitrary filenames. The
ordinary nested-product regression and all three actual CLI trajectories above pass after that
correction. Cleanup JSON now exposes the original reports rather than equating physical completion
with graceful game Close.

The final sample production binary build and all-target release Clippy with
`-D clippy::correctness` pass. Root/sample formatting, affected TOML, added local document
links/fragments and whitespace checks pass. Existing advisory warnings remain outside that
correctness gate. The native process backend itself is unchanged in this unit; its previous root,
independent and Editor cohorts retain the shared-process record above rather than being rerun as
unrelated UI/native matrices. All seven scoped implementation/review agents are closed. UI and
native mixed-DPI execution remain paused.

This remains Windows x64, Rust 1.95.0, release/locked/offline, one Cargo job and the shared root
`target`. These elapsed runs are fixture observations, not P50/P95 edit latency or performance
budgets. Linux execution, desktop post-start exchange/service classification, hard compiler
interruption, native-effect rollback, open save storage and full R12 remain open. Editor UI and
physical mixed-DPI work stay paused.

## Desktop Development And Original Native Session

Status: active. Extend the game-owned development workflow through an actual desktop product.
Keep the complete outcome: real body/private-structure edits, canonical state restoration, correct
native presentation, sole-route adoption and original Runtime/GPU/window/process retirement.
The source/headless qualification above is the control, not desktop acceptance. Editor UI,
dialogue integration UI and physical mixed-DPI work remain paused.

At `8cd36d4`, `DesktopRun` owns the original Runtime, Winit session, local WGPU Host and project
context, but its initial execution blocks until shutdown. The presentation observer is read-only
and cannot Pause/Step/submit/capture. A game-owned finite protocol must keep native events and
original presentation/retirement responsive while waiting for requests. Deepen that existing
owner with caller-driven native polling and managed control; do not put game control into the
renderer, add a second Runtime/event pump or expose unrestricted mutable World/GPU access.

The local source comparison pins Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`
(`crates/bevy_app/src/app.rs`, `crates/bevy_winit/src/state.rs`) and Godot
`7a37606a433c0e7735525037cdf774f043146a21` (`core/os/main_loop.h`, `main/main.cpp`). Both keep
native dispatch and simulation iteration distinguishable within the original engine application.
Nara retains its stricter managed authority and original cleanup contracts instead of adding a
parallel external driver. Those source references are architectural evidence, not native
qualification of this implementation.

The dependency-ordered implementation is:

1. Record the original managed drive's full-App-frame versus exact-fixed-tick completion explicitly.
   An exact tick does not include the later paused maintenance frame and cannot authorize capture
   by itself. Native polling retains this original fact and invalidates capture after new controls,
   submitted gameplay or native driver input. Game capture still validates its own queues, source,
   logical time and authoritative services.
2. Refactor the existing retained Winit session so bounded polling and blocking execution share
   one native dispatch/retirement implementation. Keep the original event loop, mapped windows,
   input connections, providers, GPU results and affinity until actual retirement. Polling never
   treats a live game as cleanup failure or silently renews an expired cleanup drive. Explicit
   Close/retry retains the original owner. Support initially paused, non-activating hidden native
   candidates and explicit reveal through this same owner.
3. Expose concrete polling, managed controls/results, bounded gameplay submission and healthy
   paused capture through `DesktopRun`. Source/processed construction and the existing blocking
   launch reuse its original ownership and close path. A poll wait bounds native waiting, not
   arbitrary source startup, callbacks, drivers or compiler work.
4. Classify the actual desktop game inventory. Rebuild native window/input/GPU/derived extraction
   services in a fresh Runtime; never serialize handles or broadly allow every desktop plugin.
   Preserve exact game configuration/Schema/content compatibility and refuse unsupported save,
   watch, task, physics/audio or other authoritative services. Share the game's capture/restore
   semantics between the two real placements without a universal serialization/provider layer.
5. Add an explicit desktop execution selection to the existing game development owner and finite
   child protocol. Keep immutable executable/source observations, session/request correlation,
   isolated disposable probes, a fresh original-basis adopter and final revalidation. Candidates
   must not receive ordinary native or semantic input before qualification. The first visible
   candidate picture must be its restored game; mere startup READY is insufficient. Define and
   exercise reveal/adoption failure and post-commit cleanup outcomes without hiding effects or
   discarding a usable incumbent.
6. Qualify the actual ordinary desktop game and public managed control before joining it to real
   A/B/C builds. Capture the original stock render input and correlate the exact Runtime/App/render/
   View/window/device identities with an actual surface presentation. Use finite game-specific
   GPU pixel oracles over that captured input; distinguish replay readback from original-window
   pixels and do not claim scanout. Keep actual gameplay, draw capture and presentation joined.
7. Run the production desktop adoption trajectory with compile/start/capture/restore/probe/source/
   presentation refusal, stale/foreign/consumed identities, Pending-time continued play and exact
   original GPU/window/process cleanup. First qualify on Windows/DX12 hardware, explicit native
   target, serial Cargo and shared `target`; retain unsupported-platform and performance limits.
8. Update ADR0076, ADR0116's separate game placement, guides and the R12 ledger with actual results.
   `desktop-supervised-play/v1` retains one active child and one-shot READY. A future Editor
   integration needs explicit eligibility and its original source/generation/nominal owner;
   completing the game placement does not silently extend that protocol or complete R12.

### Desktop Polling And Control Qualification

Implementation revisions are `f707d24` for original frame kind, `70d6a4f` for retained Winit
polling, `92c5986` for public desktop Host control and `6d3a865` for the ordinary game/GPU consumer.
Every managed drive outcome carries its actual full-App-frame or exact-fixed-tick execution
kind. The retained Winit session and
public desktop product now use that fact for paused capture. Blocking execution and bounded
polling share the original native pump, Runtime, presentation driver and physical retirement.
The public product admits managed controls and typed gameplay commands without exposing mutable
World or graphics authority. Source and supplied-Runtime construction retain the same close path.

Independent core/Host, native-session and game-render consumer reviews close five concrete
lifecycle/evidence corrections: ordinary startup cleanup polls cannot renew an incomplete drive;
initial Pause refuses an already pending Resume; its startup failure survives delayed cleanup;
an already recorded Runtime fault survives immediate Close; and hidden fullscreen selection is
deferred until reveal of the same original window. The game consumer additionally requires a
presentation at or after its captured App-frame floor, so an old matching picture cannot satisfy
the final Resume/Pause witness. These corrections retain the original assertions and owners.

| Scope | Actual result |
| --- | --- |
| Complete `nara_app` | 168/168, one intentional skip, run `9a3d27ff-2d55-410f-bf69-57693348093f`; includes the explicit frame-kind controls. |
| Final all-feature Winit CPU | 62/62, 15 native cases skipped, run `4bbc55d7-da08-483d-974c-a9d2bbd1d072`. Original controls plus live polling, paused-boundary invalidation, retained post-fault output and explicit-only incomplete-cleanup retry pass. |
| Selected original native Winit | 5/5, run `e464131a-30dc-44c7-9318-f11815b6fa0b`. Same-window hidden/reveal/fullscreen/close, sequential pump custody and Wait maintenance pass. Physical mixed-DPI cases are not selected. |
| Root desktop Host and public control | 23/23, 654 unrelated cases skipped, run `f0ba72a0-d5f4-4711-84c2-159e81e1b8ff`. Includes original presentation/retirement controls, startup Pause refusal, retained incomplete cleanup and close-before-source-start. |
| Actual ordinary desktop game and GPU replay | 1/1, no skips, run `503ee101-d33f-4e1f-91bb-2c13d10f42dd`, 3.793 seconds. Main-thread child first refuses a pending-Resume initial-pause conflict and recovers its physical pump, then runs actual movement/pickup/marker/travel/Step/reveal/Resume/Pause through public APIs. Original native retirement, contained child exit/readers/output and independent pixel readback pass without force kill or output loss. Its first release build separately takes 18m22s. |
| Final default source/headless sample regression | 78/78, two actual-build cases intentionally skipped, run `20486457-bb06-4803-bf6e-a5b33e6d1b37`, 10.792 seconds after an 8m36s release rebuild. Preserves the shipped empty-feature recipe and all ordinary state/source/CLI/gameplay checks. Desktop compilation installs a different input inventory, as documented below. |

The independently compiled game consumer uses its ordinary desktop recipe and stock render
transport. It retains at most eight original inputs after Seal and before Publish, joining the
selected input to the exact Runtime/App/render/View/window/device/extent presentation facts.
Its separate image replay occurs after original native retirement. Finite pixel expectations
cover the non-anchor player, Cavern marker, visible pickup white/shadow interiors and background;
they do not cover diamond transparency contours, Meadow disappearance pixels, the swapchain or
scanout. The visual expectation deliberately has no observed configuration fingerprint and is
never admitted as a canonical restore state. Full desktop state/service classification and input
continuity remain the subsequent complete outcome below.

This is Windows x64, hardware DX12, Rust 1.95.0, release/locked/offline, one Cargo job and the
shared root target. Elapsed build/test observations are not edit-latency or frame-time budgets.
The sample enables a direct optional WGPU dependency at the existing engine Git revision; its
lock changes only that package edge. Physical keyboard transfer, serialized scheduling phase,
native pause-duration exclusion, actual desktop A/B/C adoption, foreign native execution and
full R12 remain open. UI and physical mixed-DPI execution stay paused.

The minimal `desktop-winit,render-wgpu` root library builds with default features disabled and
without `runtime-2d` or `serde`. Correctness Clippy passes for all-feature App/Winit targets,
the selected root desktop library/public test and all desktop sample targets. Root/sample
formatting, affected TOML/lock parsing, added local links/fragments and whitespace checks pass;
existing advisory warnings remain. The scoped implementation/review threads and both bounded
follow-up source-study threads are closed. Those studies identify the still-unimplemented
input, time and service-inventory work below rather than claiming another accepted workflow.

### Desktop Input Continuity Findings

The source audit identifies a concrete missing owner operation. Restoring `Session::held[right]`
does not restore an `InputTimeline` member or `SemanticActions` contribution. The original release
path ignores a key with no active member, so a later native Release can leave restored movement
stuck. Desktop acceptance therefore still requires transferred held intent and its subsequent
release, including a real window dispatch trajectory; a neutral-only capture is only a control.

The bounded follow-up source study identifies the concrete implementation pressure. The four
game bindings are `a/d/w/s`, four Button actions in the `gameplay` context. A restored member
must also establish the original `ButtonInput` projection: its normal Release currently expects
an existing member, so installing only `SemanticActions` would still be incomplete. Input
quiescence must include retained deferred action records, not only `action_scanned`; a paused
frame can scan a Release while deferring its semantic effect. The original timeline's retained
record accounting provides a conservative refusal before a more precise predicate is justified.

The restored member's admission provenance and a later native terminal trigger are separate
facts; the old process's press sequence must not become the new member's authority. The current
Winit keyboard match discards `is_synthetic`, and its input gate initially assumes focus even
for a hidden window. Preserve that flag and add explicit dormant/activation handling through
the original owner before claiming hidden candidate input isolation. A visibility change alone
does not establish focus or distinguish a key released during the cross-process focus gap.

The selected direction retains the original Input owner: explicitly admit a bounded restored
logical contribution, with new local member/endpoint authority and original terminal allowances,
without fabricating a physical Press or repeating a Started gameplay command. Native trigger
provenance remains distinct from restored contribution provenance. Future release, focus loss,
mapping/context cancellation and native retirement must settle that original contribution once;
re-enabling a context or receiving repeated/synthetic Down cannot resurrect it. No separate key
state map, gameplay side channel or replay-queue policy becomes a second input authority.

The hidden candidate and disposable probe need explicit native input isolation in the original
session. Lifecycle events still pass through original window dispatch. Adopt the sole ordinary
route before a reveal that can change incumbent focus; post-commit activation failure remains
an activation result with retained paused owners, not a false adoption rollback. Dormant logical
intent, real focus acquisition, activation and cancellation need their own exact original-owner
observations. `with_initial_visibility(false)` alone does not prove those semantics.

The concrete four-key game drives the bounded public interface and original-owner operations;
do not infer a generic continuity framework or complete desktop reconstruction from this input
prerequisite. Acceptance needs typed driver, native dispatch and actual rebuilt-game evidence.
Capture must also reject an unconsumed/staged release;
comparing only the game's held bits or one input scan cursor is insufficient. Bevy's pinned
keyboard focus cleanup/synthetic filtering and Godot's `release_pressed_events` provide related
focus policy evidence, not this cross-Runtime transfer. Version one separately refused nonzero
time remainder/debt and scaling. The time successor below addresses phase through its original
owner; it does not make desktop input continuation eligible.

The current successor implements finite declared keyboard Button holds in the original Input
owner. `KeyRestored` records allocate fresh local member identities and original terminal credits,
establish semantic held values without Started, and project buttons without transient Press.
Capture validates complete consumer quiescence, exact binding/context lifetime and both held
projections. Batch refusal leaves members and capacity unchanged. Admission provenance remains
distinct from the terminal trigger; native activation mismatch uses its own invalidation reason.

Source study also identifies the exact-step input gap: projection alone is insufficient because
semantic resolution and action-command mapping previously ran only in PreUpdate. Prepare the
same original operations before fixed command admission, with once-per-App-frame cursors so
ordinary frames and multiple ticks do not repeat edges. Unresolved UI decisions stay pending.
This is a required original-owner correction, not a separate Stop-command path. Acceptance
includes restored D, staged native Release, one exact tick with no further movement and one stop.

Native activation needs explicit isolation, focus acknowledgement and physical reconciliation.
Windows focus synthesis has no complete public snapshot marker; a synthetic Down cannot prove
that an old held member should continue. The original native owner queries declared physical
keys after actual focus acknowledgement, stages mismatch releases and remains paused until
the original input/gameplay consumers settle them. Unsupported keys/backends or denied focus
retain an unavailable/waiting result. Posted WM_KEYUP proves message dispatch; only separate
OS input/state observations can establish the physical hold. Keep these evidence scopes distinct.

### Keyboard Continuation And Native Activation Qualification

The November 14 implementation keeps keyboard membership in the original `nara_input` timeline,
Button projection and semantic contribution owner. A finite `KeyboardActionBinding` inventory
captures exact held values. Paused restoration validates the complete map/context lifetime,
original endpoint and available batch/terminal capacity before creating new local members.
`KeyRestored` emits neither transient Press nor Started. Capture refuses retained or deferred
consumer work, extra held members and mismatched projections. Member admission origin remains
separate from a later terminal trigger; reserved termination works under ordinary storage pressure.

Exact fixed steps now reuse the original Input projection/staging/resolution and gameplay command
mapper before command admission. Per-App-frame cursors preserve ordinary PreUpdate and catch-up
semantics. Independent review finds a late-input regression: staging in catch-up tick two retained
an old mapping even though resolution had already run. The added public regression first fails
at the action cursor assertion, run `12d49cd2-7360-41c9-bb77-bdbb326fe9f5`; applying the same frame
condition to staging and resolution leaves that input for the next First mapping boundary.
The final public exact-input cohort passes 6/6, run `8e94a072-076c-4768-8515-478feab2d189`.
Another review correction preserves Synthetic/Platform ReleaseAll trigger origin independently
of Restored membership; the public tests compare both paths.

`WinitSession` and `DesktopRun` expose capture/restore plus explicit input isolation and activation.
Dormant native focus/lifecycle facts still reach the original session while ordinary keyboard,
mouse and pointer ingress stays isolated. Actual original-window focus acknowledgement precedes
a Windows physical A/D/W/S query, with focus and keyboard-layout checks around it. Missing holds
produce original `ActivationMismatch` releases. `ReconciliationPending` admits one exact original
Step to consume Stop before movement; ordinary input and Resume remain gated until maintenance
and physical reconciliation finish. Repeated/synthetic Down cannot manufacture held intent.
Denied focus remains waiting; unsupported keys/backends remain unavailable with the paused owner.

Independent native-path review also closes two paused reactivation cases. Active-to-unfocused-to-
focused transitions restore focus in the original Input endpoint, and pending releases from
ordinary Platform members receive the same original Step opportunity as Restored members. Two
CPU regressions retain the exact endpoint/generation, one Stop, no extra movement and a single
accepted reconciliation ticket. They explicitly do not establish native focus.

| Final ordinary scope | Actual result |
| --- | --- |
| All-feature Input, Gameplay and Winit | 336/336, 18 native cases intentionally skipped, run `2a729efd-aa39-4388-b228-a271a61b9495`. Includes all 13 public keyboard-continuation cases and the eight native-session CPU cases. |
| Public exact-input consumer | 6/6, run `8e94a072-076c-4768-8515-478feab2d189`; restored Release/context loss stops before movement, ordinary catch-up emits no duplicate commands, and late input uses the next First mapping. |
| Default source/headless sample | 97/97, two unchanged actual-build cases skipped, run `e04a9e24-efe7-4998-95b4-6491f71ddd2d`. Original development Host, JSON CLI, canonical state, time and gameplay regressions pass. |
| Desktop-feature sample library | 66/66, run `84274391-813b-4408-9421-6a61cfabd416`. Actual installed movement/command maps retain the four key/action/context declarations, payload direction order and all prior bindings. |

The first final CPU run passes 335/336 because the new refocus fixture reads `ButtonInput` before
the admitted ordinary Press reaches its projection boundary. Completing original paused
maintenance fixes the fixture without changing production code or weakening the observation.
The final 336-test run above includes that exact path. Earlier 334/334 results predate the two
reactivation regressions and are not substituted for this final source qualification.

Actual Windows native reusable qualification passes 7/7, run
`3b3cd675-150d-4578-8c0b-7ca32d7d94e3`, including the original hidden/reveal/fullscreen/retirement
controls. One new trajectory confirms the release gap; another pairs SendInput Down/Up with an
independent OS held-state observation, then dispatches WM_KEYUP through the same original HWND
and observes one original release/Stop. Posted messages alone do not establish physical state.
The third new native case refuses unsupported activation while retaining the paused candidate.
Physical mixed-DPI tests are not selected.

The independently compiled game's actual desktop/GPU consumer passes 1/1, run
`a50073ba-1feb-46b1-b5ba-9f1f22b2f357`, 5.713 seconds after a 4m13s release build. The game derives
its four declarations from the actual movement map, restores D without Started, rejects early
capture/Resume and acquires real focus. Physically Up D requires reconciliation; one original
Step clears game held intent without changing player position. All prior movement, pickup,
marker, travel, scheduling-phase, original presentation/retirement and finite GPU replay checks
remain. The replay is independent image rendering after original native retirement, not swapchain
capture or scanout. The existing contained child exit/readers/output checks still apply.

This scope is Windows x64, Rust 1.95.0, release/locked/offline, one Cargo job and the shared root
target; the game uses hardware DX12 without forced fallback. Only an existing Windows API package
edge is required in the root/sample locks; no dependency versions change. Full desktop canonical
capture, service eligibility, actual rebuilt-candidate restoration/presentation, route adoption
and predecessor retirement remain the next integrated outcome. No desktop service is silently
added to the game's resume allowlist. Foreign native execution, gamepad/mouse continuation,
arbitrary services, latency budgets and full R12 remain open. UI and physical mixed-DPI stay paused.

### Desktop Time Continuity Findings

Source inspection identifies a second required continuation owner. Winit supplies wall-clock
deltas, so a normal Running full frame followed by Pause can retain nonzero fixed remainder.
Paused maintenance and exact steps preserve it. With a 20 ms timestep, a 16 ms running frame
followed by Pause leaves 16 ms pending even if no fixed tick ran. The current game format
correctly refuses that capture; an initially paused, exact-step-only trajectory cannot replace
ordinary desktop reconstruction acceptance. This is a source-derived behavior, not a measured
refusal rate or a claim that every Resume/Pause has nonzero remainder.

The game's logical completed ticks and elapsed time do not describe this phase. With 13 ms
pending, the same next 7 ms should trigger a tick; a fresh candidate reset to zero would not.
The original `nara_app` FixedTime/TimeFramePlan owner therefore needs a bounded continuation
capture/initialization operation at a healthy managed boundary. Preserve pending phase and
validate actual timestep, work/debt caps, catch-up policy, scale and max delta without admitting
contradictory independent pending/remainder/debt values. Derive render interpolation through
that same owner. Keep the candidate's new Runtime/frame/tick identity and the game's separate
logical clock; do not copy the entire time resources or private active-frame fields.

At `ce4fdb0` this operation is unimplemented and its public value/API is not selected. It cannot simply
mutate FixedTime from game `resume::finish`: TaskUpdate executes after the frame plan is already
committed. Version the bounded game format and exact-state oracle together with the managed
time operation, and apply it before final canonical comparison and restored presentation.

The current implementation unit selects one portable `RuntimeTimeContinuation` and concrete
capture/application methods on the original Runtime, projected through the existing Headless
and desktop owners. It carries actual policy plus pending duration, preserving the receiving
Runtime's history and deriving interpolation. The game format moves to version two with a
required explicit time value; version one is refused. The game can initialize the fresh
candidate's phase before admitting its isolated restore command, then complete paused
maintenance. Exact stepping and later reconstruction maintenance preserve it, so no active-frame
time mutation or extra game coordinator is needed. Verify nonzero remainder, legal debt,
policy/refusal/fault preservation and matched subsequent-delta gameplay, then qualify native
pause sampling and the actual desktop time operation. Full desktop input/adoption remains open.

The selected native correction distinguishes supplied next intervals from already elapsed
wall-clock intervals through `RuntimeInstance::drive_elapsed`. Entering an elapsed interval
Paused preserves that interval's paused App policy even if Resume applies at its end; the
original RealTime receives the whole duration. The next interval advances virtual time. Winit
keeps its original `last_frame` sample and uses this entry without an extra App frame, new clock
or baseline reset. Explicit `drive(delta)`, Stop and exact-step behavior retain their contracts.
The pinned Bevy time source distinguishes Instant and ManualDuration inputs; Godot keeps native
sampling separate from Scene processing pause. Those implementations support the ownership
comparison but are not an existing drop-in fix for Nara's retained Wait session.

Winit also owns the new process's `last_frame` sampling baseline. At `ce4fdb0` Wait maintenance
does not update that baseline, so paused/hidden qualification time can enter the next resumed
delta. Define the native pause/activation sampling contract together with the original real/
virtual-time policy; do not serialize an Instant, clear accumulated phase or disguise waiting as
simulation. Acceptance needs a nonzero-remainder Running/Pause capture, matched subsequent
delta sequences, legal debt and policy refusals, plus a long paused/hidden wait control. Until
qualification, retain the incumbent and report the existing remainder/debt refusal honestly.

### Scheduling Continuation Qualification

Implementation revisions: App time owner and elapsed entry `a18bf17`, original Host/Winit
integration `1fdcbd5`, and game format/protocol plus complete consumer updates `b86269d`.

The concrete time owner now carries policy plus one pending duration, validates the receiver's
actual policy, preserves local history and derives render interpolation. Headless and desktop
delegates retain their original healthy paused boundary and application/fault distinction. Game
resume version two requires an explicit bounded time object and refuses version one. Its finite
continuity protocol also moves to version two, refusing an old child at the original version gate.
Progress saves retain their separate format and migration. The game initializes a fresh candidate before
its isolated restore command, so exact steps and reconstruction maintenance preserve phase.

Independent core review finds one concrete authority gap: a validator can report a canonical
fault while returning Ok. The original common validator now rechecks the reporter after each
callback, so capture cannot return success and an entry fault cannot apply phase. Regression
controls distinguish that from a later application fault, preserving both the actual applied
fact and first reported fault. Bounded rereview closes the finding. Independent game and native
reviews find no remaining P1/P2. Missing-resource, actual Last-stage policy changes, portable
duration overflow, signed-zero conversion and the outer Host's failed-close state are exercised.

| Scope | Actual result |
| --- | --- |
| Complete App and authority controls | 178/178, one intentional skip, run `3dddbabf-a34f-4aef-b19b-3ca261e2d38a`. Includes old explicit Resume/Step behavior and original lifecycle tests. |
| Final ordinary source/headless game | 97/97, two actual-build cases kept separate, run `025195e0-d918-46c1-a284-55f6c6a3400b`, 12.114 seconds. Includes the final protocol version. Complete meaningful nonzero-phase and PreserveDebt reconstruction match subsequent delta sequences, full state and fresh Runtime history. Format/policy refusals and applied-fault physical Close pass. |
| All-feature Winit CPU | 67/67, 15 native cases skipped, run `be66e762-dc90-4d78-a54d-0333c1f99d71`. Deterministic original boundary calls settle a 60-second paused Wait with full RealTime and preserved 13 ms phase; the next 7 ms triggers one tick. Hidden, debt, refusal, Stop and fault controls pass. |
| Selected native Winit | 5/5, run `d5db4515-9f45-4a0d-98aa-947733c55c04`. Original reusable/maintenance/hidden/fullscreen/close trajectories pass; mixed-DPI cases are not selected. |
| Actual desktop game and independent GPU replay | 1/1, run `65cc2be7-8caf-4247-b2c7-6cf94ce049f5`, 5.886 seconds after an 8m24s release build. The original public Host initializes 13 ms phase, refuses capture until its next paused frame and returns the exact value. All prior gameplay/control/native-retirement assertions remain, and original sealed/presented time facts match the public value before independent pixel replay. |
| Actual Rust builds and production adoption | 2/2, run `6710033d-dda7-4e86-a81a-19ce154f6d4a`, 230.833 seconds including nested Cargo. Original A/B/C reconstruction passes in 162.590 seconds and production Host adoption in 68.242 seconds through the version-two message/state shape. All prior source, failure, isolation, nominal-ticket, Pending-time execution and physical retirement assertions remain. These regressions retain their source/headless placement. |

The first ordinary game compile finds a new test-plugin helper returning `Result<&mut App>`
instead of `Result<()>`; its explicit success return is corrected before execution. Two later
runs pass 96/97 and expose mistakes in the new debt fixture: local-headless does not imply
PreserveDebt, and 150 ms plus the later 60 ms must retain 10 ms at a 20 ms timestep. The fixture
now explicitly selects preserve-debt in its private source copy and checks both precise durations;
all intermediate state and continued-gameplay assertions remain. The final 97/97 above passes.

Correctness Clippy passes for App/Winit all-target/all-feature, the root desktop library/public
control test, and all desktop sample targets. The minimal desktop root library still builds
without default features, runtime-2d or serde. Root/sample formatting, affected TOML, added local
links/fragments and whitespace checks pass. Existing advisory warnings remain outside the
correctness gate. All five scoped implementation/review threads are closed; all owned Cargo and
native test processes reach their original terminal observations.

Executed scope is Windows x64, hardware DX12, Rust 1.95.0, release/locked/offline, jobs one and the
shared root target. The 60-second injected CPU interval is deterministic state-machine evidence,
not a physical 60-second native pause measurement. Actual native/game and nested-build results
above retain their own scope. No frame-time, compiler-latency, memory or cross-platform budget
is inferred. The finite desktop picture expectation still lacks authoritative full configuration
and private inactive-level state; it is not admitted as a restore payload. Game scaling other
than one, desktop canonical service/input reconstruction and post-start adoption remain open.

This is a scheduling continuation prerequisite, not desktop canonical reconstruction, native
held-key transfer, scanout, physical mixed-DPI or full R12. The existing desktop control consumer
and actual rebuilt-game trajectories must retain their separate execution evidence. UI stays paused.

### Desktop Service Inventory And Placement

The ordinary desktop profile adds the following ten plugins to the supported headless inventory.
The desktop successor now records these exact IDs as explicitly reconstructible services for the
game-owned capture boundary. This classification does not serialize their handles or declare
their fresh candidate ready; each service must be rebuilt by the new Runtime and its presentation
must still be qualified separately.

| Plugin | Required continuation classification |
| --- | --- |
| `nara.render` | Rebuild Views, render frame/status and transform presentation projection. |
| `nara.image.prepare` | Rebuild prepared images from the same admitted assets and source facts. |
| `nara.sprite-render` | Rebuild extraction, ordering and batches from reconstructed game state. |
| `nara.window` | Create new native/window registry state; retain original retirement and new focus authority. |
| `nara.render-wgpu.capture` | Rebuild packet slot, managed connection and backend state under the fresh owner. |
| `nara.render-wgpu.window-presentation` | Rebuild the original candidate's surface/presentation; keep receipts local to their owner. |
| `nara.render-wgpu.stock-capture` | Rebuild stock builder/draft and preserve its close obligations. |
| `nara.render-wgpu.stock-policy` | Reinstall the same validated stock policy/composer configuration. |
| `nara.render-wgpu.stock-sprite` | Reconstruct capture contributions from the new sprite batches. |
| `two-level-game.desktop-input` | Rebuild binding configuration; implement the authoritative input continuation above before admission. |

There is no separate `nara.winit` plugin in this inventory; `nara.render-components` already
belongs to the shared inventory. Compilation features and actual placement remain distinct:
`configured_recipe` currently installs desktop-input whenever compiled with the `desktop`
feature, even for a selected headless profile. The explicit inventory now admits the known
desktop-input declaration while unknown custom services still refuse capture. The qualification
observer `two-level-game.test.desktop-capture` is separately named as evidence-only; it retains
sealed WGPU inputs but owns no game state or native authority. At this capture-only milestone,
the development Host still requires empty features; the explicit placement successor below owns
its later desktop extension.

The concrete game successor should share private eligibility, canonical World capture,
fresh-candidate checks and restore-command construction between its two public placements.
The game now exposes a desktop capture adapter that uses the same canonical capture routine through
`DesktopRun`'s healthy paused boundary. It retains the actual configuration fingerprint, Schema,
level stamps, time phase, counters and semantic state; native handles, WGPU resources and
presentation receipts remain with `DesktopRun`. The real desktop control trajectory captures this
value and compares the visual oracle's represented semantic fields before replaying the captured
state through the presentation evidence path. Its visual oracle does not independently check the
configuration fingerprint or complete inactive-level state; canonical restoration and an actual
ordinary-recipe incompatibility control belong to the successor below. A first RED run correctly
refused the evidence-only observer as an unknown service; the explicit classification fixed that
without admitting an unrelated custom service.

Desktop restoration still submits through `DesktopRun`, polls the original Step ticket and
completes paused maintenance through the existing game `begin_from_command`, travel and finish
owners.
Extend the finite continuity child/protocol, process client, development Host and CLI with
explicit placement, restored-presentation confirmation and separate adoption/activation/
retirement results. Preserve the current executable/source envelope, disposable probe, fresh
adopter and final source/dual-state revalidation. These are required implementation changes;
the current visual control consumer does not satisfy them.

### Desktop Candidate Reconstruction Successor

Status: qualified within the finite Windows workflow recorded below. Complete the concrete
desktop reconstruction path before adding candidate adoption credit. Share private fresh-game
checks and restore-command construction with the
headless placement; the game-owned desktop adapter drives only original `DesktopRun` controls,
time continuation, command submission and paused maintenance. Its finite cooperative deadline
cannot interrupt native APIs, callbacks or graphics drivers. A failure after application retains
the original partially changed candidate for physical close; it is not a game rollback.

Desktop capture must compare the original native Input owner's declared movement holds against
the game's held bits and refuse any pending or unsupported input. Known service IDs alone cannot
make this capture complete. Restore only a healthy fresh paused isolated candidate, preflight
configuration/Schema and empty native membership, initialize time, admit one restore command,
wait for its exact Step and Scene reconstruction, restore fresh keyboard members, then compare
the complete canonical encoded state. Keep the original Runtime clocks and native/GPU/Scene
identities fresh. The source/headless capture adapter must refuse native presentation placement
instead of bypassing the desktop input checks.

The ordinary native consumer reconstructs meaningful multi-instance progress, non-anchor player
state, nonzero time phase and held intent through these public APIs. It observes the restored
stock input joined to the original presentation receipt at or after the capture's App frame and
retains the finite GPU replay oracle after physical cleanup. Sequential native reconstruction
does not prove an incumbent-preserving cross-process route exchange. That integration still
requires explicit placement in the existing finite child protocol and development Host, a
discarded probe, fresh adopter, final source/dual-state revalidation, restored-presentation
confirmation, adopt-before-reveal, actual activation and independent predecessor retirement.

The production desktop child additionally declares `two-level-game.development.presentation`.
It rebuilds a bounded history of canonical state digests at the original stock Seal and joins
only the matching original presentation receipt. The observer owns no native handle, GPU submit
or game authority, and its configuration declaration remains part of compatibility. An actual
plain recipe without the qualification observer must refuse that observer-bearing state without
changing its fresh canonical state or command queue; a modified fingerprint alone is insufficient
as the only control.

Retain the previous gameplay, cleanup and refusal controls. Unknown services, open native save
storage, stale configuration/content, pending work, incompatible time and native input mismatch
must not become passing captures. No universal serializer, process framework, second native pump,
render owner or input authority is added. UI and physical mixed-DPI work remain paused.

### Desktop Reconstruction And Production Adoption Qualification

Date: 2026-11-14. Qualified within the executed Windows scope. Implementation revisions:
`4080306` for original Input/Host handoff controls and `65c0f75` for desktop reconstruction,
production adoption, CLI and complete consumers. The implementation connects the concrete
desktop adapter, original Input/Winit controls, finite child, process client, production
development Host and ordinary JSON CLI. `SourceDesktop` is explicit; the default remains
`SourceHeadless`. The game-owned protocol moves to version three and requires matching placement
in READY and every response. Resume format two, progress saves and supervised Editor Play retain
their independent formats and owners.

Capture now checks complete declared native movement membership against game held intent, as
well as the original healthy full paused frame. Headless capture refuses native presentation
placement. Desktop restoration preflights freshness, configuration and Schema, restores time and
the original game command, waits for its exact managed Step/Scene completion, restores fresh
keyboard members and compares the entire canonical encoding. The ordinary-recipe control really
omits the qualification observer; it proves a different actual configuration refuses restoration
without changing fresh state, tick or queued commands. The earlier capture-only visual oracle
did not independently establish this compatibility fact.

Production probes use claimed Synthetic keyboard admission through the original Input owner,
including original action resolution and terminal obligations. Prepare/adopt freezes incumbent
native ingress before final capture; explicit activation is needed to resume native input after
a refusal too. A complete canonical digest is retained at the original stock Seal and joined to
the original Runtime/App/render/window/surface/backend/device receipt. Confirmation requests a
new App frame and refuses state drift. Probe presentation and physical Close precede a different
fresh adopter. The route exchange still has no synchronous predecessor Close; new active commands
work while cleanup is Pending. Native reveal/focus/reconciliation is a separate outcome after
commit. `Closing` retains the same child/native owner and request sequence for explicit retry.

Independent input review identifies two P2 boundaries: Press-only controlled input could enter
activation before its pending Input event was consumed, and native physical loss could report a
Synthetic terminal trigger. Activation now refuses pending inventory before requesting native focus while
keeping Isolated and original Step available. Physical loss reports Platform trigger with the
original Synthetic member/press lineage and credits. Two bounded original-owner regressions
accompany these fixes. Final static input, canonical/recipe, child/Seal/receipt and Host/process/CLI
reviews find no remaining P1/P2; actual execution below remains the separate acceptance authority.

| Scope | Actual result |
| --- | --- |
| Complete affected Input/Gameplay/Winit CPU and public targets | 346/346, 18 intentional native skips, run `2cfdcec3-cace-4f8f-b320-de9649ba5e9b`, 12.570 seconds. Includes both final P2 regressions and all prior keyboard/provenance/capacity/activation/retirement controls. |
| Root public exact-input controls | 6/6, run `a0c1e3db-e2eb-4e59-aeb4-a30687aba5f4`, 0.336 seconds. Original Press lowering, staged release, cancellation and exact-tick ordering pass. |
| Complete ordinary source/headless game and CLI CPU targets | 101/101, two actual-build cases kept separate, run `e36340da-e73f-47c7-a6f1-d819f340c244`, 11.478 seconds. Complete reconstruction/time/source fencing and abnormal-versus-physical CLI close controls pass after protocol migration. |
| Desktop-feature game library and finite child/CLI CPU targets | 74/74, run `29ccc5c7-4b54-42ef-8b0a-4de9ac7df2ab`, 6.961 seconds. Includes the existing retained-Close and post-admission timeout cases; it is not native presentation evidence. |
| Original actual source/headless A/B/C and production Host adoption | 2/2, two ordinary cases outside the ignored-only filter, run `015b394e-58d1-418e-adfa-5f5d1382acfa`, 174.219 seconds including nested Cargo. The independent A/B/C trajectory passes in 98.828 seconds and the production adoption trajectory in 75.391 seconds with version-three headless replies; all original failure/source/ticket/Pending-time/physical-cleanup checks remain. |
| Original reusable native Winit trajectories | 7/7, 92 outside the ignored-only filter, run `b1b0f64a-521c-48a0-9640-1368b7a7a522`, 2.160 seconds. Hidden polling/fullscreen, original-loop reuse, real focus/physical key reconciliation and retirement pass; mixed-DPI is not selected. |
| Original desktop gameplay/control and GPU replay | 1/1, run `b0d47c84-5d20-45df-9163-fec13cc4ec54`, 5.876 seconds. All prior gameplay/input/time/native cleanup and replay assertions remain; the added real game-held/native-inventory mismatch correctly refuses capture. |
| Sequential native desktop reconstruction and independent GPU replay | 1/1, run `8e0cf920-58e5-4240-8c02-d23c1a92fe92`, 12.284 seconds after a 3m34s release build. Complete multi-instance state, 13 ms phase, fresh Runtime/Scene/render/native/GPU identities, ordinary-recipe incompatibility, held continuation/reconciliation and physical cleanup pass on hardware DX12. |
| Production desktop actual body/private-structure edits and adoption | 1/1, run `0259249f-8372-4140-9bb2-447ec2dc8fc5`, 131.575 seconds including nested dev-profile Cargo. The incumbent survives compile/probe/stale-state/source-drift refusals; probes are discarded, B/C restore complete state and presentation, Pending-time commands run new logic, final native activation and all original clean retirement reports pass. |
| Ordinary JSON development CLI with actual children/builds | 4/4: source/headless adoption, source/desktop adoption plus presentation/activation, desktop malformed-selector refusal with cleanup, and desktop EOF cleanup. Seven exact incumbent/candidate native handles are observed exited. All returned game/exit/containment/readers/output facts are clean; the deliberate input-refusal case exits unsuccessfully and the other three exit successfully. Desktop children report Vulkan. Work storage is removed only after the actual exits. The ordinary CLI executable is built in release; its explicit child selections use dev. |

The first all-target check finds only a missing crate-root protocol import in the new desktop
test; the corrected all-target check passes. The first actual-build run
`dbb1afc6-ace3-46a0-b967-3d5e3819c946` fails its new backend assertion after 312.377 seconds,
including an initial 5m06s nested dev build: exact product launch intentionally projects only the
reviewed desktop-session environment, so the parent's DX12 hint does not reach the child, which
selects hardware Vulkan. The fixture now reads the actual first hardware backend and requires
the same supported native backend through incumbent/B/C presentations. Complete canonical,
presentation, hardware-adapter and physical-cleanup checks remain. This does not add ambient
variables to the shared process projection or claim that these children ran DX12.

Correctness Clippy passes the affected Input/Gameplay/Winit all-target/all-feature graph, the
root desktop library and every desktop sample target. The minimal root desktop library builds
without default features, runtime-2d or serde. Root/sample format checks, affected TOML, added
local document links/fragments and whitespace checks pass. Existing style/dead-code diagnostics
remain outside the correctness gate; this is not a warning-free claim. All eight scoped
implementation/review threads are closed. Owned Cargo sequences terminate, and native/game/CLI
owners reach the actual cleanup observations recorded above. The root exact-input command first
used the sample-only feature name `desktop`; correcting the command to the root's actual feature
contract produces the 6/6 result without source changes. These cohorts overlap and are not a
combined independent test count.

Executed scope is Windows x64, Rust 1.95.0, locked/offline, jobs one and the shared root target.
The outer tests use release; actual Cargo selections inside the A/B/C fixture use dev. The
sequential native trajectory and contained child trajectories retain their distinct backend
selection contracts. A presentation receipt proves original submission/present association,
not GPU completion or scanout; independent replay pixels are not swapchain readback. Numeric
latency, frame-time, CPU, memory and cross-platform budgets remain unqualified. Explicit child
graphics selection is not yet a public Host option; the sealed default's actual adapter is
reported instead. The finite development protocol remains paused/Step-driven rather than an
Editor-integrated continuous Play session. Unsupported native services, open SaveSlot,
arbitrary component/World serialization and complete R12 remain open. UI and physical mixed-DPI
work remain paused.

The next runtime workflow should connect ordinary continuous execution to the same explicit
pause, input freeze, rebuild, correct-result confirmation and postcommit activation/retirement
controls. Measure edit-to-correct-playable-result and retained/idle runtime costs separately from
compiler time before broadening service migration. Supervised Editor Play requires reconciliation
of its existing protocol owner; this game-owned command path does not silently extend it.

## Continuous Desktop Development Successor

Status: implemented and qualified within the finite Windows workflow below. Extend the same
concrete source/desktop development Host to run the ordinary game continuously between control
requests. The original desktop main thread keeps pumping
its existing native event loop. Only original managed Runtime Pause/Resume tickets can change
execution; no new scheduler, event loop, input route or synthetic frame delta is introduced.
Desktop-only execution status reports actual managed state, native activation and read-only game
observation. It is distinct from complete canonical paused capture and original presentation.

Resume requires original native activation. Pause waits for its exact ticket and a full original
paused boundary; pending native/game input may still refuse canonical capture and must not be
silently dropped. Capture, exact Step, probe preparation and adoption keep their paused contracts.
A control refusal before admission remains recoverable. A failure after admitted control retains
the original owner and truthful outcome/cleanup path. Native activation is separate from Resume.
Prepared candidates stay hidden, isolated and paused. Adoption refuses a Running incumbent;
after Pause, any changed canonical state, including continued time, invalidates the old probe
basis and requires new qualification. No extra generic revision owner replaces the full-state
comparison.

Use a real continuous desktop consumer with bounded matched native key delivery and complete
physical cleanup: original active play, Pause/isolate/capture, actual body/private-structure
builds, restored presentation, adoption, explicit activation, Resume and further native gameplay.
Keep all prior exact-step, source/probe/failure and Pending-retirement tests. Ordinary JSON CLI
must execute the new controls through the production Host. Protocol version four makes the new
execution operations explicit; resume format two and supervised Editor Play remain separate.
Record actual elapsed/correct-result observations without claiming a latency distribution or
numeric performance budget from one acceptance run. UI and physical mixed-DPI remain paused.

### Continuous Development Qualification

Date: 2026-11-14. Production implementation: `a1490b9`; actual native consumer: `ec57133`.
The later Scene/value and Winit integration changes retain complete native execution at
`88798b0`, run `7d3718dd-2228-433e-94b5-d7b7f41ca6ed`: 1/1 in 111.572 seconds after a
38.29-second outer release build. The complete original A160/B80/private-structure C trajectory,
restored presentation, continuous native gameplay, stale refusal, Pending cleanup and Running
Close pass on hardware Vulkan. The
[foundation successor record](2026-10-23-foundation-architecture-rebaseline-plan.md#scene-staging-asset-reservations-and-live-export)
retains the earlier unexpected B exit and externally held-key refusal separately. The first
failure remains unexplained; this repeat does not establish its root cause or a performance gain.
`522e707` preserves the existing bounded process diagnostics and requires all original scan-mapped
A/D/W/S keys to be Up before the probe takes input ownership. No assertion or external held key is
silently cleared. Editor/Dialogue UI remain paused; bounded mixed-DPI qualification has resumed
and is recorded under its own native geometry contract.

The concrete child uses the original
`DesktopRun::observe` for `ExecutionStatus`, separately reporting managed Running/Paused,
native activation and game observation. Pause/Resume wait for their exact original control
ticket, then verify actual Runtime state. Repeated controls return actual state without claiming
a new ticket; repeated Pause still completes original paused maintenance. Resume refuses absent
activation before control admission. A later control wait/observation failure fences successor
execution and keeps original cleanup custody. Running-only observation is not a canonical
capture or a presentation proof.

All paused-only child operations now refuse Running before entering mutation/failure handling.
This matters for ordinary Step/capture refusal and for an existing candidate's attempted adoption:
neither may accidentally retire a healthy Running incumbent. Source/dual-state/presentation
revalidation remains unchanged; a candidate prepared before subsequent continuous gameplay cannot
silently replace that newer state. The CLI returns successful execution controls independently
from a later expected full-capture refusal. No implicit Pause, Resume or activation is added.

Independent child/control and Host/process/CLI reviews find no remaining P1/P2. The new native
consumer sends matched scan-code D Down/Up to the exact foreground child. It reads actual
completed tick differences and applies each selected executable's movement law; it does not use
requested Step, manufactured frame deltas or status polling as proof of a tick. It also checks
movement cessation after Up, stale prepared/cancelled/consumed tickets, original source facts,
complete canonical paused restore/presentation, Pending-predecessor play and Running Close.
Its guard releases injected input and restores the observed original foreground; failed cleanup
retains original owners and source/work copies.

| Scope | Actual result |
| --- | --- |
| Ordinary game library, finite child and CLI CPU targets | 73/73, run `95b2d3da-58b6-4fbc-afad-756f457379ec`, 10.612 seconds. Includes actual unchanged headless state after all three desktop-control refusals, continued original Step/Close and rejection of caller-supplied execution facts. |
| Final complete ordinary source/headless package | 103/103, two actual-build cases kept separate, run `fb91213f-6af9-4670-b0f4-0eab301764da`, 12.307 seconds. Includes complete resume, scheduling phase, source fencing, progress/save and CLI controls. |
| Final desktop-feature library, finite child and CLI CPU targets | 76/76, run `c4a944eb-4bac-4816-abf9-c97ae9764e09`, 8.286 seconds. Retained Close, post-admission timeout, placement and execution-fact refusal controls remain. |
| Existing actual source/headless A/B/C and production Host adoption | 2/2, two ordinary cases outside the ignored-only filter, run `faf5ec20-a6cf-4409-bfc9-8194669469b8`, 115.818 seconds including nested Cargo. The original A/B/C trajectory passes in 48.109 seconds and production Host adoption in 67.709 seconds. The latter now additionally refuses all three desktop controls while preserving the exact live headless state and route. |
| Existing actual paused desktop A/B/C adoption | 1/1, run `5fb0bc06-05d1-41b0-9d4c-3c0041d12126`, 124.229 seconds including nested Cargo. All original compile/probe/source-drift/state/ticket/complete-presentation/activation/Pending-time/physical-cleanup controls pass through protocol four. |
| Actual continuous desktop body/private-structure rebuild/adoption | 1/1, run `aca24f64-97bd-4ff1-a216-20a159b25214`, 102.744 seconds including nested dev-profile Cargo after a 1m11s outer release build. A160, edited B80 and structurally edited C execute real native input continuously; resumed stale-candidate refusal, independent predecessor cleanup and final Running Close pass. |
| Ordinary continuous JSON CLI and actual native processes | 1/1 complete trajectory. Original and freshly adopted children each advance 30 naturally completed ticks, with recoverable Running-operation refusals, repeated controls, stable paused captures, exact restored presentation, Pending-time play and actual normal native exits. The contained children report hardware Vulkan. Logged observation blocks are approximately 1.924 seconds each and include control/IPC/paused confirmation work; these are not frame-time or throughput measurements. Work storage is removed after the exact process handles are observed exited. |

The sample adds only a Windows test dependency edge to the already locked `windows-sys 0.61.2`
for actual matched input and foreground observation. No dependency version changes, runtime
process-environment widening, new native authority, public root Host API or scheduler is required.
Production library/bin checking passes under locked/offline resolution. Native execution is
Windows x64 with a real hardware adapter selected by the contained child's default policy.
The complete scenario duration includes several builds/probes, native activation and cleanup;
it is not a per-edit latency, performance improvement or P50/P95 result. This game-owned continuous
workflow does not integrate supervised Editor Play, remove finite session bounds, migrate arbitrary
native services or qualify Linux/macOS. Full R12 and measured product/cost goals remain active.

Correctness Clippy passes all default and desktop-feature sample targets. Root/sample format
checks, the unchanged-version manifest/lock dependency edge, added local links/fragments and
whitespace checks pass. Existing style/dead-code warnings remain outside that gate. The two
production review lanes and additional native-consumer custody review are independent from
implementation; no remaining P1/P2 is found. This is static review, not injected execution of
SendInput failure, external focus theft or native cleanup timeout. All six scoped threads are
closed. All owned Cargo/native/CLI sequences are terminal, and the original physical cleanup
observations above retain their actual scope. Only the game-owned concrete Host,
protocol, CLI and consumers change; the root native process, Input, Winit, scheduler and GPU owners
retain their already qualified source. This is why no unrelated Editor or complete framework
matrix is repeated for this unit.

The next cost attribution should separate actual Cargo work from source checks, captured
executable installation, startup/restore, complete-state probing, fresh-adopter presentation,
activation and predecessor retirement. For continuous execution, compare the ordinary game with
the development presentation observer under the same scene and pacing: the latter currently
captures canonical state and computes a digest at every eligible Seal, retaining eight inputs.
Measure that work, native polling and finite file-protocol traffic before choosing a cache or
notification optimization. Retaining complete source/state/presentation checks is part of any
accepted optimization. No full service serializer, replacement native transport or new scheduler
is implied by this measurement follow-up.

## Development Cost Attribution And Measured Optimization

Status: attribution qualified within the finite Windows scope below; no performance change is
selected without further locating evidence. Keep the qualified continuous reconstruction workflow and its complete state,
source, input, presentation and physical-cleanup checks. Add default-disabled concrete Host
timing observations at original stage boundaries. Fixed phase aggregates retain counts, failed
operations, inclusive total and maximum elapsed spans without callbacks, an event stream or a new
tracing authority. Nested phases overlap and cannot be added into a total latency. Disabled
timing must not read a clock for each observed operation or change admission and cleanup.

Measure actual no-edit, movement-body and private-state-structure changes in an external copy of
Two-Level Game through the existing production desktop Host and native Cargo owner. For each
class use one excluded warmup and five retained attempts; keep each Host below its existing eight
rebuild allowance. Each attempt qualifies complete probes, disposes the probe, restores a fresh
adopter, revalidates final source/state/presentation, exchanges the route, activates and executes
new logic while predecessor cleanup is Pending, then observes normal cleanup. Record actual
compiler/profile/target/source/hardware and exact timing endpoints. Raw per-attempt phases and
outer operation spans accompany the report. Five samples give a finite median/range comparison,
not a stable P95 or cross-platform budget.

Compare ordinary running and paused idle game execution with the development presentation
observer under the same scene, actual native backend, window size and pacing. Alternate pair
order, exclude fixed warmup frames, collect bounded accepted-frame observations and actual
semantic/tick facts, and retire original native owners before reusing the physical loop. The
frame-start-to-post-Host wall spans and optional original GPU-completion observations have
different endpoints; neither is GPU execution time or scanout. No synthetic Step or fabricated
delta may stand in for naturally completed running frames. Invalid input, changed scene, lost
receipts or incomplete cleanup invalidate that run.

Paced wall spans can hide CPU differences. The measurement-only `cost-measurement` feature adds
read-only current-process CPU and memory counters through the already locked Windows SDK package.
Sample counters at the first and last measured post-Host callback, outside startup/warmup. Their
CPU difference covers all process threads, observer/polling/background/driver work over the
reported frame intervals, not one individual frame or GPU execution. Working-set and private
memory values are whole-process point observations; sequential runs share loaded code/driver
allocations, so these cannot prove a per-UI footprint or a leak. Normal game and child feature
selections do not enable this measurement dependency.

Choose an optimization only after baseline attribution identifies material repeated work. Any
change must preserve complete canonical and exact Seal/receipt association, remain within the
original game/native/GPU owners and pass before/after correctness and matched cost observations.
Do not introduce generic cache, snapshot, process, scheduler or performance-management layers.
Keep uncertain measurements and remaining numerical/foreign-platform/Editor integration limits
explicit. UI and physical mixed-DPI stay paused.

### Cost Attribution Qualification

Date: 2026-11-14. Actual endpoints, raw samples, machine scope and overlap/quantization limits
belong to the [measured report](../benchmarks/2026-11-14-development-runtime-costs.md). The production
Host adds default-disabled fixed cumulative stages without callbacks or a new lifecycle owner.
Elapsed completed calls retain returned failure counts and original results; panic/unfinished
calls are not recorded. Disabled observation performs no instrumentation clock reads.
Implementation revisions are `f1c2256` for Host timings and `f79c7da` for the complete measurement
consumers and structural-fixture correction. Final qualification covers the same source measured
before those local commits; the raw metadata preserves that chronology.

The first actual rebuild run `7f173559-fe78-427a-8d69-d10773064080` fails in the structure cohort
after 341.663 seconds because the reused structural fixture required a prior 80-speed edit.
That run is not accepted as the baseline. The owning helper now checks preservation of the
original movement implementation, so structure-only experiments stay at speed 160 and existing
80-speed structure regressions retain their oracle. A second measurement-label finding removes
the unconditional release string in favor of actual `debug_assertions`, with execution profile
recorded in the external command metadata. Independent bounded rereview closes both P2 findings.

| Scope | Actual result |
| --- | --- |
| Timing opt-in, results, failures, missing phases and saturation | 4/4, 66 outside this filter, run `723c5d41-4e4f-423d-9116-0c88e0d4c1cd`, 0.132 seconds. |
| Complete measured actual desktop rebuild/adoption | 1/1, run `9d957f4c-11f9-48e3-8d26-bfb83a2feddc`, 469.399 seconds. Three warmups plus fifteen retained no-edit/body/structure workflows preserve complete probe/state/source/presentation/native activation and clean original process receipts. |
| Final ordinary package | 107/107, two actual-build cases kept separate, run `f497ce8a-b3ee-49fb-b072-55240553b71f`, 10.644 seconds. |
| Final desktop-feature library, child and CLI CPU targets | 80/80, run `3d76ee66-b791-4c45-835b-1ff35b86dc65`, 7.290 seconds. |
| Existing default-disabled headless actual builds/adoption | 2/2, two ordinary cases outside ignored-only filter, run `6947afa2-efe6-4762-a439-2dfa11ec540b`, 128.138 seconds. Original A/B/C takes 52.615 seconds; production Host adoption takes 75.521 seconds. |
| Existing default-disabled continuous native gameplay/adoption | 1/1, run `d8d55286-601b-43ba-8c76-56447a931271`, 100.894 seconds. All original real input, modified logic, source/state/presentation, Pending-time execution and Running Close assertions remain. |

Retained complete correct-running medians are 14.7706 seconds without edits, 23.7901 for actual
body edits and 25.4273 for private structure edits. Cargo-adapter stage medians are 3.7280,
12.1573 and 14.0347 seconds. Installation is about 0.1 second; original two-child startup is
about 3.2 seconds and six exact presentation checks about 2.0 seconds. These stage observations
overlap their parent operations and do not form an additive partition.

The ordinary/witness comparison completes six real native runs per recorded comparison with
120 exact accepted frames after 30 warmups, explicit canonical-witness coverage, final original
GPU-completion seals and physical event-loop recovery. Paced frame wall spans overlap around
33.3 ms. Subsequent whole-process CPU observations are quantized and overlap substantially;
working-set/private observations are not per-UI footprint or leak proof. No witness cache,
installation cache, weakened qualification, alternate transport or performance improvement is
selected from these inconclusive or low-cost dimensions. Further attribution should focus on
Cargo admission/commands/capture and original child/control turnaround while keeping the same
complete endpoint. Representative numeric budgets and full R12 remain active.

Final all-feature/default correctness Clippy, root/sample formatting, affected manifest/lock,
raw-record consistency/digests, local documentation links and whitespace checks pass. The opt-in
measurement executable writes a new explicit result file after full cleanup; ordinary startup
text is not mistaken for its structured JSON. It records actual Schema/content/time-policy
agreement while leaving the deliberately different recipe configurations separately visible.
Windows process counter ABI/arithmetic and native measurement cleanup receive independent
read-only review; no remaining P1/P2 is found. All five scoped threads close and all owned
Cargo/native processes reach terminal observations. Whole-process CPU quantization, sequential
driver/cache memory carryover, small workloads, sample count and unchanged foreign/Editor scope
remain limitations, not performance budgets.

### Cargo Segments And Paused Control Turnaround

Status: qualified within the finite Windows scope below, at `d11a8c0`/`ebbc1e6`.
Attribute the original Cargo adapter
through its existing `CargoBuildPoint` boundaries, keeping the total Host phase and explicitly
inclusive segment names. The intervals include their adjacent source/tool/lease checks; a build
segment is not raw compiler time. Disabled Host observation must remain free of instrumentation
clock reads. Preserve the existing callback admission, failures, retained cleanup and exact
captured result rather than adding another build or trace owner.

Measure repeated actual paused capture, execution-status and canonical-presentation requests
through the public production desktop Host before changing the child. Keep a meaningful complete
game state, rotate request order, exclude warmups, preserve the exact original state and
presentation identities and physically retire the original process. Retain raw samples and the
actual compiler/features/native backend. A response duration includes file transport and original
native work; it is not a CPU-only or interprocess transport benchmark.

The current child's quiescence wait polls the native Runtime before checking whether its original
complete paused boundary is already eligible for capture. Evaluate checking that original
authority first and polling only when it is incomplete. Preserve the serve loop's native ingress
poll before request reception, deadline/health checks, pending maintenance, strict capture
eligibility and fresh presentation confirmation. No cached state, weaker receipt, skipped source
check or replacement input/pump authority may stand in for the original completed frame.

Choose and qualify this change only against matched before/after control observations and the
complete no-edit/body/private-structure rebuild endpoint. Keep initial startup, Running controls,
stale-candidate refusal, failure custody, matched input and physical Close in their existing
regression scope. Numerical tails, foreign platforms, Editor integration and full R12 remain open.

The [paused-control report](../benchmarks/2026-11-14-development-runtime-costs.md#paused-control-turnaround-successor)
retains before/after runs `d2233cb8`/`6940ede9`: one excluded warmup and ten retained requests
per operation in each original child. Exact complete canonical bytes and execution status agree;
the only changed declared child input is the quiescence wait. Capture and fresh-presentation
medians change from 135.344/461.789 ms to 60.965/268.541 ms. Status-query ranges overlap.
Original input reception, initial startup, complete paused-frame authority and fresh original
presentation remain required. These correlated process-local samples do not establish tails,
CPU savings or scanout latency.

Complete workflow run `3aa881d4` passes all eighteen real no-edit/body/private-structure attempts
in 419.557 seconds. Correct-running retained medians are 12.6964/20.9315/23.1441 seconds,
including original probe disposal, fresh restoration, final revalidation, post-adoption Step and
natural resumed execution; predecessor retirement remains separate. All phase failures are zero.
The older full-workflow baseline predates the six-segment instrumentation and runs under varying
compile/cache/load conditions, so the report does not claim a stable end-to-end improvement.
Native continuous and paused-adoption regressions pass 2/2 in `9561f554`, preserving the
original input, stale-candidate refusal and physical-cleanup assertions.

Six original Cargo intervals now expose metadata/rechecks at roughly 1.3–1.5 seconds and
compilation/checks/decoding/artifact hashing at roughly 2.0/10.4/12.2 seconds by class.
Two original child startups still cost roughly 3.2 seconds. Further attribution should focus on
those existing owners. Captured-byte/provenance and final-validation stages remain small;
no cache or weaker validation is selected from these observations. Two independent source and
measurement reviews find no remaining P1/P2; their platform, outer-source and sample limits are
retained in the report.
Final all-feature/default ordinary tests pass 113/113 and 110/110 in `fdc5fbf1`/`ba2c2509`, with
hardware/actual-build cases kept separate. All-target/all-feature correctness Clippy and sample
formatting pass. Four scoped agents close; existing warnings and broader platform/product limits
remain. UI and physical mixed-DPI work stay paused, and full R12 remains active.

### Declaration-Only Cargo Preparation

Status: qualified within the finite Windows scope below, at `215ef54`; the affected logical-input
adapter integration is corrected independently at `eec5f00`.
A bounded alternating command-only experiment on the same pinned
toolchain/desktop selection observes full metadata around 1.064 seconds and 1.32 MB, versus
`--no-deps` declarations around 72 ms and 7 KB. This exploratory result is not complete build
acceptance and does not remove graph/feature requirements from consumers that actually need them.

The finite synchronous build adapter needs Cargo's selected workspace package, binary declaration,
required features, source path and output directory before execution. Its actual compiler artifact
already owns the resolved compiled feature observation. Add an explicit declaration constructor
on the existing `CargoBuildInvocation`, sharing metadata bounds and selectors, and use
`cargo metadata --no-deps` only in that concrete adapter. Keep full `CargoMetadataFacts` strict
for Editor/graph consumers. Represent unavailable workspace feature observations explicitly;
`None` is different from a resolved empty set. Do not infer feature closure, target eligibility
or dependency resolution in Nara, cache metadata, or add a second Cargo/process authority.

Retain the selected artifact's exact manifest/package/target/source/required-feature/profile/output
binding, existing checkpoints, source/tool/lease revalidation, bounded diagnostics, immutable
executable capture and cleanup custody. Update affected public callers for the explicit optional
observation rather than returning a misleading empty feature set. Compare matched complete
no-edit/body/private-structure workflows against the current six-segment baseline and execute
the existing independent native Cargo consumer, failure/stale controls and relevant Editor
build-facts regressions. Full R12, foreign execution and paused UI/DPI scope remain open.

The original exact binding, decoder and cleanup CPU cohort passes 44/44 in `16fc6ee6`;
the independent declaration/decoder API consumer passes 3/3 in `d836b769`. Existing actual
native/foreign Cargo consumers pass 2/2 in `963fd950`, retaining dependency-qualified
`required-features`, metadata-only decoy features, failed compilation diagnostics, source drift,
revocation, immutable last-good output and source-removed native execution.

Complete run `c4051bab` passes all eighteen game workflows in 424.893 seconds. Metadata/recheck
medians fall from 1.5414/1.2933/1.3158 seconds to 0.1685/0.1586/0.1464 seconds across the
three classes. Correct-running medians are 11.7132/21.8066/22.0341 seconds. The body compilation
segment grows on this local run, so its full median is slower despite the metadata improvement.
The [report](../benchmarks/2026-11-14-development-runtime-costs.md#declaration-only-cargo-preparation-successor)
keeps this attribution and all raw observations separate from an overall speedup claim.

An affected Editor compile exposes an inherited missing `KeyRestored` match arm in the original
UI timeline adapter. Restore is a logical held-member admission, not a physical Press/Started
edge. Explicitly pass it through without synthesizing navigation or activation; preserve the
original input owner and execute the affected UI/Editor CPU regressions. This small integration
correction does not resume paused UI feature work. Final verification is recorded with the
qualified source and report rather than inferred from the independent Cargo tests.

The corrected Editor build-facts cohort passes 11/11 in `7ca6cf6e`, and affected UI library
tests pass 124/124 in `1a0b57af`. Root tooling and independent all-target correctness Clippy,
affected root/Editor/UI and independent formatting pass. The declaration API migration is
explicitly breaking: invocation `metadata_features()` returns an optional observation, and
finite-build metadata digests now cover declaration-only responses. Strict full graph facts
keep their existing interface and Editor lifecycle. Two independent bounded reviews find no
remaining P1/P2; all four scoped agents close. No dependency versions change.

The next startup attribution should distinguish namespace preparation, exact launch authority,
native spawn and initialized READY within the existing game process owner. There is no evidence
to remove the neutral keyboard inventory's full paused-frame/maintenance preconditions. Keep
fresh restored presentation separate from READY and preserve complete workflow/state/failure
and physical cleanup acceptance. This qualified preparation change does not complete R12 or
resume paused Editor UI and physical mixed-DPI work.

### Original Child Startup Attribution

Status: qualified within the finite Windows scope below, at `ed4cd86`.
Split the original `ChildStartup` total into namespace preparation,
exact executable launch authority, original native spawn and initialized READY wait. Keep these
as default-disabled elapsed observations inside the existing game process owner. Namespace
preparation includes original project/store checks, private control-directory admission, retained
lease, identities and command construction. Exact launch still checks original installed bytes
and restrictive file/directory handles. Native spawn still owns suspended-image/containment/output
admission, and READY wait still validates the exact original protocol response and startup state.
Errors belong to their actual phase without losing pre-spawn or post-spawn custody.

Measure six alternating headless/desktop rounds through public `DevelopmentHost`, excluding the
first original process for each placement and retaining five independent starts per placement.
Compile the two explicit feature selections with the same pinned tools and inventory. Verify
fresh complete state, zero progressed logical ticks, exact active generation, per-host phase
counts and normal original physical retirement. Desktop presentation remains a separate fresh
receipt after READY; the two placements' recipes/configuration are deliberately distinct.
Write raw results only after every original owner and source fixture closes. Preserve disabled
instrumentation's zero clock reads, ordinary operation results and all existing limits.

Choose any startup optimization only from the measured owning phase and evidence of redundant
work. Do not remove source/executable/namespace validation, native admission, neutral input
maintenance, fresh restored presentation or normal cleanup to make startup numbers smaller.
Keep complete rebuild/adoption, Running controls and prior failures in their appropriate existing
regression scope. Full R12, foreign native execution and paused UI/DPI scope remain open.

Actual run `15323d4f` passes in 81.953 seconds with twelve independent successful starts and
an excluded real malformed-project failure after spawn. Five retained per-placement medians
place `ChildStartup` at 145.984 ms headless and 1595.177 ms desktop. Original native spawn is
30.522/34.682 ms, exact launch authority 23.813/34.087 ms, namespace preparation 4.437/5.099 ms,
and initialized READY wait 89.252/1518.218 ms. Complete public Host start is separately
242.078/1780.243 ms, including installation and immediate post-READY observation. Children fit
inside the original startup total with small construction gaps; neither parent/child values nor
separately computed medians are additive. The [report](../benchmarks/2026-11-14-development-runtime-costs.md#original-child-startup-attribution)
retains all raw observations, complete canonical bases, original presentation identities and
cleanup receipts, including the actual failed child without a normal-exit claim.

No startup optimization is selected. Most desktop startup falls inside the broad original READY
wait, which includes source/Runtime/native/GPU, protocol transport and polling. Next attribution
must separate those original child stages before choosing a change. Original continuous/paused
adoption passes 2/2 in `45251983`, desktop CPU 83/83 in `1a4a8738` and final default-disabled
ordinary tests 110/110 in `94a9563c`. All-target/all-feature correctness Clippy, formatting and
latest consumer compilation pass. Process and measurement reviews close with no remaining P1/P2.
A test-only P3 panic/fixture retention issue is fixed after measurement and compile-checked;
its raw record retains the earlier source digest, and injected-panic runtime proof is not claimed.
All three scoped agents close. Full R12 and the paused UI/DPI scope remain active/open as before.

### Original Desktop Startup Operations

Status: qualified within the finite Windows scope below, at `11c1a67`.
The parent READY interval is dominated by desktop initialization, but
does not identify source, Runtime, native session or first-render work. Observe these original
operations in `DesktopRun` through an explicit default-disabled option and fixed read-only
snapshot: content preparation, Runtime startup completion, native session construction, the
first nonblocking native poll and accumulated native polling through first actual presentation.
The aggregate observes the original receipt, includes the remainder of the presenting poll and
excludes caller gaps; it overlaps the first-poll span. Keep it absent before presentation or when
blocking execution interrupts polling. Reuse the existing owner and preserve its failure, context transfer,
cleanup, input and presentation order. Completed elapsed spans do not imply successful startup.
Missing phases are explicitly unobserved; an already-started Runtime has no source/startup spans,
and blocking execution must not mislabel its whole lifetime as a first native poll.

Consume the observations in the existing ordinary desktop runtime comparison, alongside its
original paused initialization, neutral keyboard maintenance, actual accepted frames, canonical
witness, telemetry seal and physical event-loop recovery. The root API requires no game process
protocol extension, trace registry, scheduler or alternate runtime owner. Keep it disabled in
ordinary game and development child recipes. The same direct consumer may distinguish wall
spans around initial settlement and neutral maintenance, but not identify pure GPU initialization
from native polls that also create windows and execute frames. Initial exploratory measurements
showed why the first poll is insufficient: subsequent runs reused the physical loop and returned
from the first poll in microseconds while initial settlement still took hundreds of milliseconds.
Keep that exploration distinct from the completed aggregate's qualification.

Verify default-off behavior, unobserved/failed preparation, already-started and pre-start-close
paths, stable snapshots after later work, relevant native product regressions and the actual
consumer. Choose further optimization only from attribution that identifies the responsible
original operation. Keep shared-driver/cache, sample/platform and cost limits explicit; full R12
and paused UI/DPI work remain open.

The [qualification and raw observations](../benchmarks/2026-11-14-development-runtime-costs.md#original-desktop-startup-operations)
retain the corrected five-span consumer: six release/DX12 runs preserve all original 30-frame
warmups, 120 exact measured frames, canonical witnesses and physical cleanup. Later first polls
take 0.003–0.004 ms while total native polling through first presentation takes 622–1043 ms;
neither is GPU execution or a cold-start/tail budget. The public Copy/Eq/Debug snapshot excludes
private unfinished accumulation and stays fixed after completion. Final root/public controls
pass 28/28 in `a085eacd`; original default-disabled continuous desktop rebuild/adoption passes
1/1 in `50d3eb09`. Minimal code-first feature checking, selected root and all-target/all-feature
sample correctness Clippy, formatting and document/data checks pass. Independent source review
closes its two P2 findings; all three unit agents close. Further attribution should distinguish
original window/device/resource/first-frame operations before selecting any optimization.

### Native First-Presentation Attribution And Demand Preparation

Status: qualified within the finite Windows scope below, at `42b4eef`/`44725e0`.
Locate the work inside the measured native polling span before
choosing a device, resource or pipeline change. Inspect the original Winit and WGPU owners in
parallel, and preserve their native, queue, source and cleanup contracts. Keep the existing
complete ordinary runtime comparison as the actual measurement consumer. Retain its first
original presentation's App/render/window/device identities and App-start-to-post-Host span,
separately from the 30-frame warmup and 120-frame running cohort.

Use a finite, explicitly temporary source-local timing experiment if existing observations
cannot locate the cost. Record its exact patch and raw output outside the checkout, then remove
only those owned instrumentation hunks. Do not introduce a permanent trace callback, registry,
global device/cache owner or general profiling framework for this investigation. Distinguish
actual elapsed measurements from source-proven call counts and from expected savings.

Select a production change only when it removes unnecessary work or repairs the owning contract
without weakening packet/format/material/target admission, original presentation or physical
retirement. A candidate demand-driven preparation change must still support the first actual
use of every affected stock path and reuse the existing device/format/resource owners. Qualify
the changed public runtime, affected native renderer paths and real Rust rebuild/restore/
continued-gameplay workflow. Keep UI/DPI work paused and full R12 open.

The [first-presentation report](../benchmarks/2026-11-14-development-runtime-costs.md#first-presentation-and-original-gpu-calls)
retains three distinct six-run datasets: original baseline, explicitly instrumented attribution
and final production without probes. First actual App/render/window/device facts remain separate
from the 30-frame running warmup and 120-frame sample cohort. Adapter request is the largest
measured original call at 524–740 ms; Instance, Device and quad Pipeline spans are smaller and
still include driver waits. The exact temporary probe/consumer delta is retained and all probe
code removed. Correct handoff isolation, required Canvas/final presentation and the independent
fresh adopter remain.

Only two source-proven redundancies are changed: material layout is prepared for actual extension
use, and Surface configuration consumes one capabilities snapshot while retaining pinned WGPU
defaults and the original mode/color policy. Real default/custom failure/retry, texture/parameter
pixels, cache reuse, cleanup and device replacement pass in renderer run `02ad471f`, 24/24.
The final six-run consumer preserves every original witness, frame, neutral-state and cleanup
oracle. Complete continuous/paused desktop body/structure rebuild/adoption passes 2/2 in
`d593e489`, 227.734 seconds including actual nested Cargo. Minimal feature checking, renderer/
sample all-target/all-feature correctness Clippy, formatting and document/data checks pass.
Independent source reviews find no P1/P2; four scoped agents close. Sequential timings do not
establish a dominant-latency improvement, cold-start or tail budget. Further work should locate
the original Adapter request cost before changing policy, scheduling or device reuse.

## Host-Composed Execution Capture

Status: qualified within the finite Windows scope below, at `3e8ba7d`/`588e698`. The bounded Adapter source follow-up
finds no justified change to current selection or ownership. Continue the complete diagnostic
workflow without turning that source inspection into a performance claim or speculative cache.

Use the existing independent editor-panel consumer as one concrete Host: delayed source import,
stale-basis edit refusal, original game desktop frames/completion and complete Close. Keep Editor
UI, Dialogue and physical mixed-DPI work paused. Compose a thread-local standard tracing Layer
in the consumer; add no public collector, sink trait, global subscriber, clock or budget manager.
The already-selected `editor-work-observation` feature owns the new static lifecycle spans.
Import retains an explicit operation Span through actual worker return and publication retirement,
propagates the scoped Dispatch/parent to the existing worker, and distinguishes incomplete Drop.
Prepared editing records its original basis and typed prepare/commit outcome without refreshing
stale data or changing error/diagnostic publication.

The consumer's fixed capture accepts only static metadata/outcomes and whitelisted numeric
correlations. Bound event and active-span storage, report reserved payload bytes, overflow/faults
and incomplete capture, and export standard Chrome Trace Event JSON only after product cleanup.
Do not derive event order/lifetimes from aggregate timing snapshots. Original frame starts and
completion sample durations preserve distinct Runtime, App-frame, render-transaction, window,
backend and epoch identities; sample reception is not its completion timestamp or GPU duration.

Qualify actual delayed import and stale edit without a native Editor frontend, then the original
desktop/GPU path. Compare finite identical workloads with capture disabled/enabled, forcing
overflow and collection/export failure separately; product outcomes, state and physical cleanup
must agree. Measure enabled/disabled elapsed overhead, fixed retained storage and dropped counts
without claiming controlled cold starts or reliable tails. Inspect the exported file with a
standard viewer/parser and retain decision-local raw facts. ADRs 0048/0068 now explicitly separate
safe execution correlations from diagnostic-field logging; their original privacy and ownership
contracts remain. Full R12, representative budgets and paused product work remain open.

The implementation and qualification now complete this bounded successor. The independent
module consumer's actual Host performs a delayed asset import with a controlled 10 ms worker
hold, a valid property edit to 23, a stale-basis rejection that preserves the complete document
and history, actual Undo/Redo, and complete Session/OpenHost cleanup. Its original Two-Level Game
desktop path retains the complete canonical paused state, twelve original presentations and
completion samples, telemetry seal, target destruction and physical event-loop recovery. Eleven
serial Windows/DX12 release runs compare excluded disabled/enabled warmups, three retained
disabled/enabled pairs, overflow, collection failure and export refusal. All product state,
authoring results, adapter/extent facts and physical cleanup agree across modes.

The enabled run retains 111–121 events with zero drops, active spans, rejected fields or faults;
the fixed payload reservation is 1,698,304 bytes and observed high-water payload is 45,304–49,384
bytes. Disabled capture retains zero bytes/events. Overflow and collection-failure controls report
their explicit incomplete/drop/fault facts, and export refusal preserves `AlreadyExists` after
product cleanup. Perfetto v59.1 parses the retained enabled trace: 115 intervals on 115
capture-local tracks, one held import, one stale-revision refusal, twelve matched presentation /
completion pairs, no missing parents and no parser errors. The three retained elapsed pairs vary
from -32.158 to +82.930 ms; the enabled mean is 1.09% above disabled in this shared-process,
shared-cache sequence. This is an overhead observation, not a stable budget or tail claim.

The first collector attempt exposed a real concurrent producer loss: one span disappeared with
the original `try_lock`. A focused four-thread regression now retains all 256 span lifetimes and
512 records. The private collector visits fields and resolves parents outside its mutex, then
uses the mutex only for bounded primitive state; it makes no lock-free or hard real-time claim.
The final selected consumer/collector cohort passes 34/34 (`66ecadf1-8e06-412a-be06-8e9099e768d3`),
the eleven native runs pass, and the bounded independent review finds no P1/P2. Raw JSON and the
viewer query are retained under the existing benchmark data owner. This qualifies the concrete
decision-local Host capture, not a public tracing framework, a profiler UI, per-pass GPU timing,
foreign execution, a representative game performance budget or full R12.

The final original import-lifecycle/settings-transition selection passes 10/10
(`8b309ff4-a07b-40b8-8202-9e06873b5973`), with 1359 other tests outside its filter. The required
root feature combination includes `runtime-2d,serde` as well as `tooling,editor-work-observation`;
the preceding narrower invocation selected zero tests and proves compilation only. Feature-off
root checking, feature-on root and selected independent-consumer correctness Clippy, root and
independent formatting, new document targets, JSON and canonical raw-record digests pass.
This closes the capture unit's verification record without repeating unchanged product work.

## Explicit Paused Desktop Cadence

Status: implemented and qualified for the known source-desktop inventory; full Foundation/R12
remains active. Implementation revisions are `ef67473` and `8b8954c`. The existing
[runtime cost report](../benchmarks/2026-11-14-development-runtime-costs.md)
observes ten App frames in every 300 ms paused interval despite zero simulation ticks. Compare
the existing continuous and demand-driven policies at the original Winit session, rather than
adding automatic Runtime pause scheduling or another wake owner.

Expose caller-controlled changes between the existing `Poll` and `Wait` policies through the
live original Winit session and DesktopRun. Switching to Wait preserves pending frame demand,
paused capture eligibility, the elapsed-time baseline and presentation maintenance deadlines.
Switching back to Poll requests original frame work. Refuse changes after fault or retirement.
The game-owned source-desktop continuity child selects Wait for its known paused inventory and
Poll for continuous gameplay; exact Step, semantic admission, native input, geometry changes,
presentation maintenance, telemetry seal and Close retain their original owners.

This is an explicit consumer policy. Wait does not discover ECS changes, wake on arbitrary task
completion, or keep real-time shader effects animated. Games needing those behaviors retain Poll
or explicitly arrange original frame demand. Runtime pause itself does not select a cadence.
Resume must still settle all paused wall time without simulation catch-up, including across
cadence changes; no clock baseline is reset to hide elapsed time.

Before selecting the change, extend the existing release runtime-cost consumer with paired
300 ms Poll/Wait paused intervals in alternating order, following a bounded native settle.
Retain complete canonical state, original frame/tick counters, actual presentation counts,
process CPU observations, poll counts, actual maintenance and resumed gameplay. Acceptance
requires unchanged canonical state and zero simulation/logical ticks in both intervals, at
least an 80% reduction in aggregate paused App frames, and complete original telemetry/native
cleanup. CPU counter quantization and the presentation observer's maintenance polling remain
explicit; frame suppression alone is not a CPU-free or universal power-saving claim.

Qualify switch/refusal/demand/deadline/elapsed semantics at the original session, actual native
window demand, and the production continuous game/rebuild consumer. Keep source-free shipping
Player defaults and paused Editor/UI/Dialogue work unchanged. Full Foundation/R12 stays active.

The original live cadence interface is implemented without another scheduler or clock owner.
The six new CPU controls preserve pending controls and explicit maintenance, original deadlines,
fault/incomplete-retirement refusal and long-wait time settlement, including the existing 13 ms
phase followed by a 7 ms interval. The complete Winit all-feature CPU cohort passes 87/87
(`d0bf3498-ff4b-4392-aee8-ae4b4f1ad125`), and actual native cadence/maintenance passes 2/2
(`4f19ba71-294e-49c2-a013-8d22d1326f11`). The sample's release all-target desktop/cost-feature
cohort passes 149/149 (`65619666-42d7-4a01-b2f7-403ac6f29065`), with nine actual native/build
cases kept separate.

The six matched Windows/RTX4090/DX12 runs pass the preselected gate: Poll produces sixty App
frames and sixty actual presentations across the paired idle intervals; Wait produces zero.
All canonical bytes and simulation/logical ticks remain unchanged. Every run separately
completes Wait maintenance, one exact Step with full maintenance, resumed gameplay, telemetry
seal and physical retirement. Quantized process CPU ranges overlap, and Wait still performs
120–124 observer/caller polls per interval. This selects suppressed demand-free App/presentation
work for the concrete child; it does not establish stable CPU or power savings. The
[cost report](../benchmarks/2026-11-14-development-runtime-costs.md#explicit-paused-native-cadence)
retains conditions, raw data and original outcomes.

Independent review found an integration gap before native game acceptance: the original
desktop reconstruction helper only polled, so an asynchronous Scene result could complete
without another App frame under Wait. Each required paused maintenance pass now explicitly
requests a full original frame until restoration finishes. The bounded rereview closes that
finding. The first actual desktop attempt fails at the preexisting structural-edit fixture's
assumption that `held` is Session's first field; the prior Player unit added `player_launch`
before it. The corrected bounded edit replaces the exact field inside the complete private
declaration, preserving every other field, attribute and public DTO check. Its independent
review closes without a P1/P2. Both complete actual desktop workflows then pass 2/2
(`565c699b-d665-4e25-8ebd-ece4df9dd413`), preserving body/structure rebuild, fresh restoration,
adoption, continuous native input and original cleanup. The continuous child reports Vulkan,
separately from the measured consumer's DX12; a parent hint cannot stand in for the child's
actual backend. No new public task wake or universal paused policy is selected.

Final release correctness Clippy passes the Winit all-feature/all-target owner, the root
desktop Runtime library feature closure, and all desktop/cost sample targets. Root/sample
formatting, new document targets, JSON and LF-normalized raw-record integrity checks pass.
Existing lint/style warnings remain outside that correctness gate. All nine scoped research,
implementation and review threads close. No unrelated Editor, Open-GPUI or shipping Player
acceptance matrix is repeated, and no remote publication is performed.

The remaining measured pressure is the observer/caller maintenance polling under Wait.
Attribute actual pending GPU completion, observer deadlines and child protocol polling before
choosing another change; removing App work does not prove a whole-process budget. Retain
explicit App work for asynchronous result integration and real-time paused consumers rather
than selecting a universal dirty-state or task-wake policy without a complete consumer.

## Explicit Observer Deadlines And Pending Submission Maintenance

Status: implemented and qualified in the original native game/Host scope; full Foundation/R12
remains active. Qualified revisions are `47490b1` and `bf92633`, continuing the original-owner
runtime work from `6a665ab`. The previous presentation observer forced a 2 ms maintenance wake
even when it had no deadline or new work. Conversely, an original GPU prefix without an
observer did not itself arrange the next Wait-mode wake.
Resolve both responsibilities together; merely removing the timer would lose GPU progress.

Keep `DesktopPresentationControl::Continue` as no requested observer wake and add explicit
`WakeAt(Instant)` for the observer's next required observation. Native/frame demand may deliver
an earlier callback. Each callback supplies its current request; Continue withdraws only that
observer's deadline. Merge the requested time with original first-present/READY, unsettled
submission, telemetry and seal deadlines. No second scheduler, GPU poll thread or App frame is
introduced for maintenance. Existing finite tools with observer-owned timeouts must declare
their original absolute deadlines; caller-owned protocol polling remains separate.

The original WgpuRenderHost reports whether its submission-prefix custody still needs
maintenance, independently of telemetry enablement or sample capacity. Prefixes already
completed but awaiting owner bookkeeping also keep that responsibility. Preserve callback
abandonment recovery, samples with a concurrent maintenance error, and mandatory final seal
delivery/veto. Release-induced cache cleanup belongs after the corresponding original prefix
completion; avoid declaring a clean maintenance pass before its callback drops are observed.
Telemetry counters alone are insufficient evidence of a settled native submission.

This is not a claim that every live Host cache is empty or that arbitrary future external
Drop/World/task changes auto-wake a native loop. Existing producer/policy/history/Feature cache
cleanup remains at its original owner boundaries, before admission and during retirement;
interaction revocation remains synchronous. Check Accepted contracts before selecting any new
final-drop notifier. Do not turn a bounded submitted-work query into a universal quiescence
service or make retained bytes/merely live references a permanent polling requirement.

Extend the existing ordinary release comparison with three finite idle conditions per run:
Poll, Wait with an explicitly requested periodic 2 ms observer deadline, and Wait with no
observer deadline. The periodic condition is a genuine consumer control for the previous
behavior. Alternate order, retain 100 ms settling and 300 ms idle spans, and let the caller
use the remaining interval as its native wait cap instead of independently capping every poll
at 2 ms. Record actual observer callbacks, native poll calls, App/presentation counters,
canonical bytes, completion and physical cleanup. Acceptance requires unchanged complete
state, zero paused ticks, at least 80% fewer idle App frames than Poll and at least 90% fewer
observer callbacks/native poll calls than periodic Wait. Preserve raw rejected-gate reports.
Whole-process CPU remains a separately observed, quantized metric rather than a substituted
callback count or power claim.

Also exercise an original explicit 100 ms observer deadline with a 1 s caller wait cap: it must
be observed by 250 ms in this finite desktop workload without advancing App/simulation state.
These are measured cooperative acceptance bounds, not interruptible callback/native-API
guarantees. Qualify no-observer and telemetry-disabled submission progress through the real
Host, owner deadline merging/withdrawal and final-veto tests, then the complete ordinary
control/telemetry/native workflow. Keep paused Editor/Dialogue product work and full R12 open.

Implementation keeps submitted-work demand in `WgpuRuntimeMaintenance::needs_poll`, derived
from original retained prefixes and pending/buffered telemetry responsibilities. It does not
infer all-cache quiescence from telemetry. A completed-prefix removal triggers another original
cache cleanup pass so callback-induced final-owner release is handled before reporting settled
submission work. Continue no longer schedules an observer timer; WakeAt merges with original
GPU, verification and seal deadlines. Five finite public consumers now declare their existing
absolute observer timeout. Source audit found no Accepted requirement for an additional idle
cache-drop notification service in this private root Host, so no such mechanism is added.

Root presentation/verification/control CPU tests pass 27/27
(`db654a36-d228-4515-aff8-6d447b15fce6`); affected Host/submission/telemetry CPU passes 45/45
(`89d0b2ea-8aef-4c7e-81d8-990f69bf623d`). Real DX12 GPU tests pass 17/17
(`464b5c78-73f3-4927-bd11-cad554c3baee`), including failed uploads and callback abandonment;
the four new maintenance tests also pass software DX12 (`d743e53f-829b-4aa8-82a3-c75f36c2ebbc`).
Actual telemetry-disabled submissions settle through their original prefixes. The independent
GPU tests manually register telemetry and do not stand in for Runtime presentation or claim
a measured GPU busy interval.

All six complete v4 Windows/RTX4090/DX12 native runs pass. Each compares three idle conditions
with identical complete state and zero simulation/logical ticks, preserving original running
samples, explicit maintenance/Step/Resume/Pause, telemetry seal and physical cleanup. Aggregate
Poll App frames/presentations are 60/60 and both Wait conditions are 0/0. Periodic observer Wait
records 1474 callbacks and 737 native polls; no-deadline Wait records 12 and six, a 99.1859%
reduction in both. Original 100 ms observer deadlines arrive at 100.0873–100.6636 ms despite
1 s caller wait caps, passing the finite 250 ms gate without App/game/presentation changes.
Whole-process CPU values are quantized and this does not establish a stable CPU or power budget.
The [report and full records](../benchmarks/2026-11-14-development-runtime-costs.md#observer-deadlines-and-submitted-work-maintenance)
retain all counts, actual wall spans, deadlines, state, cleanup and LF-normalized digests.

Two initial consumer compilations hit JSON macro expansion limits; the report literals are
split into bounded object construction without removing fields or increasing global compiler
limits. The first native comparison itself passes. Correctness Clippy passes the affected
GPU all-target closure, root desktop library and ordinary measurement executable. The five
migrated finite observers compile in their independent public consumer feature closures;
their unchanged native drawing workflows are not relabeled as newly executed. Seven scoped
research/implementation/review threads close with no remaining reported P1/P2. CI selects
the new software-DX12 maintenance cohort locally; no remote CI execution/publication is claimed.
Root, sample and all affected independent workspace formatting checks pass. New relative
document targets, workflow YAML, JSON, exact metadata observations and LF-normalized raw-record
digests pass. No dependency manifest or lockfile change is part of this successor.

This completes the selected pending-submission/deadline successor. The source-desktop child
still independently polls its control directory every 2 ms; replacing that protocol polling
requires attribution and a complete close/rebuild consumer, not an inferred benefit from these
observer measurements. Arbitrary Task/World/cache-lifetime notification, foreign native
execution, representative CPU/power budgets and the full Foundation plan remain open.

## Native Request Hints For The Development Child

Status: implemented and qualified for the finite Windows workflows below; full Foundation/R12
remains active and Editor/UI/Dialogue product work stays paused. Continue runtime work from
`66e5643`; qualified implementation is `686a5d9`/`021814c`. Before this unit, the source-game
child probed its private request file at a 2 ms cadence even
when its original paused native session has no work. Preserve the capability-relative packet
transport, exact session/generation/request validation, finite budgets and original native
process custody; change only the child's waiting and required lifecycle evidence.

Use one game-private, nonrecursive `notify::RecommendedWatcher` for the existing private
control directory. Retain that original watcher before setup can fail and explicitly poll
its existing vendor `try_close`/join API through partial startup, Close and outer failure
cleanup. Do not route control packets through asset-domain Source roots or extract a universal
watch framework. A final Closed reply requires both original game/native cleanup and watcher
join; unfinished close keeps the same owners for retry. Parent response polling remains a
separate measured responsibility.

Expose a cloneable maintenance-only wake handle from the original WinitSession/DesktopRun.
Use the physical event loop's original non-reused session identity, coalesce notifications,
revoke at retirement and reject stale handles across sequential sessions. The handle carries
no payload, Runtime/Window/GPU access or operation result. It requests no App frame and does
not reset elapsed time or paused capture eligibility. Actual native/input/control/frame work
and original maintenance deadlines remain authoritative.

Watcher callbacks retain only a bounded dirty/failure hint and wake the original consumer.
Cover atomic request publication's create/rename/modify events, uncertain/rescan events and
backend errors. Paths from notifications grant no file authority. Definite consumed-request
removal cannot publish a new command and
does not wake the consumer; uncertain/root removal and rescan retain reconciliation hints.
Arm native watching before the first packet check; acknowledge hints before checking the
original fixed inbox, then recheck before sleeping so publication during entry or event
coalescing cannot strand a request. Headless placement uses the same hint with a bounded
condition-variable wait.

Native watcher delivery is not a durable message queue. Keep an explicit 250 ms reconciliation
ceiling within existing total/inactivity/operation deadlines, including when notifications are
lost or the watcher has stopped. Observed backend failures remain failures rather than silent
success. Windows vendor notify still performs a 100 ms internal wait and has legacy error
reporting limitations; this work does not claim zero periodic wakeups or universal reliable
filesystem notification. Do not increase ordinary operation deadlines to conceal those limits.

Add game-owned diagnostic control-service counters to the response envelope, outside canonical
state and permissions, with a new exact protocol version. Read them only as observations from
the original selected child, not cross-process authority or proof of native cleanup. Expose
the last active child's observation through the concrete development Host so an independent
consumer can attribute request checks, waits/hints and watcher join without private hooks.

Qualify the same actual source/headless and source/desktop Host with periodic 2 ms and
notification-driven 250 ms reconciliation variants, changing only that concrete child constant
in the owned fixture. Use the original public Cargo capture, fresh processes, three alternating
pairs per placement and 500 ms idle intervals. Retain exact complete game state, process CPU
observations, real request latency and original cleanup. Acceptance requires at least 90%
fewer aggregate idle request-file checks, every measured ordinary request within 100 ms on
the available idle Windows desktop, successful explicit Frame/Step/capture and complete
original game/watcher/process retirement. These are finite cooperative criteria, not hard
real-time guarantees or stable CPU/power budgets. Rejected comparisons preserve raw facts.

Read the original `FrameExecutionStart` sequence into the game-owned observation, outside
canonical state. Two consecutive explicit public Frame calls must advance the original App
sequence, exactly once for the quiescent headless case, while preserving simulation/logical
ticks and complete canonical bytes. State equality alone cannot distinguish Frame from Observe.
If comparison cleanup fails, write the completed and current partial run, original error,
cleanup duration and retained owner directories before propagating failure. Preserve the same
Host/Fixture custody until process exit; an absent JSON file must not erase obtained evidence.

Also test atomic rename and already-published request hints, duplicate/uncertain events,
partial watch setup, errors, repeated Close and stale native wake handles. Run the actual
body/private-structure desktop adoption workflow after integration; a notification alone
does not prove reconstruction, presentation, exact adoption or continued gameplay. Full
Foundation/R12 remains active.

### Request Notification Qualification

Winit all-feature CPU passes 95/95 (`66b3743a-54b3-4c24-be75-ef0608c81ebd`); the actual
native cadence/wake cohort passes 4/4 (`73eeb0cb-5b06-4048-94b7-4d24b4c1234a`). The original
App/time/capture boundary, current-session handoff, stale-route rejection and retirement remain
covered. The final sample ordinary desktop/cost closure passes 170/170
(`81663bbb-9279-4644-9ead-0ecc9d68a85f`), including actual rename watching, partial setup,
join panic, repeated Close, condition-variable races and protocol diagnostic validation.
Ten native/build cases are skipped by that default run; selected actual execution is recorded below.
Original retained-pump/maintenance native controls also pass 8/8
(`2171b9e7-1e83-431a-9d34-e61cb6ff09fe`), including sequential handoff, late/queued old wakes,
keyboard continuation, hidden windows and startup recovery.

The first all-target check selected the old continuity tests through an implicit child-module
path; the watcher now explicitly selects its own tests. Independent protocol review also found
that invalid READY/reply semantics could overwrite the last valid diagnostic observation.
Validation now precedes the commit, with invalid startup kind/state, wrong Pause/Resume and
invalid Capture counterexamples; recoverable domain refusals retain their original semantics.
An earlier sample run passes 170/170 (`1a6e1752-ee79-4b0d-8b2e-5685f1f808df`) before the
separate measurement-review corrections. The final run above includes the original App-frame
observation and partial rejected-report preservation, not just that earlier source scope.

Independent source reviews close the native wake, game watcher/lifecycle, diagnostic protocol
and measurement findings. Their read-only conclusions do not replace the actual
twelve-process comparison or complete body/private-structure adoption workflow. Results and
limits are recorded in the [existing cost report](../benchmarks/2026-11-14-development-runtime-costs.md#native-request-notifications-and-reconciliation).

The first complete twelve-child comparison rejects headless 105-to-eleven checks (89.5238%)
while desktop passes 601-to-twelve (98.0033%); all original state, controls, latency and cleanup
checks pass. Its full rejected report is retained. Definite removal of the consumed request
was an unnecessary hint; filtering it preserves uncertain/root/rescan cases and the original
publication/250 ms fallback. The updated ordinary suite passes 170/170
(`dfcfd649-b328-4478-b18e-e014c24fc2ae`). The identical-gate rerun passes
(`077d5e17-a1ed-41f0-93b4-81a9de297c26`): headless checks 103-to-nine (91.2621%), desktop
594-to-twelve (97.9798%), all 150 selected requests within 100 ms, maximum 74.629 ms. All
twelve original game/watcher/process owners and four Fixture roots retire normally. Original
App sequences verify real Frame work without changing simulation/logical ticks or canonical
bytes. The child reports Vulkan/discrete-gpu; captured games use dev profile, no defaults,
headless no features and desktop only desktop. These are not release-game or DX12 measurements.

Actual production Host rebuild/adoption regressions pass 3/3
(`9fecaca5-bdff-4720-a4ef-dd3a5908cbbb`): continuous desktop, paused desktop and headless.
Each retains real Rust body/private-structure edits, semantic probes/fresh adopter state,
original pending predecessor cleanup and continued gameplay. Intentional compile-error controls
remain recoverable domain failures; all original game/watcher/native owners close. The continuous
child reports Vulkan and ends Running before normal Close; measured total test time is 244.838 s,
including actual nested builds, not a new edit-latency or runtime-tail budget.

The independently configured no-default-feature sample suite passes 166/166
(`65bd2042-31d2-4fc4-a235-5bca651487e6`). A final headless-only unused-reference warning
cleanup changes no operation; its child watcher/lifecycle suite passes 16/16
(`e7ade70b-f888-48b5-af7d-5db7f1934f17`). Locked/offline all-feature/all-target game checking,
Winit/game all-target/all-feature correctness Clippy and root/sample formatting pass. Existing
unrelated style/dead-code warnings remain. All ten scoped research/implementation/review
threads close. No new general watch service, vendor changes, dependency upgrades, registry
snapshots, remote CI run or publication are part of this unit. The sample lock adds the existing
vendor notify package's platform closure and necessary version disambiguation only.

The [raw metadata](../benchmarks/data/2026-11-14-development-runtime/request-notifications-metadata.json)
binds qualified revisions, passing/rejected reports, exact LF-normalized digests, profiles,
request selection and unexecuted scopes. Final document/link/data/whitespace checks close the
unit without rerunning unchanged product acceptance. Original parent response polling, vendor
internal wakes, cooperative operation limits, foreign native execution, representative
CPU/power/tail budgets and complete Foundation/R12 remain open.

## Physical Task Return And Game Capture

Status: active, continuing from `b18735b`. Runtime control/time and Scene request/residency
source review finds no new demonstrated P1/P2 in its bounded scope. The concrete game capture
instead uses diagnostic `TaskStats.queued/running` to infer authoritative task return. Queued
terminal work is excluded from queued statistics, and cancelled capture disposal has its own
original executor custody. A held cancelled capture can therefore outlive zero diagnostic
counts. Reproduce this through real original workers and a public paused game before claiming
the correction qualified.

Expose one non-driving `TaskPools::return_state()` snapshot through the existing
`TaskReturnState`. Reuse original root queues, running/disposal custody and activity pending
returns; do not create another counter, close authority, admission freeze or quiescence
service. A scope excludes sibling work and task cleanup failures while retaining shared worker
failure. Root facades describe their whole original facility. Pending work remains Pending
even if a terminal is already consumed or its handle is gone. Once that work returns, sticky
cleanup failure remains distinct from successful return. Published outputs, future admission,
worker joins and domain-native cleanup retain their own contracts.

Migrate the concrete game eligibility and its existing settling boundaries to this physical
return observation. Refuse pending work and returned cleanup failure without changing its
canonical state, Runtime/frame/tick identity, Scene receipt or gameplay queue. A successful
release must restore capture eligibility and permit fresh-runtime reconstruction and continued
ordinary gameplay. Test both cancelled-capture custody with zero diagnostic counts and sticky
failed disposal. Keep the original failed Close result separate from physical completion.

Qualify original scoped/root task execution, cancellation, captured Drop, completed output
ownership and original close paths; then the ordinary game default/desktop combinations and
actual headless/paused/continuous Rust rebuild/adoption workflows. Reuse root target and serial
Rust 1.95 nextest, independent review, correctness Clippy and formatting. Preserve paused
UI/Dialogue work and leave full Foundation/R12 active. A source-reviewed branch or snapshot
alone does not prove an isolated World checkpoint or arbitrary native rollback.

### Physical Return Implementation And Qualification

The ordinary public game regression first fails as intended, 0/1
(`f8fb5d14-60d2-4630-b16c-8396724963ee`). Original threaded workers are occupied, a queued
capture is cancelled and its terminal consumed, then its original Drop is held after all
diagnostic queued/running counts reach zero. The original game incorrectly admits capture.
The test releases its gates and closes the same original game even on that assertion unwind;
it does not mistake handle/terminal disposal for physical return. The release build takes
7m41s; that is verification compilation, not runtime-cost evidence.

Independent review identifies a separate P1 in the first root query: sequential per-kind
empty reads can miss continuously owned Compute-to-Io child work. A deterministic real-worker
handoff fails 0/1 (`04647778-85d9-4315-af9f-a42c15f1ceb8`), observing Returned instead of
Pending. Its child is released and original shutdown completes before the final RED assertion.
The correction holds the original Io, Compute and AsyncCompute queue guards in one fixed
order, evaluates work and failure under that common cut, then releases all guards. Scope
pending-return accounting is unchanged. A single `cfg(test)` callback coordinates this
interleaving only after guards are gone; shipping query state has no callback/counter addition.

The complete owning Task library then passes 109/109, no skips
(`56fb7364-bfb3-4dd0-861d-762d69238141`). Its twelve new controls include the deterministic
handoff, 128 real cross-kind child links, cancelled queued/raw entries, held capture Drop with
zero diagnostic work counts, terminal/observer loss, sticky disposal failure, original close,
root running bookkeeping, activity/sibling isolation and separately owned published outputs.
This does not qualify arbitrary escaped work, future admission or native service cleanup.
Game and actual native rebuild/adoption qualification follows the consumer migration below.

After migration, the complete ordinary desktop/cost game closure passes 172/172
(`ad65354d-338e-4a5f-88f6-c1f8a02c33cc`), including both new public game cases and all
existing ordinary controls. Held cancelled-capture disposal refuses state without changing
Runtime/tick/Scene facts; releasing it recovers byte-identical capture, fresh-runtime restore
and continued ordinary gameplay. Returned cleanup failure still refuses capture after an
ordinary tick, and original Close completes physically with its failed result preserved.
The full rebuild takes 10m13s; no runtime or compiler-speed claim follows from that duration.

The concrete child waits for physical return before READY, capture and restored observation
within its existing deadlines. Native paused maintenance still requests complete App work
when reconstruction needs it; waiting only for completed capture disposal does not request
extra App frames. The actual canonical capture revalidates the original query rather than
treating the preceding observation as a permission token. Resume format v2 and protocol v5
field definitions remain unchanged. Independent Task and game reviews close the findings;
the first five scoped research/implementation/review threads close.

The first actual native cohort passes the continuous-desktop, paused-desktop and headless
rebuild/adoption workflows but rejects the separate notification check-count gate
(`4a6b4e70-f2ed-4eb3-bf36-10ffcc941e1d`, 3 passed/1 failed). Headless periodic/notification
checks are 107/11, 89.7196% fewer, below the unchanged 90% requirement; desktop is 580/12,
97.9310% fewer. All canonical/control/presentation/cleanup checks and 150 selected requests
within 100 ms pass. The [existing report](../benchmarks/2026-11-14-development-runtime-costs.md#physical-task-return-and-request-check-ordering)
retains this rejection and its raw source/build provenance.

The child loop now waits before another inbox probe after a response unless a hint is already
pending. Startup still probes immediately; hints and original reconciliation waits make the
next probe due. Acknowledgement remains before receive, and the loop rechecks original lifetime
and request-budget limits after waiting. Native maintenance and the 250 ms ceiling are unchanged.
The correction changes no counter definition, transport, numerical gate or operation deadline.
The sixth independent review closes with no P1/P2; all six scoped agents are closed.

Final affected child tests pass 16/16 (`f81d441f-803e-4e0e-83c4-03acb43fb9f5`). The same
twelve-child comparison passes (`9482c865-13d3-48c1-a091-d1957cbb7302`, 135.308 s including
four actual builds): headless checks 105-to-nine, 91.4286% fewer, desktop 595-to-ten, 98.3193%
fewer. All 150 selected requests return within 100 ms, maximum 80.7093 ms. Original canonical
state, explicit App-frame progression, one-tick Step, joined watcher/native cleanup and four
Fixture removals remain valid. Both variants use the same corrected loop and native watchers;
their sole difference is the original 2 versus 250 ms reconciliation constant.

All three complete real rebuild/adoption workflows then pass again on the final loop
(`691442f5-2255-40d7-94ed-e005750fae11`, 246.749 s): continuous desktop, paused desktop and
headless. Real function-body and private-structure builds, expected compile failures, stale/
foreign candidate refusal, fresh-adopter state, continued play and separate predecessor/native
retirement retain their original owners. The continuous child reports Vulkan and ends Running
before Close. These are correctness workflows, not controlled iteration-speed measurements.
Outer tests use release; actual captured game children use dev with explicit no-default feature
selection. Headless/default and final static checks follow separately.

## First Initialized Frame Cost Attribution

Status: implemented and qualified for Windows/Rust1.95 hardware DX12, starting from `3be7b0a`
after source-free audible delivery. Continue the qualified
[active gameplay baseline](../benchmarks/2026-11-14-development-runtime-costs.md#active-gameplay-workload-baseline)
through the same ordinary Travel consumer. Its first initialized frame contains approximately
0.6 seconds between original App-frame entry and post-Host observation. This broad span does not
identify Shader, device, source preparation or GPU execution as the cause.

Enable the existing original Desktop startup timings. Add opt-in fixed Runtime transaction wall
observations for namespace preparation, Surface acquisition, candidate preparation, program execution
and Surface presentation inside the original WGPU Host. Keep observations with original transaction
identity and clear them at each new transaction. Disabled instrumentation performs no new clock
reads; completed returned errors can retain reached operation spans, while unwinding and unreached
operations cannot invent completion. Observation never creates a submission/presentation receipt,
changes error handling, waits for extra work or grants device authority.

Extend the existing complete native consumer with first-presentation startup snapshots and every
original frame's phase/work facts. Check matching Runtime/App/render/window/backend/device identity,
complete five-phase observations for actual initialized successful presentations and finite nonnegative
accounting inside the original broad frame. Retain unmeasured residual explicitly; driver waits and
error-scope polling remain wall time. Keep all movement, pickups, markers, travel/restart, exact
completion, observer-panic cleanup and physical native retirement assertions. Three sequential runs
share process/driver/cache state and are not cold-start or independent hardware trials.

Pinned Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f` separates Main startup, per-device
RenderStartup, asset preparation and pipeline states. Its native renderer initialization still
blocks, and PipelineCache is not a deduplicating disk PSO cache. Pinned Godot
`7a37606a433c0e7735525037cdf774f043146a21` separates Display/Rendering/Load Game benchmarks
from actual RenderingDevice submission/presentation. Its current 2D pipeline path still waits
for compilation. Follow their ownership distinctions without assuming asynchronous names or
cache presence remove work. Relevant source paths are `crates/bevy_app/src/main_schedule.rs`,
`crates/bevy_render/src/{settings.rs,render_asset.rs,render_resource/pipeline_cache.rs}` and
Godot `main/main.cpp`, `core/os/os.cpp`,
`servers/rendering/{rendering_device.cpp,renderer_rd/renderer_canvas_render_rd.cpp}`.

Qualify the instrumentation through CPU and independent public consumers plus actual hardware DX12
Travel/panic-cleanup acceptance. Attribute the observed first-frame cost before choosing an
optimization; a moved cost must not be reported as a lower complete startup endpoint. Preserve
the original render/device/resource-prefix and Runtime owners. No generic profiler, second render
World, cross-Runtime cache, speculative prewarming, numeric production budget or complete
Foundation/R12 claim follows. Editor/UI/Dialogue product work remains paused.

The [executed attribution](../benchmarks/2026-11-14-development-runtime-costs.md#first-initialized-frame-attribution)
retains the complete report and selected-source/executable provenance. Owning CPU controls pass
19/19, the ordinary game library and metric controls 130/130, actual three-run Travel and
registered-observer panic cleanup 2/2, and the affected original native Host cohort 8/8. The new
observations preserve returned-error, no-clock-disabled, per-transaction reset and unwind semantics.
The sample checks exact phase sums inside matching frame envelopes, keeps actual spans without
an early App start, and preserves verified original cleanup when attribution itself refuses.

First initialized outer spans are 619.0608/599.9730/633.0067 ms. Namespace preparation accounts for
587.5639/571.3827/606.9740 ms, or 94.91%/95.23%/95.89% of those same frames. Program execution is
29.8027/27.5906/25.0220 ms. Whole entry-to-first-present endpoints are
734.7560/626.6711/675.7288 ms. These observations locate the next investigation in the original
device/namespace operations; they do not isolate a single Adapter/driver call or establish a
speedup. Later Surface-acquisition medians near 6.1 ms retain native pacing and do not imply
gameplay CPU/GPU cost. Default clocks remain disabled for ordinary/shipping consumers.

Minimal no-default WGPU library compilation, all-target/all-feature owning correctness Clippy,
consumer executable/test correctness Clippy and root/sample formatting pass. Existing unrelated
non-correctness warnings remain. Source/CPU/consumer review and document/provenance closure stay
with this unit; no remote CI or other-platform execution is claimed. The next original-owner
cost investigation must retain the complete entry endpoint and all correct-result/cleanup facts.

Owner code is committed at `46f798c`; complete consumer/evidence at `e88ec46`. Both independent
reviews close with no remaining P1/P2 after the two consumer corrections. All five scoped agents
are closed. Added document links/fragments, source/executable/report hashes and whitespace checks
pass. Local commits retain the paused UI/Dialogue work; no remote publication or whole-Goal
completion follows.

## Original Namespace Acquisition Costs

Status: implemented and qualified for Windows/Rust1.95 hardware DX12, starting from `4b4ed44`.
The preceding complete workload locates approximately
95% of the first initialized frame in the original namespace preparation call. Resolve its
actual Instance/Surface/Adapter/Device/configuration costs in that same complete consumer before
selecting an optimization. Earlier temporary Adapter probes are investigation leads, not evidence
for these new frames.

Extend the existing opt-in transaction observation with five fixed nested native-operation spans.
Borrow their temporary value through the original private Host/backend/device calls; keep GPU,
device epochs, admission, loss, Surface and submission custody unchanged. Record returned calls,
including errors, before propagating the result. A Ready device must retain missing Adapter/Device
request spans; a returned ensure/configure no-op can be measured. Clear nested data with every
original transaction and publish it only when the containing namespace call returns. Disabled
instrumentation makes no clock reads, allocations, logging, native requests or extra polls.

The consumer retains all complete Travel and panic-cleanup controls, exact frame and completion
joins, startup observations and entry-to-first-presentation endpoint. Nested operations are disjoint
inside the parent namespace span; their sum cannot exceed that same observed parent, and missing
calls remain distinct from zero. They are not additive with the parent. Report reuse and unmeasured
namespace remainder without equating either to CPU usage, GPU execution or scanout. Keep selected
source/binary/report provenance and actual default-off native controls.

Inspect the locked official wgpu `cddb2f5d80dc0009b2ef6767635140cacf90a007` selection path before
changing Nara policy. Its Core request enumerates candidates and checks fallback/surface eligibility
before power-preference sorting; DX12 HAL creates capability-probe devices while exposing adapters.
Switching power preference alone does not omit that work. Reject a workaround that bypasses real
Surface/capability/fallback checks, rewrites official HAL selection or introduces a cross-Runtime
GPU/cache owner merely to improve a local number. A justified optimization must improve a complete
correct-result endpoint with the original native ownership and failure/cleanup facts. Full
Foundation/R12, representative workload budgets, foreign execution and paused UI remain open.

The [executed namespace results](../benchmarks/2026-11-14-development-runtime-costs.md#original-namespace-acquisition-results)
retain 479 initialized frames across the same three full Travel trajectories. Adapter requests
take 502.3248/451.8774/450.7521 ms; Device requests 52.5445/62.0042/60.6964 ms; Instance preparation
40.9853/39.5566/37.3908 ms. Surface creation is 0.0279/0.0149/0.0144 ms and configuration
4.9608/3.7883/3.7944 ms. Checked nested residuals remain explicit. Each run has one exact request
pair in its first original namespace and none in later Ready frames through five Scene-instance
changes. Independent native owner controls preserve original acquisition, reuse, loss/recovery,
features and late predecessor isolation. These spans do not identify a particular native driver
or SetupAPI call, and batch-to-batch ranges are not an optimization comparison.

Owning CPU controls pass 27/27, independent game library/metric controls 132/132, full native
Travel/observer-panic cleanup 2/2 and native Device/Host controls 11/11. Minimal no-default WGPU
compilation, owning all-target/all-feature and consumer correctness Clippy, root/sample formatting
and exact source/executable/report provenance pass; existing warnings remain. Independent review
closes a private-call migration omission and strengthens fresh-versus-reused per-frame attribution.
Both reviews close without remaining P1/P2; all four scoped agents are closed.

Retain the official acquisition path. Explicit DX12 and absent validation/GBV overrides already
apply in this measured release workflow. Power preference, eager manual enumeration and added
cross-Runtime caching do not provide an evidence-supported fix under the current contract.
More detailed native call-stack sampling can remain an upstream/platform investigation. The
runtime delivery continuation returns to larger named gameplay/resource workloads and budget
attribution; it does not keep expanding a timing framework or claim full Foundation completion.

The original-owner implementation is committed at `4a2584d`; the complete consumer and raw
evidence at `e29e51c`. Source, executable/report hashes, added document links/fragments and
whitespace checks pass. All changes remain local, with paused UI/Dialogue work preserved.

## Authored Tile Workloads Through Native Travel

Status: implemented and qualified on Windows/Rust1.95 hardware DX12, starting from `51486f2`
after the bounded namespace investigation. The original-owner correction is `2b98a0c` and the
complete consumer/evidence is `9f0e5dc`. Extend the
ordinary complete game consumer with explicit 64/4,096/16,384-cell source-authored Tilemap
placements, three original native Travel lifetimes per population. These are named test workloads,
not supported-world capacity claims. Keep the same source game, native pacing, semantic script,
two levels, six Scene members, markers, pickups, restart, five instance changes and complete
Runtime/native retirement. Reuse the original public source/Prefab/TileSet/Image/stock/Host owners.

Construct bounded private source copies; leave the checked-out game content unchanged. Every
population covers the same 256-by-256 world footprint. The 16,384-cell representation exceeds
one source file's original wire-node bound, so use an existing root-bound Prefab for half of the
map instead of increasing limits or removing cell color data. Attach maps to existing checkpoint
and portal identities, preserving all game roles and final membership. TileSets use original
16-by-16 Images with ordinary source RON/metadata/dependency admission. Validate authored files
with original default limits. Report actual source bytes/digests and source-layout differences;
do not treat different Prefab preparation as a pure cell-count scaling experiment.

At exact original post-Host observations, retain only scalar content/instance/extraction/queue/
successful-drawing and logical GPU resource facts. Keep source-generation, App/render/window/
device receipt and completion joins. Distinguish authored cells, extracted cells, visible queued
quads and successful draws. Root content-budget internals remain with their original owner;
process points and tracked GPU payloads are not total memory or a leak proof. Record transition
curves without claiming a resource plateau from unquiesced samples or evicting caches for a test.

Retain a bounded original sealed capture from each level and the final returned level, accepting
each only through its matching real presentation. After original World/source retirement, replay
those exact inputs through an independent public Host with equal-sized target adaptation and
independent known-source ground/background pixel witnesses. Retain/reread earlier outputs after
later publication and producer retirement, then require original-prefix and Host cleanup. Pixels
are captured-input replay evidence, not screen capture/scanout. Preserve failure/panic custody.

Supplement externally visible counts with a focused original content-owner regression: large
raw/compiled Scene values survive while an escaped source needs them; root-bound Prefab inputs
are retained by their original snapshot and need not survive flat compilation's last snapshot.
Keep the captured generation's original conservative budget until its final legal source/pixel
owner drops, distinguishing allocation reachability from the shared reservation lifetime.
Use actual Weak references and original budget, not a new public memory dashboard. Existing
cancel/failed-travel/retained-image controls remain independent safety evidence.

Qualify pure source/accounting/oracle controls, complete native population workflows and observer
failure cleanup serially. Report finite per-run distributions, transition categories and exact
source/toolchain/hardware conditions. No preselected numeric speed target or universal tail/VRAM
budget is introduced; choose subsequent owner-local changes only from this complete workload.
Full Foundation/R9/R12 and foreign execution remain open; Editor/UI/Dialogue work stays paused.

The first independent sample CPU cohort passes 138/138 without skips at
`12e7f345-c6b4-48c6-9f7a-ca0721fb2f9d`, including source preservation, all three populations
under original default file limits, cleanup refusal controls and independent pixel-oracle
negative controls. Two read-only reviews close without remaining P1/P2. The failure wrapper now
requires copied-source cleanup after observer panic as well as original native retirement;
positive wrappers inspect actual replay pixels, retained-output rereads and final Host accounting.
Native gameplay, GPU replay and the original content-budget regression remain unqualified until
their execution below; source generation and compilation alone do not close this unit.

The first original content-owner run is RED, 1/2 at
`cfc40141-7874-47cb-bc48-99bf99921d87`: the existing escaped-source control passes, while the
new two-map source exceeds the original compilation working allowance (633,004,514 modeled
bytes versus 536,869,785 available). This is an allocation-model diagnostic, not measured RSS.
The staged reference planner pre-reserves the complete value model multiplied by configured
maximum depth even when no EntityRef leaves exist. Replace only its record/default/replan
reservations with Reflect's existing admitted planner; charge actual traversal and allocation
deltas against the original cumulative ledger, preserving Schema admission, structural/unmarked
reference validation, error classes and ceilings. Property-claim reservations retain their
separate unchanged caller contract. Add a large explicit-list regression with and without one
real local reference and retain exact/one-short quota controls.

The first native population run is also RED at `ef877cbe-34e3-4de0-8d74-61935f9a9fbe`, before
qualifying gameplay/pixels: the fixture omitted an explicit nearest sampler while its independent
texel oracle requires one. Author `NEAREST_CLAMP` in the copied TileSet and assert its decoded
source value; do not relax the oracle into accepting an unrelated filtering result. That failure
still reports original native retirement and copied-source removal.

The admitted-planner correction also charges each later plan visit through Reflect's original
path-access model; planning does not prepay later eager prefix copies. Persistent references
receive the same visit admission even though they create no source-origin entry. Long-key
controls preserve default work-byte refusal, then isolate their original copy/working guard
using test-local options. Production defaults and the actual Tile workloads remain unchanged.
The first full corrected Reflect/Scene cohort passes 976/976 at
`f5beb3b0-f9b4-4fd6-8e0a-f31bbfd49f8f` in the debug test profile. Earlier quota-oracle failures
retain their stricter exact/one-short acceptance after accounting for expansion as well as sealing.

The complete content-owner cohort then exposes a separate receiving-limit mismatch: dependency
collection uses the generic 16,384-node reference default after admitting a much larger complete
component. Move the existing value-to-reference-limit conversion from Scene into Reflect's
`ComponentValueMaterializationLimits` and reuse it in Scene and root content collection. The
root retains its original receiving value policy and actual allocation callback; no new parser,
inventory cache or larger file/Host-memory setting is introduced. A 32,768-item no-asset control
must still scan the complete value and preserve the caller's exact refusal. The first combined
run passes 1,048/1,049; its remaining fixture assertion incorrectly expects a disk Schema bundle
in a Native-provider closure. Remove that unconsumed fixture artifact and keep exact membership
for the seven actual source files. Final large-source custody and native qualification remain
required below.

### Authored Tile Qualification And Continuation

The final [measured report](../benchmarks/2026-11-14-development-runtime-costs.md#authored-tile-workloads-through-native-travel)
retains exact source/executable/report hashes and all four raw positive reports. Final shared
Reflect/Scene/content/residency controls pass 1,049/1,049, with 867 cases outside the selected
filter, at `ad17f549-d801-443e-97f1-d2bbd76a28d5` in the debug test profile. The game library,
source, oracle and metric controls pass 138/138 release at
`283fb232-f419-431f-997d-535bb0ab08c5`. The complete native population/baseline and two panic-cleanup
controls pass 4/4 release hardware DX12 at `6f494450-d654-4ff7-b087-39f1ea8ee691`.

Nine Tilemap lifetimes retain 1,551 initialized presentations, complete all original gameplay
facts and physically retire the original Runtime/window. All five predecessor Scene receipts
stop resolving; six source views retire; copied sources are explicitly removed. Twenty-seven
selected original inputs then pass independent known-source GPU pixel checks and exact retained
image rereads after later publication/producer retirement. Original submission prefixes complete
and final Host resource/candidate counters and cleanup faults are zero. Three independent baseline
lifetimes retain 257 initialized presentations and all earlier assertions. These are captured-input
image checks, not swapchain capture, complete rasterization, GPU duration or scanout.

Shared all-target/all-feature, root-library and consumer correctness Clippy, root/sample formatting
and document/provenance checks pass; warnings remain. Independent rereview closes the missing
plan-visit debit and confirms the shared conversion and root receiving-policy seam. All six scoped
agents are closed. No Cargo commands overlap, no cache is cleaned and no remote publication or
remote CI execution occurs. Paused UI/Dialogue changes remain excluded.

The final stable-frame wall medians remain near 16.5 ms and include large Surface-acquisition
waits; they are not rendering CPU or a production tail target. Scene-transition observations and
complete intervals grow with the larger source workloads, with the 16K Prefab layout as an
additional variable. Continue with owner-local attribution of Scene preparation/replacement and
extraction under this same complete consumer before selecting reuse or another optimization.
Keep source-copy generation outside the runtime entry endpoint, and retain default admission,
source freshness, original results and physical cleanup. Numeric budgets, foreign execution and
full Foundation/R9/R12 remain open.

### Existing Scene Span Attribution

The original `a58c630` cross-thread claim is rejected: its thread-local subscriber omitted IO
worker spans. The following 109-entry/range observations are historical partial coverage. Passing
gameplay and named-span checks did not prove worker work. Correct the binary-local collector
before ranking costs: install its optional subscriber once before Runtime construction, bind
each new span to the original run, and finish after physical retirement but before independent
replay. Require actual worker expansion, six owner preparations/publications, and no unfinished or
retained spans. A real spawned-thread and consecutive-run control must catch the omission and
reject reentrant/unfinished observations. No production Task/Scene lifecycle owner changes follow.

The initial opt-in `scene-work-observation` implementation consumes existing `nara::scene::work`
spans without changing the default runtime. Its feature-qualified metric controls pass 138/138,
and the native population/baseline/panic cohort passes 4/4. These results qualify the original
gameplay and cleanup checks, not complete observation. Each historical run observes 109 entries.
Across 64/4K/16K cells, the partial nested wall sums range over 8.883–9.439 ms, 96.911–126.164 ms
and 816.388–1,052.739 ms. The 16K partial observations include compiled-component preflight
(`scene_decode`) at 233.047–322.339 ms, expansion at 200.378–241.257 ms, flat admission at
33.138–42.060 ms and unbound preparation at 201.786–278.200 ms. Source I/O and JSON/RON parsing
are not individually timed by these phases.
The [raw feature-qualified reports](../benchmarks/2026-11-14-development-runtime-costs.md#existing-scene-work-span-attribution)
retain exact source, executable and report metadata.

These historical spans omit worker threads and are not additive CPU time or a total allocation
model. The Sprite extraction/queue path also remains outside this span set.
The 16K root-bound Prefab representation remains a source-layout confound. Keep complete source
decode, Prefab/Scene admission, authorization, source order, multi-View diagnostics, cleared-dirty
refresh and captured-input lifetimes. The next candidate is a per-extraction TileSet context or
UV reuse, or an existing-owner extraction span, followed by the complete native consumer. Do not
introduce a cross-frame cache or dirty-only skip before that evidence exists.

### Cross-Thread Observation And Per-Map Extraction Context

Correct the measurement before using it to select Scene optimizations. Install one optional
subscriber in the explicit measurement binary, attach every span to its original run at creation,
and distinguish owner/worker entries. Seal only after physical Runtime retirement and before
independent GPU replay. Reject live, reentrant or otherwise unfinished spans; a late old span must
not charge the next run. Preserve observation and cleanup facts on every failure path. Nested
wall sums overlap and are not end-to-end preparation or CPU durations.

Within the original full Tilemap extraction, introduce a private per-map context for the current
authorized image/material, transformed tile axes and the most recent index/UV result. Retain the
complete cell scan, per-cell diagnostics, colors, positions, ordering and Y-sort. Invalid repeated
indices still count per cell; revisiting an earlier index recomputes it. The context ends with
that map's extraction: no cross-frame cache, dirty-only skip or new resource authority follows.

Require complete Sprite tests, an independent all-field multi-map/next-frame oracle, actual
cross-thread and consecutive-run controls, and the unchanged complete native gameplay/pixel/
retirement cohort. Default feature-off execution must retain disabled observation. Rebind new
raw reports to their actual source and executable; keep the old partial attempt explicitly
rejected. No elapsed-time improvement is claimed without a controlled extraction measurement.

The final observed-source cohort passes 139/139 release game/metric/thread controls at
`bc6bdf20-73d9-47ac-a51b-81ad35982046`; complete Sprite controls pass 56/56 at
`2dd0e831-f328-4e9f-9935-3312e0d8c7fa`. The full native population/baseline and two panic-cleanup
controls pass 4/4 hardware DX12 at `67547b48-173f-4906-a027-82289f09380e`. The
[corrected report and exact metadata](../benchmarks/2026-11-14-development-runtime-costs.md#corrected-original-worker-coverage-and-per-map-extraction)
retain all four positive raw reports. Every complete run records 149 original entries: expansion
and flat admission each include eight worker calls; preparation and publication each include six
owner calls and no worker calls. Physical retirement leaves zero live/unfinished spans. A real
worker barrier proves finish refusal while active and prevents late old exits charging a new run.

The nine Tile lifetimes retain 1,655 initialized presentations, complete original gameplay and
source-retirement facts, and pass 27 original-input GPU replays with exact retained rereads and
Host/prefix cleanup. Three baseline lifetimes retain 261 initialized presentations. Both panic
controls retain complete observation and native cleanup, with copied-source removal for Tilemaps.
Independent reviews close the omitted worker coverage, incomplete failure report and decode
semantics; the extraction review reports no remaining P1/P2. The first corrected native cohort
was RED 1/4 before the evidence path and diagnostic/report fixes; the report retains its exact
limits rather than treating a later pass as a causal explanation for an earlier native stop.

The complete native cohort without `scene-work-observation` passes 4/4 at
`57d61c33-2a4f-4781-a547-fde00c9599f9`; both failure controls retain explicitly disabled Scene
records, and all original gameplay/pixel/physical-cleanup assertions remain. Separate instrumented
and uninstrumented sequential runs do not establish a controlled speedup.

The 16K lifetime's nested wall ranges are expansion 1,844.166–2,038.457 ms, compiled-component
preflight 558.751–660.309 ms, flat admission 370.201–504.269 ms and owner unbound preparation
203.776–303.773 ms. They include all repeated attempts, overlap and do not measure CPU or one
transition. Source I/O and JSON/RON parsing remain separately unobserved, and Prefab halves
remain a representation confound. Continue with existing-owner Scene/source-work attribution
before broader reuse or caching. The per-map context has behavioral/native acceptance, not an
elapsed-time improvement claim. Numeric runtime budgets, foreign execution and full Foundation/
R9/R12 remain open; paused Editor/UI/Dialogue work remains excluded.

The observation correction is `a91e68c`, and per-map extraction is `f25312c`. Scoped release
correctness Clippy, root/sample formatting, workflow YAML, added document links and exact selected
source/report hashes pass; warnings remain. All four scoped agents are closed. No Cargo commands
overlap, no cache is cleaned and no remote publication or remote CI execution occurs.

### Source Decode And Expansion Attribution Successor

Continue the same complete ordinary Travel consumer with default admission and original native/
GPU/source retirement. Attribute the existing content read envelope, source Scene and Prefab
decoding separately from compiled-component preflight. The read envelope includes capability
opening, bounded byte reads, digesting and receiving-budget bookkeeping; it is not pure disk I/O.
Source decoding includes the existing JSON/RON wire checks, not only a parser call. Extend existing
opt-in span collection rather than introducing a timing service or a new source/cache owner.

Separate existing staged-compiler structure, program composition, component materialization and
reference closure only where those original boundaries explain the measured expansion envelope.
Keep full errors, cumulative work/retained limits, source freshness, Schema and authorization
checks. Obtain actual worker/owner counts from the complete game rather than assuming one source
preparation per committed Scene instance. Nested measurements overlap and include instrumentation.

Use the measured source and compiler split to select one concrete owner-local reduction. A
candidate must preserve complete source interpretation and current source observations, pass its
original boundary tests, and retain the complete native gameplay/pixel/failure cleanup acceptance.
Do not turn an immutable source snapshot into permission to reuse an unvalidated later source.
Record source/toolchain/executable/report provenance and distinguish intermediate observation
from any subsequently qualified optimization. Numeric budgets and full Foundation/R9/R12 remain
open; Editor/UI/Dialogue work stays paused.

Source review explains the existing nine-versus-six counts without redundant Scene publication.
The four scripted Travel operations each prepare another checkpoint level to validate its saved
pickups/markers, acknowledge cancellation, then prepare and commit the destination. Expansion
is one startup owner call plus eight worker preparations. Publication is startup plus four Travel
commits plus one Retry; Retry uses the resident compiled source and does not expand again.
Retain these checkpoint validation preparations and original cancellation custody. Preparation
count alone is not evidence that those checks can be skipped or cached across source changes.

The intermediate 16K CLI consumer passes all three original Travel lifetimes, physical retirement
and nine retained-input GPU replays. The selected source and binary are frozen before value-cost
reuse; the [raw report](../benchmarks/data/2026-11-14-development-runtime/authored-tiles-source-before-16384.json)
and [exact source/binary metadata](../benchmarks/data/2026-11-14-development-runtime/authored-tiles-source-before-metadata.json)
retain that intermediate scope. Its old raw scope sentence is corrected by the metadata and
subsequent output text, without rewriting measured data. Source Scene candidate decoding ranges
over 1,181.626–1,206.328 ms and Prefab candidate
decoding over 1,148.630–1,160.499 ms per complete lifetime; content-read envelopes are
75.700–82.034 ms. Expansion is 1,725.355–1,778.274 ms, including structure at
1,188.633–1,201.951 ms and component materialization at 506.802–542.232 ms. Program composition
and reference closure are below 0.1 ms each. These nested ranges overlap and are not CPU time.

This selects reuse of already validated value-cost facts inside the existing compiler candidate.
Repeated full-tree cost scans are source-confirmed in materialization/reference/output handling;
they cannot explain or fix the larger nested Prefab source-decoding envelope. Preserve migration,
default materialization, Properties mutation and recomposition invalidation; retain full reference
planning/visit admission and charge added metadata to the original working allowance. Final
qualification still requires the original boundary tests and complete native wrapper cohort.

### Validated Value Cost Reuse

The selected implementation retains `ComponentValueCost` in the existing private reference
program. Component validation returns the cost it already measured; normalized values acquire
that fact after migration and default expansion. Opaque records retain the original admitted raw
cost. Each contextual default owns a separate cost; hidden defaults do not enter effective-value
output totals. Every Properties layer replaces value and cost together from its final validated
candidate, including Absent default restoration, and invalidates the previous reference plan.
Reference closure does not mutate the value. Output uses the same cost plus the existing exact
old/new reference replacement delta, then stops using that pre-rewrite fact.

Recomposition drops both the materialized document and reference programs, so no cost survives
into the next attempt or another compilation. Complete source-depth checks, Schema/version/owner
validation, reference planning and every original plan-visit admission remain. The added inline
cost metadata is charged at the main component-program scratch entry; contextual-default entries
already admit `size_of::<ReferenceProgram>()`. No public record, cache, compiler owner or ceiling
changes. Newly increased working metadata is included in exact current allowance checks rather
than promising identical old boundary bytes.

Original content/residency and public content-limit controls pass 81/81 at
`5f7edfe7-3856-4320-bf0e-0ddcfc230414`, with 867 cases outside the selected filter; full
Scene/Reflect all-feature library controls pass 981/981 at
`919378ba-391d-4097-8fad-507cba2ace0a`. Four added tests independently check effective value
nodes/bytes after layered growth, shrink, Absent, migration and default expansion, plus opaque raw
value and hidden-default admission. Existing exact/reference/depth/failure controls remain.
Independent correctness and admission reviews report no P1/P2. Observed/exact/one-short tests
alone do not prove that each modeled metadata debit is present; that debit and unchanged complete
reference visits are additionally source-reviewed.

The value-bearing public conversion cohort passes 5/5 at
`9ca1d716-a883-4906-8d98-89ffbb30aba3`. The new case removes the unrelated watcher query and
requires the selected Link to load S/1, P/1 and P/2, witnessing two reference-driven candidate
recompositions. It checks exact one-component/two-node/fourteen-byte output, independent one-short
node/byte diagnostics, complete source/provenance, original data and prepare-time history, then
commit/Undo/Redo. Existing query-only and full-activation cycle controls remain. A first 5/5 run
is repeated after removing an irrefutable-pattern warning; no behavioral assertion is weakened.

The release game/library/metric/span/native cohort passes 143/143 at
`5b9cc68a-ac8f-4742-8865-cf9a622e65de`, without skips. This includes the original four native
wrappers: nine authored Tile lifetimes with 2,379 initialized presentations, three baseline
lifetimes with 257, both observer-panic cleanup controls and 27 original-input GPU replays.
Source decode/read positive guards require actual owner and worker entries; 16K also requires
worker Prefab candidate decoding. Every positive record has zero live/unfinished spans. Existing
gameplay, source freshness/retirement, copied-source removal, pixel rereads and original
Host/prefix/physical cleanup assertions remain. The [raw reports and exact metadata](../benchmarks/2026-11-14-development-runtime-costs.md#validated-value-cost-qualification)
bind the changed source/executable and compiler rather than reusing earlier results.

Four additional sequential processes run the frozen pre-reuse executable and new executable in
before/after/after/before order. All twelve original 16K lifetimes and 36 GPU replays pass. Across
six lifetimes per version, compile-output wall medians are 38.455 ms before and 0.145 ms after;
component materialization is 591.236/592.519 ms and complete expansion 1,891.559/1,969.560 ms.
Read/decode phases and whole-process walls vary too. The [paired raw evidence](../benchmarks/2026-11-14-development-runtime-costs.md#validated-value-cost-qualification)
supports the local output-stage reduction and preserved behavior, not a consistent end-to-end
speedup, CPU-duration or production-tail claim. Source parsing remains the larger separate target.

Remaining checks and evidence closure do not broaden this unit into a cache or parser rewrite.
Carry original limits, source observations and physical retirement into subsequent owner-local
source-decoding work. Full Foundation/R9/R12, numeric budgets and foreign execution remain open;
paused Editor/UI/Dialogue changes remain excluded.

Source attribution is committed at `a7c7a27`; validated compiler costs and their owner/public
regressions are `4aaa3d8`. No-default-feature Scene and feature-off workload target checks pass.
Root content, Scene all-target/all-feature and game workload/metric/span release correctness
Clippy, root/sample formatting and evidence checks pass. Existing warnings remain. All four
scoped agents are closed. Cargo/native workloads run serially against the original target,
without cache clean, remote CI execution or remote publication.

### Source Payload Decode Successor

Continue the original complete Travel consumer with separate wire-shape preflight, persistent
header decode, semantic wire payload decode and domain-budget/canonicalization spans. Preserve
byte/shape/header-contract/payload/admission order, complete JSON/RON grammar, exact receiving
limits and structured error stages. Measure before selecting a parser or serialization change.
The optional spans stay in the existing format owner and default builds acquire no clocks.

Investigate `ComponentValue`'s existing serde wire adapter: its untagged payload decoder buffers
even canonical type-before-value input and can repeatedly deserialize nested Map/List contents.
If actual payload cost justifies it, use serde's typed MapAccess/DeserializeSeed dispatch for a
known tag and retain the existing ordinary payload fallback when value precedes type. Preserve
arbitrary field order, JSON/RON and parent-buffered values, numeric/null/reference semantics,
strict duplicate/unknown fields and existing outer format limits. Do not build a JSON/RON parser,
change source syntax, cache later files or skip compatibility/shape checks.

Qualification requires shared Reflect/Scene and public format controls, adversarial field-order/
type/number/duplicate cases and complete native gameplay/pixel/failure cleanup. Freeze the same
instrumented pre-change executable/source before balanced old/new comparison; distinguish local
payload timing from full process, CPU and scanout. Full Foundation/R9/R12 and foreign execution
remain open; Editor/UI/Dialogue work stays paused.

The [original instrumented payload baseline](../benchmarks/data/2026-11-14-development-runtime/authored-tiles-payload-before-16384.json)
and [exact selected source/executable metadata](../benchmarks/data/2026-11-14-development-runtime/authored-tiles-payload-before-metadata.json)
retain three complete 16K Travel lifetimes, original physical retirement and nine GPU replays.
Each lifetime has eighteen Scene/Prefab format decodes. Nested payload-decode wall sums range
over 3,436.010–3,814.791 ms, wire preflight 592.815–746.551 ms, header decode 68.950–79.720 ms
and domain/canonical finish 74.173–82.646 ms. They are not CPU time or one transition delay.
This selects the existing component-value container decoder rather than weakening the mandatory
wire/header gates. Serde's untagged payload buffers complete values even after a known tag and
replays nested container contents; scalar semantics and value-before-type fallback remain.

Twenty-seven additive compatibility cases plus the existing public authored-reference wire
control pass 28/28 on the original decoder at `202c3fa8-0371-41fc-ac78-592d04a54114`, with 200
outside the filter. The first streaming attempt also passes those cases, but independent review
finds a missing direct RON child tuple/brace-map case. Previously buffered container children
accept Map/Seq forms that raw `deserialize_struct` does not. Add an explicit regression and use
the shared value visitor through a nested-value seed; retain the original top-level struct
entry. Raw RON struct keys also need serde's identifier/string handling. Full shared/public and
native qualification remain required; the first 28-case green result is incomplete evidence.

The corrected implementation uses an ordinary nested-value seed to accept the original container
child Map/Seq forms, while retaining the top-level struct entry. Wire field identifiers and
business Map keys preserve RON identifiers, quoted keys and previously buffered strings/UTF-8
bytes. Numbers, Null, EntityRef, unknown tags and value-before-type retain the original payload
classification. Serialization is unchanged. Malformed child values may report an earlier real
parser coordinate; typed error stage and syntax/data distinction remain, without synthetic offsets.

Full Reflect/Scene library controls pass 1,010/1,010 at
`80570866-d780-40ba-bdf8-45f79a405323`, including the direct child-form regression and a new
source-format diagnostic control. The public format/conversion cohort passes 36/36 at
`52a4c6d3-daa4-49f3-b434-4b1367a51093`. The two concrete child forms are also added to the
parent-buffered matrix, followed by a focused rerun. Full receiving-budget and native game
qualification remain in progress; no elapsed improvement is claimed from source inspection.

The strengthened wire matrix passes 29/29 at `898eb88b-c488-459c-8305-68d94f96459d`, with 200
outside its filter. Original content/residency and public receiving limits pass 81/81 at
`4383d866-1d2a-4b51-b555-f82b7203ec80`, with 867 outside the selected filter. Existing exact
quotas, failed-load refunds, original source generations and typed error stages remain.
Independent resource/error review reports no P1/P2 after the RON child-form correction. The
parser API retains actual locations; the new semantic diagnostic control checks stage/data/
location presence rather than freezing the previous buffering cursor or proving a particular
bad-child coordinate. No custom offsets or parser replay are introduced.

The final release game/library/metric/span/native cohort passes 143/143 at
`9e5dfac7-e5d6-43d6-9755-96c4819ca17f`, without skips. Nine Tile lifetimes retain 1,951 initialized
presentations and twenty-seven original-input GPU replays with exact retained-image rereads;
three baseline lifetimes retain 273. Both panic controls preserve original physical retirement,
complete Scene span closure and copied-source removal where applicable. The source decoder
envelopes each contribute exactly one call to each new format subphase, including actual worker
and owner execution. All original gameplay/source/Host/prefix assertions remain. The
[qualified reports and comparison](../benchmarks/2026-11-14-development-runtime-costs.md#streamed-container-qualification-and-comparison)
retain exact changed-source, executable and raw-report hashes.

The frozen original and new executables complete twelve additional 16K Travel lifetimes and
36 GPU replays in before/after/after/before order. Across six lifetimes per version, payload wall
medians are 3,463.926/1,197.454 ms and complete expansion 3,477.677/2,504.648 ms; wire/header
medians stay nearly unchanged. Complete process walls in order are 43.919/34.897/34.488/42.650
seconds. Both new groups are faster for this finite declared workload, while component
materialization is higher. Nested wall sums are not CPU, a single transition delay or scanout;
driver/cache/clock state and host load remain uncontrolled. These results do not establish a
statistical guarantee, production tail budget, physical-memory bound or equal gains on the
unchanged scalar/value-first paths. Full Foundation/R9/R12 and foreign execution remain open.

The source-format observations are `e40f2a5`; the streaming adapter, compatibility matrix and
semantic diagnostic control are `920cd9a`. Public Reflect catalog/bundle/authored/durable-reference
controls pass 20/20 at `958f73cd-de1a-42b2-978b-378031da424c`. Reflect/Scene feature-free and
feature-off workload targets compile. Owning all-target/all-feature and game workload/metric/span
release correctness Clippy pass; root/sample formatting and exact source/executable/report
provenance pass. Existing warnings remain. Parser-location tests establish real location
presence and typed stages, not a frozen semantic-error cursor or every malformed child's exact
coordinate. All four scoped agents are closed. Cargo and native workloads run serially against
the original target without cache clean, remote CI execution or remote publication.

Continue from these measurements through existing owners. Wire-shape checking, component
materialization and retained resource costs remain separate candidates; preserve complete input
validation and the same game/source/pixel/retirement workflow. Numeric budgets, larger named
game workloads and foreign platform execution remain required before broader runtime claims.

### Component Materialization And Bounded Cost Return

Continue the original complete Travel/source/pixel/retirement consumer. Split existing component
materialization into original-value admission, clone, migration with post-migration admission,
default materialization, semantic validation, reference planning and reference-plan validation.
The optional fixed spans stay at those existing owner boundaries and default builds add no clocks.
They are nested wall envelopes, not instruction-level CPU or allocation profiling.

Inspect the original Reflect raw materialization walker, which already counts complete nodes and
logical bytes while enforcing depth and source-reference policy. Its Scene caller currently
drops that cost and walks the same immutable value again. Return the checked complete cost from
that original owner and reuse it for the immediate bounded-cost result; preserve all checks,
error order and receiving limits, including authored references and separate hidden defaults.
Avoid a second traversal implementation, reusable cross-value witness or a source cache. Native
and admitted validation callers retain their original semantics and exact admission errors.

Measure the unchanged implementation with the same added phase hooks before selecting wider
reuse across migration/default mutation. Qualify the direct owning cost/error boundary and all
affected Scene/Reflect, public receiving-limit and complete native game controls. Freeze an
instrumented original executable for before/after/after/before observation. Full Foundation/
R9/R12, runtime budgets and foreign execution remain open; Editor/UI/Dialogue stays paused.

The [instrumented original materialization report](../benchmarks/data/2026-11-14-development-runtime/authored-tiles-value-before-16384.json)
and [selected source/executable metadata](../benchmarks/data/2026-11-14-development-runtime/authored-tiles-value-before-metadata.json)
retain three complete original 16K Travel lifetimes, physical retirement and nine GPU replays.
Each lifetime records 180 original component operations in each of the seven added subphases.
Component materialization wall sums are 1,086.821–1,394.656 ms; admission is
156.483–201.889 ms, clone 290.633–341.635 ms, migration including post-admission
160.732–203.455 ms, defaults 131.073–188.435 ms, semantic validation 57.301–80.043 ms and
reference planning 288.431–381.335 ms. Reference-program validation is 0.019–0.023 ms.
These are nested lifetime wall observations, not CPU time or a single transition delay.

The selected cost-return implementation uses the same original raw walker, complete limits and
Source policy, returning its counters only after success. Native and admitted wrappers preserve
their unit return and exact refusal identity. The existing owned default materializer returns a
checked cost only when insertion already performed a preflight; no-insertion returns no cost
and preserves the public owned operation's no-scan move contract. Scene carries the admitted raw
cost, replaces it after actual migration, then selects inserted-default cost or the unchanged
cost. Full semantic validation and reference planning/visits remain. Properties Set/Absent still
refreshes cost from its final validated candidate. Opaque values retain admitted raw cost.

Scene and Material production libraries compile with all features. Focused cost/default/reference/
property/quota controls pass 33/33 at `94ef707d-93d7-4849-8462-6236b9ccb177`, with 982 outside the
filter. Full Reflect/Scene/Material library controls pass 1,094/1,094 without skips at
`591b3a6f-6a3d-4bde-b991-c23012233d6c`; public Reflect defaults/encoding/property-reference controls
pass 26/26 at `02863b1e-1122-4ff6-a19b-cc0167171cff`, and public Scene format/reference conversion
passes 36/36 at `df30cdc4-db4a-4c4d-bac6-ab8a03f8215a`. Five additive Reflect controls compare
the returned result with independently computed complete costs, exercise authored routes and
node/byte/depth refusal order, missing nested defaults and selected overrides, and preserve
17K-value/scalar allocations on the no-insertion path. No new cost cache, owned witness or
larger budget is introduced. This implementation removes redundant cost walks but does not
claim to eliminate mandatory value clones or full semantic/reference validation.

Independent correctness review closes without P1/P2 after checking the full reference visit and
output-cost delta chain, migrated/default/Properties costs, original quota/error controls and all
direct hidden callers. Material's callers discard the successful cost; Reflect property preserves
unit success; Scene Properties discards the intermediate default cost and revalidates the final
changed candidate. The public conversion run above checks value-bearing reference-triggered
recomposition, exact effective quota refusal, source/history preservation and commit/Undo/Redo.
Original receiving-limit and full native game/pixel/failure qualification remain in progress.

The original root content/residency and public receiving-limit cohort passes 81/81 at
`c089c7f4-6b7e-4dad-8527-148b4cfdadf0`, with 867 outside the filter. The complete release game/
library/metric/span/native cohort passes 143/143 at `761c1fcb-c2f7-4c0c-911e-3ded640f3c58`, with
no skips. Nine Tile lifetimes retain 845 initialized presentations and 27 original-input GPU
replays; three baseline lifetimes retain 193. All original gameplay/source/retained-pixel/Host/
prefix/physical-cleanup assertions remain, including both panic controls. Seven materialization
phase call counts agree for the actual ordinary operations, include owner and worker execution,
and leave zero live/unfinished spans at sealing. The
[qualified reports and comparison](../benchmarks/2026-11-14-development-runtime-costs.md#bounded-cost-return-qualification-and-comparison)
retain exact source/executable/report hashes and actual native scope.

Four frozen-old/new processes in before/after/after/before order complete twelve additional 16K
lifetimes and 36 GPU replays. Six-lifetime wall medians are cost/clone admission
82.823/46.477 ms, migration with post-admission 81.628/47.319 ms, defaults 52.881/45.535 ms,
semantic validation 19.397/1.306 ms and complete component materialization 514.843/460.009 ms.
Clone and reference planning are higher; complete expansion is 1,092.808/1,045.651 ms. Process
walls are 20.277/20.809/20.414/20.199 seconds: both changed groups are slower, so no end-to-end
speedup is demonstrated. Keep local reductions distinct from whole gameplay/pacing/replay cost.
Nested wall spans, uncontrolled host/cache/clock state and finite correlated samples do not
qualify CPU/GPU duration, statistical confidence, runtime tail or physical-memory budgets.

Implementation and owning regressions are `e9c6f5d`. Reflect's feature-free cost/reference
cohort passes 17/17 at `766afa03-a7e1-4a2d-9e4b-3e59ac5f6848`, with 178 outside the filter.
Reflect/Scene/Material feature-free libraries and the observation-off workload compile. Owning
all-target/all-feature and workload/metric/span release correctness Clippy, root/sample
formatting and exact source/executable/report checks pass; existing warnings remain. Independent
review and its hidden-caller/reference/accounting follow-up close without P1/P2. Both scoped
agents are closed. Cargo/native workloads remain serial against the original target, without
cache clean, remote CI execution or remote publication. Original paused UI/Dialogue changes stay
excluded. Continue runtime work through complete consumers; larger named workloads, numeric
budgets, foreign execution and full Foundation/R9/R12 remain open.

### Reference Planning Without Duplicate Payload Scans

Status: implemented at `d5acd0d` and qualified within the finite Windows scope below. The same
complete authored Tile game continues from `17e0d4f`; its qualified executable was frozen before
editing, with an original 16K run retaining complete gameplay, physical retirement and independent
captured-input GPU replay. The preceding bounded-cost-return comparison does not establish
end-to-end improvement; do not infer a whole-game gain from fewer walks.

Reflect's reference collector first scans the complete raw payload under the receiving limits,
then interprets Schema fields, then scans the raw payload again to detect undeclared references
and enforce combined result/payload limits. Return that first scan's existing complete node/byte
counters privately. After all Schema interpretation succeeds, omit the second scan only when the
first found no EntityRef and its checked counts plus actual reference-result counts fit the same
receiving limits. Complete counts are necessary: ordinary Tilemap data includes a non-null TileSet
AssetRef, so a zero-result-only condition would miss the actual large component. If an EntityRef
exists or either combined count exceeds its limit or overflows, keep the original second scan and
its refusal order. Complete initial structural admission and every existing Schema/path/capability/
shape check remain, including selected field/value and AssetRef callers.

Account for actual traversal and scratch growth through the original callback. Removing physical
work may reduce cumulative work charges; it must not raise a configured ceiling, suppress a real
visit, change reference-bearing errors, or reuse a result across different values. Keep the first
undeclared path, node/byte/depth refusal, admitted versus unadmitted Schema distinction, original
reference visits/rewrite and failure identity. No cache, public witness or new traversal owner is
needed. Qualify those receiving boundaries and public consumers, then run the original complete
native game and frozen-old/new comparison before claiming a benefit. Editor/UI/Dialogue stays
paused, and full Foundation/R9/R12, numeric budgets and foreign execution remain open.

The implementation retains the complete first raw walk's node/byte counters only inside the
current collector call. Checked addition proves the final combined limits before the redundant
walk is omitted. Over-limit and overflowing results run the original final walker rather than
substituting a different refusal. Eight additive tests exercise complete one-pass work debit,
exact/one-short callback admission, implicit opaque and explicit Null payloads, three collector
seams, raw/Schema error precedence, late undeclared references, public rewriting and AssetRef
combined limits. A non-null TileSet plus 4,097 scalar cells charges exactly 4,096 more raw nodes
than one cell through both entity planning and asset collection; no payload bytes or mandatory
Schema interpretation disappear. The prior zero-output-only candidate was broadened before
qualification because it would miss ordinary asset-backed Tilemaps.

Complete all-feature Reflect/Scene/Material libraries pass 1,102/1,102 without skips at
`fff0419b-6cc6-4c50-841c-fa322d5fe295`. Public Reflect controls pass 26/26 at
`c0f42bb6-f85f-4a66-8a2d-46d5dfcdc8e4`, and public Scene format/selected-reference conversion
passes 36/36 at `f03b2736-e0b9-4037-8f1c-95e16c4fae49`. Feature-free new controls pass 8/8 at
`626ec272-e62c-43be-aef4-5feac70e2455`, with 195 outside the filter. Independent review of the
final combined-count condition and all eight tests finds no P1/P2.

The separate ownership study finds one complete source-record clone, required by the current
immutable source and independently mutable candidate. Migration and defaults already consume the
owned value rather than cloning its existing tree again. Do not erase that clone by sharing mutable
source state. A later candidate can inspect the post-migration cost refresh when the original owned
migration contract guarantees an unchanged current-version value; real migrations and hidden
defaults retain their complete receiving checks and accounting. Keep that change separate from
this reference-scan comparison.

The unchanged root content/residency and public receiving-limit cohort passes 81/81 at
`2f4f749f-9dbb-42e0-85a8-2a8e1d09affc`, with 1,547 cases outside the selected filter; all 81
test names match the preceding qualification. The complete release game/library/metric/span/
native cohort passes 143/143 without skips at `8cfca769-639e-4c18-ac9c-8f937602b92f`. Nine
64/4K/16K Tile lifetimes retain 771 initialized presentations and 27 original-input GPU replays;
three baseline lifetimes retain 195. Complete gameplay, original receipts, copied-source removal,
retained-image rereads, original Host/submission-prefix retirement and both observer-panic cleanup
controls remain. The qualified executable hash matches the frozen paired-after executable.

The [balanced comparison and exact evidence](../benchmarks/2026-11-14-development-runtime-costs.md#reference-planning-without-duplicate-payload-scans)
retain twelve additional 16K lifetimes and 36 GPU replays. Six-lifetime cumulative wall medians
are reference planning 105.020/49.845 ms, component materialization 420.116/361.724 ms and
expansion 996.156/945.196 ms before/after. Whole-process walls are
19.190/18.975/18.742/19.728 seconds in before/after/after/before order. Both changed groups are
faster in this observation; uncontrolled host/driver/cache/clock state and correlated samples
do not establish a general speedup, CPU/GPU duration, physical-memory or production-tail budgets.
Nested spans overlap and are not one transition delay. Full Foundation/R9/R12 and foreign
execution remain open; Editor/UI/Dialogue work stays paused.

Owning all-target/all-feature correctness Clippy, feature-free Reflect/Scene/Material library
checks, root/sample formatting and selected source/executable/report checks pass; existing
warnings remain. Four scoped research/implementation/review agents are closed. One intermediate
release build was stopped before execution when the zero-result-only condition was superseded;
the final source then passed its complete release build and every recorded acceptance gate.
Cargo/native workloads remain serial against the original target, without cache cleaning,
remote CI execution or publication. Paused UI/Dialogue changes remain excluded. This completes
the named reference-scan unit, not the full Foundation/R9/R12 or its numeric runtime budgets.

### Current-Version Migration Cost Reuse

Status: implemented at `81a9d59` and qualified within the finite Windows scope below. The complete
authored Tile game continues from `9ed1b1f`; its qualified executable and exact source hashes were
frozen before editing. The native comparison retains the same compiler/profile and phase hooks
in before/after/after/before order. The original immutable source still needs one
complete candidate clone. This unit addresses only the pure cost refresh after an owned migration
returns its input version unchanged.

`SchemaAuthority::migrate_owned` has three concrete routes. Native and Mixed-native use Reflect's
owned migration, which moves a current-version value unchanged without invoking a migration
callback; actual migrations strictly advance the version. Mixed-data and Authoring accept only
the current version and move the same value. Record that internal guarantee at the owner, then
reuse the already bounded raw cost only for unchanged-version success. Actual migration output
still receives its complete depth/node/byte admission before defaults. Preserve source syntax
policy, hidden contextual defaults, owner/Schema checks, source-upgrade reporting, default insertion,
semantic validation, reference planning and final effective costs. No configured limit, callback
work charge, public interface, new cache or cross-value witness changes.

Extend the existing migration/default cost matrix with a large explicit current-version payload;
retain grow/shrink/default cases, exact and one-short final limits, owned-storage controls and all
affected Scene/public reference-conversion tests. Qualify the unchanged native population/baseline/
panic consumers, then compare frozen programs before reporting a benefit. Full Foundation/R9/R12,
numeric budgets and foreign execution remain open; Editor/UI/Dialogue work stays paused.

Implementation and the existing cost-matrix extension are `81a9d59`. Complete all-feature Scene
library controls plus Reflect's owned migration/storage control pass 783/783 at
`fef40ab3-18f1-412c-9c4a-c9928106a32d`, with 240 unchanged Reflect cases outside the filter.
Public format/effective-reference/hidden-default/selected-reference controls pass 45/45 at
`9760a272-3b8f-4d4b-ab5b-027effec2646`. Independent review confirms the concrete authority's
unchanged-value guarantee, pre-migration immutability, actual-migration admission and final
cost/reference deltas without P1/P2 or additional test gaps. The scoped reviewer is closed.
The complete release game/library/metric/span/native cohort passes 143/143 without skips at
`a2c95a6c-d55d-485f-b0c1-3480b59a2f5f`. Nine 64/4K/16K native Tile lifetimes retain 773 initialized
presentations and 27 original-input GPU replays; three baseline lifetimes retain 191. Both observer
panic controls and original gameplay, retirement of sources/receipts, copied-source removal,
rereads of retained images and Host/submission-prefix cleanup pass. The qualified executable hash matches the
frozen paired-after program.

The [balanced comparison and exact evidence](../benchmarks/2026-11-14-development-runtime-costs.md#current-version-migration-cost-reuse)
add twelve 16K lifetimes and 36 GPU replays. Cumulative migration wall median is 43.325/0.051 ms,
complete materialization 359.971/338.568 ms and expansion 945.356/937.516 ms before/after. Clone,
defaults and reference planning are higher, without established causes. Whole-process walls are
18.620/18.620/18.626/18.712 seconds: no meaningful end-to-end speedup is established. Nested spans
and correlated samples do not qualify CPU/GPU duration, physical-memory or production-tail budgets.
Full Foundation/R9/R12 and foreign execution remain open, with Editor/UI/Dialogue paused.

Owning Scene all-target/all-feature correctness Clippy, feature-free Scene library checks, root
formatting and selected source/executable/report integrity pass; existing warnings remain. The
ordinary game and observation source hashes are unchanged. The correctness reviewer and separate
ownership-study agent are both closed. Cargo/native work stays serial against the original target,
without cache cleaning, remote CI or publication. This closes the named cost-reuse unit; it does
not select or implement the following shared-storage study.

### Shared Component Payload Ownership Study

Status: source research at `81a9d59`; no shared-storage interface or implementation is selected. The
remaining clone envelope motivates a separate complete ownership change rather than moving
copies into migration/default/reference entry points. A self-contained component payload could
share its own allocation without retaining an entire raw Prefab, resolver or snapshot. Retaining
an entire document would change the existing
[escaped Tilemap custody control](../../src/project_content/tests/tilemap_custody.rs): raw Prefabs
can retire while an escaped SceneSource still reads its inherited values. Backing through a
snapshot or ProjectSourceContent would also risk a strong cycle through its own candidate.

Raw source retention and compiled output currently receive separate retained allowances through
the root ProjectContentLease; compilation's cumulative working model is not that lease.
ResolvedPrefabSource carries no budget or generation claim. Compiled data preparation can escape
independently, and public `snapshot.scene().clone()` currently owns independent value trees.
Cheap shared clones must not keep original allocations alive after their original charge is
refunded. Each retained owner could conservatively keep a complete logical allowance, including
the shared-container overhead; deduplicating actual allocation charges instead requires its
original custody to follow the last holder. Neither choice changes per-occurrence materialized
node/byte limits. No budget-saving claim follows from sharing alone.

Before implementation, select the source/compiled interface and audit its real public consumers.
Prove raw-document release, payload/candidate reachability, independent preparation, source edit
isolation, exact/one-short work and retained limits, generation capacity and final refund through
the original 16K Travel/native/pixel workflow. Actual defaults/properties/reference writes must
separate storage only when mutation is needed; mechanical `Arc::make_mut` at every owned entry
can merely relocate the clone. Keep original retained owners and receipts; no universal cache or
new lifecycle authority is implied by this study. The scoped read-only research agent is closed.

### Component Payload Sharing Through Existing Retained Owners

Status: implemented at `bcc481f`; Windows qualification is recorded below. Select a component-local
immutable value allocation, independent of raw documents,
resolvers and snapshots. `SceneComponentRecord` keeps its source metadata and wire shape, but
replaces direct public value-field access with borrowed access, explicit mutable access/replacement
and owned extraction. Public record/document Clone remains a deep value copy. Only the private
compiler may share source payload storage into its draft and sealed move-only compiled result.
Compiled value access remains borrowed, so callers cannot detach an uncharged shared allocation.

Keep full logical payload allowances for each source, compiled and independently prepared owner,
including the new backing allocation overhead. Do not deduplicate charges or enlarge limits.
Materialized node/byte ceilings remain per occurrence. Existing raw admission, hidden-default
copies, mutation preparation, immutable publication, receiving definitions and native World checks
remain authoritative. A shared value never owns its source document or ProjectSourceContent.

Actual migration first obtains an isolated owned value; current versions retain shared storage.
Default planning uses the existing Reflect planner and produces an owned value only on insertion.
Reference planning/visits remain complete; final lowering requests mutable storage only when local
or authored references actually need rewriting. Data preparation keeps its independent retained
allowance before sharing immutable payload storage. Source edits and public clones must never
change an older compiled/prepared value.

Migrate real source consumers directly, including independent fixtures and the existing reference
Editor, without resuming UI/Dialogue product work or changing paused integration files. Preserve
the wire format and remove the superseded public field rather than keeping parallel value owners.
Qualify sharing and copy-on-write identity, public deep-clone escape, raw Prefab release, independent
preparation, exact/one-short work and retained limits, generation capacity, final refunds and all
actual mutation routes. Run the original complete native Tile populations, source removal,
retained-input GPU replay and panic cleanup; compare a frozen original executable under the same
compiler/profile and phase hooks before claiming speed or memory gains. Full Foundation/R9/R12,
numeric budgets and foreign execution remain open; UI/Dialogue product work stays paused.

The private `ComponentPayload` owns only an immutable `Arc<ComponentValue>`. Public record Clone
deep-copies its value, metadata and contextual defaults; private compilation alone can share the
main value into the draft and sealed component. `value()`, `value_mut()`, `set_value()` and
`into_value()` replace the former source field directly. The original Reflect default planner
returns a borrowed effective value when it inserts nothing. Final reference lowering retains
the complete visit and requests mutable storage only for an actual changed reference, including
the unchanged root-local case. Runtime cook remains an independent deep copy and admits every
new backing allocation before copying. Root source, compiled and overlay retention retain full
logical payload and backing fees independently.

Two independent ownership/budget reviews found and closed a needless copy for unchanged
root-local references and a missing runtime-cook backing fee. The exact cook test independently
calculates 2,482 bytes for its three scalar roots: exact passes and one byte short refuses before
copying. Compiler/preparation controls exercise real migration, default insertion, property and
reference changes, public deep-clone escape, source edits, cancellation and bounded receiving
definitions. Two initial test-fixture failures were corrected: metadata registration belongs in
the prepared World baseline, and original provenance must be counted once. No product assertion
or capacity was reduced.

The focused storage/compiler/default/preparation/cook controls pass 20/20 at
`da9bed9c-7cd3-41c5-baa6-1cff5ad40891`; complete all-feature Reflect/Scene libraries pass
1,037/1,037 at `7a60ce7c-3fb2-4e5e-ba81-8bd29e06df2b`. Selected public Scene format, data binding,
defaults, reference and conversion consumers pass 104/104 at
`db5e234e-75f7-484c-9514-88995e6bc146`. Tooling/UI libraries pass 304/304 at
`7cdffe09-197b-45e6-85b0-c8b7c722afe2` after three remaining old-field test accesses were migrated.
Root content/residency/retention and public receiving-limit controls pass 91/91 at
`12fc3d5f-0c14-4fbe-aaaa-f67f2e9315ad`, with 1,538 outside the selected filter. The actual 4K direct
plus 4K root-bound Prefab Tilemap test proves shared raw/compiled values, independent public
clones and edits, raw Prefab release, original generation/allowance custody and final refund.
A public deep clone remains readable after the original allowance is refunded. Source and
compiled retention each independently charge the complete payload and backing. These CPU results
do not establish copying speed, physical-memory savings or a runtime budget.

The frozen sharing-only release program completes the unchanged balanced 16K comparison before
the separate Gameplay correction. All twelve complete native lifetimes and 36 original-input
GPU replays pass. The [report and exact evidence](../benchmarks/2026-11-14-development-runtime-costs.md#component-local-payload-sharing)
record capture 178.443/0.026 ms, defaults 44.249/212.121 ms, complete component materialization
335.002/323.956 ms and expansion 938.288/910.035 ms before/after. The workload omits Tilemap
`sort_key` and `y_sort_anchor`; actual default insertion therefore copies the complete shared
`cells` value. This moves that required copy into the default envelope. Whole-process walls
19.158/18.514/19.097/18.609 seconds establish no stable end-to-end speedup. Do not rewrite the
source to hide omitted defaults, report the capture hook as an accelerated identical deep copy,
or claim a memory/production budget from sharing. Later optimization of small effective-field
changes over large payloads needs its own ownership/admission study and complete consumer;
this unit does not select a persistent value tree or a second value/retention owner.

The first complete independent Scene/Material/Asset-domain cohort runs 262 tests at
`80e20046-18d6-47ad-88a8-de3ca2b15c8e`: 261 pass, one fails and one additional owner-trial timing case is
explicitly skipped. The failure is the existing two-seat raw/semantic replay, not a Scene
storage assertion. A focused repeat at `87c15e4b-56bb-493f-a0eb-33c5001d075a` reproduces it.
Exact Step resolves newly admitted input, but `add_local_gameplay_command_systems` currently
places custom producers only in ordinary PreUpdate. Their mapper therefore misses those
outcomes, and Resume cannot reconstruct the consumed changes. This contradicts the existing
Input guide's exact-step mapper contract. Keep the correction as a separate Gameplay owner unit
and preserve complete raw/semantic per-tick replay, fresh connection authority, continuation and
cleanup assertions; do not weaken or exclude the failing workflow to qualify Scene.

The final complete independent Scene/Material/Asset-domain cohort passes 262/262 at
`52c46779-78a8-4d77-8335-efcab1466713`, with the original owner-trial timing case explicitly skipped.
Complete release game/library/metric/span/native
acceptance passes 143/143 without skips at `a1230ba7-fe18-4fe3-b831-d6703b6c9763`. Nine 64/4K/16K
Tile lifetimes retain 773 initialized presentations and 27 original-input GPU replays; the three
baseline lifetimes retain 193. Original gameplay, old-receipt refusal, source removal, retained
image rereads, original submission-prefix/Host/native-window retirement and both observer-panic
cleanup controls remain. The final executable includes the separately qualified App-owned Step
correction below; it is distinct from the sharing-only frozen comparison executable.
The [qualification metadata](../benchmarks/data/2026-11-14-development-runtime/authored-tiles-shared-payload-metadata.json)
retains the selected source/program/report hashes, exact commands' compiler/backend scope and
all final run IDs. Source hashes are recorded after formatting and a Rustdoc-only Input ordering
correction; there is no later behavior change or substituted native program.

Additional public prepared-authoring acceptance passes 20/20 at
`c33505e4-0fb4-4c08-b267-ca4f0a1cfe70`, with three explicitly selected manual measurement cases
left ignored. This is not a scalar allocation or UI performance measurement. Feature-free
storage/default controls pass 10/10 at `56044aca-1090-4bf4-9e7d-1929efb7672c`, with 482 outside
the filter. Owning Reflect/Scene and root/App/Input/Gameplay/Tooling/UI/Editor all-target/all-feature
correctness Clippy pass; existing warnings remain. Six additional independently locked release
all-target checks pass for behavior languages, Motion consumer, Cargo consumer, native UI Host,
render consumer and stock consumer. Root/sample/affected independent formatting, owned document
links, report hashes and whitespace pass. The one previously clean GPUI content fixture receives
only its source-accessor migration; the original paused UI/Dialogue changes remain unchanged.
All eleven scoped agents are closed. Cargo/native work stays serial, using the original target,
without remote CI or publication. Full Foundation/R9/R10/R12, numeric budgets and foreign execution
remain open. This completes the named storage/consumer qualification, not those broader outcomes.

### Custom Gameplay Producers During Exact Step

Status: implemented at `c8f7c7d`; final qualification is recorded below. This separate correction
follows the existing two-seat replay failure found during Scene consumer acceptance. Exact Step already resolves
pending semantic input, but the public dynamic-producer helper previously ran only in ordinary
PreUpdate. Those producers missed the consumed outcomes. Preserve the original Input, command
queue, runtime/time/fault owners and one instance of every supplied system.

The original App owns a private intent-preparation phase after ordinary PreUpdate and before
the exact fixed transaction. It runs for zero-tick frames, once for a catch-up frame and once
for exact Step; paused maintenance skips it. Ordinary action resolution stays in PreUpdate.
For an unresolved exact-step frame, Input projects, stages and resolves through that same private
phase. Existing raw fixed projection and original timeline cursors remain. Gameplay registers
the supplied systems directly in the App-owned phase, orders collection before lowering and
keeps the original frame cursor and next-open-tick command admission.
This intentionally changes the helper's ordering boundary: its constraints now order systems
within the shared collection, rather than against arbitrary raw PreUpdate peers. Migrate related
producers through the helper; ordinary-only work remains in PreUpdate before collection.

Do not clone systems or their Local/deferred state across schedules. A provisional private
`Resource<Schedule>` passed 55 Gameplay and six independent replay tests, but independent review
found that its early Gameplay Finish initialization bypassed final App graph admission. A later
plugin Finish could register a cycle or change Local initialization inputs after that private
graph had initialized. That implementation is superseded before commit. The original App registry
now initializes the complete graph only after all plugin Finish hooks and applies its selected
automatic execution profile and managed fault boundary. No module-local executor or nested
schedule owner remains.

Acceptance preserves complete per-tick raw/semantic replay, connection renewal, held controls,
configuration boundaries and continued gameplay. The exact Step must admit both newly changed
player axes on tick eight, and Resume must not replay them. Owner controls exercise no-input,
zero/multiple ticks, shared Local state, ordered deferred effects, system/command failures,
late-Finish graph refusal before Startup and Local initialization after final configuration.
Original App controls also cover private-phase manual-entry refusal, paused/exact-step placement
and actual Parallel execution of the new automatic phase. Full Foundation/R10/R12, physical
controller backends and general recording/rollback remain open; UI/Dialogue stays paused.

Final App/Input/Gameplay libraries plus public schedule-registry, compatibility and execution-profile
controls pass 250/250 at `a6b35a0f-1363-4f9f-94dd-6746bdc29f36`, without skips. The late-Finish cycle
refuses at final seal before its observed Startup marker, Local reads the later Finish value,
and the private phase executes through the actual selected Parallel executor. Complete independent
consumers pass 262/262 at `52c46779-78a8-4d77-8335-efcab1466713`, including the entire raw/semantic
two-seat replay, with the original
owner-trial timing case skipped. Final release game/library/metric/span/native acceptance passes
143/143 at `a1230ba7-fe18-4fe3-b831-d6703b6c9763`, with all original native/pixel/panic cleanup
assertions. Independent final review closes the superseded private-schedule P1 and finds no
remaining P1/P2. The remaining Input ordering Rustdoc is synchronized without another behavior
change. These Windows/Rust1.95 results do not qualify foreign native execution or complete R10/R12.

### Closed Effective Values And Large Payload Follow-up

Status: Reflect/Scene research at `2dd0083`. No new value representation is selected or implemented;
the existing sharing qualification and measured limits remain unchanged. The
[pinned Tilemap ownership study](../research/2026-11-17-large-tilemap-payload-ownership.md)
compares current Nara, Godot and Bevy sources, including typed inline and external-asset alternatives.

The [current value representation](../../crates/nara_reflect/src/value.rs),
`ComponentValue::List(Vec<ComponentValue>)` and `ComponentValue::Map(BTreeMap<String, ComponentValue>)`,
has deep public `Clone`. The private
[`ComponentPayload`](../../crates/nara_scene/src/component_payload.rs) shares only a complete tree;
`make_mut()` and `into_owned()` copy shared storage. Preserve source wire and omissions,
`SceneComponentRecord::clone()` independence, and its `value()`, `value_mut()`, `set_value()` and
`into_value()` ownership rules. Public access must not detach an uncharged shared allocation.
[ADR0045](../architecture/adr/0045-component-schema-capability-metadata.md#effective-values) keeps
migration before one Reflect default interpretation. [ADR0006](../architecture/adr/0006-scene-and-prefab-data-model.md)
requires closed current compiled values, selected owner, complete canonical provenance and full
batch validation before codecs; receiving definitions cannot supply missing defaults or migrations.

Three implementable alternatives remain under consideration:

1. **Reuse an already owned candidate.**
   [`Compiler::materialize_program`](../../crates/nara_scene/src/prefab/overlay/compile/staged/values.rs)
   obtains an isolated migration input through `SceneComponentRecord::take_value()`, then stores
   the owned result. `Compiler::materialize_defaults` subsequently borrows that result, so an
   insertion through `effective_component_value_cow_impl` copies the tree again. This is a real
   current structural gap and a separate bounded correction: route an independently owned result
   through `effective_component_value_owned_with_default_overrides`, preserving the shared
   planner/preflight/application. Public interfaces need not change. A private non-copying
   uniqueness check could extend the route; `into_owned()` itself is not such a check. This does
   not remove the first copy of a current-version shared source with omitted fields and is not
   evidence of a 16K optimization.

2. **Frozen overlay with a borrowed effective-value view: conditional research candidate.**
   Reuse [`DefaultPlan`, `plan_defaults_with_admission` and `preflight_plan`](../../crates/nara_reflect/src/effective.rs)
   to freeze selected defaults and paths over private admitted backing. Selection finishes during
   compilation; reads do not consult a receiving Schema. Unchanged children remain borrowed, with
   no second default interpreter. This requires complete consumer migration: registry
   `ComponentCodec::preflight_with_context` and `register_*_with_codec` inputs; derived
   `PersistentComponentProvider::__decode_persistent_component`, `PersistentFieldCodec::decode`
   and generated field reads; compiler draft/output and `CompiledSceneComponent::value()`;
   shape/coverage and complete native validation; asset/entity reference planning, visits and
   rewriting; and actual cook/export consumers. A compatibility adapter that silently flattens
   before the real codec merely relocates the copy. Actual property/reference writes and
   [`runtime_scene::cook::clone_claims`](../../crates/nara_scene/src/runtime_scene/cook.rs) may retain
   explicit admitted deep-materialization paths. Source records can retain their existing public
   owning interfaces; compiled/expanded observations need views. The compiled contract must
   describe a frozen explicit effective value without weakening closure or receiving checks.

3. **Private persistent children.** Extend the same read interface with path-copying Map/List
   storage for repeated property and reference edits. Preserve the public owning `ComponentValue`
   and deep extraction; expose no clonable backing handle. This adds compiler mutation work,
   retained old-backing accounting and bounded compaction. A shallow root Map copy alone still
   deep-clones its List children. Current evidence does not select a persistent editing tree.

All alternatives retain complete per-occurrence materialized node/byte/depth limits and independent
full logical payload plus backing charges for source, compiled and prepared owners. Sharing cannot
deduplicate allowances or enlarge limits. `Compiler::reserve_program_temporary` and
`value_work_bytes` currently reserve whole-value work using `96 * nodes + logical_bytes`; overlay
storage would need separate admitted patch/backing allocations while preserving complete logical
value and traversal charges. Every actual deep extraction must still be admitted before copying.

Preplanning can consume one ordinary plan before capture for an admitted current-version value.
It cannot make retained source storage unique or justify filling authored omissions. Old versions
must first execute `ComponentRegistrySnapshot::migrate_component_value_owned` once, then plan its
actual result. Do not probe by executing migration/native callbacks twice or run
`component_defaults_are_explicit_with_admission` followed by a second charged plan.

A large generic List with small omitted defaults is useful supplemental mechanism coverage.
It does not itself establish pressure to replace every codec's read interface. Reflect/derive,
Scene compiler/receiver/cook and domain codecs would all need migration if a view is selected;
that cost belongs in the decision before implementation.

#### Next Executable Consumer: Tilemap Payload Representation Decision

Historical selection at this point in the investigation. The frozen-read successor below was
implemented; remaining representation experiments are now deferred by the
[November 17 Runtime delivery priority](2026-10-23-foundation-architecture-rebaseline-plan.md#november-17-runtime-delivery-priority).
The candidate table preserves the original decision context, not an active execution queue.

Execution direction: first compare the actual Tilemap ownership boundaries, then select one
complete production refactor. The first named discriminator remains the original 16K Travel
consumer, including direct and root-bound Prefab content. A generic effective-value view remains
a candidate. Do not make it an engine-wide prerequisite before evaluating domain ownership.

| Candidate | Distinct hypothesis | Required discriminating evidence |
| --- | --- | --- |
| Frozen defaults plus borrowed read view | The existing generic component boundary is appropriate, but effective reads should not require a full owning tree. | Preserve source wire/omissions and consume the frozen result directly through actual Tilemap validation, references and native codec. Ordinary flattening or offsetting traversal costs can disprove the expected benefit. |
| Separate persistent configuration and cell records | The current component boundary couples unrelated configuration defaults to large data; existing whole-component sharing may suffice at the right boundary. | Keep the current cell List/Map representation initially. Configuration default insertion must not own/copy cells again. Joining the records must preserve exact entity, Prefab/override, receiving, native preparation and cook semantics without rebuilding the monolithic generic tree. |
| Compact inline cells | Nested generic source decoding/allocation warrants a domain-specific encoding. | Decode before constructing the old generic cell tree. Measure remaining whole-block copying separately; packing after full generic decoding does not test this hypothesis. Preserve Nara coordinate/index/color semantics rather than copying narrower upstream encodings. |

Trace each candidate through source decode, effective defaults, raw/declared references and
validation, compiled storage, native preparation and cook. Identify each allocation, full walk,
retained old backing, public-interface change, Schema version, Prefab path and source-upgrade
responsibility. Correct the affected Accepted ADRs before changing their contracts. A runtime-only
Rust struct split occurs too late to resolve the measured compiler copy. A persistent-record split
also has migration/ergonomic costs and must not be presumed cheap.

Prefer an initial bounded vertical comparison of separate persistent configuration/cells while
keeping the existing cell encoding. This distinguishes ownership granularity from source encoding;
it does not require three production implementations or a new benchmarking framework. Preserve
large generic Scene data support. Select a shared read interface only if an actual retained mixed
value/override/reference workflow cannot reasonably be served by the domain boundary, or measured
complete-consumer evidence justifies its additional migration cost.

All candidates retain complete per-occurrence materialized node/byte/depth ceilings, source,
compiled and prepared owners' full logical payload/backing fees, original cumulative work and
pre-allocation scratch/copy admission. A packed block is not a one-node budget exemption. Receiving
definitions cannot add defaults or migrations; all records validate before the first native codec.
Public source copies remain independent. Preserve cancellation, generation capacity, old captured
values after source edits and final refunds. Keep one original publication, residency and render owner.

The existing compiler/receiver/cook controls and 64/4K/16K travel/source-removal/retained-input GPU
replay/panic cleanup remain the product acceptance. Compare the frozen qualified `2dd0083` program
with actual final source, equal compiler/profile, equivalent gameplay/geometry and balanced order.
The view candidate preserves source bytes. Record or encoding changes retain the original baseline
and explicitly identify semantically equivalent new input; report old-format upgrade separately
from ordinary new-format loading. Do not fill omitted `sort_key`/`y_sort_anchor` to conceal the
problem. If component count or phase calls change, retain and explain actual counts rather than
silently dropping the original oracle. Report cumulative nested phase observations separately
from whole-process variation; no physical-memory, CPU/GPU duration or production-tail claim follows
from sharing or packed storage alone.

Only the comparison selects the full production direction. Source evidence from official Godot
and Bevy supports testing inline and configuration/data granularity, not compulsory external cell
assets or a fastest implementation. Independent reuse/publication/lifetime pressure is needed
before adding cell asset identity. A plan review corrected the earlier premature preference for
engine-wide borrowed values and the use of a synthetic generic List as its primary justification;
neither finding changes the isolated-migration correction below.

#### Bounded Persistent Tilemap Record Probe

Status: test-only comparison implemented at `7088b9e` and qualified on Windows/Rust1.95;
no production representation selected. Reuse the
ordinary `runtime_workload` source generator at 16,384 cells and isolate its two terrain members
only for the CPU decision probe. Preserve the direct record, root-bound Prefab declaration,
original source keys/IDs/placements, cell encoding and omitted defaults. Compare the stock two
records against four explicit configuration/cell records on the same two entities. Do not call
this a complete game or performance run.

The public consumer must demonstrate that configuration defaults do not replace the Cells
allocation, while stock defaults still isolate their complete value. Check all 16,384 cell
values, full native configuration/dirty metadata, canonical provenance, receiving exact and
one-short aggregate limits, no target publication on refusal/cancellation, independent cooked
values and source files unchanged after original Scene retirement. An explicit Cells-only
control must remain without Configuration: the experiment must expose the missing structural
pairing contract rather than hide it in a codec or required-component hook.

Trace source-upgrade and native/render joining separately. Current one-value component migration
does not transform one source record into two; field-level upgrade, finite property tools,
flattened live export or localized Prefab conversion do not silently supply the complete native
declaration/history conversion. Preserve Add/Replace/Suppress/Properties semantics if that
production direction is selected. The
[study](../research/2026-11-17-large-tilemap-payload-ownership.md#bounded-persistent-record-comparison)
records the concrete costs and distinguishes them from the frozen-view alternative. Passing
this probe alone cannot select the production split or replace the original complete Travel,
source-removal, retained-input GPU replay, failure cleanup and balanced frozen-program comparison.

Run the bounded CPU consumer serially against the existing root target:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,cost-measurement --test tile_payload_trial --test-threads=1 --no-fail-fast --no-tests fail
```

The first release build takes 14m43s. Initial execution passes 4/5 without skips at
`492f16a7-e0de-43f5-9450-de9db584c0ed`: the receiving-boundary control and three original
source-generator controls pass; the full ownership/cook test reaches an actual processed-wire
limit, `wire-nodes`, observed 1,000,001 against the default 1,000,000. This is not a compiler
failure or a passing cook. The successor retains all 16,384 cells and requires the same default
refusal from the stock control. Both complete positive cooks and trial readmission explicitly
request 2,000,000 wire nodes through the existing public limit value; all other original ceilings
remain. No product default changes.

The corrected release consumer passes 5/5 without skips at
`3f266fd4-7807-4add-9b82-b01d917dc2a0`, 4.327 seconds after the 46.56-second test rebuild. The
complete ownership/cook/edit-isolation case takes 1.804 seconds and the receiving/pairing control
0.921 seconds; the other three cases are the original source-generator controls. These are test
durations, not product loading or performance measurements. Stock and split both preserve the
default `wire-nodes` refusal. Under the same explicitly requested finite wire limit, both cook
all 16,384 cells with identical cell values and canonical provenance; the split wire readmits
under that same limit with independent storage. No game capacity claim follows.

Actual raw/compiled identity proves that default insertion retains direct and inherited Cells
while the stock value isolates. A single source-cell color edit changes only the new direct
candidate: old compiled, prepared native, cooked and readmitted values remain unchanged, the
inherited allocation remains shared and disk sources remain identical. Complete typed comparison
includes stock configuration, cells and dirty metadata, with the temporary TileSet-handle reuse
explicitly confined to a data comparison. Exact receiving nodes/bytes/depth 6 succeeds; one-short
nodes/bytes and depth 5 each refuse with the actual codec error and unchanged entity set.
Independent review identifies and closes the initial depth-1 coverage gap, then rechecks the
source-edit control; separate review closes the actual cook-limit correction without a P1/P2.
Focused release correctness Clippy and sample formatting pass; existing unrelated warnings
remain. No production source, manifest, lockfile, global limit, codec interface or native owner
changes. All four scoped research/implementation/review agents close, Cargo remains serial with
one job and the original target, and paused UI/Dialogue files remain untouched.

The probe establishes a feasible storage boundary and exposes its costs, not a production winner.
The next decision must reconcile explicit native same-entity joining, actual TileSet membership,
ordinary mutation/extraction and complete direct/Prefab source-upgrade semantics against the
closed-view alternative. Preserve the original complete 64/4K/16K game/source-removal/retained-GPU
and balanced frozen-program acceptance for the selected production direction. Full Foundation,
R9/R10/R12, numeric budgets and foreign execution remain open.

#### Frozen Effective Reads Through The Existing Component

Status: implemented at `6ceb534` and qualified on Windows/Rust 1.95 within the CPU/public/native
and measured scope below. Keep the current Tilemap source record, Schema,
Prefab property/version semantics and complete native Tilemap. Configuration/cells separation
adds no independently required current consumer capability: mutation already changes cells
without copying them, and stock extraction still needs both configuration and cells. Selecting
it only to avoid a generic default copy adds a new pairing/deletion contract, native joins and
a complete one-to-two source-upgrade operation. Compact inline cells remains an independent
decode hypothesis; it needs budget-aware packed values, lossless opaque-extra preservation and
property upgrades and still copies the whole block during default insertion. Neither becomes
the production default in this unit.

Implement a borrowed `ComponentValueRef` Interface over ordinary owned values and a frozen
default-insertion tree. Reflect retains the single existing default planner, complete effective
preflight and source-selected defaults. Freeze only the chosen insertions, including missing
Map ancestors and contiguous List suffixes; retain raw backing through the existing private
Scene owner. The frozen value is closed explicit data, with no receiving-Schema lookup or later
default interpretation. Reads expose scalar values and borrowed Map/List traversal, not a
clonable owning backing handle. Public source values/wire/omissions and deep Clone remain.

Migrate real native codec and derived field decoding, full value/shape/reference validation,
Scene compiler/expanded/compiled/prepared/data reads, Tilemap membership and native decoding,
and actual cook/export to that same borrowed read Interface. A codec compatibility path that
first flattens the complete value is not the selected implementation. Property/reference writes,
public deep extraction and independent cooked ownership may explicitly materialize after the
original owner admits the complete allocation. Reuse already unique migrated storage where
appropriate; do not lose the preceding owned-default correction.

Keep complete per-occurrence node/byte/depth checks, actual Schema/owner witnesses, source and
compiled/prepared independent full logical/backing fees, cumulative work and pre-allocation
admission. Account frozen paths/default data and traversal scratch honestly. This is not a
general persistent editing tree, a new publication owner or a packed-value budget exemption.
No new domain record, format version or source upgrade is needed for this storage change.

Acceptance remains the original direct/root-bound 16K source with omitted defaults. Prove the
unmodified cell backing reaches actual membership and native codecs without flattening; compare
effective values with the original materializer, including nested/default override/Null/sparse
refusal cases, exact/one-short receiving limits, source edits, old prepared/cooked values and
final refunds. Qualify derived and handwritten independent codecs plus the original full game,
source removal, retained-input GPU replay and panic cleanup. Run the unchanged balanced frozen
program comparison before any performance claim. Keep all Foundation/R9/R10/R12 completion,
numeric budgets, foreign native execution and paused UI/Dialogue scopes distinct.

Implementation begins after `c63e297` and lands at `6ceb534`. Reflect has borrowed scalar/container reads,
one canonical logical-cost walk for owned and frozen data, owned selected insertions from the
existing planner, actual native/derived decode callbacks and full shape/reference/property
validation. Scene retains a separate private effective payload and public read-only expanded
component rather than changing source record ownership. Native Tilemap, Material, UI, Camera
and Transform decoders use the read Interface directly. Source wire, domain schemas and native
Tilemap composition remain unchanged. Public callback/provider signatures change from
`&ComponentValue` to `ComponentValueRef`; Clone copies only the borrow. The
[component guide](../guides/persistent-components.md#borrowed-native-decoding) documents explicit
owned extraction and the absence of an automatic flattening bridge.

The first Reflect all-feature release production check passes in 8.91 seconds. Its first
public consumer passes 7/7 without skips at `d51d25f8-3145-464c-ab09-c693e42f6b37`. Independent
review identifies a canonical-cost forwarding regression: replacing the supplied accumulator
discarded its nonzero starting cost. The shared walk now accepts that initial cost and preserves
the original saturation/minimum-node/refusal policy. The untouched original saturation test
passes with the complete owner/public cohort, 265/265 without skips at
`115ccda4-885b-4511-99ca-dd68c2c51f09`, 13.221 seconds after a 1m35s release build. This includes
14 frozen tests, nonempty Authored route allocation accounting, actual native callback pointer
checks, derived nested values and exact selector/receiving/refusal controls. Review closes the
accumulator finding; separate Scene and registry/derived/Inspector static reviews find no P1/P2.
These are bounded source/CPU/API results, not full consumer or performance qualification.

The first root Runtime production check reports ten Scene call migrations involving explicit
Cow/raw reborrows. After correction it reaches one root expanded-record retention call, which
now charges the new record's complete payload, backing, frozen data and contextual metadata.
The selected Runtime/UI/render production closure then passes in 12.34 seconds. The root
all-feature closure additionally exposes one finite source-tool initial-value borrow; that path
now explicitly checks its actual unchanged raw subtree. The final all-feature root production
check passes in 13.74 seconds. The first Scene/Tooling test-target check reports thirty Scene and
one Tooling API migrations; corrected complete all-feature test-target checking passes in
37.24 seconds. This proves compilation, not execution of those test targets. Original API
semantics/assertions remain, with explicit owning extraction for intentionally owned native
payloads. Scene's unchanged structural root-work allowance is also preserved before its actual
owner tests. Independent public integrations, domain owners, game/native/GPU and balanced
comparison remain required. No new runtime capacity, cost reduction or complete Foundation
claim is made at this intermediate checkpoint.

The historical record probe at `7088b9e` remains the pre-change comparison. Its current consumer
now observes the changed stock contract explicitly: the defaulted root is a frozen read, and
the complete `cells` subtree retains its exact original source allocation for both direct and
root-bound records. The former whole-root-copy assertion is replaced by those stronger borrowed
data observations. The original 16K population, values, provenance, receiving/cancellation,
independent cook and matched wire-limit controls remain; the original gameplay/native workload
and its metric-count oracles are unchanged. Current execution of this migrated probe is pending.

The first Scene/Tooling owner-test build stops before execution when its output fingerprint
directory disappears during build-cache cleanup (`os error 3` while writing dependency
information). No behavioral result is claimed from that attempt. After the user reports cleanup
complete, the resumed release build reuses the original target with one job. A bounded
cross-owner review also finds one unmigrated root retention-test pointer comparison; its
correction extracts the actual raw subtree borrow and preserves the original allocation-identity
assertion. Review closes that finding; root execution remains a separate gate. The previously
frozen `2dd0083` measurement executable survives outside target, with its original qualified
hash and source receipt intact, so the balanced comparison need not reconstruct its baseline.

That resumed release build completes in 28m55s. Actual execution passes 981/983 without skips
at `af664887-063e-470b-be49-66ff84807419`, 62.296 seconds. Two test corrections preserve their
original behavior: the new Inspector depth fixture registers a real native binding before registry
freeze, and the existing reference-work oracle includes the now-admitted no-insertion default
planner. The latter adds exactly one node and zero bytes per component; exact success and
one-short node/byte refusal remain. Independent review confirms the owning call, total and
unchanged product limits. All Scene/Tooling test targets then check successfully in the release
profile. Complete owner/domain CPU execution continues in the debug test profile; native game
qualification and the balanced program comparison retain their release scope.

The complete all-feature debug owner/domain run executes 1,608 tests in 104.336 seconds at
`2af6aee7-eafd-41d3-af07-0c9ce0584988`: 1,607 pass, one new Material fixture fails and no tests
skip. Scene 801/801, Tooling 182/182, Reflect 258/258, Tilemap 105/105, Sprite 15/15,
Transform 40/40 and UI 127/127 all pass, including both preceding corrections. The remaining
fixture incorrectly expects depth 5 for a complete Product texture reference whose actual
`texture_overrides -> reference -> value -> product -> kind` reaches depth 6. Its corrected
control accepts both owned and frozen inputs at depth 6 and refuses depth 5, retaining exact
node/byte boundaries and the real native resource application. The complete all-feature Material
library then passes 80/80 without skips at `304af988-8591-4836-9e6e-7efd5b7bcaad`, 3.200 seconds.
No production decoder or limit changes for these fixture corrections. Root/consumer/native
qualification and the unchanged balanced comparison remain required.

The root retained-content/custody/startup cohort then passes 139/139, with 801 outside its filter,
at `ebb67d7b-b579-44d6-b390-4f460c1c0e80`, 11.926 seconds in the debug test profile. It includes
the original large root-bound Tilemap source and last-owner refund, repaired effective references,
processed resources, original-base retention after file removal, exact overlap refusals and
startup replacement. The corrected raw backing-pointer assertion compiles and passes. The
selected feature closure is `runtime-2d,serde,project-tooling`; no native presentation follows
from this CPU result. Complete release game/metric/native and 16K probe acceptance is next.

The complete release sample/metric/span/native/probe run builds in 15m37s and executes 148 tests
in 61.371 seconds at `2e00e020-54bc-44f8-981b-a3801c76ebea`: 147 pass, one CPU pointer control
fails and none skip. All 143 game/library/metric/span/native cases pass, including the original
64/4K/16K Travel populations, 27 original-input GPU replays and both observer-panic cleanup
controls. The pointer control incorrectly compares inherited compilation with the original side
of a public deep Prefab clone, rather than the allocation actually transferred into the resolver.
It now resolves that actual source, also verifies that the public clone remains independent,
and explicitly drops the extra observation before the original source/resolver retirement.
Direct and inherited stock cells must still retain their exact respective original allocations.
Independent review confirms the correction without P1/P2. The complete probe then passes 5/5
without skips at `d70b2cc3-68f2-41d8-8140-13ec9ea75dbd`, 5.034 seconds. Production code, receiving
ceilings, original native trajectory and its qualified executable remain unchanged by this fix.

The [current native evidence](../benchmarks/2026-11-14-development-runtime-costs.md#frozen-effective-reads)
retains nine Tile lifetimes with 2,923 initialized presentations and 27 replays, plus three
baseline lifetimes with 485 initialized presentations. Those counts describe the paced workload,
not throughput. The unchanged balanced frozen-program comparison completes twelve additional
16K lifetimes and 36 GPU replays. Default wall median falls from 242.509 to 94.931 ms, but complete
component materialization is 371.721/367.683 ms and expansion 1,002.514/1,009.882 ms before/after.
Four complete process walls establish no stable end-to-end speedup. More expensive full validation
and raw-descendant traversal offset most of the removed copying. The linked report preserves
all source/program/report/order hashes and excludes CPU/GPU duration, physical-memory and numeric
tail-budget claims. Independent public consumers and final owning checks remain in progress.

The complete independently locked Scene/Material/Asset-domain consumer selection passes
262/262, with the existing manual timing case skipped, at `be70cdcc-975b-499b-9e96-b57d65bc0e23`,
30.337 seconds in the debug test profile. It retains handwritten/derived codecs, source-free
native and data cook, actual products, TileSet membership, prepared cancellation, exact closure
retention/refund, local input replay and final effective reference demand. Selected public
Reflect integrations pass 134/134 without skips at `4273561d-4ae9-4a2e-a74a-fdeadc6d38fb`,
7.033 seconds, including frozen/derived nested values, exact materialization, ownership, property
and registry boundaries. Selected public Scene integrations pass 179/179 without skips at
`b14a5ad1-6987-484a-aef1-dd21000f7381`, 8.306 seconds, retaining mixed/data native preparation,
runtime/export references, direct/Prefab conversion, saved-source extraction and exact receiving
refusals. The ordinary reference-game compatibility consumer and final checks continue separately.

Ordinary reference-game library/authoring/first-wave/Prefab/Vec2 migration controls pass 44/44
without skips at `ceed3315-fc6c-4738-997f-00d098267f6e`, 5.909 seconds. The independently locked
language-consumer compilation identifies one remaining public copy-operation signature: its
existing snapshot observation calls `rewrite_declared_entity_references` with a compiled read.
The existing rewrite/remap operations now accept that borrowed input, plan the complete reference
structure first and create one independent owning result before rewriting. They do not flatten
before a native codec or change the compiler's private plan/in-place path. The new public control
checks frozen defaults, payload independence, refusal before callbacks, input preservation and
result survival after source/default retirement. The first test compile misses its shape import;
after correction, the complete selected Reflect owner/reference cohort passes 285/285 without
skips at `6264e448-4ffd-4a45-9d39-d08f3bf93b93`, 9.475 seconds, and actual language-consumer
test-target checking passes. Independent API review finds no remaining P1/P2. This late public
copy-input migration is qualified separately from the preceding native/measurement executable;
the recorded binary and source hashes retain that exact earlier scope. Foreign language/native
execution is not inferred from this compilation.

Final owning all-target/all-feature correctness Clippy, all-feature root library Clippy and
sample library/record-probe Clippy pass; existing warnings remain. Feature-free Reflect/Scene
libraries compile. Root, sample, independent-module and reference-game formatting pass after
correcting one new import's wrapping. Added document paths/fragments, CI YAML, JSON report/hash
integrity and Git whitespace checks pass. CI adds the finite 16K CPU record probe to the existing
runtime metric/span step; no remote CI or publication runs. The late public reference-copy entry
is independently reviewed and tested; no relevant private compiler/native consumer changes
follow the recorded complete native and balanced comparison. All seven scoped agents started
during this resumed qualification are closed. Cargo stays serial with one job and the original
target; paused UI/Dialogue work remains byte-identical and excluded from commits.

This closes the selected frozen-read implementation and its complete Windows consumer scope.
It establishes allocation identity and preserved semantics, not stable end-to-end acceleration.
The next owner-local optimization must attribute repeated complete validation and raw-subtree
traversal against the same frozen programs and original game. Full Foundation/R9/R10/R12,
physical-memory and numerical runtime-tail budgets, larger named workloads and foreign execution
remain open. No domain record split, source upgrade, new cache or lifecycle owner is introduced.

#### Validation Locality After The Frozen-Read Comparison

Status: scoped compiler implementation at `9fc9322` and Windows qualification complete below;
source-compiled receiving reuse remains a follow-up. The [validation study](../research/2026-11-17-large-tilemap-payload-ownership.md#5-validation-at-owning-boundaries)
compares actual Godot parsing/resource/property paths with Bevy Reflect, resolved Scene and typed
Tilemap paths. Both check construction or changed state without imposing Nara's repeated generic
node/byte/work-budget passes on this route. Godot also allows some property/dependency errors to
continue, while Bevy's typed construction leaves domain constraints to their owners. Neither is
evidence that a successful load universally validates every value, nor a reason to retain every
current Nara scan.

The first correction should remove repeated cost admission of the same unchanged effective value
inside the original compiler operation. `materialize_defaults` already returns its complete cost;
the following `validate_component_value_with_limits` rescans a frozen read for the same budget
before doing necessary field semantics. Reuse the original cost and admitted schema while retaining
required-field, shape, numeric and reference checks. Preserve standalone public validators for
unadmitted dynamic values. Do not expose a public unchecked constructor or introduce a general
validation-receipt framework merely to connect these existing operations.

Trace source-compiled receiving preparation separately from that immediate duplicate. Its current
complete pass also checks namespace, products and the receiving definition. Only reuse pure facts
when the existing immutable candidate owns them against the exact receiving definitions/policy.
The processed Runtime candidate already has this distinction. Migration and arbitrary callback
output, property edits and reference rewrites invalidate applicable old facts; current World,
resource/reservation and asynchronous publication checks stay at their actual consuming owners.
Reference-free schema metadata alone does not prove opaque payloads contain no undeclared reference.

Existing per-occurrence logical/depth/byte and retained-owner allowances remain the contract for
the first correction. Removing an operation should remove its actual work/scratch fee rather than
emulate the old traversal to preserve a counter. Record revised work expectations and refusal
ordering where affected; exact/one-short receiving limits, original diagnostics and refusal before
callbacks or allocation remain useful behavioral controls. More elaborate accounting needs a
named bounded-operation consumer and must not become a prerequisite for ordinary native gameplay.

For subsequent performance work, review validation placement before adding another layer. Trace
the corresponding pinned Godot/Bevy operation, the error being prevented, the data origin and
mutation since the preceding check. Prefer typed construction and owner-retained facts for
unchanged internal data. A generic budget or defensive full scan needs a named failure/workload
contract; neither an old test nor a speculative future plugin is sufficient justification.
Check the same operation after edits or different receiving constraints rather than trusting
an unrelated earlier input. This rule addresses overvalidation during design, not only a later
optimization pass over accumulated validators.

Keep normal typed gameplay separate from persistent dynamic input admission. Do not route an
ordinary native component read through a generic value tree or make each frame re-establish
unchanged loading facts. Prefer validation in constructors, loaders, setters or changed-data
systems, following the corresponding reference path. Keep publication-time checks specific to
the mutable state that can actually become stale. A constant-time identity/version/bounds check
and a complete payload traversal have different costs and purposes; name that distinction before
adding either. Use debug assertions for internal developer invariants only when release behavior
does not rely on accepting external malformed input.

Qualify the affected Reflect/Scene and public receiving paths, then the unchanged authored game
and original-input GPU replay/retirement trajectory. Compare balanced frozen programs against the
qualified frozen-read executable; avoid simultaneous representation/encoding changes that obscure
the cause. Do not rerun unrelated Editor or UI suites. The pinned-source investigation itself ran
no Cargo, runtime or upstream benchmark; implementation results follow below. Full Foundation/R9/R10/R12 remain
active; this unit does not reopen UI, Dialogue or physical mixed-DPI product work.

##### Implementation And Qualification

Reflect now exposes its existing explicit-field semantics through one hidden operation. The
standalone validator still performs its ordinary schema/default/value admission before invoking
those semantics. Scene invokes the semantic operation immediately after its original source,
migration and default-materialization admission, retaining the complete cost for reference
planning. This removes the frozen-view cost walk at that point without changing source wire,
default selection, numeric domains, reference interpretation, receiving limits or publication.
No owning receipt, generic cache or new lifecycle authority is introduced. ADRs0045/0049 and the
component guide distinguish reusable immutable facts from current receiving state.

Production all-feature Reflect/Scene libraries compile. Reflect owner/frozen public controls pass
266/266 without skips at `b1dca14d-0f3b-425a-8e2d-4d338aa6c3c0`, 7.833 seconds; all-feature
Scene/Tooling libraries pass 983/983 without skips at `ad83d126-81de-42f0-a92a-02e1e6012984`,
62.517 seconds in the debug test profile. The selected public Scene run passes 79/80: all
76 existing cases pass, while one new positive fixture assumes a fresh World has zero entities.
Nara's World has internal resource entities. The fixture now compares the actual population
before/after preparation rather than hardcoding an implementation detail. All four new public
cases pass at `797f942b-8eec-4343-878c-0a7684615870`, 0.274 seconds. They retain opaque source
allocation through native preparation, reject missing/wrong/undeclared/out-of-domain fields,
reject malformed migration output and preserve exact/one-short receiving node/byte/depth limits
before codecs without changing the World. Independent implementation review finds no P1/P2.
Release game/library/metric/span/probe/native qualification passes 148/148 without skips at
`44005346-b3e6-43f8-b7da-9e86dea54b48`, 62.398 seconds after a 9m31s serial build. The original
64/4K/16K travel lifetimes, 27 retained-input GPU replays, baseline and both observer-panic cleanup
controls remain. The same executable then completes four serial before/after/after/before
16K processes: twelve complete lifetimes and 36 replays. Source workload, manifests/locks,
original value-phase call counts and compiler/profile remain unchanged.

The [recorded paired comparison](../benchmarks/2026-11-14-development-runtime-costs.md#compiler-validation-locality)
has semantic-validation wall medians of 83.977/0.815 ms, complete component materialization
394.190/310.312 ms and expansion 1,056.126/940.849 ms before/after. These are cumulative wall
envelopes, not one function's exclusive CPU time. Process walls 21.673/19.663/21.925/22.214 seconds
remain variable; no stable whole-process, GPU/CPU duration, physical-memory, frame-tail or numerical
budget claim follows. Four summary exports preserve the phase, source, acceptance, cleanup and
replay facts plus complete raw-report digests while omitting repeated per-frame arrays. Original
programs and complete reports remain outside target. The independently locked Scene consumer
passes 157/157 with one existing manual timing case skipped at
`8e25b7bf-f997-4a89-ae73-79b90bba5d83`, 18.772 seconds after a 6m04s serial debug build.
Owning all-target/all-feature correctness Clippy, feature-free Reflect/Scene compilation and root
formatting checks pass; pre-existing warnings remain. Independent final implementation and new
public-test review find no P1/P2. The two scoped agents are closed. No remote publication, new
dependencies, source-format/capacity changes or UI/Dialogue edits accompany this unit.

The bounded receiving audit identifies the next separate operation to reconcile. Compiled sealing
validates closed values, but a bare `CompiledSceneCandidate` does not retain exact receiving
definitions or a complete logical cost/max-depth summary. Root project-backed `SceneSource`
already checks an exact bindings witness, while its Direct branch delegates to ordinary receiving
validation. `spawn.rs` currently revalidates both compiled routes, unlike the processed Runtime
route. Within `validate_effective_component_values`, aggregate admission and native/reference
validation already walk complete data before the frozen field-validator path repeats cost
admission. First remove that repetition inside the receiving owner; then evaluate reuse through
the existing project witness without adding a blanket trusted boolean or new certificate/cache.
`compiled_scene_retained_bytes` also recomputes immutable costs; retaining a final cost summary
could avoid inspection while preserving each owner's fee. Current World, reservations, actual
product state and publication checks remain distinct. This audit is source evidence, not a new
implemented receiving fast path or qualification of that future change. Full Foundation/R9/R10/R12,
raw-subtree costs, physical-memory and numerical runtime-tail budgets remain open.

#### Receiving Field Validation Locality

Status: scoped implementation at `f5c34ff` and Windows qualification complete below, following the compiler
correction at `9fc9322`. The original shared effective-value kernel serves expanded drafts, compiled sealing,
processed claim admission and source-compiled receiving preparation. Before field semantics it
already checks that defaults are explicit, admits the complete aggregate value cost and checks
Native depth/reference requirements. Calling the default-selecting full validator again repeats
cost admission for frozen values and default-planner work for explicit raw values.

Reuse the same semantic operation with the existing admission callback for schema-index,
shape-validation and possible diagnostic allocations. Keep standalone/default-selecting validation
as its existing entry operation, delegating only its semantic tail. The receiving kernel uses the
explicit tail only after its original checks succeed. Preserve required fields, numeric domains,
key uniqueness, declared coverage, opaque reference inspection, exact receiving limits and the
original caller's refusal identity. Removed planner/traversal operations no longer charge their
work. Update internal-work oracles to describe actual work; do not emulate obsolete observations
or add a public trusted flag, generic receipt, cache or additional validation framework.

Qualify the affected Reflect/Scene/Tooling owners, public semantic and receiving controls, and the
independently locked Scene consumer. Preserve the unchanged original release game, authored
64/4K/16K travel, retained-input GPU replay and panic cleanup trajectory. Compare the new program
with the qualified compiler-locality executable using the existing balanced 16K workload and
hooks; inspect flat admission, preparation and complete expansion envelopes separately. This
unit does not skip the receiving pipeline, change definitions/limits, alter World/resource checks
or claim broader exact-definition reuse. Independent review and precise local commits close only
this selected replacement; full Foundation/R9/R10/R12 remain active.

##### Implementation And Initial Qualification

Reflect's full admission entry still selects/checks defaults and costs before delegating to the
existing semantic tail. The explicit semantic tail also accepts the original admission callback
for actual index/field/diagnostic work, without default selection or another payload-cost scan.
The shared Scene kernel uses it only after the pre-existing complete receiving checks. All three
Schema authorities and Draft/Compiled/Runtime roles retain the same definitions, value limits,
reference behavior and current-state/publication paths. No source/API consumer constructs a
new trusted receipt or cache. The component guide and ADR0045 record this division.

The all-feature Reflect/Scene/Tooling libraries execute 1,241 tests at
`d393ddf3-c28b-48dc-ad4d-44cc1b2c9c3a`, 69.337 seconds after a 1m42s debug build: 1,239 pass,
two Scene tests fail on obsolete work expectations. The wide-optional test explicitly demanded
1,024 observations from the removed duplicate default planner. It now retains the complete
raw/Native prefix, index-allocation refusal, exact caller refusal and zero-codec checks without
that count. The aggregate reference-work oracle now accounts the semantic operation actually
performed, preserving exact/one-short cumulative node/byte refusal and unchanged input. Both
corrected cases pass at `d44f8217-f8b9-40b9-9fee-4670f014d218`, 0.098 seconds, with the other
799 Scene tests outside that focused selection. No production correction follows the first run.

Explicit semantic/frozen-read public controls pass 11/11 without skips at
`3b8c46e4-f491-43bf-ac16-6db935731e63`, 0.581 seconds. The three new cases cover closed raw/frozen
semantic parity, original non-Clone refusal identity at every actual callback boundary and
unchanged semantic work for opaque payloads containing one or 4,096 nested elements. They do not
bind historical counters. Selected public Scene receiving/binding/data/export/reference/migration
controls pass 74/74 without skips at `7e169f90-41e9-4ced-b542-0812e36d247c`, 3.398 seconds.
Independent implementation and final test/oracle review find no P1/P2; both scoped agents are
closed. Complete release game/library/metric/span/probe/native qualification passes 148/148
without skips at `de56ec26-1245-45b8-a35f-23cc43605bc9`, 63.357 seconds after a 9m39s serial
build. Original baseline and authored 64/4K/16K travel, 27 retained-input GPU replays, both
observer-panic cleanup controls and the 16K CPU record probe remain unchanged.

The identical executable then completes four serial before/after/after/before 16K processes,
twelve complete lifetimes and 36 GPU replays. The
[paired report](../benchmarks/2026-11-14-development-runtime-costs.md#receiving-field-validation-locality)
records flat-admission wall medians 490.235/383.589 ms, native decoding/preflight 932.016/776.535 ms
and unbound preparation 358.099/299.969 ms before/after. Those cumulative envelopes overlap and
cannot be added. Compiler semantics are unchanged; variation in unrelated phase walls is not
credited to this change. Process walls are 19.897/18.794/18.705/20.162 seconds, an observation
without statistical confidence, CPU/GPU-duration, physical-memory or numeric frame-tail claims.
Original source/compiler/lock/observation hashes, workload and value-phase call counts remain;
finite summaries retain complete phase/source/acceptance/cleanup/replay facts and raw-report hashes.
The independently locked Scene consumer passes 157/157 with one existing manual timing case
skipped at `74609df9-14e1-4d7e-8d10-bf652f50bc5e`, 18.732 seconds after a 5m41s debug build.
Owning all-target/all-feature correctness Clippy, feature-free Reflect/Scene production compilation
and root formatting pass; pre-existing warnings remain. Added document/report links, exact report
hashes and Git whitespace pass. No dependency or lock changes, remote CI or publication occur.
Paused UI/Dialogue files remain byte-identical and outside commits. The two scoped agents are
closed; all owned Cargo/native measurement processes are terminal.

This closes the receiving kernel's duplicate field-validation operation within the qualified
Windows scope. Complete cost/depth/Native-reference walks and native codec preparation remain
elsewhere; their consolidation or reuse requires the same source and receiving distinction.
An applicable immutable final cost/depth summary could serve stricter limits and separate retained
fees without new scans, but no generic receipt, blanket trusted flag or broader exact-definition
reuse is implemented here. Evaluate that next against the actual remaining workloads and existing
project witness. Source decoding and ordinary game/runtime delivery remain priorities; full
Foundation/R9/R10/R12, physical-memory and numerical runtime-tail budgets remain open.

#### Single Native Value Admission

Status: scoped implementation at `ab19dc9` and Windows qualification complete below after receiving-field
locality. The original Reflect Native materialization walker already computes complete node/byte cost while checking
depth and Native references, but its admitted entry discards that result. Scene first performs a
separate complete cost walk, then invokes the Native walker. Return the existing exact cost and
let the receiving owner's aggregate allowance consume it after one accepted traversal. Keep the
original walker, scratch admission and source/native policies; do not create a generic summary,
certificate, cache or new traversal framework merely to carry an existing result.

Refused values have a distinct existing contract: complete aggregate cost is classified before
Native/depth diagnostics, and a value that fits the aggregate cost remains charged even after
Native failure. On a Native value error only, retain the cost-only classification in the original
Scene budget owner to preserve that whole-record charge and node-before-byte aggregate errors.
Caller admission refusal immediately returns its exact error without retry. This failure path
does not justify a second scan for accepted values. Scratch/work admission follows actual
operations; removing the success-path cost scan also removes its allocation/work fee.

Acceptance includes returned exact raw/frozen Native cost, complete nested/reference cost,
strict receiving node/byte/depth errors, original refusal identity, no source mutation and one
Native callback envelope for accepted values. Failed early Authored/depth checks must still
account for later unvisited siblings before accepting another record; zero-byte remaining space
can admit Null, while zero nodes rejects any value. Both Native and failure-classification
admission refusals must stop immediately. Preserve complete public Scene, independent consumer,
original release game/native and balanced frozen-program workflows. Keep exact definitions,
current World/resource checks and publication unchanged. Full Foundation/R9/R10/R12 remain active.

##### Implementation And Initial Qualification

The existing admitted Native entry returns its complete `ComponentValueCost` instead of discarding
it. Scene consumes that cost under its actual remaining aggregate limits. Accepted values execute
one Native callback envelope; rejected Native values use cost classification only to preserve the
complete charge and aggregate diagnostic contract. Caller admission errors bypass classification
and return immediately. Source/default admission retains its existing unit result and semantics.
No new walker, summary owner or cache is introduced. Production all-feature Reflect/Scene compile.

All-feature Reflect/Scene/Tooling libraries pass 1,244/1,244 without skips at
`466371ef-aa07-4137-95e9-c5523cf8dc04`, 70.923 seconds after a 1m44s debug build. New Scene controls
check one accepted Native work envelope, complete cost after early depth/Authored failure with
unvisited siblings, subsequent-record allowance, node/byte diagnostic precedence, zero-byte Null
admission and exact refusal identity in both Native and failure-classification callbacks. The
wide-optional scratch oracle removes its obsolete success-path cost scan, retaining index refusal
before allocation and zero-codec assertions. No production or test failure occurs in this run.

Public Reflect native-cost/explicit/frozen controls pass 14/14 at
`91a76783-5d3b-4403-952a-c9197403b6a9`, 0.728 seconds, and selected public Scene workflows pass
74/74 at `21958154-8edb-4ef2-8853-c1b87e96e395`, 3.379 seconds, without skips. Raw and frozen
Native values return the same complete 15-node/89-byte fixture cost, strict limits still reject
and every actual callback boundary preserves the original non-Clone refusal object. Source
cost accepts the Authored-reference fixture while Native admission rejects it. Independent
implementation and final test review find no P1/P2; both scoped agents are closed. Complete release
game/library/metrics/span/probe/native qualification passes 148/148 without skips at
`becaacb3-a0e5-488e-b930-e756009d455e`, 64.198 seconds after a 9m31s serial build. Original
baseline and authored 64/4K/16K travel, 27 retained-input GPU replays, both observer-panic cleanup
controls and the 16K CPU record probe remain.

The same after executable then completes four serial balanced 16K processes, twelve lifetimes
and 36 replays. The [paired report](../benchmarks/2026-11-14-development-runtime-costs.md#single-native-value-admission)
records flat-admission wall medians 384.536/334.978 ms, Native decoding/preflight
788.297/711.898 ms and unbound preparation 303.411/280.802 ms before/after. Overlapping cumulative
envelopes are not additive CPU costs. Compiler semantics, defaults and source decoding are
unchanged; their variation is not credited to this operation. Process walls
18.715/18.648/18.477/19.139 seconds are a finite observation without statistical, CPU/GPU-duration,
physical-memory or numerical frame-tail claims. Source/compiler/lock/observation controls and
phase call counts remain. Summary exports preserve complete phase/source/acceptance/cleanup/
replay facts and original raw-report hashes; complete raw programs/reports remain outside target.
The independently locked Scene consumer passes 157/157 with one existing manual timing case
skipped at `f044f58d-f993-4eea-8564-a566a2e80ab2`, 19.674 seconds after a 4m57s debug build.
Owning all-target/all-feature correctness Clippy, feature-free Reflect/Scene production compilation
and root formatting pass; pre-existing warnings remain. New document/report links, report hashes
and Git whitespace pass. No dependency/lock, source-format, numerical capacity, UI/Dialogue or
remote-publication changes accompany this unit. All owned Cargo/native measurement processes
are terminal and both scoped agents are closed.

This closes the selected receiving success-path consolidation. A cost is a value result from the
existing check, not an independent lifecycle or generic certificate. It does not authorize a
different receiving definition or mutable World state. The next reuse decision should start from
the existing immutable `CompiledSceneComponent`, final cost/depth facts and project definition
witness. Cover every actual constructor, native-preparation copy, direct data value and processed
admission before using retained facts; a bare input reference or trusted flag is insufficient.
Keep current World, namespace, product and publication checks separate from those immutable
facts. Source decoding, remaining traversal costs and full Foundation/R9/R10/R12 remain open;
no physical-memory or numerical runtime-tail qualification follows from these wall envelopes.

#### Immutable Compiled Value Measurements

Status: implemented at `9583ad7` and qualified on Windows/Rust1.95 below. Retain exact
logical cost and maximum root-at-one depth in the existing immutable `CompiledSceneComponent`.
The original Native walker measures both while establishing its Native reference requirement;
these plain value facts establish neither receiving schema semantics nor resource/World authority.
There is no separately owning validation receipt, generic cache, serialized certificate or public
unchecked constructor. Extend the original checked result, not a second traversal mechanism.

The concrete sealer collects measurements during its complete flat validation in the same sorted
entity/component order used to construct records. Admit the finite temporary measurement storage
before allocation and require complete correspondence before publishing. Direct data publication
uses measurements from its own Native check. Native preparation preserves the facts with the
unchanged private payload. Independent cooked claims establish their own measurements after their
actual decoding/admission; source copies cannot attach stale facts to changed values.

The shared receiving kernel may consume the stored complete cost and Native/depth fact only when
that exact immutable record fits the remaining receiving node/byte and depth limits. A stricter
or exhausted limit uses the original walker and failure classification, preserving observed
diagnostics and whole-record charging. Required fields, numeric domains, explicit defaults,
undeclared references, actual resources, local namespace and receiving World/publication checks
remain. This does not implement schema/witness-wide pure validation skipping.

Use `value_cost` for the original root retained-fee model and immutable data-group size checks
instead of rescanning the value. Each independent owner still pays the complete logical fee plus
actual record/measurement metadata. Do not deduplicate by storage identity. Validate every actual
constructor, direct data replacement/removal, native-preparation copy, raw/frozen nested values,
different record orders, stricter receiving policy and independently cooked input. Retain original
public/independent workflows, release game/native trajectory and balanced frozen-program cost
comparison. Full Foundation/R9/R10/R12 and broader definition reuse remain active.

##### Implementation And Qualification

Reflect's existing Native depth/reference walker now returns complete cost and observed maximum
depth together. The concrete Scene sealer retains those facts with each immutable component in
its existing sorted record order; its finite temporary measurement storage is admitted before
allocation. Preparation copies the unchanged facts. Direct data mutation establishes new facts
through the original Native check. Stored data groups and the root retained-fee model reuse them
without another payload walk, retaining complete logical fees and actual record metadata.
Independent cooked decoding/readmission measures its own values. No definition witness, public
unchecked constructor, generic cache or separate lifecycle owner is introduced.

All-feature Reflect/Scene production libraries compile. Complete Reflect/Scene/Tooling owner
libraries pass 1,246/1,246 without skips at `f4ff32e9-b864-4398-8315-5fa44548a31f`, 69.385
seconds. Public native-cost, explicit/frozen and Scene binding/export/reference/locality workflows
pass 87/87 without skips at `4be37942-0a3b-4acf-949b-f428a8c95aa9`, 3.940 seconds, including
all four new compiled-measurement cases. They preserve raw/frozen values through sorting,
preparation and final-source release, exact/stricter aggregate diagnostics before codecs, changed
schema/default refusal, direct replacement, cook reference rewriting and independently modified
cooked input. Root project/runtime content, retained fees and startup/replacement tests pass
162/162 at `a8dd6186-9f02-4c54-b2eb-9e957b18faa9` with 1,383 outside the filter, 13.356
seconds. This checks complete fees and last-owner
release after replacing recursive fee calculation with stored cost.

The first public command repeats `--all-features` and is rejected before compilation; the
corrected invocation produces the passing public result above. No production or test assertion
failure occurs in these runs. Independent production and bounded final test review find no P1/P2;
all three scoped agents are closed. Complete release game/library/metrics/span/probe/native
qualification passes 148/148 without skips at `17831171-2490-434c-aa49-e6871884fc38`, 68.194
seconds after a 10m07s serial build. It retains original baseline and authored 64/4K/16K travel,
27 original-input GPU replays, both observer-panic cleanup controls and the complete 16K CPU
record probe. Four further serial balanced before/after/after/before 16K processes complete twelve
lifetimes and 36 GPU replays, with original source, compiler, phase-call and cleanup controls.
The [paired report](../benchmarks/2026-11-14-development-runtime-costs.md#immutable-compiled-value-measurements)
records cumulative Native decoding/preflight wall medians 746.259/590.254 ms and unbound
preparation 295.399/230.450 ms before/after. The envelopes overlap and savings are not additive.
Original flat admission still walks the value while measuring depth; source decode, defaults,
compiler semantics and expansion are not optimized or credited for observed variation. Before
process drift and correlated lifetimes prevent a statistical/general process claim. These are
neither exclusive CPU/GPU nor physical-memory, scanout, frame-P99 or numerical tail-budget results.

The independently locked Scene consumer passes 157/157 with one existing manual timing case
skipped at `12c34905-e116-4ae4-bdc1-711eca70c97d`, 18.979 seconds after a 6m54s debug build.
Owning all-target/all-feature and affected root-library correctness Clippy pass; existing warnings
remain. Feature-free Reflect/Scene libraries compile. Root and sample formatting pass. The frozen
program receipt's 147 source/control hashes still match the qualified tree, and all 15 paused
UI/Dialogue paths retain their original bytes. Finite summary exports omit only the four prior
raw per-frame arrays and preserve complete phase/source/acceptance/cleanup/replay facts,
distributions and original complete-report hashes. Added document paths/fragments, summary
digests, complete acceptance facts and Git whitespace checks pass. No dependency/lock, source-format, numerical
capacity, UI/Dialogue or remote-publication changes belong to this unit.

This closes immutable compiled-value measurement and its selected complete Windows consumer
scope. The corresponding pinned Godot/Bevy paths motivate construction/change-boundary checks,
not repeated generic scans of unchanged internal data. Future work must identify the specific
invariant and mutation before adding validation. Remaining reference traversal, source decoding,
broader receiving-definition reuse, larger named workloads and numerical runtime-tail/resource
budgets require their own attribution. Full Foundation/R9/R10/R12 and foreign execution remain
open; the finite wall comparison is not their completion.

#### Same-Operation Native Codec Admission

Status: implemented at `6128594` and qualified on Windows/Rust1.95 below. The receiving
Scene kernel fully checks explicit defaults, aggregate limits, Native references, field semantics,
assets, local references and current flat definitions before its first codec. It then invokes
Reflect's standalone native preflight, which selects defaults and repeats Native and field
admission for the same unchanged value and registry. Remove that second pass within this exact
operation. Existing processed Runtime preparation has already checked its exact immutable
definition selection before entering the same dispatch loop.

Keep Reflect's standalone public preflight complete for arbitrary dynamic input. Its documented
default selection, materialization/Native and field checks must remain. Add only an engine-internal
dispatch into the original registry binding for a value whose complete receiving admission just
succeeded. The shared Scene loop uses it after all-record validation, including the existing
stricter-policy and different-definition paths. Do not add a caller-controlled skip flag, a public
unchecked constructor, another stored witness, global cache or new lifecycle owner. Broader
cross-preparation definition reuse stays separate.

The public decode context allows a codec to change the shared materialization limits. Capture
the receiving limits at entry and compare them before dispatch. Unchanged limits can reuse the
just-completed checks. If an earlier codec changes them, use the original standalone preflight
for the later value so stricter-policy refusal and diagnostic precedence remain. This is a
constant-time check of actual mutable policy, not a new payload scan on unchanged input.

The original codec still constructs and checks its domain value, resolves actual assets through
the original decode context and returns its bound `PreparedComponent`. Required components,
native claims, lifecycle hooks, target World, resource baselines, publication and retirement keep
their existing owners. A later invalid record must still prevent every earlier codec; domain
codec failure must still preserve the World and allow an ordinary retry. Independent cooked
claims obtain their existing complete admission before any Runtime reuse.

Qualify raw/frozen, source/native and cooked/readmitted public workflows, concrete codec refusal
and standalone default/Native refusal. Reuse existing exact/stricter limits, receiving-definition,
asset/reference, lifecycle and independent consumer controls rather than invent another matrix.
Retain the original release game, source removal, accepted GPU replay and cleanup trajectory.
Compare balanced frozen programs against the qualified immutable-measurement executable with
the same source/compiler/observation controls. Report overlapping wall envelopes and unchanged
phases honestly. Full Foundation/R9/R10/R12 remains active; Editor/UI/Dialogue stays paused.

##### Implementation And Qualification

Reflect's internal dispatch selects the same original binding, invokes its codec with the same
context and binds the resulting `PreparedComponent`. The standalone public preflight is unchanged.
Scene captures its admitted limits once, completes the original all-record receiving check and
uses internal dispatch only while those limits remain equal. A codec's changed limits route
subsequent records through the original preflight without replacing the entry baseline. No new
stored identity, flag selected by callers, retained allocation or lifecycle owner is introduced.

Production all-feature Reflect/Scene libraries compile. The initial owner-library selection passes
1,246/1,246 without skips at `4b67287b-89f5-48cd-8827-d368ad311f5c`, 70.880 seconds, before
the mutable-policy guard is added. Independent source review confirms the guard and exact
direct/Source/Runtime paths. It also catches a new test's `unwrap_err` requirement for
`PreparedComponent: Debug`; error extraction is corrected without changing the production type.
The in-flight compilation reports that original fixture error; the next stable-source invocation
passes all 1,353/1,353 owner/public tests without skips at `de175f8b-6ace-423f-9194-a2dbb97a07ba`,
75.043 seconds. This includes all 1,246 owner cases and 107 selected public controls, including
the three new native-decoding workflows, standalone defaults/Native refusals, strict receiving
limits, changed Schema/defaults, assets/references, export and original data publication.

The new real-codec policy case tightens shared limits in the first sorted entity, requires the
later value to refuse before its codec, compares its observed/maximum diagnostic against the
standalone public preflight and preserves the original World/identity state. Source raw/frozen
and independently cooked/readmitted values retain their complete defaults, context and opaque
allocation identity through prepare and original-owner retirement. Concrete codec/domain refusal
publishes no Scene and allows an ordinary retry under the same occurrence. Preparation checks
actual population changes rather than assuming the World's internal resource entity count.
Both scoped agents are closed. Complete release game/library/metrics/span/probe/native
qualification passes 148/148 without skips at `fc15dda2-6df9-4ff1-bc62-048c82ef638e`, 62.612
seconds after a 9m51s serial build. Original baseline and authored 64/4K/16K travel, 27 GPU
replays, both observer-panic cleanup controls and the full 16K CPU record probe remain.
Four balanced serial 16K processes then complete twelve lifetimes and 36 replays. The
[paired report](../benchmarks/2026-11-14-development-runtime-costs.md#same-operation-native-codec-admission)
records cumulative Native decoding/preflight wall medians 600.139/474.344 ms and unbound
preparation 240.167/189.013 ms before/after. The envelopes overlap and savings are not additive
CPU costs. Source decode, compiler/default work and flat sealing are unchanged; their observed
variation is not credited to this operation. Correlated lifetimes and uncontrolled host/driver/
cache/clock state prevent statistical or general process/CPU/GPU/physical-memory/frame-tail
claims.

The independently locked Scene consumer passes 157/157 with one existing manual timing case
skipped at `8c70103d-cdb8-4836-8d2f-e92207afa60f`, 18.858 seconds after a 5m11s debug build.
Owning all-target/all-feature correctness Clippy and feature-free Reflect/Scene library compilation
pass; pre-existing warnings remain. Root and sample formatting pass. The 148 frozen source/control
hashes match the qualified tree and all 15 paused UI/Dialogue paths remain byte-identical. Finite
summary exports retain complete phase/source/acceptance/cleanup/replay facts, distributions and
original raw-report digests, omitting only the same four raw per-frame arrays as prior comparisons.
Added local document paths/fragments, summary/raw report digests, complete acceptance facts and
Git whitespace checks pass.
No dependency/lock, source-format, numerical capacity or remote-publication change belongs to
this unit.

This closes same-operation codec admission and the selected complete Windows consumer scope.
It removes locally repeated work without making arbitrary dynamic input trusted or adding
cross-stage identity/state merely to reuse a check that just ran. Broader receiving-definition
reuse, remaining source/reference traversal, larger named workloads, numerical runtime budgets,
foreign execution and full Foundation/R9/R10/R12 remain open.

#### Known-Tag Scalar Payload Decoding

Status: implemented at `3ecb373` and qualified on Windows/Rust1.95 below. The final
same-operation codec comparison still observes about 716 ms of source payload decoding and
337 ms of separate shape preflight per complete 16K lifetime. These cumulative wall envelopes
are not exclusive costs. The existing known-tag List/Map decoder streams containers, but scalar
values still pass through an untagged payload enum that buffers serde Content and attempts
unrelated alternatives even after the outer value tag is known.

For known Bool/I64/U64/F64/String tags, interpret serde's actual scalar callback once and feed
the existing final tag/payload conversion. Preserve integer-versus-float token classification,
signed/unsigned ranges and casts, finite float rejection and signed-zero bits. In particular,
do not replace deserialize-any classification with target-specific numeric parsing: RON's numeric
syntax and JSON's bare negative zero differ. Preserve UTF-8 string/byte handling and reject chars,
options, wrong kinds and nonfinite values as before. Unknown tags, value-before-type inputs,
EntityRef and Null retain the original untagged fallback. Existing List/Map and nested value
visitors, duplicates, field order, serialization and parent-buffered forms remain.

This changes neither source format nor parser ownership, structural preflight, mandatory header
gates, receiving semantics, limits, retention or publication. Do not build another parser,
general validation receipt or new decoder framework. Differential tests compare known-tag
inputs with the original value-before-tag fallback, including numeric/byte edge cases and
buffered enum parents. Retain the existing complete wire/format controls and public consumers,
then the unchanged release game/native/GPU/cleanup workflow and balanced original-program
comparison against the qualified codec-reuse executable. Full Foundation/R9/R10/R12, broader
source/preflight work, numerical budgets and foreign execution remain open.

##### Implementation And Qualification

Known Bool/I64/U64/F64/String payloads now use one scalar visitor and the existing final
tag/payload conversion. Signed values remain signed; unsigned values first use the original
representable-I64 classification and otherwise retain U64. Finite F64 construction and the
existing final integer casts preserve the wire policy. Strings consume serde's actual string or
UTF-8 byte callbacks without an intermediate untagged Content buffer. Null, EntityRef, unknown
tags and value-before-type retain the old fallback. Existing container paths are unchanged.

The initial original wire selection passes 28/28 with 230 outside the filter at
`900ce0a4-8d37-4a8b-b354-2c0dcdf36196`, 0.859 seconds. Four new differential controls then
expose a real compatibility regression: serde's default `visit_char` forwards to `visit_str`,
so known-tag String accepts a RON character that the old fallback rejects. The run passes 30/32
with two character controls failing at `45822446-7dfb-4866-94c8-1daa29fb8c0b`, 1.004 seconds.
The visitor now explicitly rejects characters without broadening String syntax. Source review
checks the actual locked RON0.8.1/Serde1.0.228 behavior; an earlier inspection of another locally
installed RON version is not used as contract or qualification evidence. Review finds no
remaining P1/P2 after that correction; both scoped agents are closed.

Complete Reflect/Scene libraries and selected public wire, Schema/authoring bundle, property,
format, migration, compiled-measurement, native-codec, export and Runtime workflows pass
1,195/1,195 without skips at `ed903f02-0cef-4f69-9836-c4434aa35ce3`, 77.073 seconds after a
2m52s debug build. All 32 wire controls pass, including differential numeric classification and
exact float bits, UTF-8 bytes/char refusal, options/newtypes/suffix rejection under the selected
RON grammar and internal/adjacent parent buffering in both field orders. This retains behavior,
not an exact syntax-error-message or parser-position guarantee. Complete release game/library/
metrics/span/probe/native qualification passes 148/148 without skips at
`4a5726ff-79d7-4cd4-a0d2-48ab080999a1`, 64.939 seconds after a 10m08s serial build. Original
baseline and authored 64/4K/16K travel, 27 original-input GPU replays, both observer-panic cleanup
controls and the full 16K CPU record probe remain.

Four balanced serial 16K processes complete twelve lifetimes and 36 replays. The
[paired report](../benchmarks/2026-11-14-development-runtime-costs.md#known-tag-scalar-payload-decoding)
records payload-decode wall medians 713.247/297.126 ms before/after, 58.34% lower in this finite
sample. Structure and expansion include nested Prefab decoding; their lower observed values
overlap that work and cannot be added as exclusive CPU savings. Wire preflight increases from
332.650 to 383.826 ms despite no implementation change there. Other unchanged phases vary and
receive no optimization credit. First-after process wall 18.116 seconds is slightly above the
last-before 18.059 seconds, so whole-process ordering is not consistent across all observations.
Correlated lifetimes and uncontrolled host/driver/cache/clock state prevent statistical, general
process, CPU/GPU, physical-memory, scanout or numerical frame-tail claims. The independently
locked Scene consumer passes 157/157 with one existing manual timing case skipped at
`ba8b1ef6-f225-4c30-b2d0-a397d56a3287`, 20.975 seconds after a 4m59s debug build. Final owning
all-target/all-feature correctness Clippy, feature-free Reflect/Scene compilation and root/sample
formatting pass; pre-existing warnings remain. The qualified frozen receipt's 151 source/control
hashes match the implementation, and all 15 paused UI/Dialogue paths remain byte-identical.
Finite summaries retain complete source/phase/acceptance/cleanup/replay facts, distributions and
original raw-report digests, omitting only the four prior raw per-frame arrays. Added document
paths/fragments, summary/raw-report hashes, complete acceptance facts and Git whitespace pass.
No dependency/lock,
source-format, numerical-capacity or remote-publication change belongs to this unit.

This closes known-tag scalar decoding and the selected complete Windows consumer scope.
Structural file gates, header interpretation and pure semantic validation retain their own
responsibilities; a new pass still requires a concrete changed invariant or receiving policy.
Wire/header consolidation, remaining source/reference costs, larger named workloads, numerical
runtime/resource budgets, foreign execution and full Foundation/R9/R10/R12 remain open.

#### Borrowed Keys In Structural Preflight

Status: implemented at `9bd3a54` and qualified on Windows/Rust1.95 below. Structural file
preflight remains about 333/384 ms before/after the separate scalar change. It validates actual
wire nodes, depth, strings, container sizes and duplicate decoded keys before header/domain
construction. That role is distinct from later semantic admission. The current duplicate-key
set nevertheless copies every input-lifetime borrowed key into String, even though serde's
borrow guarantees the key remains valid until the parse finishes.

Store keys as private Cow values in the existing per-map set. Borrow only `visit_borrowed_str`;
copy transient `visit_str` after original key budgets, and move already owned `visit_string`
after those same checks. Equality/order must compare decoded string contents across storage
forms. The set and every borrow remain within the original preflight call. Keep actual key-node,
per-string/aggregate byte, map-item, duplicate and nested-scope behavior, failure ordering and
syntax-adapter allocation limits. Escaped text may already allocate in serde; this operation
does not claim to eliminate that allocation or the set's own bounded nodes.

The existing allocation test currently requires an accepted long borrowed key to allocate its
full byte length. Replace that obsolete implementation expectation with no-copy acceptance plus
actual duplicate detection and all original limit-before-copy negative controls. Add public
borrowed/owned/transient and nested/escaped-key compatibility where it discriminates lifetime
or identity errors. Do not add another parser, validation certificate, key interner, generic
scratch owner or format-specific fast scanner. Header compatibility still precedes domain
payload construction even when payload appears first; no check is deleted or ceiling raised.

Qualify Core and affected Reflect/Scene file/format paths, independent consumers and the complete
unchanged original game/native/GPU/cleanup trajectory. Compare frozen programs against the
qualified scalar-decoding executable with original sources, hooks and phase counts. Report the
key-copy allocation observation separately from full process memory and overlapping wall costs.
Header fusion, remaining reference/source costs, numerical budgets, foreign execution and full
Foundation/R9/R10/R12 remain open. Paused Editor/UI/Dialogue work stays excluded.

##### Implementation And Qualification

The existing key seed/visitor now returns Cow. `visit_borrowed_str` checks the original string
budget and retains the input borrow; transient strings are copied only after the same checks,
and owned strings move. The original BTreeSet compares decoded contents across representations
and drops all borrowed/owned keys at the same per-map boundary. It does not intern keys or retain
them after preflight. Original key-node, item, duplicate, string/depth and aggregate ordering
remain; domain parsing and header gates are unchanged.

Core format/allocation controls pass 13/13 without skips at
`15b2cb45-d8b3-499d-bcb1-2fa3e39bff9f`, 0.866 seconds after a 19.10-second debug build.
Accepted 4,096-byte borrowed keys and their actual duplicate now avoid an equally large copy,
while original 1MiB per-string/aggregate refusals still precede Nara copies. This observes the
largest allocator request within that finite call, not all process memory or the set's own
allocation. Owned/escaped keys retain exact charging and decoded duplicate behavior.

Complete Core/Reflect/Scene libraries and selected public allocation, format, catalog/bundle,
authored-wire, compiled-measurement, native-codec and receiving-locality controls pass
1,148/1,148 without skips at `962e10e9-bcda-4d70-9012-8ed10dc56f50`, 65.467 seconds after a
2m49s debug build. Three added public cases exercise borrowed/owned/transient callbacks with a
reused scratch buffer, decoded non-ASCII/escaped equality without Unicode normalization,
nested duplicate scopes, cumulative strings and container/budget failure precedence. Existing
Scene/Prefab/Patch JSON/RON paths and header-before-payload controls retain their original
assertions. Independent production review finds no P1/P2; both scoped agents are closed. No
test failure occurs in these runs. Complete release game/library/metrics/span/probe/native
qualification passes 148/148 without skips at `51cedbd5-91ef-40cb-9bf1-c0598c6b9553`, 57.311
seconds after a 10m41s serial build. Baseline and authored 64/4K/16K travel, 27 GPU replays,
both observer-panic cleanup controls and the full 16K CPU record probe remain.

Four balanced serial 16K processes then complete twelve lifetimes and 36 replays. The
[paired report](../benchmarks/2026-11-14-development-runtime-costs.md#borrowed-keys-in-structural-preflight)
records structural-preflight wall medians 338.446/206.105 ms before/after, 39.10% lower in this
finite observation. Source payload decode is nearly equal, while nested structure/expansion
includes this preflight work and cannot be added as exclusive CPU savings. Unchanged header,
compiler/default/reference and native-preparation variations receive no credit. Second-after
process wall 16.037 seconds is slightly above final-before 16.014 seconds; whole-process ordering
is inconsistent. Correlated lifetimes and uncontrolled host/driver/cache/clock state prevent
statistical, general process, CPU/GPU, physical-memory, scanout or numerical frame-tail claims.
The independently locked Scene consumer passes 157/157 with one existing manual timing case
skipped at `e1c85f65-dc00-43c3-845a-124c9a04bdd0`, 18.952 seconds after a 4m58s debug build.
Owning all-target/all-feature correctness Clippy, feature-free Core/Reflect/Scene compilation and
root/sample formatting pass; pre-existing warnings remain. The frozen receipt's 154 selected
source/control hashes match the implementation and all 15 paused UI/Dialogue paths remain
byte-identical. The additional Core before-revision blob hashes are explicitly separate from the
preceding selected working-source receipt; they match both the qualified before implementation
and its frozen build base. Finite summaries retain complete phase/source/acceptance/cleanup/
replay facts, distributions and original raw-report digests, omitting only the four prior raw
per-frame arrays. Added document paths/fragments, summary/raw report hashes, complete acceptance
facts and Git whitespace checks pass. No dependency/lock, source-format, numerical-capacity or remote-publication
change belongs to this unit.

This closes borrowed-key storage and the selected complete Windows consumer scope. It reduces
temporary copying within an existing required pass; it does not claim another engine uses this
same preflight contract, remove domain checks, or qualify whole-process memory. Header fusion,
remaining reference/source work, larger named workloads, numerical runtime/resource budgets,
foreign execution and full Foundation/R9/R10/R12 remain open.

#### Mature-Engine Module Timing Comparison

Before selecting another validation optimization, establish local reference magnitudes with
ordinary public Nara, Bevy and Godot consumers. Use 64, 4,096 and 16,384 records containing the
same four integer fields in one scene-owned collection. Measure warm file reading, native scene
decode, resolve/compile where separately exposed, and instantiation into fresh runtime storage.
Record exact versions, source bytes, representation, entity counts, raw samples and full resulting
data checks outside the timed phases. Godot's packed Variant and native TileMap storage are
separate representation controls; they are not generic-scene parity. No render, steady-frame or
whole-engine speed ranking follows from this CPU-only comparison.

Defer structural/header scan fusion: preserving current shape-first errors and strict envelope
semantics requires extra parser state, and the existing cumulative header envelope is only about
41 ms across 18 file decodes. The reference measurements should determine whether that work is
worth its complexity. The previous complete game's cumulative lifetime envelopes remain separate
from these single-file operations. This comparison does not change production validation or
resume Editor/UI/Dialogue work.

The ordinary public [comparison consumers](../benchmarks/fixtures/runtime_scene_comparison/README.md)
and [local report](../benchmarks/2026-11-17-runtime-module-comparison.md) complete this bounded
reference unit on Windows. Nine final serial processes produce 72 case records across the three
populations, with 648 retained iterations and complete data checks. Both Rust native row types
use the same 16-byte i32/u32 fields. Official Bevy World RON and typed Scene are separate paths;
Godot's actual native cell setter is separate from shared metadata instantiation. Nara's lock
retains root versions/sources, and original file limits/refusals are reported explicitly.

At 16K, Nara JSON/compact-JSON decode medians are 37.013/26.331 ms, followed by roughly 18 ms
compile and 8 ms prepare/commit; Bevy World RON decodes in 25.897 ms and writes already decoded
native data in 0.390 ms. Godot generic/packed/native-tile loads are 71.194/15.741/18.683 ms,
with very different encoded bytes and instantiation ownership. These are operation magnitudes,
not engine-speed ratios. Start with a provisional 30–40 ms comparable warm source-to-native
target and a 5–10 ms compile-plus-preparation target, then separately measure pure native commit
and real owner occupancy. The report keeps these proposals distinct from measured acceptance.
Its [runtime target application](../benchmarks/2026-11-17-runtime-module-comparison.md#applying-the-targets-to-runtime-development)
separates the frozen cross-engine baseline from later Nara improvements and orders the next
operation-specific controls: remaining value admission, entity/transform work, changed tiles
and steady gameplay/rendering. Each new control measures its completed work and unchanged-data
case; no blanket per-module millisecond limit or recurring full validation pass is introduced.
No production validator, source format, cache, native owner or dependency changes in this unit.
Post-decode attribution and the actual domain/processed representation take priority over header
fusion. Full Foundation/R9/R10/R12, foreign execution and numerical frame-tail budgets remain open.

#### Joint Effective Reference Interpretation

Status: implemented at `6295ccb1cfd8bee7be963d34d9b11211d572cba2` and qualified on Windows/Rust
1.95.0 below. The preceding effective Scene kernel collected assets and then planned entity
references from the same unchanged value and Schema. Both entered the same complete raw-reference
scan and declared-shape interpreter. Attribution below justifies sharing that interpretation.
Feature-gated Scene spans reuse the existing binary-local observer in the small public collection
consumer and complete game; default production has no subscriber.

The selected replacement is one finite joint reference interpretation returning the asset
inventory and entity plan to their existing Scene caller. Asset existence/product checks and
entity namespace closure remain at their original owners and in their current order. No retained
cache, detached proof, second source owner, receiving-context reuse or new public author model
is selected. Independently used asset/entity operations remain complete for their named use.

Preserve hidden/undeclared entity refusal, actual asset decoding, admitted Schema/field shapes,
strict versions, optional and nested paths, opaque/frozen defaults, and exact callback error
custody. Admit actual shared traversal/output allocations before work; do not retain a redundant
scan solely to reproduce historical work counters. Compare positive and refused joint results
with existing independent interpretation, including namespace/asset failures, tighter receivers
and allocation/work refusal. Qualification must include public Reflect/Scene consumers and the
original authored 64/4K/16K game/native/GPU-replay/cleanup workflow. Use a frozen before program
and paired complete-game observations before claiming a speedup; isolated collection timings
alone do not qualify engine-wide or frame-tail improvement. Full Foundation remains active.

The first attributed original 16K compact case completes all 12 warmup/sample iterations with
exact data checks and no unfinished observer spans. Across compile and prepare, asset/entity
reference phases each run 24 times and accumulate 86.787/86.823 ms. Original flat admission is
133.341 ms over 12 calls and unbound preparation is 100.880 ms over 12 calls. This locates shared
interpretation cost; spans are nested wall time and cannot be added to their parent envelopes.
The separate staged value-reference planner remains 43.062 ms over 12 calls and is outside this
replacement. A frozen original complete 16K program independently passes three lifetimes, nine
GPU replays and physical copied-source cleanup before modification; its actual process wall is
18.227 seconds. The existing executable/source receipt is retained for the paired comparison.

The joint implementation reuses the complete asset-mode interpreter and retains only its
already owned entity-leaf paths in operation-local pending scratch. Assets are decoded once.
The original Scene resolver/product checks precede pending-plan output allocation and local
namespace closure. Both standalone single-kind collectors keep their complete contracts.
Production code and test review find no P1/P2. Initial production compilation and 15 focused
Reflect tests pass. The first complete owner/public run has 1,143 passes and five fixture/fee
failures: the old working-budget oracle includes the removed second traversal, new World counts
mistakenly include Resource entities, one narrowing fixture changes an EntityRef shape without
its capability, and the source-closure case expects flat-asset diagnostics before the earlier
compiler reference closure. Correct the fixtures to actual public boundaries without changing
production semantics or weakening exact/one-short limits, source/content checks and no-publication
assertions. Five corrected controls pass at `cefd4d09-aae1-4bc9-913f-171afd883d90`; the complete
owner/public rerun passes 1,148/1,148 without skips at
`fd2e6d8f-23f9-417d-9430-e4008de98645`, 87.844 seconds. Corrected-test review finds no remaining
P1/P2 and confirms that resource initialization is distinct from premature Scene publication.

The original complete release game/library/metric/span/probe/native selection passes 148/148
without skips at `ea33b63a-7f3d-4366-8f33-dc37a44c94d9`, 64.448 seconds after a serial 10m32s
build. It preserves actual baseline and 64/4K/16K travel, 27 original-input GPU replays, both
observer-panic physical cleanup controls and the complete 16K record/cook probe. The added three
effective-admission spans have matching original call counts on owner/worker paths. Paired
before/after measurements complete four serial processes in before/after/after/before order,
each with three full 16K lifetimes and nine GPU replays. Flat-admission wall medians across six
lifetimes/version are 365.118/242.532 ms before/after; unbound preparation is 195.573/120.923 ms.
Component materialization and staged value-reference planning remain nearly equal. The changed
work is not the earlier 120ms staged planner, source parser or render path. Complete process
walls are 18.227/15.989/15.415/17.042 seconds, with both after groups lower in this observation.
These uncontrolled, correlated samples establish no general whole-process, exclusive CPU/GPU,
physical-memory or numerical frame-tail budget. New after-only subspans add observation overhead;
operation-local pending leaf storage is not a memory-reduction claim.

The independently locked Scene consumer passes 157/157 with one original manual timing case
skipped at `bd7a95f1-4231-46f6-a4a5-10edd4f2a5b2`, 19.591 seconds after a serial 6m14s build.
Owning all-target/all-feature correctness Clippy and feature-free Reflect/Scene production
compilation pass; existing warnings remain. Root, sample and scoped small-consumer format
checks pass. The final small observed program also passes all nine population/encoding cases
with complete data checks and no unfinished spans. Its 16K compact case has 24 joint asset
interpretations totalling 84.155 ms and 24 entity finalizations totalling 0.008 ms; flat/unbound
totals are 88.449/57.928 ms over 12 calls. These unpaired diagnostic numbers confirm the relocated
work; the frozen complete-game comparison above remains the performance evidence.

Eight scoped agents cover two attribution questions, Reflect implementation, public Scene
controls, production/test/correction reviews and numeric report review; all are closed with no
remaining P1/P2. Added document targets/headings, raw/summary/program/source digests, TOML,
Python syntax and whitespace checks pass. All 15 paused UI/Dialogue paths remain byte-identical,
the reference checkouts remain unchanged, and no remote publication occurs. This closes the
joint interpretation unit and its selected Windows consumers, not full Foundation/R9/R10/R12,
remaining source/representation work, numerical runtime-tail budgets or foreign execution.

Continue from the unchanged original-value admission and staged reference-plan envelopes in
this same complete game. Establish which value/Schema facts each owns before selecting another
joint pass or domain/processed Tile representation. Do not convert the observed saved walk into
a universal cache, omit current receiving checks, or reopen header fusion without new attribution.

#### Reference Paths From Active Container Cursors

Status: implemented at `26c12dc8c991d671e8206f56d96de500b4bab07e` and qualified on Windows/Rust
1.95.0 below. The preceding reference interpreter maintained a borrowed full path plus an Exit
frame for every ordinary Map/List child, although the raw
scan needs an owned path only when it reaches an EntityReference. Compare a direct iterative
visitor whose stack holds only active container iterators and their selected child segments.
Reconstruct the exact root-prefix/ancestor path when an actual reference needs declaration lookup
or a diagnostic. This changes traversal mechanics, not which values are visited or admitted.

Keep full raw-tail observation after the first hidden reference, decoded Map ordering, exact
list indices/root prefixes, frozen defaults, declared-path equality, UTF-8 byte accounting,
nodes/bytes/depth limits and caller error identity. Actual stack/path allocation still requires
admission before allocation; removed scratch work does not retain historical fees. Scalar and
empty-container cases should not need a path-maintenance stack. No metadata cache, new source
certificate, Schema exemption or receiving-context reuse is selected.

Use the existing joint-reference implementation as the frozen control. Qualify public standalone
and joint visitors, nested/late hidden references, exact/tighter limits, callback refusal and the
original complete game/native consumers. Compare unchanged full source/receiving paths and
complete-game envelopes before retaining the experiment; an internal loop change alone is not
an improvement claim. A parallel read-only assessment of source metric ownership remains a
separate future choice and cannot silently add cached facts to this walker.

The private iterative implementation replaces Visit/Exit events and a per-node mutable path
with active Map/List cursor frames. Paths are materialized from the supplied root prefix and
selected ancestor segments only at a reference that still needs declaration lookup. The first
production check passes after correcting a syntax error caught before compilation. Existing
reference tests initially pass 22/23: the remaining test assumes root-path scratch must allocate
before any node observation. Its corrected refusal control admits the root visit and refuses
the first actual nonempty-container allocation, before children or Schema errors. No redundant
scratch is retained solely to preserve that old fee sequence.

Six new bounded cases compare exact paths/counts against an independent recursive owned-value
oracle, including merged frozen Maps/List suffixes, explicit Null, long UTF-8 paths, persistent
UUID text work, authored routes, root prefixes and depth 64/65. Every callback refusal checks
the exact caller error and no later callback. Scalar/empty cases have zero modeled allocation
debit and flat 16K scalars have bounded cursor scratch; this is not allocator/RSS evidence.
The complete focused reference cohort passes 29/29 at
`f9f6cdc0-5173-446b-b11a-f4a5810c9c0d`, 1.110 seconds, with 246 outside the filter. Independent
production and test review finds no P1/P2. A four-process small public consumer comparison
completes all population/encoding/data checks and supports continuing to the original game;
its exploratory timing is not substituted for complete native qualification. Full owner/public
qualification passes 1,154/1,154 without skips at
`33e9a2fc-0c87-45ff-a5a8-545eccdb6ae4`, 66.616 seconds after a 3m35s debug build. This includes
the original joint/standalone reference, immutable measurement, Native codec, data/native
receiving, authored route and processed readmission controls.

The complete original release game/library/metrics/span/native/16K-record selection passes
148/148 without skips at `540dd04b-3ce7-4795-94a8-4ae0a09ebe67`, 66.510 seconds after a serial
10m21s build. Original 64/4K/16K travel, 27 original-input GPU replays and both observer-panic
physical cleanup controls remain. Four frozen before/after/after/before processes add twelve
16K lifetimes and 36 GPU replays, all accepted with original copied-source/native cleanup.
Across six lifetimes/version, staged reference-plan wall medians are 119.518/70.918 ms,
flat admission 237.061/188.517 ms, and unbound preparation 127.484/83.392 ms before/after.
The changed planner is contained in component materialization/expansion, and joint interpretation
is contained in flat/receiving work; do not sum those envelopes. Unchanged parser, value-cost
admission, defaults and publication variations receive no optimization credit.

The [paired report](../benchmarks/2026-11-14-development-runtime-costs.md#reference-paths-from-active-container-cursors)
retains exact source/program hashes, full accepted phase counts, six lifetimes/version and
process variation. Process walls are 15.041/14.216/14.306/16.074 seconds. Both after groups have
lower changed envelopes and process walls in this finite observation, without controlled
host/cache/clock/driver state or independent lifetime samples. No statistical/general speedup,
exclusive CPU/GPU, physical-memory or numerical frame-tail result follows. Both programs already
have identical observer names/configuration.

The independently locked Scene consumer passes 157/157 with one original manual timing case
skipped at `626af222-a93f-41b9-b5bb-b9dde14dec58`, 20.848 seconds after a serial 6m03s debug build.
Owning all-target/all-feature correctness Clippy, feature-free Reflect/Scene library compilation,
root/sample formatting and document/whitespace checks pass; existing warnings remain. Source,
raw/summary and frozen executable hashes match the qualified implementation. Six scoped agents
cover source-metric ownership, pinned upstream iteration, cursor tests, production/test review
and numeric report review; all are closed with no remaining P1/P2. All 15 paused UI/Dialogue
paths remain byte-identical, reference checkouts remain unchanged and no remote publication
occurs. No dependency, format, public author API, Schema rule or receiving policy changes.
This closes the cursor unit and selected Windows workflows; full Foundation/R9/R10/R12,
remaining source/representation work, numeric runtime-tail budgets and foreign execution stay open.

Continue by attributing original-value and Native cost/depth walking, with their unchanged
complete value contract, against the remaining measured envelopes. Reuse cursor mechanisms
only where a complete consumer identifies a matching cost. Do not retain cross-stage source
metrics without reconciling public writes, exact data ownership and tighter receiving limits;
the investigation above deliberately defers such a cache. A typed/processed Tile comparison
remains an alternative for actual source representation cost, not an assumed consequence of
this private loop improvement.

#### Value Admission Through Active Container Cursors

Status: implemented at `240a45352e5b745923af3f33b1d6ed19367bc7e1` and qualified on Windows/Rust
1.95.0 below. Attribute the remaining complete source and Native cost/depth walks without changing
their receiving policies. The preceding budget walker
pushes a Value event for every child and pops/reinserts its parent iterator. Compare a direct
current-value loop with only active Map/List cursors, advancing each iterator in place. Scalar
and empty-container values need no traversal allocation; nonempty container scratch still needs
admission before growth. Removed scratch work does not retain obsolete allocation fees.

Preserve complete logical node/byte cost, maximum observed depth, Map key ordering/UTF-8 bytes,
frozen insertions, retained-default storage modeling, authored-route counts, Native rejection and
SourceContext's complete tail scan after finding its first contextual reference. Caller refusals
retain exact identity and stop before the refused operation. Source/native logical units,
Schema/resource validation, error ordering among value checks, defaults and World publication
stay with their original owners. No source metric cache, certificate, public format or new
validation authority is selected.

Freeze the qualified `26c12dc` complete-game binary as the control. Compare the small public
Scene consumer before paying for the full release game build; retain the private algorithm only
if the changed envelopes improve in its actual consumers. Then qualify the affected original
Reflect/Scene consumers, complete game/native workflows and independently locked Scene module.
Record paired complete-game observations and scope the result separately from the older
cross-engine single-file table. Numerical frame-tail and physical-memory claims need their own
measurements; fewer stack operations alone do not establish them.

The private implementation keeps only active Map/List iterators and processes the current value
directly. Production Reflect/Scene compilation passes. The first test compilation exposes an
old `ValueFrame` import in a frozen-default allocation oracle. Its corrected exact fee admits
one container-cursor allocation for the nonempty source Map and none for the selected scalar
authored default; complete retained-reference storage assertions remain. Native caller-refusal
controls now observe the root node before its first actual nonempty-container allocation. The
original focused cohort passes 42/42, 1.504 seconds, with 241 outside the filter. Independent
production review finds no P1/P2.

Four exploratory public-consumer processes pass all 36 population/encoding cases and complete
data checks. At 16K, pooled cumulative original-value admission medians are 34.005/16.076 ms
before/after and Native admission 38.318/23.786 ms; each case includes three warmups and nine
retained samples. These are nested CPU-consumer observations, not single-load or frame times.
The frozen exploratory control predates a runtime-inert test-module registration; exact program
hashes and prior source receipt remain in the local observation directory. These results justify
continuing to the original complete game, rather than claiming its acceptance from this control.
The new eight-case recursive-oracle cohort passes in the first full owner/public selection.
That selection passes 1,162/1,167 at `e9af5326-4e13-439c-b63d-4d3fc10b91d8`, 64.587 seconds,
without skips. All five failures reach one property-test helper's obsolete initial-stack
allocation assertion. Its correction expects the root-node observation first and retains the
same success/error comparison, every caller-refusal boundary, shape/declaration ordering and
finite ceilings. Only that test helper changes after this run; production remains unchanged.
The complete Reflect library rerun passes 283/283 without skips at
`f1d5e7fe-c3bb-4528-9e8d-7df3e262fd4b`, 8.829 seconds after a 12.64-second build. All eight
new oracle cases and the five corrected property cases pass. The other 884 owner/public cases
passed with the same production implementation in the first run and are not rerun merely for
this test-only correction. Independent test review finds no P1/P2. The original release
game/library/metrics/span/native/16K-record selection passes 148/148 without skips at
`3ae4945d-079b-4ade-a077-105e72b70a6f`, 68.382 seconds after a serial 9m10s build. Complete
64/4K/16K travel, 27 original-input GPU replays and both observer-panic physical cleanup controls
remain. Four additional frozen before/after/after/before processes complete twelve 16K lifetimes
and 36 GPU replays with exact expected phase calls and complete original cleanup.

The [paired report](../benchmarks/2026-11-14-development-runtime-costs.md#value-admission-through-active-container-cursors)
retains 161 after source/control hashes, original before/after programs and four finite summaries.
Across six lifetimes/version, cumulative original-value admission medians are 85.248/54.006 ms,
Native admission 114.366/75.490 ms, defaults 108.043/73.455 ms and flat admission
183.112/145.077 ms. Unbound preparation is 82.313/82.281 ms, without a demonstrated improvement.
Parent/child envelopes overlap; no unchanged parser/planner/publication stage receives credit.
Process walls, the initial control's different log sink, correlated lifetimes and uncontrolled
host/cache/clock/driver state remain explicit limits. No single-load, frame-P99, GPU-time,
physical-memory or universal speedup follows. The independently locked Scene consumer passes
157/157 with one original manual timing skip at `32df911f-edf0-48eb-a193-1e426f13586a`,
18.875 seconds after a serial 4m36s build. Owning all-target/all-feature correctness Clippy,
feature-free Reflect/Scene compilation, root/sample formatting and document/whitespace checks
pass; existing warnings remain. Exact source, raw/summary and frozen program hashes match the
qualified implementation. Three scoped implementation/review agents are closed with no remaining
P1/P2. All 15 paused UI/Dialogue files remain byte-identical. No dependency, format or public
receiving policy changes; no remote publication occurs. This closes the value-cursor unit,
not full Foundation/R9/R10/R12, numerical runtime-tail budgets or foreign execution.

Continue by attributing the unchanged unbound-preparation envelope through existing Native
construction, binding, allocation and identity owners before choosing another optimization.
Retain the original source/processed and typed-record comparisons as representation decisions,
not an already adopted format. Keep unchanged/single-change/full-replacement workloads separate
from file-loading targets; cross-stage raw metrics still require reconciling mutable escapes
and exact owning data rather than a cache keyed by source path.

#### Unbound Preparation Cost Attribution

Status: implemented at `aeeb79f2792635b08f71a3aa849c2b926d3bba1e` and qualified on Windows/Rust
1.95.0 below. The value-cursor comparison leaves original unbound preparation
at approximately 82 ms cumulative across six calls per lifetime. Existing identity/binding spans
are too small to explain it, and the existing native-preview span is not exercised by this game.
Locate the remaining work before changing receiving validation or introducing another cache.

Reuse the feature-gated Scene spans and original binary-local collector. Add a small fixed set
of scopes at existing boundaries: unbound preflight, receiving value validation, component
preflight, World preparation and candidate materialization. Keep early refusal, borrowed decode
context, candidate disposal, asset effects and identity admission in their original order.
Source/native/processed policies and output remain unchanged. No per-field profiler, new public
instrumentation API, source certificate or alternative lifetime owner is selected.

Qualify the observer's complete/failed collection behavior and original prepare/commit/refusal
consumers, then run the actual authored 16K game with its original travel, retained inputs,
GPU replay and physical cleanup. Report call counts and nested scopes separately from exclusive
CPU work. The added observation has its own overhead; it cannot establish a new performance
improvement against the previous collector's smaller phase set. Use a complete operation-level
measurement to choose the next Native codec, allocation or validation change.

Source tracing confirms that the stock Tilemap builds native storage during codec preflight
when Scene supplies its scratch AssetServer. Later candidate materialization normally moves and
boxes that already prepared value. Loading diagnostics also use the same preflight function:
the old `scene_decode` reports fifteen calls, while unbound preparation reports six. The collector
therefore records two fixed unbound receiving/component buckets using the actual tracing span's
ancestor chain, leaving non-unbound diagnostic calls in separate buckets. It does not infer
those calls from the ambient current span or change the production preparation interface.
The original collection test covers both uses, run isolation and its existing worker/reentrant/
retained-span refusal behavior. Production scope review finds no P1/P2; focused original
Scene prepare/public admission controls pass 64/64 with 772 outside the filter at
`0898b259-84c0-4bd3-a599-9bd0c5d41b48`, 3.742 seconds after a 53.49-second debug build. The
original release game/library/metrics/span/native/record selection passes 148/148 without skips
at `57a983b9-c093-4fe6-bf81-696c3a1e8852`, 63.608 seconds after a serial 7m23s build. Original
64/4K/16K travel, 27 retained-input GPU replays and both observer-panic cleanup controls remain.
The subsequently added explicit-parent/grandparent and `parent: None` controls pass separately
1/1 at `b940fde0-e164-4d77-a094-6be61d3a00ae`; test-only additions do not trigger another full
game run. Observer review confirms actual registry ancestry and no remaining P1/P2.

One additional frozen 16K process completes three original lifetimes, nine retained-input GPU
replays, exact old/new phase counts and complete physical cleanup. The
[attribution report](../benchmarks/2026-11-14-development-runtime-costs.md#unbound-preparation-cost-attribution)
retains exact source/program/report identities and nested cumulative medians: original unbound
81.791 ms, its preflight 81.248 ms, receiving validation 35.120 ms, component preflight
46.094 ms, World preparation 0.501 ms and candidate materialization 0.028 ms. Diagnostic-only
receiving/component work is separately 54.711/59.384 ms across nine calls, outside the six
unbound calls. The component scope includes all codecs/data preparation, not only Tilemap.
This attributes cost without proving a speed improvement against the smaller old phase set.
Nested walls, correlated lifetimes and uncontrolled host/cache/clock/driver state grant no
exclusive CPU, GPU, physical-memory or frame-tail result.

Owning Scene all-target/all-feature and sample program/observer correctness Clippy, feature-free
Scene compilation, root/sample formatting and document/source/hash checks pass; existing warnings
remain. Added timing scopes compile out without `work-observation`; no public interface, file
format, dependency or receiving policy changes. Existing independent Scene evidence remains applicable
to unchanged behavior; no unrelated public/official-engine matrix is rerun. The original paused
UI/Dialogue work remains byte-identical. Four scoped source-tracing, production/observer and numeric
review agents are closed with no remaining P1/P2. Final numeric review confirms all eight table
rows, exact 9+6 attribution, 162 source hashes and raw/summary equivalence. Its minor receipt
closure note is resolved by the explicit qualified implementation hash and tested-before-commit
scope; no product rerun is required for that documentation closure. Full Foundation/R9/R10/R12
and foreign execution remain open.

Next compare Tilemap-native uniqueness admission directly into its final cell tree, eliminating
the current independent coordinate set and temporary typed Vec if the complete consumer benefits.
Preserve duplicate input indices, all numeric/color checks, deferred asset effects and ordinary
dirty-revision semantics. The World/identity envelope is not a worthwhile first optimization here.
Keep cross-stage reference-fact reuse as a separate exact immutable value/Schema ownership design;
the existing cost/depth facts alone do not establish declared-reference coverage.

#### Direct Native Tilemap Construction

Status: experiment rejected after complete comparison; production restored, additional public
behavior controls retained at `8e8a1f60158c6b7f96af915607b0d926812a39ba`. The preceding attributed
16K game spends
about 46 ms cumulative in actual component preflight and another 59 ms in separate diagnostic
component preflight. Static source identifies duplicate coordinate indexing and a temporary
typed Vec in stock Tilemap decoding before final native cell-tree construction. Compare a
single native construction within `nara_tilemap`, without changing Scene or generic value owners.

Parse each complete cell in original input order and use the final BTreeMap entry to reject
duplicate coordinates with the same input-index diagnostic. Keep configuration/TileSet preflight
order, finite numeric/color conversion and source/frozen borrowing. A partially built Tilemap
remains local and is dropped on failure; no World or independent retained handle is published.
The existing deferred asset candidate may own that pure native value before handle resolution,
then set its resolved TileSet and move it through the original batch. No new candidate type,
preparation owner, public API, source format, domain exemption or cross-stage cache is selected.

Preserve exact per-cell dirty revision and chunk updates in input order. Do not combine this
experiment with a new initial dirty-batch policy. Remove the independent coordinate BTreeSet,
intermediate cell Vec and later second construction loop instead of retaining compatibility
helpers. Compare full native values, later edits, duplicate/malformed failure ordering and
deferred asset lifecycle through public codecs, then original complete game/source/processed
and independently locked delivery consumers. Freeze the qualified attribution program as the
same-observer control and retain the change only if its complete consumers benefit.

Production review finds no P1/P2 and identifies a real cost tradeoff: deferred/diagnostic candidates
now own final native cell and dirty-chunk trees before asset resolution, rather than only typed
cells. Sparse data spanning many chunks may change retained storage and diagnostic construction
cost. Measure diagnostic and actual unbound component preflight separately; dense-map results
do not establish a general heap or sparse-map improvement. The initial original Tilemap and
related render selection passes 113/113 with 48 outside the filter; no old assertion is changed.
The diagnostic component-preflight span ends before its returned candidates are discarded in
`validate_compiled_scene`; final-map destruction is outside that bucket, whereas the old temporary
coordinate-set destruction occurred inside `read_cells`. Do not interpret that child bucket
alone as a complete Check speedup. Complete consumer observations and preserved lifetime cleanup
remain necessary even if individual construction timers decline.
The frozen qualified attribution program completes its original three 16K lifetimes/nine GPU
replays as the control. The initial owner/render selection passes at
`3b460e2d-fb3f-4b97-8206-053a4c2aa0fa`, 7.048 seconds after a 1m32s serial debug build.
Independently locked Runtime TileSet delivery passes 14/14 without skips at
`d0547e7a-7c41-4880-a4a8-12add2858f10`, 1.371 seconds after a 1m13s build. This retains actual
source-free primary/auxiliary publication, travel, retained-pack and failure-preservation behavior.
Five added public codec controls pass at `0a350f1b-4b81-48b8-82ca-6175a9c46e46`, 0.325 seconds,
with 105 existing cases outside that selection. They compare complete values and subsequent
dirty revisions with ordinary setters, exact late duplicate indices and malformed-cell ordering,
omitted/explicit defaults and owned deferred candidate success/failure/drop isolation. No test
assertion or allowance is relaxed. The candidate's original release
game/library/metrics/span/native/record selection passes 148/148 without skips at
`66fadd4c-d835-4e56-aeb8-4750b8377fb6`, 64.871 seconds after a 5m26s serial build. Original
64/4K/16K travel, 27 retained-input GPU replays, both observer-panic cleanup controls and the
complete 16K record/cook probe remain.

Four frozen before/after/after/before processes add twelve 16K lifetimes and 36 GPU replays,
all accepted with identical phase counts and original cleanup. Actual unbound component
preflight cumulative medians fall from 46.1215 to 42.54965 ms, but diagnostic component preflight
rises from 58.688 to 64.5459 ms. The combined `scene_decode` envelope is 196.600/196.310 ms,
without a meaningful demonstrated gain, and process walls 15.769/15.799/15.962/15.701 seconds
show no consistent improvement. Nested walls and correlated lifetimes are descriptive; the
diagnostic candidate destruction boundary also differs. The dense game does not settle sparse
dirty-chunk retained-memory costs.

Reject this direct-final-tree design instead of shipping a faster sub-operation at the expense
of diagnostic work. Manual `apply_patch` restores only this experiment's production changes.
The final `lib.rs` difference is solely a `cfg(test)` module registration; public/domain/runtime
behavior is unchanged. All five public controls pass with the restored algorithm as part of the
full 110/110 Tilemap library selection without skips, 6.747 seconds, at
`0c30b04b-53e0-404e-b705-6e57aa777e1b`. The
[rejected experiment receipt](../benchmarks/2026-11-14-development-runtime-costs.md#direct-native-tilemap-construction-experiment)
retains the exact candidate patch, source/program hashes, raw report identities and complete
finite summaries; measured candidate hashes are explicitly not final production hashes.
The restored release program is rebuilt and completes another original three 16K lifetimes,
nine retained-input GPU replays and complete cleanup; that is correctness evidence, not a timing
improvement claim. Its receipt stores final source/program/raw-report hashes separately from the
rejected candidate. Owning all-target/all-feature correctness Clippy, feature-free Tilemap library,
root/sample formatting, source/program/patch/report equivalence and document/whitespace checks
pass; existing warnings remain. Final numeric review confirms candidate recovery from the patch,
exact table/counts and raw/summary equivalence. Its early documentation-status note is closed
after final restored execution and checks, without another unchanged-product run. Four scoped
implementation/production/test/numeric agents are closed with no remaining P1/P2. The original
15 paused UI/Dialogue files remain byte-identical; no dependency or remote publication changes.
This rejects the chosen experiment rather than claiming full Foundation/R9/R10/R12 completion.

The next construction design must distinguish the compact typed data needed by diagnostics
from the mutable native storage actually needed by a World. Preserve complete codec refusal,
input-index precedence and ownership while investigating a compact unique-cell representation
or domain-owned reuse through the existing real consumers. Do not infer that every stage may
skip checks, adopt a cross-stage cache keyed by source path, or make a broad Scene validation
API solely to rescue this failed loop experiment. Full Foundation remains active.

#### Native Tile Coordinate Membership Index

Status: performance decision deferred after two complete comparisons; original production index
restored. The rejected direct-construction experiment shows that diagnostic
preflight must keep its compact typed cells instead of eagerly building final native/dirty trees.
The existing coordinate BTreeSet is used only for `insert` membership, never ordered traversal.
Compare the standard library's randomized HashSet at this private duplicate-check owner.

Keep incremental growth, complete per-cell parsing before duplicate rejection, first original
duplicate input index, typed Vec order, deferred asset lifetime and final native BTreeMap/dirty
revision construction unchanged. Do not preallocate the complete set from untrusted list length,
replace the default randomized hasher or add a hashing dependency. Output order and diagnostics
must not depend on bucket order; the set is never iterated. Source/native/domain/receiving checks
and public formats/interfaces remain unchanged. This changes the membership implementation,
not the code's guarantees or the stored cell representation.

Use existing full Tilemap/related render and public deferred/error controls, independently
locked actual TileSet delivery and the original game's complete native travel/replay/cleanup.
Do not add another test framework or repeat unrelated runtime libraries/official-engine suites.
Freeze the qualified restored program with its unchanged 46-phase observer, compare both actual
and diagnostic construction and retain the new index only if complete consumers support it.
Record all unchanged/correlated wall spans and ordinary randomized-hash/run variation; no blanket
memory, frame-tail or worst-case latency claim follows from expected membership complexity.

Initial owner/related-render qualification passes 118/118 with 48 outside the filter at
`3dccd22e-cbf7-4a11-a034-262b51503984`, 7.755 seconds after a 22.66-second serial debug build.
This includes all five public construction/lifetime/error controls added in the rejected experiment,
without changing their assertions. Independently locked actual Runtime TileSet delivery passes
14/14 without skips at `4694adb7-1865-4702-816c-5eea1f3164a1`, 1.436 seconds after a 1m11s build.
The frozen qualified control completes another original three 16K lifetimes/nine GPU replays and
cleanup. The candidate production diff is only the membership type and initializer. Original
release game/library/metrics/span/native/record qualification passes 148/148 without skips at
`92461861-6dbf-47bb-92e4-e509e9d34e34`, 64.843 seconds after a serial 6m49s build. Original
64/4K/16K travel, 27 GPU replays, both observer-panic cleanup controls and the complete 16K record/
cook probe remain. Independent production review finds no P1/P2; the set is never iterated and
`TileCoord`'s Hash/Eq agree, so input/error/dirty order is independent of bucket order.

The first before/after/after/before series shows changing host cost: diagnostic component
medians are 84.918/76.020 ms, actual unbound component 63.707/61.983 ms and unchanged receiving
57.004/51.795 ms. One additional consecutive four-process series resolves the earlier separated
control timing but still shows large unchanged-stage drift: diagnostic 84.586/64.520 ms,
actual component 59.816/48.352 ms, unchanged receiving 52.277/43.735 ms and payload decode
334.478/302.882 ms. In total all 24 original 16K lifetimes and 72 GPU replays complete with exact
expected phase calls, gameplay and physical cleanup. Neither nested/correlated wall series
isolates the effect of two source lines. Do not normalize unchanged phases away or turn these
observations into a CPU, memory, frame-P99, general speedup or regression claim.

Defer the index choice and restore only this experiment's two production lines with `apply_patch`.
All final production/source/control hashes match the qualified baseline exactly. The
[inconclusive report](../benchmarks/2026-11-14-development-runtime-costs.md#native-tile-coordinate-membership-index)
stores one finite observation artifact with exact raw/program/source identities and the
recoverable candidate patch; selected retained/omitted raw fields are explicit. Existing process
CPU observations cover different work and threads, so they are not a valid per-phase substitute.
No new profiler, hash dependency or public API is added to rescue the small index candidate.
The restored original program is rebuilt in 3m48s and completes another three 16K lifetimes/nine
GPU replays with physical cleanup. The same finite observation artifact records that final
program/raw-report identity and selected complete facts, explicitly as correctness rather than
performance evidence. All 163 final source/control hashes equal the qualified baseline; no
Rust or dependency changes remain. Root/sample formatting and document/whitespace checks pass;
there is no reason to rerun unchanged owner/official suites merely for the evidence closure.
Main verification recomputes every median and phase count, compares selected facts with all raw
reports, checks both candidate/control programs and the restored program, all source hash tables,
the applicable candidate patch and local document links. The final independent disposition review
is partial: it confirms the two-line patch, restoration, ordering and honest deferred claims, but
does not independently recompute the numeric/source hashes. Do not treat main verification as a
second independent review. Three scoped source/production/disposition agents are closed; no
concrete P1/P2 remains in the reviewed scope. These limits are sufficient for documenting a
non-adopted experiment and are not a performance approval or full Foundation qualification.

Stop iterating on this small index under uncontrolled wall measurements. Next inspect the larger
repeated Tilemap domain/diagnostic preparation through existing source, Check and Runtime owners.
Choose a concrete representation/handoff only after identifying its full receiving, failure,
mutable escape and retained-resource effects. The previously rejected final-tree choice is not
reintroduced, and a source-path cache is not selected. Full Foundation/R9/R10/R12 remains open.

#### Codec-Owned Diagnostic Validation

Status: implemented at `1b4a4ad` and behavior-qualified on Windows/Rust1.95; elapsed-time
improvement remains unqualified. Check and source-load diagnosis previously asked
the same codec as actual Runtime preparation to construct a candidate, although diagnostics
discarded it. The stock Tilemap parsed complete cells and retained a typed Vec for that candidate.
The earlier experiments show that eagerly constructing final trees does not serve diagnostics,
and fine-grained membership timing cannot select a reliable optimization under host variation.
Separate these two real consumers at the original codec interface instead of adding a cache.

Reflect may register an explicit context-aware diagnostic validator alongside the exact typed
preflight/encode callbacks. Existing registrations retain their current preflight fallback and
candidate lifetime: compiled diagnostic batches retain fallback candidates until complete
batch disposal; source-authoring diagnostics drop each immediately. The explicit validator
must preserve the preflight domain refusal and context effects
for the same effective input/context; it produces neither `T` nor a prepared candidate. Actual
preparation calls only preflight, not validator plus preflight. Generic Schema/default/native
reference/limit checks remain shared, and a complete Scene batch is admitted before its first
domain callback. Earlier callbacks that tighten context limits keep the later full-admission
fallback. No new public Scene authority, detached result, mutable World access or retained proof
is introduced.

Scene diagnostic paths use the validator dispatch; actual spawn and replacement retain original
typed candidates and resource/World checks. Tilemap opts in using one shared configuration parser
and complete cell visitor. The visitor retains coordinate uniqueness and exact original input
error order. Diagnostics need no typed cell Vec or final native tree; actual preflight gathers
the same owned cells and preserves deferred TileSet resolution and dirty revision order. The
existing standalone membership operation remains complete and is not silently treated as codec
validation. Third-party codecs without an explicit validator still execute their original code.

Qualify public fallback/explicit dispatch, defaults/limits/hidden references, current context
changes and exact diagnostics, native preparation/commit, dropped candidate effects, source/
processed delivery and the original game's full travel/GPU/cleanup workflow. Inspect the complete
source Check consumer as well as compiled diagnosis. Do not infer performance from deleted
allocation sites; distinguish removed diagnostic native construction from still-required generic
receiving/reference checks. This interface improvement does not complete full Foundation.

Initial Windows/Rust1.95 production compilation passes. The public Reflect/Scene and real
third-party Check selection passes 13/13 without skips at
`ded8ca96-ea5e-4ebd-b3fc-7186c3881e32`. Its first build exposes a stale owned-value clone in
the existing business-diagnostic fixture, corrected to the borrowed reader's explicit `to_owned`.
New Scene tests initially assume a zero-entity fresh World and codec-level Schema errors;
they now compare the actual initial population, require no identity-domain publication, and
assert the original compiler refusal before any domain callback. No production policy changes
follow from those fixture corrections.

Independent Reflect and Scene/Tilemap source reviews find no remaining P1/P2. A review removes
an eager full-list Vec reservation proposed during integration: the old fallible collection
grows with its successful prefix, so reserving the whole malformed list would increase failed
preparation memory. The shared visitor now preserves prefix growth. Complete owner/public
consumers preserve the existing behavior: the full Reflect/Scene/Tilemap library selection runs
1,200 tests at `af0b47e0-fc68-4fc9-9736-fafde8a837a9`, with 1,199 passing and only the new
asset-context fixture failing because independent global AssetId allocations differ. The final
test starts both catalogues from one untyped identity, then compares full catalogue equality,
exact refusal and actual type-claim mutation; it passes 1/1 at
`8a9dc68d-6187-46b6-af6d-bdbdaba7148d`. Unchanged passing tests are not rerun for that fixture-only
correction. Scene public receiving/preparation/reference controls pass 20/20 without skips at
`7b1d4e4d-5686-4e71-86b7-0e26b00f3069`; independently locked codec/preparation/inventory and
source/processed TileSet consumers pass 33/33 without skips at
`6c5d0a6c-a378-48ae-938a-918ef0038bcd`. The original release game/library/metrics/span/native
selection passes 148/148 with no skips at `61e588e2-3dd0-469b-b258-86464739ccb1`. Final review
strengthens provider composition with valid input and an explicit no-candidate result, so a
lost validator cannot pass by producing the same fallback error. That case passes 1/1 at
`df3d1820-8514-4d47-8fc0-75ef8f80307d`.

The [same-observer report](../benchmarks/2026-11-14-development-runtime-costs.md#codec-owned-diagnostic-validation)
retains four complete serial before/after/after/before processes: twelve original 16K lifetimes,
36 GPU replays and complete original cleanup pass. The earlier development baseline is excluded.
The after program is slower across multiple unchanged stages as well as diagnostic dispatch,
so the comparison establishes no latency improvement or isolated causal regression. Retain the
exact behavioral separation and removed diagnostic native construction without claiming the
cross-engine timing milestones are achieved. Do not add normalization or repeat small experiments
until a favorable number appears. Source payload representation, remaining receiving work and
controlled complete-consumer costs still require attribution; full Foundation remains open.

Scoped shared all-target/all-feature and root public-consumer correctness Clippy pass, as do
feature-free Reflect/Scene/Tilemap production compilation, root/sample formatting and targeted
document/source/report checks. Existing warnings remain. Independent source, final acceptance
and numeric reviews close all actionable findings; the timing report's disposition is checked
against exact raw arrays and source/program provenance. No official engine test suite is rerun,
and paused UI/Dialogue files remain byte-identical.

#### Compact Inline Tilemap Source Successor

Status: **Deferred by the November 17 execution priority.** No format, codec, schema or consumer
switch is implemented. Complete Runtime capabilities and usable game workflows take priority
over compact representation, small performance experiments and additional attribution loops.
Existing correctness and admission contracts remain; ordinary gameplay needs no extra validation
pass merely to support a prospective optimization.

The [existing source study](../research/2026-11-17-large-tilemap-payload-ownership.md#8-compact-inline-domain-format-budget-and-migration-clarification)
retains the findings for a future demonstrated need. A format change would require complete
source/Prefab/overlay evolution, authored numeric and unknown-member preservation, and cooked
delivery; it is not a codec-only substitution. No implementation or benchmark campaign is
scheduled. Full Foundation remains active; Editor/UI/Dialogue work stays paused.

### Preserve Isolated Migration Storage During Defaults

Status: implemented at `e2c2a38` and qualified on Windows/Rust1.95 below. Correct the independently identified
already-owned path before selecting a broader value representation. An actual source migration
obtains an isolated owned value and stores its
result in a fresh component-local allocation. If that result still needs defaults, the current
borrowed materializer copies it again. This is unnecessary ownership loss, distinct from the first
copy still required for a shared current-version source with omitted fields.

Reflect keeps one default selection, plan, complete effective preflight and application. Add its
hidden in-place operation for an already admitted isolated value. Scene uses `Arc::get_mut` to
obtain private unique storage without requesting copy-on-write; only that route uses the in-place
operation. Shared source values retain the existing borrowed materializer. Do not infer uniqueness
from a detached count, expose a public mutable backing handle, clone contextual declarations or
plan twice. Original source/context validation, migration callback order, diagnostic mapping,
complete effective limits and conservative work/retained fees remain unchanged.

Acceptance requires actual migration followed by missing-default insertion to preserve the
migration result's large List allocation, while source and preceding compiled output remain
unchanged. The original migration callback runs once per attempted compilation; failure paths
retain their full diagnostics and bounded admission. Reflect's nested Map/List suffix and contextual
override matrix compares owned/borrowed/in-place costs and exact/one-short byte/node/depth errors,
with no mutation on preflight refusal. Explicit Null and no-insertion prior admission remain.
This does not claim a speedup for the unchanged 16K Tile workload or qualify a frozen-overlay,
packed cell format, independent cell asset or persistent editing tree.

The full all-feature Reflect/Scene release libraries pass 1,038/1,038 without skips at
`f30f45c2-3038-4f7d-a25b-1c30bb557505`. The new real registry migration test preserves the migration
result's List allocation through default insertion and checks exact work, one-short work,
effective-depth and initial source-node refusal with the original callback counts. Source and
previous compiled values remain unchanged. The initial focused Reflect defaults/admission cohort
also passes 20/20 at `db721847-b773-45af-868f-02e7558904b6`, with 221 outside its filter.
Independent code review finds no P1/P2 and requests two additional direct failure controls:
late sparse-plan refusal before any insertion and actual migrated invalid-default-parent refusal.
Both public integration controls pass in the complete selected public effective-value cohort,
18/18 without skips at `91f03b40-a6df-49e8-82de-04fb614d2530`. Selected Scene format, default-property, effective-reference, selected-entry
and data-binding consumers pass 62/62 at `90757ed3-925e-4a74-bc27-a71cd7504140`, without skips.
Owning all-target/all-feature release correctness Clippy, feature-free Reflect/Scene production
library checks, final root formatting and owned document links/whitespace pass; existing warnings
remain. No manifest, lockfile or public source/compiled interface changes are needed.
No complete native game or balanced cost comparison is rerun for this already-owned correction;
the qualified `2dd0083` program is frozen separately as the next representation comparison's
baseline. These results establish allocation identity and semantic/refusal behavior, not a new
16K speedup, physical-memory or runtime-tail budget.

An independent plan review also found that the first follow-up draft prematurely selected a
common borrowed view and used a synthetic generic List as its main justification. The corrected
decision above first compares the actual persistent Tilemap configuration/data boundary and its
source-upgrade/native composition costs with the view and packed alternatives. Final review closes
both findings without a remaining P1/P2. That document closure is not a measured result for any
new representation. All five scoped research/implementation/review agents are closed.
Cargo stays serial with one build job against the original target. The first full owner release
test build takes 25 minutes; later selected public compilation takes 7m38s. No product acceptance
failed or was replaced by a weaker assertion. Original paused UI/Dialogue work remains unchanged,
and no remote CI or publication occurs. Full Foundation/R9/R10/R12 and foreign execution stay open.

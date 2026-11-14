# Rust State Capture, Rebuild And Reconstruction

Date: 2026-11-13. Updated: 2026-11-14. Status: qualified for the shipped Windows source/headless
workflow and explicit source/desktop reconstruction/adoption through the finite development Host.
Continuous Editor Play integration and complete R12 remain open. Owner: the foundation rebaseline
R12 runtime lane.
Editor UI and physical mixed-DPI work remain paused.

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

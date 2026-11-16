# Rust State Capture, Rebuild And Reconstruction

Date: 2026-11-13. Updated: 2026-11-16. Status: qualified for the shipped Windows source/headless
workflow and explicit source/desktop reconstruction/adoption through the finite development Host.
Continuous Editor Play integration and complete R12 remain open. Owner: the foundation rebaseline
R12 runtime lane.
Editor/UI/Dialogue product work remains paused. Physical mixed-DPI testing is separately
authorized and has bounded [Runtime drawing/picking qualification](2026-11-14-durable-material-and-shader-plan.md#presented-drawing-and-picking-successor).

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

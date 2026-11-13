# Rust State Capture, Rebuild And Reconstruction

Date: 2026-11-13. Status: in progress. Owner: the foundation rebaseline R12 runtime lane.
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

The owning contracts are [ADR0076](../architecture/adr/0076-play-runtime-debug-control-and-observation.md),
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
| Verification driver and SaveSlot | Automated script cursor is not an interactive-game resume format. The finite process workflow uses ordinary semantic commands. Storage retains its original explicit close owner; no live file handle or pending publication is captured. |

The persisted compatibility record must include the admitted Schema and content closure relevant
to every restored level, not only a source pathname or the startup Scene. Executable provenance is
tracked separately so an explicitly compatible body/private-structure edit can change code while
unsupported content or schema changes fail before replacing the original. The exact bounded DTO,
canonical encoding and validation errors follow implementation of this inventory; they are not
yet a general engine checkpoint Interface.

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
script is needed. No native UI/GPU test is needed for this CPU lifecycle change; physical OS
process exchange and actual rebuilt game remain unimplemented.

Existing R6 automatic source refresh and R10 input/replay evidence remain under their owning
plans. Game-owned resume format, compatibility, fresh reconstruction, actual Rust body/structural
rebuild, finite process exchange and candidate adoption are the remaining work in this plan.
No exact rebuild/restore, native Editor iteration, universal replay, arbitrary World snapshot or
whole-foundation completion is claimed.

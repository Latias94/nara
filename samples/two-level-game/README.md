# Two-Level Exploration

A small independent Rust product using Nara's public project, command, Scene and filesystem APIs.
It owns ordinary Rust modules for gameplay and checkpoint data; the Reference Game and its wave
CLI remain separate consumers.

The blue explorer moves with WASD and automatically collects nearby gold gems. E travels between
Meadow and Cavern. Q switches between two independently persistent instances of the current source.
M places a pink dynamic marker. K saves the current checkpoint and L restores the slot. R restarts
the current logical level at its checkpoint, or retries a failed reconstruction. Closing the window
uses normal Runtime shutdown, including pending Scene preparation and temporary-file cleanup.
The green square marks the checkpoint; the purple square marks the destination portal.

## Build And Run

Run commands from this package directory. The optional desktop target needs the platform's normal
native window and GPU support. Reuse the engine target directory and one build job when developing
inside the repository.

```text
nara schema . --write
nara schema . --check
nara check .
cargo run --bin headless
cargo run --bin headless -- --verify-travel
cargo run --features desktop --bin desktop -- --save /absolute/existing-parent/checkpoint.json
cargo run --features desktop --bin desktop -- --verify-first-present
```

On Windows, pass a fully qualified path such as `D:\GameSaves\checkpoint.json`. The application
does not choose the process working directory or silently create an arbitrary save parent. Omit
`--save` for a session without persistence; K/L then report the absent application slot.

The authoring bundle is generated from this product's real native schema provider with the existing
schema command. It is not a hand-maintained second definition of component layouts.

## Checkpoint Acceptance

Create an application-owned directory, then run the same headless binary twice:

```text
cargo run --bin headless -- --verify-save /absolute/existing-parent/checkpoint.json
cargo run --bin headless -- --verify-restore /absolute/existing-parent/checkpoint.json
```

The save trajectory collects in Meadow 0, Cavern 0 and Cavern 1, places markers with IDs 1/2/3,
returns to Cavern 0, retries that current level and publishes the slot there. It requires score 30,
three collected facts and next marker ID 4 in the actual file readback. The second invocation starts
with a fresh process/World, restores Cavern 0, creates marker 4, visits Cavern 1 and returns to Meadow
0. It checks actual Sprite visibility, live dynamic marker IDs and changed `CurrentScene` instance
receipts, not just copied DTO values. Both invocations print `verified=true` only after completing
their finite command trajectory.

```text
cargo nextest run --manifest-path samples/two-level-game/Cargo.toml --all-targets
```

The command above is relative to the engine root. Tests include separate save/restore processes,
an actual destination source change after startup, corrupt save rejection, exact local conflict
checks, interruption before publication, and pure bounded format/migration cases. Desktop
`--verify-first-present` proves presentation only; it does not prove that a human exercised every
key or that a particular platform has passed the full interactive trajectory.

## Exact Game Reconstruction

The separate `resume` format preserves the complete supported game semantics: progress across
level instances, the live player transform, held movement intent, logical game time, fixed
scheduling phase and actual time policy, counters and admitted configuration/Schema/content
identities. Resume format version two requires an explicit time value; version one is refused
rather than assumed to have zero phase. The finite continuity protocol also moves to version
two, so an older child is refused at the version gate. The progress-save slot above retains its original
anchor-based behavior and its separate version-one migration.

Use `capture_resume(&session)` after the original `HeadlessSession` has completed a paused frame.
Capture refuses pending commands, travel or authoritative tasks, scripted verification, unknown
plugin services, an open save slot and a non-unit Runtime time scale. The shipped source/headless
recipe is the first supported inventory. Extend that explicit inventory and compatibility policy
when adding authoritative gameplay state or services.

Start and pause a fresh candidate with the same ordinary product recipe, then call
`request_resume(&mut candidate, &state)`. It first validates and initializes the phase through the
original time owner, completes paused maintenance, then admits one command through the existing
gameplay queue. Nonzero remainder and legal whole-tick debt survive the isolated exact tick and
later maintenance. Drive complete paused maintenance frames, with an explicit deadline, until
`GameSnapshot::restoring` clears or an error occurs. Compare `capture_resume(&candidate)` with
the original state before allowing gameplay to continue. Rejected reconstruction remains
ineligible even if an ordinary command later clears the displayed game error. Retain the original
paused session and captured executable until the candidate passes state and continued-play checks.

The fresh Runtime keeps its own clock, command authority and World identity. Logical game time
is restored separately. The portable time value carries only policy and pending duration; the
receiving Runtime validates its actual policy and derives interpolation while preserving its
local history. Durations outside the game's `u64` nanosecond representation refuse capture.
Scene numbers are local to a World; compare scoped identities when
proving fresh construction. Current Scene and dynamic markers rebuild through their existing
owners, with no old ECS entity, input connection or native handle in the format.

The Host-driven `continuity` binary exchanges bounded versioned files in an explicitly supplied
private temporary directory outside project source. Its arguments are `--project-root ABS`,
`--control-dir ABS`, `--session STRING` and `--generation STRING`. The process starts paused and
accepts finite Observe/Step/Frame/Capture/Restore/Close operations. Session, generation and request
IDs are validated before effects; stdout/stderr carry diagnostics. The test Host owns child
creation, deadlines, immutable executable installations and physical retirement.
Its native child execution uses the same `project_tooling::process` owner as supervised Editor
Play. Each game launch consumes restrictive executable/directory capabilities; only a matching
game Close reply and the original native cleanup receipt prove normal retirement. Diagnostic
output remains a bounded in-memory tail, independent of the file-based game protocol. Candidate
failure cleanup retains its original owner until containment and output readers have completed.

From the engine root, the actual Rust-edit qualification uses the existing shared target and
installed Rust 1.95 baseline:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --test continuity --run-ignored only --test-threads=1
```

Set `RUSTUP_TOOLCHAIN=1.95.0`, `CARGO_BUILD_JOBS=1`, `CARGO_NET_OFFLINE=true` and an absolute
`CARGO_TARGET_DIR` pointing to the engine's `target` before that command. The test copies source
into its own temporary tree and uses public Cargo executable capture. It performs real movement-body
and private Rust-structure edits, then checks fresh-process state and continued gameplay. It also
exercises build/startup/compatibility failure, stale requests/replies and incumbent retention. Run it serially; normal
package tests keep this nested-build case ignored. Execution results and limits belong to the
[reconstruction plan](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md).

## Source/Headless Development Host

The ordinary [development Host](src/development/mod.rs) owns candidate preparation and the single
active command route. The lower-level A/B/C test above remains an independent regression. This
workflow supports the shipped source/headless game inventory; it does not extend desktop Play,
restore native GPU/audio/physics services or patch Rust code inside a running process.

Construct `DevelopmentHost` with the explicit absolute project root and an existing trusted work
parent outside that project. `start` consumes a `CapturedCargoExecutable` selecting this game's
native `continuity` binary with default features disabled and no requested/resolved features.
`capture` and `step` always target the active game. Supply an explicit public `CargoBuildRequest`,
the expected initial complete `ResumeState` and finite `ProbeStep` expectations to
`prepare_rebuild`. The Host builds the edited source and compares complete canonical state after
each probe step. It closes that probe through the game and native owners, then restores the
original basis in another fresh process. Probe progress is discarded.

The returned `PreparedRebuild` is a nominal ticket retained by its original Host. `adopt` checks
the prospective source envelope and both complete states before exchanging the command route.
The incumbent may keep stepping while a candidate is retained; if its basis changes, adoption
refuses and the ticket remains available for explicit `cancel`. Foreign, cancelled or consumed
tickets cannot select a candidate. After adoption, all ordinary commands target the new active game.

`AdoptionOutcome` reports committed adoption separately from predecessor retirement. Retirement
initially remains `Pending` until `retry_retirement` drives the original native owner. The new
active game can step/capture during that wait; another rebuild refuses until cleanup completes.
`close` fences new commands and retains all original owners and storage until physical cleanup.
Call it again if Pending. Each retained child gets its own ten-second cleanup drive; a batch may
take several drives. Drop is fallback cleanup, not evidence of successful Close.

The ordinary [`develop`](src/bin/develop.rs) binary exposes this workflow through newline-delimited
JSON. From the engine root, with absolute paths substituted:

```text
cargo +1.95.0 run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --bin develop -- --project-root ABS_GAME_ROOT --work-parent ABS_EXISTING_PARENT --target-dir ABS_ENGINE_TARGET --target x86_64-pc-windows-msvc --profile dev
```

Set the same toolchain, jobs and target environment as the actual-build test above. The explicit
target must match the build compiler's native host. This project authorizes execution of trusted
Rust/Cargo build inputs. The CLI emits one `ready` record, then one result for each command:

| JSON operation | Result |
| --- | --- |
| `{"operation":"observe"}` | Complete current `capture`. |
| `{"operation":"step","commands":[]}` | One ordinary fixed tick and its complete capture. Semantic movement commands use `{"operation":"move","direction":1,"pressed":true}`; directions are left/right/up/down = 0/1/2/3. |
| `{"operation":"prepare","basis":STATE,"probes":[{"commands":[],"expected":NEXT_STATE}]}` | Actual build, complete-state probe qualification and one retained candidate. Replace `STATE`/`NEXT_STATE` with full objects; expected state is supplied by the caller. |
| `{"operation":"adopt"}` | `committed:true`, new active generation and independent `retirement:"pending"`. Uses the CLI's retained ticket; generation observations grant no selection authority. |
| `{"operation":"cancel"}` | Cancel the CLI's exact retained candidate. |
| `{"operation":"retry_cleanup"}` | Retry retained physical cleanup without changing the active command route. |
| `{"operation":"isolate_input"}` | Freeze the desktop active game's original native keyboard ingress without discarding held intent. |
| `{"operation":"confirm_presentation"}` | Confirm a fresh desktop presentation joined to the complete current canonical state. |
| `{"operation":"activate_input"}` | Reveal/focus the desktop active window and return its original activation state and any activation error. |
| `{"operation":"close"}` | Fence commands and close the session. EOF also closes it. |

Operation refusals produce `ok:false` while retaining the active game. Malformed input ends the
session through Close and exits unsuccessfully. Unknown fields on no-argument commands are
refused, including invented generation/candidate selectors. Responses preserve bounded structured
Cargo diagnostics and distinguish adoption from cleanup. `retirements` reports each original
generation's game Close, fallback termination and native exit/containment/readers/output facts.
Physical cleanup completion alone does not claim graceful game Close. The CLI accepts at most
128 commands, 256 KiB per input line and 64 probe steps. Host rebuilds are limited to eight attempts
and 1 GiB of cumulative captured executable bytes. These allowances do not establish RSS or latency limits.
Session/idle checks occur between operations and after stdin reads; they do not interrupt blocked
stdin or the synchronous Cargo adapter's compiler/build-script calls.

The prospective source envelope covers the exact project directory and complete finite source,
assets, scenes, prefabs and schema trees, plus declared build inputs. It is checked before/after
build and before adoption. This trusted-local per-file envelope is not an atomic filesystem
snapshot, dependency discovery or rollback of arbitrary native/build-script side effects.

Run the production Host's actual-edit/adoption qualification separately and serially:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --test development_host --run-ignored only --test-threads=1 --no-fail-fast
```

Actual results and platform limits belong to the
[current plan](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md#game-owned-development-host-and-adoption).

## Caller-Driven Desktop Control

The separate [`desktop_control`](src/bin/desktop_control.rs) consumer uses the ordinary desktop
recipe with the original `DesktopRun` retained between polls. It starts hidden and paused, drives
movement, pickups, markers and travel through public semantic commands, then exercises exact
fixed steps, reveal, Resume/Pause and physical retirement. The original presentation receipt is
joined to its exact retained stock render input. After the original native owners retire, a
separate image replay checks finite player, marker, pickup-interior and background pixels.
That readback is not a capture of the swapchain or display.

From the engine root on Windows, set `WGPU_BACKEND=dx12` and
`NARA_WGPU_FORCE_FALLBACK=0`. Reuse the toolchain, serial build and shared target settings above,
then run:

```text
cargo +1.95.0 run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --bin desktop_control
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test desktop_control --run-ignored only --test-threads=1 --no-fail-fast
```

The ignored test launches that ordinary executable through the shared contained-process owner
and checks its actual exit, containment, readers and output. Executed results, the finite pixel
oracle and platform limits belong to the
[desktop development plan](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md#desktop-development-and-original-native-session).
This consumer does not reconstruct a desktop game or adopt a rebuilt executable. Its visual
expectation is not an authoritative `ResumeState` capture; the separate reconstruction and
development-Host consumers below own those complete-state and route-exchange checks.
The keyboard-continuation trajectory additionally starts isolated, restores the actual movement
bindings without Started, and reconciles a key released before focus acquisition. One original
exact Step must stop movement before native activation. This exercises the input prerequisite;
it does not qualify a rebuilt desktop candidate.

## Desktop Reconstruction And Explicit Development Placement

With the `desktop` feature, `capture_desktop_resume(&run)` captures through a healthy completed
paused `DesktopRun`. It also captures the original declared keyboard inventory and refuses if
its holds differ from game movement intent or any original input work remains pending. The
headless adapter refuses a native presentation placement. Typed gameplay Move alone is not a
native keyboard member and cannot satisfy desktop capture.

`restore_desktop_resume(&mut run, &state, deadline)` targets one fresh paused isolated game.
It validates the complete bounded state, configuration, Schema and empty native membership,
initializes original time, executes one original restore command/Step and waits for Scene
reconstruction before restoring keyboard membership. Complete canonical bytes must match.
It does not reveal, activate or replace a route. After any applied failure, retain the exact
candidate and drive physical Close; no rollback is implied. The deadline is cooperative and
cannot interrupt a graphics driver, native API or user callback.

The ordinary `desktop_reconstruction` executable uses these APIs with the same recipe in two
sequential native runs. It retains meaningful three-instance progress, a non-anchor player,
held D and 13 ms scheduling phase, verifies incompatible preflight refusal, then reconstructs
the complete state in fresh Runtime/Scene/render/native/GPU identities. Its presentation
receipt must match a restored stock-input seal at or after the capture frame; separate GPU
replay runs after original physical cleanup. Sequential reuse of one retired native pump does
not demonstrate cross-process adoption.

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test desktop_reconstruction --run-ignored only --test-threads=1 --no-fail-fast
```

The development Host's explicit `SourceDesktop` placement uses the same existing source,
executable, process and candidate owners. Its candidates start hidden and input-isolated. A
presentation confirmation joins the exact sealed canonical state to the original native receipt;
startup READY alone cannot qualify restoration. Prepare/adopt isolates incumbent native input
before final capture. `activate_active()` is separate from route adoption, so denied focus or
reveal failure retains an already adopted paused game with an actual activation result. Logical
probe movement uses the original Input driver with Synthetic provenance; it does not pretend to
observe OS input. The [owning plan](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md#desktop-candidate-reconstruction-successor)
records actual execution results and remaining integration limits.

For the CLI, compile `develop` with `--features desktop` and pass `--placement desktop` after
the Cargo argument separator. The CLI selects the `continuity` child's exact `desktop` feature;
compiling the Host with that feature alone does not select native execution. `ready` reports
the actual placement. Contained product launches retain the reviewed desktop-session environment
projection; parent `WGPU_BACKEND` and `NARA_WGPU_FORCE_FALLBACK` hints are not inherited. The child
uses the stock default selection and reports its actual adapter/backend in presentation results.
Explicit graphics selection is not yet a development-Host option. The three desktop-only JSON operations above refuse source/headless
placement. Prepare and adopt can leave the incumbent isolated even after refusal; explicitly
activate the retained game when native input should resume. A committed adoption remains
committed if subsequent activation fails. A `closing` child keeps its original native owner
until an explicit retry reaches physical completion.

Run the production desktop Host's actual body/private-structure edit qualification separately:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test development_desktop --run-ignored only --test-threads=1 --no-fail-fast
```

This game-owned development protocol is version three and requires explicit placement in every
reply. It refuses old children at the existing version gate. The resume format remains version
two; progress saves and the supervised Editor Play protocol retain their separate formats.

## Ownership And Limits

- `CurrentScene` is the only source/Scene-instance authority. Gameplay retains a logical `LevelKey`
  and bounded marker tokens for its own dynamic entities; it never maintains another Scene receipt.
  Normal R uses the retained committed source even if its disk file has changed; it is not Reload.
- `ProjectScenes` prepares only startup or declared content through the original Host loader and
  budget. The game inspects a read-only target, validates all checkpoint-referenced level sources
  one at a time, then explicitly commits the final target with the existing protected overlay.
- Collected authored pickups remain Scene members. Their game-owned collected facts make them
  inactive and set their actual Sprite alpha to zero. They are logically removed, not despawned.
- Dynamic marker IDs are game-owned serials. Live marker entities have ordinary engine identities
  and hierarchy placement. Travel supplies their exact retirement tokens and reconstructs target
  markers after commit. A reconstruction failure reports `Degraded`; it does not claim that the old
  World was rolled back. R retries reconstruction without reusing a stale Scene receipt.
  The automated trajectory verifies ordinary current-level Retry, including changed source files.
  Separate complete-frame CPU tests remove a committed marker's Sprite/root placement and alter
  its Transform, or legally retire its entity. Degraded Retry restores the exact checkpoint-backed
  marker while preserving the original `CurrentScene` receipt/source and logical progress; later
  travel retires the recovered marker and leaves unrelated runtime entities intact. These cases
  do not qualify native physics/audio reconstruction or arbitrary missing authored components.
  The [recovery qualification](../../docs/plans/2026-10-20-persistent-two-level-game-plan.md#november-13-marker-reconstruction-recovery)
  records the original failures, consumer checks and execution scope.
- The versioned slot contains only explicit checkpoint/progress facts. Restore rebuilds movement,
  player position and dynamic entities at the authored checkpoint; it is not arbitrary-tick replay
  or serialization of World entities, tasks, handles, native resources or every component.
  In this game score has one source: each distinct collected pickup awards ten points. Content
  admission rejects otherwise well-formed checkpoints whose score disagrees with those facts.
- The single local writer uses an explicitly opened `TrustedLocal` parent capability, bounded
  decoding, advisory/platform locks, expected identity plus content digest, a synced temporary file,
  atomic name replacement and identity-bound readback. This is a cooperative local-file contract,
  not protection against hostile concurrent writers. A changed target is rejected. Failed temporary
  cleanup remains owned by the slot and its Runtime close participant. File-save success reports
  the parent-directory sync fact separately; unsupported directory sync is not crash-durability proof.
  The current Linux filesystem adapter cannot discard a failed candidate by exact opened-object
  identity. That failure remains `CleanupPending` and prevents clean Runtime shutdown; the sample
  retains the cleanup owner and does not substitute a pathname deletion or report false completion.
  Save is synchronous and bounded to 64 KiB of checkpoint data; this does not guarantee frame time
  or bound operating-system I/O latency. The headless binary's ten-second deadline bounds cleanup
  retries after gameplay returns, not the whole gameplay run. Child-process qualification adds its
  own external 90-second deadline and bounded captured output.

The headless entry uses complete managed frames through `HeadlessRunIntent::frames` so PreUpdate
collects semantic commands and TaskUpdate publishes prepared Scene results. Its supplied 20 ms
real delta is simulation input, not a sleep. App time settings remain authoritative. The separate
exact-tick API intentionally omits ordinary gameplay frame stages. This finite script has a
200,000-frame ceiling and reports 30 seconds without actual progress using a monotonic clock;
neither is a gameplay performance guarantee or an interrupt of a blocked synchronous operation.
Travel, fresh-process save/restore and retained-source Retry pass the sample's 24-test Windows
cohort. The November 13 release-mode CPU/headless cohort passes 32/32, including both marker
recovery trajectories and the original save/travel controls. The earlier release export
separately passes both startup product observations and the same three
travel/save/restore operations from an unrelated directory. This does not qualify other platforms
or a complete interactive desktop control trajectory.

## Export

```text
nara export . --parent /absolute/existing-parent --destination two-level-candidate
```

The destination must be absent. Run `bin/headless --verify-travel` and the two checkpoint commands
from an unrelated directory; use `.exe` on Windows. `bin/desktop --verify-first-present` verifies
actual first presentation through the ordinary shutdown path. Both declared level sources and their
Images/metadata are delivered in the sibling `project/` tree. This is a source-carrying development
candidate, with checkpoint location still chosen by the application, rather than a full project backup.

Export uses the existing protocol-2 headless/desktop targets. `[content]` declares Cavern in addition
to implicit startup Meadow, so Check and export cover both Images and metadata through the delivery
union. Packaged executables load the sibling `project/` tree; they do not fall back to the source
checkout when that packaged tree is present but invalid. The five-file build-input observation is
not a complete hash of Rust `mod` dependencies; ordinary Cargo remains responsible for compiling
changes to these gameplay modules.

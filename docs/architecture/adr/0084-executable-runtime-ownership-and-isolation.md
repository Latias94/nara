# ADR 0084: Executable Runtime Ownership and Isolation

**Status**: Proposed
**Date**: 2026-07-13
**Last Revised**: 2026-07-21
**Owner**: `nara_app` and concrete executable hosts
**Admission Trigger**: RGF-U5 proves the code-first runtime core; RGF-U26 freezes the task-equivalent
manual counterfactual and RGF-U24 proves unpublished candidate construction, headless Host
publication, and the reversal matrix before RGF-U17 replaces the bare Play `World`
and RGF-U13 proves desktop drive/close parity; RGF-U23 then decides this ADR independently before
checking compatibility with the outer-Host decision
**Revisit Trigger**: A concrete embedded or multi-runtime workflow proves that a thin lifecycle owner
cannot preserve `App` as the sole schedule/world authority
**Related**: ADR 0003, ADR 0008, ADR 0034, ADR 0039, ADR 0042, ADR 0052, ADR 0057, ADR 0058,
ADR 0076, ADR 0081, ADR 0082

## Context

`nara_app::App` already owns the simulation `World`, plugin lifecycle, schedules, runner contract,
startup state, and time transaction. Before RGF-U17, `ScenePlaySession` owned only a bare `World`;
tooling pause was an enum rather than execution control, and Stop could remove that world without
proving that tasks or native services closed. RGF-U17 removes that baseline through a concrete
Editor trial while this ADR's final decision remains pending RGF-U23.

A `World` is an ECS state container, not an executable game instance. A complete runtime boundary
also needs:

- one driver authority and main-thread safe points;
- startup candidate publication and a runtime generation;
- pause, resume, exact fixed-step, fault, stop, and restart semantics;
- propagation of schedule, gameplay-transaction, task, and service faults;
- runtime-scoped service leases and finite close evidence;
- a replayable host-owned recipe for fresh reconstruction;
- identical behavior when driven by editor, desktop, or headless hosts.

ADR 0076 contains runtime control as one part of a much larger observation/debug/replay direction.
This proposal isolates the executable lifecycle contract without selecting system stepping,
checkpoint formats, replay persistence, or native code hot patching.

## Trial Evidence

RGF-U5 implements the code-first subset in `nara_app`: sealed-App admission, an unpublished
candidate, startup-before-promotion, non-reused generations, safe-point controls, exact fixed-tick
stepping, sticky typed faults, explicit move-only close obligations, and retryable finite close.
`nara_winit` drives that runtime instead of raw `App`; the runtime remains a thin owner around one
App and imports no project, content, tooling, window, or renderer policy.

RGF-U26 freezes the equivalent manual reference-game task and failure cuts. RGF-U24 then adds one
private product start attempt and obligation ledger, completes guarded scene/startup work while the
candidate is unpublished, and linearizes final fault observation plus owner/visibility transfer
through a single-use `RuntimePublicationSlot`. `HeadlessRun` hides that choreography, preserves
fresh generations, and retains incomplete cleanup for later bounded drives. Product and manual
paths prove the same plan, command, authoritative first tick, pre-owner rejection, late-hook
rejection, and incomplete-retirement custody semantics.

This evidence is intentionally incomplete. U13 now provides desktop product parity and U17 provides
Editor ownership, but U23 has not independently decided the runtime and Host proposals or their
compatibility. The ADR therefore remains Proposed; landed type names and tests are Trial evidence,
not authority for an unproven universal topology or final public API.

RGF-U17 adds the Editor trial: root `EditorProjectSession` owns preparation, start attempts,
published runtime generations, controls, close evidence, and retirement while `nara_tooling` and
egui retain only UI-neutral commands/views/results. Cancel during Starting retains the attempt until
retirement, normal cleanup continues across frames, close failure prevents false-success Restart,
and runtime edit/Apply Changes execute at generation-stamped safe points. Together with U13 this
completes the named product-path evidence input, but it does not decide process-global execution
exclusion, plan-versus-World registry authority, overlapping runtimes, or the final Host/runtime
combination. RGF-U23 remains the sole decision gate, so this ADR remains Proposed.

## Decision

If accepted, one executable runtime will be a thin lifecycle owner around exactly one
`nara_app::App`.

```mermaid
flowchart TD
    Host[Editor / desktop / headless host]
    Recipe[Validated replayable runtime recipe]
    Start[Host-owned runtime start attempt]
    Ledger[Attempt ownership / obligation ledger]
    Prepare[Authority-free plugin preparation]
    Reservations[Host-issued inactive service reservations]
    Commit[Fresh App closed commit and seal]
    Candidate[Private unpublished runtime candidate]
    Publish[Atomic infallible publish-and-promote]
    Runtime[Executable runtime: generation, state, control, fault, close]
    App[One nara_app::App]
    World[One live simulation World]
    Sessions[Runtime-owned service sessions or host-issued leases]
    Docs[Project documents and edit workspace]

    Host --> Recipe
    Host --> Start
    Recipe --> Start
    Start --> Ledger
    Ledger --> Prepare
    Prepare --> Reservations
    Host -->|issues through domain Adapters| Reservations
    Reservations --> Commit
    Commit --> Candidate
    Ledger --> Candidate
    Candidate -->|owns before startup| App
    Candidate --> Publish
    Publish --> Runtime
    Candidate -->|owns before publication| Sessions
    Runtime -->|owns after publication| App
    App --> World
    Runtime --> Sessions
    Host -->|single driver authority| Runtime
    Docs -->|immutable validated snapshot| Recipe
```

The conceptual names `RuntimeCandidate` and `RuntimeInstance` may be used by implementation and
plans, but this ADR freezes their unpublished/published ownership contract, not final public type
names or module layout.

The conceptual name `RuntimeStartAttempt` denotes the unique Host-owned operation that owns the
ledger, prepared owners, reservations, and any partial App before candidate admission. Only after
the App is successfully committed and sealed does the attempt contain a private
`RuntimeCandidate`, which it retains until publication or terminal retirement. It is not a Cargo
build handle, a cloneable reference, a tooling-view value, or a second active runtime. A concrete
Host may keep the type private or advanced; ordinary game authors and `nara_tooling` models do not
need it.

### Ownership

- An unpublished `RuntimeCandidate` exclusively owns one sealed, unstarted `App` plus every
  explicitly registered runtime close obligation. Admission rejects an active hook, an
  already-started App, a raw runner that can bypass runtime driving, or any first-party or
  third-party obligation-bearing declaration whose owner was not registered. Arbitrary App/World
  resources are caller-owned by default; the runtime cannot infer shutdown semantics from their
  Rust types. Immediate runtime-local cleanup may remain in the once-only plugin shutdown hook;
  owners that require Host-issued authority, asynchronous progress, or waitable close must register
  a typed close obligation before App sealing.
  After successful startup and every fallible publication precondition, one atomic, infallible
  publish-and-promote move makes the same owner the Host-visible executable runtime. No
  promoted-but-unpublished owner or fallible hook exists across that boundary. `App` remains the
  only owner of schedules, plugin lifecycle, time domains, and simulation-`World` mutation.
- The executable runtime does not register systems or plugins, expose a second scheduler, or keep a
  second time model. It delegates frame/fixed execution to its `App`.
- The host owns project/edit documents, validated settings, the runtime recipe, and reconstruction
  authority. Those values do not become mutable runtime ECS state.
- Each runtime has a non-reused generation. Runtime-owned identity domains, time debt, command and
  task queues, mutable assets, backend sessions, control requests, and faults are not shared across
  generations. Deliberately shared caller-owned resources remain outside the runtime's isolation and
  `Stopped` proof unless the caller explicitly transfers a close obligation at App sealing.
- Immutable, version-stamped project/schema/catalog snapshots may be shared only when the runtime
  recipe proves the same project revision and every consumer treats them as immutable.
- Active runtime inspection and mutation use bounded observation and safe-point commands. Tooling
  does not retain unrestricted mutable access to the runtime `World`.
- The concrete Host owns the `RuntimeStartAttempt`, the published `RuntimeInstance`, and any
  stop-first replacement intent. `nara_tooling` owns commands, views, observation policy, and Apply
  Changes models; it does not store either authority-bearing owner. This leaves in-process and
  child-process Play as replaceable Host Adapters rather than public tooling topologies.
- The platform event loop remains host-owned. It supplies events and elapsed real time through the
  runtime drive contract rather than becoming a second runtime owner. It does not retain `&mut App`
  or invoke `App::run_once` behind runtime control, fault, and close state.
- The selected Platform/Runner Adapter may be first-party or external. It is chosen before runtime
  candidate construction, drives `RuntimeInstance` through the same public control/drive/close
  contract, and cannot be replaced from a plugin hook. External candidates are not filtered by
  crate path or first-party ID.

### Construction and Publication

On the integrated product path, ADR 0082 or an explicit successor supplies one lineage-compatible
immutable project revision, resolved composition plan, replayable recipe, and immutable
service-admission requirements. A code-first caller may instead supply a directly configured sealed
App without a raw installed runner and without adopting those outer scopes. In either path, the
concrete owner begins one `RuntimeStartAttempt`, reserves its exclusive logical publication
slot/epoch, and establishes one
ownership/obligation ledger before the first fallible preparation, hook, or acquisition. This ADR
then owns the complete staged admission DAG and admits a sealed App plus that ledger into an
unpublished candidate before registry/scene/startup work:

1. verify that revision, plan, recipe, compiled capabilities, and service requirements describe one
   generation and the already validated dependency DAG;
2. under the existing ledger, privately materialize a fresh move-only plugin owner from every
   repeatable definition, preserve its exact definition/configuration key, and retain no
   installed/live plugin object in the recipe;
3. request host-issued and runtime-local inactive reservations through concrete domain Adapters,
   transferring each successful acquisition directly into the start attempt;
4. construct one fresh `App`, commit plugin build/finish, seal it, then move it and the complete
   registered ledger into one unpublished `RuntimeCandidate`;
5. freeze required schema/registry state through candidate-scoped admission;
6. bind and initialize required service sessions from the reserved authorities in dependency order;
7. preflight and spawn the selected scene snapshot;
8. complete startup schedules, initial diagnostics/status, stale-revision checks, and every other
   fallible publication precondition inside the candidate;
9. atomically and infallibly publish-and-promote the candidate into the Host's visible runtime slot
   only after every required predecessor succeeds.

These are dependency stages, not permission to consume a service early. Plugin build/finish may
declare service requirements but cannot use an active gameplay-facing session; scene spawn and
startup may consume a session only after its activation predecessor has succeeded. A future domain
may subdivide a stage, but it must preserve the ledger -> pure-validation/plugin-preparation ->
inactive-reservation -> App commit/seal -> candidate/freeze -> service-activation -> scene -> startup/
publication-preflight -> atomic publish-and-promote dependency edges.

Failure before publication returns a typed startup and shutdown report and publishes no runnable
session. Before step 4 completes, the start attempt retires its partial App, prepared owners,
inactive reservations, and other obligations directly through the ledger; no
`RuntimeCandidate` exists. After step 4, the start attempt retains the candidate and retires its
App, activated sessions, and remaining resources through the same ledger in reverse admitted
dependency order. The final boundary is one ownership and visibility cut: it consumes the
candidate directly into the Host's visible `RuntimeInstance` slot, keeps the complete ledger and
`App` under that owner, and leaves the attempt with access to neither. There is no separately
fallible promotion or final-publication phase. Publication atomicity does not claim that arbitrary
external effects are reversible, and the ledger remains shutdown/fault traversal rather than a
service locator.

That cut compare-consumes the attempt's non-reused publication epoch and exclusive slot exactly
once. A stale epoch, duplicate call, conflicting slot owner, cancelled attempt, or completion that
arrives after cancellation rejects before consuming the candidate and cannot make a runtime
visible.

### Start-Attempt and Runtime State Machines

Startup belongs to the unpublished `RuntimeStartAttempt`, not to a partially published
`RuntimeInstance`:

```mermaid
stateDiagram-v2
    [*] --> Starting
    Starting --> Ready: staged admission DAG succeeds
    Starting --> Retiring: required phase fails or cancellation applies
    Retiring --> Retired: every attempt-owned obligation retires
    Retiring --> RetirementIncomplete: shutdown error or deadline
    RetirementIncomplete --> Retiring: drive remaining retirement
    Ready --> [*]: atomic publish-and-promote RuntimeInstance
    Retired --> [*]: return failure or cancellation
```

`Ready` is consumed exactly once by the atomic publish-and-promote boundary into the only visible
runtime generation. A
`RetirementIncomplete` attempt remains owned and cannot yield a runtime or permit a conflicting child
lease until retirement succeeds or the process authority is torn down.

The published `RuntimeInstance` begins at `Running`:

```mermaid
stateDiagram-v2
    [*] --> Running: publication
    Running --> Paused: pause applies at safe point
    Paused --> Running: resume applies at safe point
    Paused --> Stepping: exact fixed-tick step accepted
    Stepping --> Paused: complete transaction succeeds
    Stepping --> Faulted: transaction or service fault

    Running --> Faulted: frame, domain, or service fault
    Paused --> Faulted: allowed real-time or service stage fault

    Running --> Stopping: stop, exit, or host close
    Paused --> Stopping: stop, exit, or host close
    Faulted --> Stopping: dispose requested
    Stopping --> Stopped: plugin shutdown attempted and every close participant completes
    Stopping --> CloseIncomplete: unfinished participant, participant error, or deadline
    CloseIncomplete --> Stopping: drive remaining close
    Stopped --> [*]
```

`Stopped -> Starting` is not a transition on the same object. Restart is a concrete Host operation
that stops the old runtime, refreshes the exact recipe if required, and begins a distinct start
attempt with a new generation.

Driving incomplete retirement or close polls only unfinished participants. It never invokes a
once-only plugin shutdown hook or already-attempted owner a second time; an unrecoverable failure
may remain incomplete until process authority is torn down.

A once-only plugin shutdown hook failure is terminal teardown evidence rather than an unfinished
close participant. If every separately registered close obligation completes, the runtime reaches
the `Stopped` ownership state, while the active Stop/RetryClose ticket records
`Failed(CloseFailed)` and a platform Host returns a distinct teardown error. `Stopped` therefore
proves ownership terminality, not that every teardown hook succeeded.

Control has two observable layers:

```text
request result: Accepted | Rejected
operation result: Pending | Applied | Failed
```

- Control requests apply only at declared safe points. Rejected requests make no partial change.
- Stop is idempotent. A Stop requested during a system or fixed transaction waits for the current
  transaction boundary before close begins.
- Restart while a start attempt, `Stepping`, `Stopping`, or incomplete retirement owns authority is
  rejected. The Host may retain one pending restart intent and continue it after the current owner
  reaches an allowed terminal state.
- A `Faulted` runtime executes no further gameplay frame. It permits bounded observation and close.

### Frame, Pause, and Exact Step

- The host provides real elapsed time; the runtime delegates one app-frame transaction to `App`.
- `Running` uses ADR 0039 normal bounded fixed catch-up and frame semantics.
- `Paused` continues explicitly allowed real-time work such as task polling, diagnostics, window
  housekeeping, asset observation, and backend liveness. It does not advance ordinary virtual or
  fixed simulation.
- Exact step is accepted only from stable `Paused`. It executes one complete fixed
  `Prepare -> Simulate -> Finalize` transaction and one gameplay
  `Admit -> Consume -> Capture -> Acknowledge` transaction, then returns to `Paused` only on
  success.
- Exact step preserves pre-existing whole-tick debt, sub-tick remainder, interpolation state,
  pause state, and time scale. It does not mean one render frame.
- Change/removal trackers rotate at the same complete transaction boundary defined by `App`; a
  wrapper cannot add a second tracker rotation.

### Fault Semantics

- The first structured runtime fault is sticky. Later observations may add bounded diagnostics but
  do not replace fault provenance.
- Gameplay ingress poison, active-batch invariant failure, admission/acknowledgement failure,
  fallible system failure, required task integration failure, and required service failure move the
  runtime to `Faulted`; they are not discarded or represented only by logs.
- A system or service fault may occur after partial runtime mutation. Nara does not claim in-place
  rollback. The failed generation is observed and discarded through normal close.
- Panic recovery is not part of this proposal. A future panic-containment contract must state
  unwind, process-abort, thread, native callback, and invariant consequences explicitly.
- Logs and tracing may mirror runtime faults but are not the queryable source of truth.

### Close and Restart

- `Plugin::shutdown` may complete immediate non-blocking teardown or initiate close work. It must not
  hide an unbounded wait from the host.
- Waiting services expose pollable, deadline-bound close participants. The host can continue
  pumping allowed real-time/platform work while close progresses.
- Only completion of every registered close participant permits `Stopped`. A timeout, unfinished
  worker owner, live native lease, or participant failure remains `CloseIncomplete` and prevents a
  conflicting replacement. A terminal plugin hook failure remains separately observable after
  ownership reaches `Stopped`. Abnormal Drop may transfer an unfinished owner to process-owned
  quarantine, but that fallback is never clean-stop evidence.
- `Drop` is best effort and cannot be used as product evidence that a runtime stopped cleanly.
- Editor Close Scene, external reload, Restart, a second Start Play, and editor exit all obey
  stop-first. Failure retains the faulted or incomplete owner and diagnostics instead of silently
  removing it.
- A fresh restart contains none of the old mutable world, queue, task, time, service, backend, or
  identity-domain state.
- This ADR owns retirement of runtime-scoped sessions and host-issued candidate leases. ADR 0082
  owns whether the parent process/domain authority may outlive or be shared by later runtimes.

### Deliberately Deferred

This proposal does not define:

- system-by-system stepping or schedule topology exposure;
- persistent checkpoints, replay artifacts, or backward debugging;
- compatible native function hot patching or state migration;
- a public universal runtime trait;
- a public object-safe runtime factory before two genuinely substitutable producers exist;
- concurrent multi-driver execution;
- a second render ECS world or Bevy `SubApp` as the Play runtime boundary.

This does not defer external runner reachability. The managed path guarantees explicit
Platform/Runner Adapter selection and one driver authority; only the exact trait/object/factory
shape remains evidence-driven. Direct `App::set_runner` plus `App::run` is a separate embedding path
and cannot be combined with managed-runtime admission for the same App.

## Alternatives Considered

### Option A: Make `App` the Complete Runtime Owner

**Pros**: Fewest types; `App` already owns the world, schedules, runner, and plugins.

**Cons**: Builder/configuration, active generation, host leases, fault/close state, and restart
recipe accumulate in one public object. Faulted/stopped ownership and unpublished candidates become
hard to represent.

**Decision**: Rejected for the product lifecycle while preserving `App` as the deep execution
module.

### Option B: Wrap One `App` in a Thin Executable Runtime Owner

**Pros**: Keeps one execution authority, gives editor/headless/desktop a shared lifecycle, and makes
generation, fault, control, close, and fresh reconstruction explicit.

**Cons**: The wrapper can drift into a second `App` if its scope is not constrained.

**Decision**: Proposed.

### Option C: Let Each Host Own `App`, Services, and Control Independently

**Pros**: Minimal common infrastructure and maximum platform freedom.

**Cons**: Editor, desktop, and headless semantics diverge; bare-world Play, destructor shutdown, and
silent session replacement remain possible.

**Decision**: Rejected.

### Option D: Use Bevy `SubApp` as the Runtime Boundary

**Pros**: Reuses a mature App-owned secondary world/update mechanism.

**Cons**: A `SubApp` is a sub-pipeline within one owning App, not an isolated Play generation,
project recipe, service-shutdown owner, or fresh restart boundary.

**Decision**: Rejected for executable-runtime ownership. Internal render extraction may still use
an equivalent private optimization later.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Startup publication | Failure in plugin/freeze/startup/spawn/service/publication-preflight phases publishes no runtime session; the final publish-and-promote cut is infallible and compare-consumes one current attempt epoch | Start-attempt fault-injection, stale/duplicate/late-epoch, and binary publication-cut tests |
| Ownership handoff | Every registered obligation is owned exactly once: retired by the start attempt before candidate admission, retired through the candidate afterward, or moved by the atomic publication cut into the visible runtime | Attempt/candidate-admission, publication-cut, and close-order tests |
| App admission | Unsealed, already-started, raw-runner, or declared obligation-bearing Apps without registration reject before candidate admission; arbitrary resources remain caller-owned | Code-first admission tests |
| Play execution | Editor Play runs startup and scheduled systems through `App`, with no bare-`World` session owner | Tooling integration and static API audit |
| Driver parity | Editor, desktop, and headless use one frame/fixed transaction for the same command stream | Reference-game semantic snapshot tests |
| Driver authority | First-party and renamed-dependency external Platform/Runner Adapters are explicitly selected, drive `RuntimeInstance`, and cannot call raw `App::run_once` behind it or coexist with an installed raw App runner | Clean-room runner, boundary, mutual-exclusion, and static-audit tests |
| Exact step | One accepted request advances exactly one complete fixed/gameplay transaction and returns to `Paused` | RGF-U5 exact-step tests |
| Fault closure | Every named gameplay/system/task/service failure reaches sticky runtime `Faulted` | Fault matrix tests |
| Runtime isolation | Two generations share no mutable World/queue/task/time/service/backend/identity state | Reconstruction tests |
| Finite close | Never-completing shutdown does not block the host and never reports `Stopped` | Deadline/shutdown fixture |
| Stop-first workspace | Close/reload/restart/second Play/editor exit cannot silently drop or replace a live/failed owner | Workspace state-machine tests |
| API authority | Runtime wrapper exposes no system/plugin registration or independent schedule/time API | Public API and dependency review |
| Early ownership value | The minimal candidate/runtime path closes named fault/ownership gaps without exceeding the public-concept, caller-glue, or lifecycle-state limits against the independently frozen manual counterfactual | RGF-U26 baseline plus RGF-U24 reversal matrix and independent review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Runtime wrapper becomes a second `App` | Critical | Medium | Prohibit schedule/plugin/time ownership; delegate exactly one `App`. |
| Plugin and runtime states drift | High | Medium | Treat plugin lifecycle as a startup/close sub-state, not a second product state machine. |
| Startup atomicity is mistaken for rollback | High | Medium | Promise unpublished candidate plus shutdown report, not reversal of external effects. |
| Attempt, candidate admission, or publication loses or double-owns a close obligation | Critical | Medium | Establish one ledger before fallible work, retain partial state in the attempt, move it once with the sealed App into the candidate, and atomically publish-and-promote the same owner; the attempt retains no published copy or intermediate owner. |
| Faulted system leaves a partially mutated World | High | Medium | Stop execution, preserve first fault, observe if safe, and discard the generation. |
| Direct tooling access bypasses safe points | High | High | Replace unrestricted Play-world mutation with commands and bounded observations. |
| Shutdown freezes editor or server | High | Medium | Use pollable close participants and deadlines; keep host pumping. |
| Recipe captures one-shot/runtime data | High | Medium | Restrict it to validated immutable inputs and reconstructible factories. |
| Platform event-loop constraints leak into runtime | Medium | Medium | Keep the event loop in the host/driver and pass normalized input/time/control. |
| Embedded shared resources invalidate isolation claims | High | Medium | Treat arbitrary resources as caller-owned, require explicit transfer for runtime close obligations, and scope isolation/Stopped evidence to registered runtime-owned state. |
| The wrapper adds more concepts than the failures justify | High | Medium | Freeze U26's task-equivalent manual raw-App path before Host implementation, then make U24 compare the same content, plan, command, fixed-tick task, and failure cuts before Editor/desktop diffusion; reject or simplify if the ordinary concept or ownership comparison fails. |

## Consequences

If accepted:

- ADR 0003 remains authoritative for `App`, plugin, schedule, and world ownership;
- ADR 0034's isolated Play decision remains, but its bare `World` session becomes an obsolete
  transitional implementation;
- ADR 0039 remains the time/frame transaction authority;
- ADR 0057 remains the fixed-tick gameplay transaction authority and its faults must reach runtime
  failure;
- ADR 0076 retains observation, cursor, stepping research, checkpoint, replay, and hot-patch scope;
  this ADR becomes the canonical runtime lifecycle subset;
- ADR 0081 structural catalog replacement uses a fresh runtime recipe/generation rather than
  unfreezing an active registry;
- an Accepted ADR 0082 or explicit successor remains the authority for outer process/project scopes,
  service-authority placement, and parent/child lifetimes. This ADR is independently authoritative
  for its executable-runtime node: candidate stages, publication, runtime-scoped session retirement,
  fault, close, and restart.

No existing ADR is marked superseded while this proposal remains non-authoritative. Acceptance
must add reciprocal refinement metadata and update implementation evidence.

## Admission Evidence

RGF-U5 has implemented the code-first candidate/runtime trial; RGF-U24 implements the concrete
headless Host/candidate trial and its U26 reversal matrix. Acceptance still requires
RGF-U17's Editor command/view ownership, RGF-U13's desktop Adapter evidence, and every success metric
above through the independent RGF-U23 review; the existence of a wrapper does not make the ADR
authoritative. A wrapper type, state enum, or bare-world adapter without scheduled execution, fault
propagation, explicit obligation ownership, and finite shutdown is insufficient. ADR 0082 may be
accepted or rejected independently. Product use with an outer Host requires an Accepted ADR 0082 or
explicit Accepted successor plus compatibility evidence; a failure there does not erase otherwise
valid code-first runtime evidence.

## Citations

- Bevy App ownership: `repo-ref/bevy/crates/bevy_app/src/app.rs`
- Bevy SubApp scope: `repo-ref/bevy/crates/bevy_app/src/sub_app.rs`
- Godot explicit main-loop lifecycle: `repo-ref/godot/core/os/main_loop.h`
- Godot SceneTree runtime boundary: `repo-ref/godot/scene/main/scene_tree.h`
- Active implementation slice: [Reference-Game-Driven Foundation Plan](../../plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md)

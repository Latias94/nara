# Owner-Local Dependent Close

## Decision And Scope

Use a domain-owned typed composite for a fixed dependent aggregate. Admit concrete local close
participants through the existing managed Runtime owner; do not require them to implement `Send`.
Keep transportable definitions separate from bound protocol owners. Do not introduce a public
close dependency graph for the currently demonstrated consumer.

ADRs 0003/0010/0042/0082/0084 own the durable contract. This design explains its comparison and
the remaining native integration work. R4a is not complete merely because a local Rust controller
can now register.

## First Principles

A consumer may still need its provider after close has been requested. Therefore callback order
cannot authorize provider destruction. The decisive fact is the end of the dependent consumer's
physical responsibility, including cleanup work and disposal that can still touch that provider.
Error severity is a separate axis: an error can accompany either an unfinished owner or a fully
retired one.

Quiescence closes admission of new work. It must preserve the event pump, executor, return path
and parent authority needed to finish admitted work. A fixed aggregate owns those dependencies
or holds explicit leases to an outer owner that retains them. It keeps child progress and error
facts, drives unfinished children, and releases a provider only after its users physically finish.
The runtime sees the aggregate's close result; its private domain report keeps child details.

## Executed Incumbent Characterization

`crates/nara_app/tests/close_dependency_characterization.rs` uses a safe resource stand-in, not a
native handle. The public Runtime API establishes these current facts:

- Reverse independent registration invokes the consumer first, but a Pending consumer does not
  stop the independent provider from releasing. A later consumer poll sees the released resource.
- A terminal incomplete consumer remains owned and is never invoked again; the independent
  provider still releases. A bounded disposable child observes this permanent responsibility
  without pretending it can be drained or using it as cleanup evidence.
- `CompleteWithError` permits physical retirement and keeps the error cause.

Those entries declare no dependencies. The first two are pressure for a stronger composition
contract, not a claim that the incumbent violates its existing independent-registration contract.

## Same-Consumer Comparison

`crates/nara_app/tests/native_close_owners.rs` compares two private candidates with the same
owner-local `Rc` controller and provider, including Pending, retryable error, terminal incomplete
error, ordinary completion and completion with error.

| Candidate | What it owns and exposes | Selection consequence |
| --- | --- | --- |
| Typed composite | Concrete consumer/provider fields, one quiesce entry, explicit completion gate, child report | Expresses the fixed dependency directly. An independently supplied concrete controller needs no runtime IDs or central ledger knowledge. |
| Two-node sealed plan | Erased nodes, declared IDs/prerequisites, validation and retained rejected entries, then quiesce and gated release | Can express the same dependency in either declaration order and reject missing/cyclic declarations. This bounded comparison does not qualify a general graph or arbitrary independent branches. |

The composite does not need the plan's registration vocabulary for this consumer. Keep that
vocabulary private to the comparison. Reconsider a sealed plan only when independently contributed
lease owners require central contributor-specific changes to the typed owner. Such a successor
must also retain rejected/partial registrations, distinguish same-ID owners, validate the complete
Host/plugin collection, and keep independent branches progressing after a failure.

This is an ownership and authoring comparison, not a performance benchmark or a native SDK test.
The supplied controller is a separate module; the renamed-root public consumer additionally
checks the selected boundary from an independent Cargo workspace.

## Local Custody Through The Existing Runtime

The local participant and obligation ledger remain on their construction thread. App, sealed
App, candidate, runtime, failure/retirement wrappers and quarantine already retain that affinity.
No additional owner, background reaper or unsafe thread transfer is needed. The ledger's former
`Send` promise is removed; its compile-fail documentation protects the new boundary. Immutable
definitions and world-free control/fault/admission values retain their transportability.

The relevant proofs include:

- standalone duplicate registration returns the exact local controller without invoking it;
- rejected App registration retains its local controller through physical close;
- finite incomplete finish, explicit retry and startup failure keep the aggregate and provider;
- normal Drop, incomplete-retirement Drop and unwind retain callback/destruction affinity;
- owner-thread quarantine keeps unfinished custody until an explicit later drive;
- completed errors retire the provider while retaining aggregate and child error evidence;
- `tests/fixtures/runtime-runner/renamed-root/tests/local_close.rs` consumes the same public
  owner boundary through the renamed `engine` dependency.

Verification results and review findings belong in the implementation ledger's linked evidence.
These checks model a controller whose progress is externally signalled; they do not prove an
arbitrary SDK can close without its event loop.

## Concrete Native Follow-Up

`NativeImgui` is an Editor Host aggregate, not a Play-generation resource. The incumbent at
`ccc1318` used a manually ordered best-effort `shutdown_graphics` sequence, and `retire_native`
dropped the aggregate after logging an error. Do not move it into a Play Runtime to reuse the
local ledger.
The [native outcome inventory](../research/2026-10-25-native-graphics-retirement-outcomes.md)
distinguishes preflight refusal, retained shutdown and errors reported after release in the actual
locked GPU/toolkit/platform implementations; it is source evidence for the next native unit.

The selected native integration keeps a live aggregate and a retained retiring predecessor in
the concrete Editor Host. Installation refuses while either slot is occupied. Transferring the
live owner to retirement stops rendering admission but keeps its Context, toolkit, exact device,
queue, primary window and event loop available. The phase owner is shared by successful and
partial construction, rather than a second teardown path for each constructor failure.

| Phase | Completion rule and retained custody |
| --- | --- |
| Quiesce | Save the layout before toolkit teardown, invalidate input and finish an open frame. |
| GPU drain | Capture device/queue before renderer construction. Flush initialization uploads once, then nonblocking poll. An invalidated-device discard explicitly destroys that exact device, then still polls for actual queue-empty completion of accepted work. |
| Renderer | Drive the owned SDK route to successful shutdown. A refusal retains the whole aggregate. An error returned after SDK release remains evidence; the idempotent next success confirms the adapter phase. Successful adapter shutdown clears its own binding clones and secondary observations. |
| Views | Bulk renderer shutdown already revoked registrations. Retire Scene/Game views and their presentation authority without fallible per-texture unregister choreography. |
| Platform | Drive platform shutdown after renderer completion; retain Context and window while refused. |
| Release | Drop the complete aggregate before admitting a replacement. Existing surface/provider leases still enforce their own release and acknowledgment rules. |

The drain token survives backend namespace invalidation and failed renderer construction. A failed
native route attachment returns its original docked renderer instead of trying cleanup and losing
custody inside an error conversion. Constructor failure before a renderer exists still retains the
captured GPU drain when initialization may have queued uploads. A first-party Host consumes these
failures and retains all successfully acquired owners until their phases complete.

Semantic Exit and native retirement are separate facts. A pending document decision keeps a live
frontend for Save/Discard/Cancel. The Host observes the Application's current exit custody rather
than inferring cancellation from a retained frame result. Only an actual `exit_ready` update grants
final process exit; pending close and native retirement retain maintenance wakes even without a
redraw. GPU reconstruction needed for a close decision preserves that intent and frontend state.
Cancellation returns control to a live frontend or reinstalls one after a predecessor retires.
An Exit arriving during a queued rebuild preserves that installation as a decision frontend; it
cannot overwrite reconstruction with a state that waits for UI it just removed. Failed installation
retains the decision but stops automatic retries until the explicit recovery action.

Normal exit ends the event loop after both semantic and native completion. A terminal event-loop
failure gets a finite local cleanup attempt; if SDK custody remains unfinished without the lost
driver, the executable fails stop rather than entering unsafe destructor ordering. This is not
arbitrary exception recovery or a new native quarantine facility. The Runtime's separately
qualified owner-thread quarantine remains unchanged.

The Host retains bounded, deduplicated phase errors, with explicit truncation/overflow evidence;
it cannot reconstruct errors already discarded inside the SDK. A final primary product error
keeps its typed source when secondary cleanup facts are attached. These facts do not grant
release permission. Native tests must qualify ordinary close, pre-presentation exit, partial
initialization, exposed completed-error retry, device loss and explicit recovery on the actual
affected platform. Outstanding detached epochs and arbitrary native teardown failures must retain
their own honest proof scope; safe synchronous Context borrowing is not bypassed to manufacture a
test.

The [native successor evidence](../knowledge/engineering/verification/2026-10/2026-10-25T145420Z-retained-editor-native-graphics-retirement-and-physical-completion-qualification-2665523023e84aba979458cba3f2affc.md)
records the concrete Host implementation and selected Windows/DX12 trajectories separately from
the earlier Rust models. R4b facility sharing/activity scopes, R4c execution phases and dynamic-code
custody remain separate work; selected native evidence does not close every SDK failure branch.

## Pinned References

- Nara incumbent and concrete aggregate: [source study](../research/2026-10-25-native-close-dependency-ordering.md).
- Bevy `85503941e488152711a4695bbe248e9ed9f453fc`,
  `crates/bevy_render/src/pipelined_rendering.rs`: returning the Render App to its owner thread
  while pumping the main-thread executor preserves the ability to finish outstanding work.
- Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558`,
  `servers/rendering/rendering_server_default.cpp` and `main/main.cpp`: render finishing and the
  pump-task completion barrier precede DisplayServer destruction in the fixed coordinator.

These concrete mechanisms support explicit completion and retained execution capability. Their
blocking waits are not adopted as Nara's responsive close API.

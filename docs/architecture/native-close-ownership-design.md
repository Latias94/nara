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

`NativeImgui` is an Editor Host aggregate, not a Play-generation resource. Its current
`shutdown_graphics` is a manually ordered best-effort sequence and `retire_native` drops the
aggregate after logging an error. Do not move it into a Play Runtime to reuse the local ledger.
The [native outcome inventory](../research/2026-10-25-native-graphics-retirement-outcomes.md)
distinguishes preflight refusal, retained shutdown and errors reported after release in the actual
locked GPU/toolkit/platform implementations; it is source evidence for the next native unit.

Before changing that path:

1. Classify actual GPU/toolkit/platform shutdown outcomes by physical completion and retained
   leases. An ordinary `Result<(), Error>` alone does not establish incomplete responsibility.
2. Separate stopped admission from release while retaining any required window/event pumping.
3. Keep a concrete retiring native aggregate in the Editor Host while close is incomplete; refuse
   conflicting reinstallation, and retain all relevant errors rather than only the first.
4. Reuse existing surface/provider typed leases and completion acknowledgments. A new graph must
   not replace a resource owner's release authority.
5. Qualify normal close, partial initialization, delayed cleanup, failed cleanup, device loss and
   explicit recovery on the actual affected native platform.

R4b facility sharing/activity scopes and R4c execution phases remain separate work. Neither the
local trait change nor the comparison closes native graphics migration or dynamic-code custody.

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

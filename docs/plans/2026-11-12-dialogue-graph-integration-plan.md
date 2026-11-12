# Dialogue Graph Domain And Native Integration

Status: In progress.

Owner: the R11c consumer in the
[foundation plan](2026-10-23-foundation-architecture-rebaseline-plan.md).
The [engine handoff](2026-11-12-nara-ui-integration-handoff.md) and
[Jellyflow audit](../research/2026-11-12-jellyflow-node-editor-audit.md) define the starting
evidence. Accepted ADR0118 retains document, Session, history and persistence authority.

## Complete Consumer

Build an independently compiled Dialogue Graph in `tests/fixtures/gpui-engine-consumer`.
One Nara extension document owns lines, choices, endings, authored positions, stable node/choice
identities, links and its inverse/history. Two native Views and an ordinary property editor
consume that same owner. A conversation preview supplies a real domain result.

This is a new document type and versioned source format. Preserve the existing list-of-lines
Dialogue format and finite ValueGraph controls. Do not reinterpret their persisted data.
Dialogue cycles are legal; a preview operation follows at most one explicit transition.
Incomplete links and an unset entry are legal authoring states with explicit preview outcomes.
Duplicate identities, foreign choice owners, dangling links and invalid source facts refuse
before changing source, allocator or history.

Node and choice IDs are stable source identities scoped to their document owner. Live forward
allocation never rewinds with Undo. Reopen creates a fresh document incarnation; old UI commands
must refuse even when the new source contains equal IDs and values. Choice order is authored
content, while port identity derives from the stable node/choice identity, never a row index.

Intrinsic height, native focus, text/IME, popups, selection, drag previews and viewport transforms
belong to each View. A completed multi-node move or choice/port/link edit creates one original
domain-history entry; cancellation creates none. Measured height does not become a history-free
source edit.

The domain View also carries live node, choice-port and link incarnations. They are excluded
from serialization and saved content identity. Undo may restore exact source bytes while
giving a restored entity a fresh incarnation, so a coalesced observer cannot revive a retired
native capability.

## Confirmed External-Document Interface Work

At Jellyflow `8a7c2df`, `ViewSession` and `WeakViewSession` hold `Weak<RuntimeShared>`.
The same object owns immutable snapshots and indices, history/command execution, connection
planning and View notifications. `submit_feedback` executes concrete private `GraphCommand`
payloads; the native journal also expects those concrete completions. The existing interface
cannot accept Nara's read-only document by substituting a snapshot getter.

Separate the read-side facts/indices/View publication from standalone mutation/history, then
give native interaction typed edit intent and identity-associated completion. Keep generic
graph queries and native lifecycle inside Jellyflow. Reopen and removal/restoration need
explicit live identity facts, not only a final set of IDs. Native intrinsic height also needs
a View-owned path: the current content-height owner emits a history-free geometry command.
These are implementation requirements of step 3 below, not existing library capabilities or
a newly selected universal protocol.

### Native Integration Audit

The subsequent source audit at Nara `b54229f` and Jellyflow `8a7c2df` identifies the actual
integration points. It changes implementation order without qualifying a new native consumer:

- `OpenGpuiSceneHost::new` already accepts `ViewSession`. Keep that Host and the existing
  Nara `Viewer::open` / `cx.open_surface` composition. External read-side publication must
  preserve snapshot equality, graph version and full node/port/edge lifetimes for the existing
  mount and authoring checks; rebuilding a fresh snapshot on every query is insufficient.
- Canvas interaction converges in `host/interaction_session.rs::bind_and_enqueue_interaction`.
  The command journal, `host/frame_feedback.rs::submit`, managed mutations and retained fields
  currently require synchronous `GraphCommand` completions. Give submitted external edits an
  exact pending/result path. The original Session outcome, rather than graph publication,
  value equality or an inferred next revision, settles the originating request once.
- `host/lifecycle.rs::close_with_request_in_app` is another command submission path. Closing
  must distinguish work not submitted from work already admitted by Nara. It must not execute
  an external command again or report admitted work as definitely unexecuted. Closing one View
  does not close the shared domain document.
- `content_size_submission_owner.rs::dispatch_prefix` currently commits geometry with
  `without_history()`. That remains a document write. External intrinsic size belongs to each
  View and must consistently feed semantic zoom, placement, complete real-port measurement,
  routing and accepted-geometry checks. A native wrapper's height alone does not update those
  consumers. Retain the existing prepaint-commit and layout-admission boundaries.

Use one original `Engine` / `EditorProjectSession`; process captured requests in the existing
`Viewer::drive`, release the Session borrow, then publish facts and settle the exact originating
request. Two graph Views, the property editor and preview remain independent local consumers.
Reopen retains the document slot but changes `revision.source_id()`, so document ID or GraphId
alone cannot keep old drafts, captures, popups or preview operations valid.

The current fixture's `DraftInput` deliberately replaces CR/LF with spaces. It can serve fields
whose policy is single-line, but cannot edit arbitrary Dialogue text/prompt without preserving
line breaks. Add a genuine multiline path with corresponding layout, caret/selection, IME and
accepted hit geometry; the existing full-byte domain tests do not establish native text fidelity.
Popup content likewise needs the existing overlay/focus/activity boundaries, outside-click and
Escape behavior, mount retirement and input isolation. Do not reduce the authored text contract
or substitute a bare canvas click for those controls.

Conversation preview can call the existing synchronous `Conversation::step`, `advance` and
`choose` after checking its captured document/revision. It needs no ValueGraph compiler,
background preview scheduler or additional GPU View. Source changes invalidate or explicitly
restart that preview without an authored edit.

The remaining external mutation interface must distinguish a proposed graph identity from an
identity allocated by the original domain. Standalone Jellyflow commands can propose node or
edge IDs; Nara owns node/choice allocation and derives a link's graph edge ID from its output
port. Bind canonical identity adoption to the exact original operation and its Session outcome.
Do not forward an opaque `GraphCommand` wholesale, guess the accepted ID from equal values or
infer acceptance from a published revision. Preserve that completion association through close,
cancellation and later independent domain edits.

## Execution

1. Implement the typed domain, atomic edits, bounded versioned byte codec, restorable content
   identity and one-step conversation preview. Use immutable snapshots with shared node content;
   retain the existing extension contract rather than adding an engine graph service.
2. Prove the complete public Session/file workflow: node and property entry points, exact-basis
   refusal, atomic dynamic choices and links, cross-entry Undo/Redo, return-to-saved clean state,
   branching, Save/Reopen and original Host retirement.
3. Open Jellyflow's external-document interface from this concrete consumer: immutable facts,
   local interaction, typed edit intent and exact completion. Preserve its independent writable
   runtime. Do not mirror two writable graphs, replace the document after every edit, add Nara
   dependencies to Jellyflow or forward its entire runtime through a speculative trait.
4. Mount ordinary native line/choice/end components with text, popup and variable-height
   content. Drive two Views and a separate property editor through the original Nara document.
   Reuse the existing logical root, renderer, device, queue, frame publication and close owners.
5. Independently review and qualify actual pixels/input on DX12 and Vulkan, preserving the
   original Scene, finite graph and standalone Jellyflow controls. Update the owning study,
   implementation ledger and handoff from actual results.

The domain and public Session workflow are prerequisites within this complete consumer.
Finishing them alone does not qualify the external-document native adapter or complete R11c.

## Acceptance

- Stable choice/real-port identity survives label edits and reordering. Add/remove/reconnect
  updates choice content and incident links atomically; refusals preserve source and history.
- Two Views and a normal property editor observe one source/history while keeping their own
  selection, draft, focus, popup and navigation state.
- Native text retains the complete supported buffer. Layout, accepted measurement, paint and
  input agree for variable-height nodes, scrolling, scale and clips.
- A completed captured node drag commits one exact-basis move. Escape, focus loss, stale basis,
  retirement and close cancel preview without a source edit.
- Undo through either entry point restores exact source and saved identity; a new edit after
  Undo invalidates Redo without reusing live allocated identities.
- Save/Reopen use the original Host and codec. Old document/command/preview observations refuse
  after replacement or close. The independent engine producer remains valid after UI close.
- Conversation preview executes actual line/choice/end transitions, including a deliberate loop
  and incomplete source, without an unbounded traversal or mutation of the authored document.

## Verification Scope

Use Rust `1.95.0`, `CARGO_BUILD_JOBS=1`, serial Cargo and the existing Nara `target`.
Start with public domain and Session tests, then affected native trajectories. Test changed
integration and ownership; do not rerun every unchanged upstream framework mechanism.
No remote publication, new Editor default, general Shader Graph, game UI choice or numeric
performance budget follows from this consumer.

## Qualified Domain And Immutable Facts

The November 12 prerequisite completes execution steps 1 and 2. Step 3 now includes immutable
Source publication into ordinary Jellyflow Views, as detailed below. External authored edit
intents/completions, View-local native intrinsic height and steps 4 and 5 remain open.

`consumer.dialogue-graph` implements the original `EditorExtensionDocument` contract. Schema 1
contains only authored entry, nodes and links. Limits are 512 nodes, 4,096 total choices,
4,096 UTF-8 bytes per text/label, 256 speaker bytes, a 2 MiB source and 64 shared Undo/Redo
entries. Snapshot moves share unchanged node bodies. Complete choice edits include ordering
and outgoing targets atomically; clients preserve unchanged choice IDs and targets.
Conversation preview follows one explicit transition per operation, including deliberate loops
and incomplete authoring states.

Stable authored IDs do not supply native capability lifetime. The original document separately
owns nonserialized node, choice-port and link incarnations; removal followed by Undo renews
restored lifetimes even if an observer misses the intermediate state. Reopen receives the
original Workspace's fresh document incarnation. Successful no-op commands preserve source,
saved-content identity, both allocators and Redo, while the original Workspace still advances
its public operation revision. Future adapters must retain the exact observed basis.

`DialogueFacts::capture` builds an actual immutable `jellyflow_core::Graph` from one captured
document/revision/source. Checked mappings retain full-width source IDs and reject foreign or
absent graph elements. Choice reordering and retargeting preserve stable port/edge identities.
Intrinsic size is absent. This capture holds no writable runtime, history, native Host or
submission authority. The optional core dependency is portable; native adapter dependencies
remain conditional on Windows.

| Verification | Actual result |
|---|---|
| Initial pure domain | Release nextest 21/21 passed; 45 older controls unselected; run `2ae016bd-2609-412e-892b-ba9b0a71cdce`. |
| Domain, immutable facts and public Session controls | Release nextest 75/75 passed, no skips, across three binaries; run `ed15941f-9ba7-4568-8bdc-b58afcda3798`. Includes 21 domain tests, four facts tests, four public Dialogue tests and 46 existing controls. |
| Conditional compilation | Locked release all-target check without default features passed. |
| Correctness and format | Locked release all-target Clippy with `jellyflow-trial,ui-work-observation` and `-D clippy::correctness`, plus package-scoped format check, passed with existing warnings. |
| Independent review | Pure domain/codec/validation/preview and facts/public Session files were reviewed separately; no concrete P1/P2 finding remained. Review is static, not native qualification. |

The first projection test build incorrectly attempted to serialize `Graph`, which intentionally
has no `Serialize` implementation. The corrected test compares sorted public node/port/edge
facts; it does not add another persistence format to Jellyflow.

Reproduce the combined CPU/public workflow from the Nara root with the documented shared
target and serialized Cargo environment:

```text
cargo +1.95.0 nextest run --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --features jellyflow-trial,ui-work-observation --lib --test graph_document --test dialogue_document --test-threads=1 --no-fail-fast --no-tests fail
```

The public tests use actual Session admission, files, captured Save attempts, Reopen and Host
retirement. Node/property entry points here are typed commands, not native controls. No new
Dialogue native trajectory ran for this prerequisite. The earlier standalone Jellyflow
DX12/Vulkan results retain their original scope and do not qualify the missing external-domain
interface.

## Private Read-Side Extraction

Jellyflow `d4125cc34247b897bacdb2e8099174dc2789cc8b` separates `GraphReadState` from the
standalone writer's history, profile, middleware and forward allocators. The read state contains the existing
graph `Arc`, revision, document epoch, policy, indices and node/port/edge incarnation maps.
Snapshot capture, lifetime queries, frame facts, port-connection queries and bounded connection
admission consume those facts directly. Original shared-state borrows and mutation/notification
guards remain at their existing entry points; no forwarding trait, duplicate graph or new
public interface is introduced.

Document replacement prepares the complete successor read state and index before publication.
Command execution, Noop, Undo/Redo, allocator behavior and actual query-work counters keep their
original contracts. In particular, a profile's presence can defer connection planning without
placing the profile callback or history inside the read state.

The first production check passed. The initial Runtime test build found one private impact
fixture still accessing the former field; moving that access to `state.read.graph` preserved
its transaction path and all assertions. The corrected release Runtime/public suite passes
599/599, no skips, run `cc14beff-c419-471e-8cce-30ea027a8bd7`. Independent source review found
no concrete P1/P2 in the extraction, including snapshot identity, publication, profile entry
and work-accounting paths.

The release adapter suite also passes 519/519, no skips, across 15 binaries; run
`30f849ce-5d10-4d58-8142-e11af7c4093c`. It preserves the existing managed edits, IME drafts,
overlay arbitration, measurement, frame publication and retirement controls. Runtime all-target
Clippy with `-D warnings`, root format check and whitespace checks pass. All Cargo invocations
were serial, with Rust `1.95.0`, one build job and the existing Nara target. The adapter's
dependency combination required a fresh build; no dependency version or lockfile changed.

```text
cargo +1.95.0 check --locked -p jellyflow-runtime --lib
cargo +1.95.0 nextest run --locked --release -p jellyflow-runtime --lib --test public_surface --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --locked --release -p jellyflow-open-gpui --lib --tests --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 clippy --locked -p jellyflow-runtime --all-targets -- -D warnings
cargo +1.95.0 fmt --all -- --check
```

Run those commands from the Jellyflow checkout. This qualification covers changed headless
owners and their existing adapter consumers. It adds no Nara native/GPU run, foreign-platform
execution, compiler-floor or performance claim, and does not qualify the unmerged integrity
repair draft. The source commit is local; the previously published Jellyflow baseline remains
`8a7c2df` at this checkpoint.

This was the private prerequisite within step 3. At that checkpoint, `ViewSession` still reached
its existing `RuntimeShared`. The public read interface below follows that extraction;
external command admission/completion, native intrinsic height and the complete Dialogue
consumer retain their separate acceptance.

## Public Immutable External Source

Implemented in local Jellyflow `046a66084e60367101a8703c2cae7626c6e8dd7e`.
Its previously published baseline remains `8a7c2df`; this prerequisite is not remotely published.

`ExternalGraphFacts` retains the original `Arc<Graph>` and validates structure plus complete,
unique, nonzero node/port/edge incarnation observations. `ExternalGraphDocument` publishes
those facts to existing `ViewSession`s without creating `GraphRuntime`, history or a codec.
Read, frame, geometry and bounded connection queries use a private source capability; authored
mutation still requires the original standalone writer or the external domain's future
intent/completion path.

Publication checks the exact previous opaque version and GraphId before installing the complete
successor. Equal authored values still advance its read version, accommodating a successful
Nara Noop with a new Workspace operation revision. Callers retain the original domain basis and
exact outcomes separately and do not republish an unchanged observation merely to read.
Stable incarnation observations preserve native keys; coalesced removal/Undo renews affected
keys. Explicit reset on Reopen retires every old native identity even for equal bytes and IDs.
Publication validates, compares and reindexes complete facts with ordered maps; it is not a
qualified incremental publication optimization.

Nara's `DialogueFacts::external_facts` shares its original graph allocation and maps the actual
domain's node, input/output-port and link incarnations. Four additional public Session/file
tests attach two Views, exercise successful Noop, coalesced remove/Undo and equal-content
Save/Reopen, and preserve one original source/history. Selection, navigation and accepted
layout remain local. Closing one View preserves the other and the domain.

Public testing also exposed an ownership gap in `submit_feedback`: an unsupported mixed
command/layout packet was consumed even before command admission. The library now returns
`FeedbackSubmissionFailure`, preserving the original cause and the original move-only feedback
only before admission. `FrameFeedback::split_commands` moves commands out while retaining the
original token and layout order. Both the library test and Nara consumer actually resubmit that
same returned token and publish its layouts; they do not manufacture a replacement frame.
After admission, no reusable packet exists and original partial receipts keep their meaning.

The first Nara run passed 7/8: the new coalesced-Undo test incorrectly expected rejection of
the entire old frame. Existing protocol deliberately classifies layouts per node. The corrected
control asserts the restored node's old layout is `Stale`, cannot republish old geometry or
mutate the domain, and a fresh frame publishes the new lifetime while the second View stays
independent. It preserves that protocol rather than widening production rejection.

External authored writes remain explicitly unavailable without closing a valid View.
Asynchronous canvas intent/completion, canonical ID adoption, automatic native content height,
multiline controls/popups and the two native Dialogue Views are still required. This public
read prerequisite does not complete step 3, R11c or the full foundation Goal.

| Verification | Actual result |
|---|---|
| Full Jellyflow Runtime/public release suite | 620/620, no skips; run `64c58d97-2287-4642-83b3-99a94752dabf`. Includes 15 crate-external Source and six new publication-impact tests. |
| Final public Source replay | 15/15, no skips; run `31d09c5e-7bdc-4565-b2b6-8e2836e51968`. |
| Retained and egui adapter regression suites | 560/560, no skips, across 23 binaries; run `4ef357f9-62d0-48df-af6e-d821b623a85f`. Retains the existing 519 retained and 41 immediate adapter controls. |
| Nara domain, graph and public Session/file cohort | 79/79, no skips, across three binaries; run `85ca360d-49a6-43e4-80e4-c8cf0f7c0d84`. The four new external Source tests preserve the earlier 75-test scope. |
| Conditional compilation | Nara fixture locked release all-target check without default features passed. |
| Correctness and formatting | Release all-target Clippy with `-D clippy::correctness` passed for Runtime, both adapters and the Nara fixture. Root/fixture format, whitespace and added local documentation links passed. Existing unrelated warnings remain. |
| Independent review | Producer and receiver source reviews found no concrete P1/P2 in publication, invalidation, notifications, feedback custody and adapter migration. All nine agents for this unit are closed. |

The first library production build found two `copied()` calls on deliberately non-Copy
incarnation wrappers; `cloned()` preserves those types. Its first full public test build also
found a nonexistent default viewport constructor, corrected to `ViewportTransform::new`.
Two retained Host tests also needed the same exact stale-frame cause patterns migrated to
`failure.error()`. No original lifetime or admission assertion was relaxed. All-target
correctness Clippy passes for Runtime and both adapters. The root command is:

```text
cargo +1.95.0 nextest run --locked --release -p jellyflow-runtime --lib --tests --test-threads=1 --no-fail-fast --no-tests fail
```

Run that command from Jellyflow. The Nara cohort uses the independent fixture manifest with
`jellyflow-trial,ui-work-observation`, `--lib --test graph_document --test dialogue_document`
and the same serial release nextest settings.

## Domain Outcomes And Atomic Canvas Operations

Status: original-owner/domain/Session prerequisite qualified. Native external transport remains open.

The next source audit finds prerequisites in the original domain rather than only in the
native adapter. The extension document's unit-returning `apply` cannot expose its allocator's
exact result. Dialogue also lacks one-operation mixed node/link removal, source-end
reconnection and creation from a wire. Complete these original owners before mapping canvas
submission into asynchronous Session work.

`EditorExtensionDocument` gains associated `Outcome`; `apply` returns that domain-owned value.
The original Workspace publishes an immutable `EditorExtensionCommandOutcome` in its ordinary
report, retaining the original command identity and accepted operation revision. Type erasure
stays at the existing document interface. Results require `Send + Sync`, with no mandatory
Clone/Debug/codec, side-channel cell, generic command bus or second result owner.
Successful Noop still publishes its exact outcome; stale, wrong-owner and domain refusals
publish none. Existing domains without additional output use `Outcome = ()`.
These immutable results describe domain execution. They do not acknowledge an external canvas
request or accept a native frame by themselves; the original pending interaction and its
single-use completion association still belong to the upcoming transport.

Dialogue's result must return allocated node/initial-output and replacement-choice identities
directly from its validated candidate. The complete operations are:

- Mixed `Remove { nodes, links }` validates every original selection before removing them
  together. Incident links selected alongside their deleted owner are valid; duplicates and
  missing items refuse the whole candidate. Empty selection preserves Noop/Redo semantics.
- `Reconnect { from, to, target }` moves an existing link and updates its target in one
  candidate. Both outputs and the target must be valid. Moving onto another occupied output
  refuses; retargeting the same output is legal. Equal values are Noop.
- `CreateFromWire` allocates the actual node/choice IDs and connects an existing output to the
  new input, or the new primary output to an existing input, in one candidate. Line and Choice
  have a primary output; End does not. Invalid endpoint, position or supported source limits
  refuse without consuming either allocator, changing history or clearing Redo.

Qualify allocator output, exact result identity/owner, Noop, refusal, one-entry Undo/Redo,
incarnations and Save/Reopen through the original domain and independent Session/file
consumer. Preserve existing graph/Source controls. The full external typed transport,
View-local native intrinsic height, multiline controls and two native Dialogue Views remain
subsequent acceptance within step 3 and the complete consumer.

The implementation replaces the unit-only result contract without adding a compatibility
method. Existing unit-returning owners explicitly declare `Outcome = ()`. The original narrow
document erasure wraps the accepted output; Scene and Prefab keep their own concrete reports.
This follows the existing owner-returned construction/composition result pattern rather than
introducing a central enum of product-specific command results.

Three new Workspace tests use a non-Clone, non-Debug outcome and two nominal owners sharing
that same result type. They cover original payload identity, revision, shared observation,
owner mismatch, stale/domain refusal, Noop and allocation retention. The full Tooling library
suite passes 179/179, no skips, run `69aaca83-98b2-442c-b459-fe79c8921e7c`.

The independent domain/graph/Session cohort passes 94/94, no skips, run
`8e01c9ad-6d33-492d-93b6-43740f8d33a4`. It preserves the prior 79 controls and adds 13 atomic
domain tests plus two complete public Session/file tests. Creation helpers now consume the
actual result instead of discovering an ID by a snapshot difference. The ordered-batch test
retains the first exact allocation while a later stale action refuses, then checks Noop and
reads the original output after real Session close. The complete canvas-command trajectory
exercises both wire directions, choice allocations, reconnection, mixed deletion, one-entry
Undo/Redo, return-to-saved identity, exact Save bytes/digest, Reopen and old-source refusal.
Earlier outcomes remain readable after Reopen; they are not rebound to current facts.

The first independent test build found the new Reopen assertion omitted `digest`. The corrected
assertion constructs the expected complete digest from the saved bytes; no persistence
assertion was replaced with a success-only check. Separate generic-result and atomic-domain
source reviews found no concrete P1/P2. Those reviews did not execute tests or qualify native
external transport, and the CPU/public results do not establish a new Dialogue native UI.

The first affected Editor test build exposed an implicit `UnwindSafe` assumption in the
internal malformed-slot panic test: reports can now contain opaque command/result values.
That test constructs only owned synthetic rejected outcomes and consumes them through the
pure dispatch mapper. Its catch uses a scoped `AssertUnwindSafe`, preserving all five
must-panic cases; the production interface gains no blanket unwind-safety implementation or
additional command/outcome bound. The corrected test joins the affected qualification filter.
Result retention includes the original command payload and output; no allocation, latency or
memory improvement is claimed by this interface change.

| Verification | Actual result |
|---|---|
| Tooling production library | Locked Rust 1.95.0 check passed. |
| Complete Tooling library tests | 179/179, no skips; run `69aaca83-98b2-442c-b459-fe79c8921e7c`. |
| Independent domain/graph/Session and file consumer | Release 94/94, no skips, three binaries; run `8e01c9ad-6d33-492d-93b6-43740f8d33a4`. |
| Independent panel consumers | 46/46, four existing native/embedded skips, 15 binaries; run `bf9d77d1-989f-44cd-9e12-0aab0bb975c0`. |
| Root persistence, exact-slot invariant and existing DX12 extension document | 45/45, 2,192 unselected tests; run `24dd5378-7dfd-4c93-879d-b970d9026e6c`. Includes 43 root persistence controls, the five-case malformed-slot test and one actual native extension-document trajectory. |
| Other migrated owner | Independently locked behavior-language trial all-target compilation passed. This adds no C#/Lua/tool-install runtime qualification. |
| Correctness | All-target Clippy with `-D clippy::correctness` passed for Tooling/Editor and the release independent GPUI fixture. Existing unrelated warnings remain. |
| Conditional compilation | Independent GPUI fixture locked release all-target check without default features passed. |
| Independent review | Generic-result, atomic-domain and scoped-unwind reviews found no concrete P1/P2. All seven scoped agents are closed. |

The native trajectory is the existing reference extension-document workflow, not the new
Dialogue canvas. Its actual save/reopen, panel and physical-close checks preserve the changed
shared interface; new Dialogue pixels/input remain open. The unit retains Open-GPUI code pin
`f75c10b` and Jellyflow `046a660`; neither peer project changes here.

The combined affected Root/Editor command, after setting the existing native font/DX12
environment, is:

```text
cargo +1.95.0 nextest run --locked -p nara -p nara-editor --features nara/project-tooling --lib --run-ignored all -E 'test(project_host::runtime::editor::persistence::tests::) | test(native_extension_document_widgets_save_reopen_and_retire_panels) | test(direct_submission_rejects_inconsistent_internal_outcome_mapping)' --test-threads=1 --no-fail-fast --no-tests fail
```

This closes the domain outcome/atomic-operation prerequisite. Original external request
binding, asynchronous admission/completion, canonical native identity adoption, View-local
intrinsic height and the complete two-View Dialogue product remain within step 3 and the
following native consumer. The full foundation Goal stays active.

## Exact External Canvas Requests And Publication Receipts

Status: renderer-neutral request and public Session consumer qualified; native integration open.

Normalize the existing fresh connection, movement and deletion intents into typed semantic
data before choosing the writer. The standalone runtime lowers that same validated data into
its existing commands. An external document receives a move-only request bound to the original
interaction ticket, snapshot, source incarnation and immutable domain basis. Keep request
binding and terminal settlement in the existing interaction session and submission lease.
Standalone and external transport entry points must refuse the wrong writer explicitly.

Retain one optional immutable owner observation with each external fact publication. Capture
that observation and the graph snapshot under the same original read borrow when binding a
request. This is not a metadata bag, domain callback, writer or replacement history. The Nara
consumer retains its exact document/revision and identity mapping there; it must not reconstruct
an old domain basis from the latest graph revision.

An accepted external result consumes its original request and an exact original incremental
publication receipt. Validate source, predecessor version, document epoch and the requested
effects against that receipt's retained snapshot. A connection additionally adopts the actual
canonical node, port and edge identities allocated or derived by the domain. Invalid evidence
returns the original request and supplied result for repair; it cannot consume or recreate an
operation. Explicit rejection, undispatched work and an unknown delivered outcome remain
different dispositions. Publications alone never acknowledge an edit.

Dialogue's associated command result retains its exact post-command immutable source as well
as the existing effect/allocated identities. Publish A from that result even when B has already
executed. A delayed completion can retain A's receipt after B is published without rolling the
current projection backward. Reopen/reset and close retire native authority while preserving
historical domain observations. Successful Noop retains its original source allocation and
still has an exact accepted Workspace revision.

Definite connection success uses an explicit canonical publication proof and still waits for
the original accepted route/geometry. Preserve standalone reconciliation controls, but disable
proposed-ID/next-revision success inference and speculative retry for unknown external work.
Exercise canonical IDs different from proposals, two Views, ordered and delayed results,
rejection/Noop, reset, close and complete Session/history/file behavior through public consumers.
Native journal admission, asynchronous delivery, intrinsic height, multiline controls and
actual two-View Dialogue pixels remain subsequent work in this same complete consumer.

### Implemented Request And Domain Consumer

The renderer-neutral request path and the independent Nara consumer are implemented.
The local Jellyflow implementation is `4bcd0f2591d1f14146c56d924059a51f9aa89686`;
it is not remotely published. Open-GPUI remains pinned to `f75c10b2`.
`GraphMutationIntent` contains validated semantic data; standalone commands and external
requests share its normalization. Explicit writer-specific binding uses the original
interaction ticket, pending state and drop lease. The bound request atomically retains the
original graph observation and one immutable owner basis. Installed external facts are shared
by `Arc`, so binding does not clone all incarnation maps.

`ExternalGraphRequest::accept` consumes the original incremental publication and validates
source, predecessor, epoch and requested effects. Creation requires a complete bijection
between proposed and canonical ports, including the selected target; equal direction/kind
cannot distinguish two semantic outputs. Dynamic keys remain domain-owned. Actual canonical
identities enter the original connection publication owner and still wait for that View's
accepted geometry. Unknown external work disables standalone proposed-ID inference and retry.

The original Dialogue result is now `DialogueCommandReceipt`: its effect retains actual
allocations, while its `Arc<DialogueView>` retains the exact post-command source. The
`DialogueCanvasRequest` consumer lowers moves, connections, both reconnection directions,
creation from either wire end and deletion into original Session commands.
`DialogueCanvasPublication` is sealed around the matching accepted command outcome and the
publication made from that exact view. Wrong evidence returns the request and publication
intact. Source or domain refusal never reexecutes an accepted operation.

Creation during source reconnection exposed another original-domain requirement.
`CreateFromWire { reconnect }` now validates the original linked output against the fixed
endpoint before allocation, then creates and replaces the link in one candidate and history
entry. Invalid relation, missing link, incompatible recipe or capacity failure preserves both
allocators, source, clock and complete Undo/Redo. The public consumer covers both endpoints
and persistence rather than reducing this interaction to an unsupported bridge operation.

Independent review found and corrected three integration defects: a normal reconnect
publication retired its old binding before the exact ACK; a successful canonical projection
still referenced detached old owners; and creation could confuse same-kind outputs.
Pending external work now survives same-incarnation graph updates until exact settlement,
while reset/close retires it. A proven projection uses only canonical owners. A delayed ACK
also reconciles current B before returning, so an already-retired A route cannot reappear
without another UI tick.

An additional owner review found that an arbitrary old basis destructor could panic between
source-state assignments. Publication now retains old facts outside the mutable borrow,
installs the complete state and View fanout, catches retirement panic, drains existing
notifications and then propagates the first panic. The regression covers publish/reset,
both Views, exact new basis, subsequent allocation and later admission.

The Runtime library plus public Source/interaction cohort passes 647/647 without skips, run
`66e1e4b8-89ff-4379-9c82-4b6e867683cd`. This includes 16 actual public interaction tests,
eight canonical-settlement controls, five normalization controls and four owner-basis tests.
These are CPU/protocol and filesystem results. No new native Dialogue, physical input,
numeric performance budget or complete R11c qualification follows.

Qualification completed November 13, 2026 on Windows 11 with Rust 1.95.0, one build job and
the shared Nara target. The final serial results are:

| Scope | Actual result |
|---|---|
| Runtime library and public Source/interaction consumers | 647/647, no skips, three binaries; run `66e1e4b8-89ff-4379-9c82-4b6e867683cd`. |
| Existing Open-GPUI and egui adapter libraries/integrations | 560/560, no skips, 23 binaries; run `ea4e72b4-910e-4cb0-a295-1b7a3ea90154`. |
| Independent Nara domain, graph and public Session/file consumer | Release 110/110, no skips, three binaries; run `644140b2-b3e8-4ed1-a57a-3506e9de829e`. Includes 11 new external Session trajectories, three reconnect-creation domain controls and two exact-source receipt controls. |
| Correctness | All-target Clippy with `-D clippy::correctness` passed for Runtime/both adapters and the feature-enabled independent Nara consumer. Nonblocking enum-size and existing project warnings remain. |
| Conditional compilation | Independently locked Nara consumer release all-target check without default features passed. |
| Supporting checks | Jellyflow workspace and Nara fixture formatting, whitespace and added local document links passed. |
| Independent review | Shared request/settlement and domain/bridge reviews found no remaining concrete P1/P2 after fixes; all eight scoped agents are closed. |

The initial production build attempted direct `Graph` equality, which that type deliberately
does not expose. Noop validation now uses the original owner's complete publication deltas and
graph version. Two public test assertions use complete serialized authored records instead of
weakening source equality to counts. The runtime/public cohort and adapter controls were rerun
after the ownership corrections and basis-retirement change. Unchanged official GPUI mechanisms
were not given another framework test matrix.

Source publication still compares complete facts and rebuilds its read index. Requests retain
their original source/basis until settlement or disposal; these results establish no total
memory, incremental-layout, native latency or frame-time budget. The native journal, managed
mutation and close paths still need external asynchronous transport. View-local intrinsic
height, full multiline controls and the two native Dialogue Views remain the next complete
consumer. The full foundation Goal stays active.

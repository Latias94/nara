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

The November 12 prerequisite completes execution steps 1 and 2. Step 3 has an immutable facts
projection, but no external-document native submission interface. Steps 4 and 5 remain open.

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

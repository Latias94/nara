# Authoring Interface Ergonomics

**Status:** Proposed design; no new interface below is implemented or Accepted.
**Date:** 2026-10-19.
**Source baseline:** `f988852`.
**Inputs:** [post-correction audit](../audits/2026-10-19-foundation-next-stage-review.md),
[strategy](../../STRATEGY.md), Accepted ADRs 0045, 0047, 0081, 0116, 0117 and 0118.

This design addresses N01-N09 without reopening the completed correction plan. It proposes the
next implementation sequence and the interfaces its consumers should exercise. Rust sketches
are directional design examples, not compiling examples of today's API. Pele is excluded.

## Start With The Developer's Work

A game developer wants to declare gameplay data, install its behavior once, edit it, understand a
failure and run the result. A tool author wants to display a field or submit a deliberate change,
with a reliable answer about that change. Neither should need to reconstruct publication,
revision, routing or retirement policy from several internal modules.

Correctness also contains information the developer must supply: persistent identity, schema
version, migration meaning and game-specific restore behavior cannot be guessed from a Rust type.
The goal is to remove mechanical bookkeeping while keeping these decisions explicit.

### Keep Four Responsibilities Legible

- A transient ECS `Component` is typed gameplay data. It need not become persistent or implement
  another Nara lifecycle trait just to participate in systems.
- A persistent component adds the schema/codec contract: stable IDs, versions, authoring metadata
  and explicit evolution. Derive handles mechanical conversion; it does not install behavior.
- A Plugin/package installs behavior, scheduling dependencies and owned services through the
  existing contribution and recipe. Components do not become plugins, and setup does not become
  asynchronous simply because a later runtime operation may wait for I/O.
- An Editor panel displays projections and submits semantic intent. It does not acquire native
  component registration, history or mutable World authority to edit saved data.

The ordinary guide should follow one real component from declaration through package installation,
creation/editing, validation failure, Save/Play and one version migration. Separate API reference
pages cannot substitute for that complete developer workflow.

| Caller | Common task | Information they should supply | Machinery the engine should own |
| --- | --- | --- | --- |
| Gameplay author | Use transient ECS data and systems | Typed data, systems and scheduling dependencies | Managed execution, faults and close |
| Persistent component author | Create/edit/version a component | Stable component/field IDs, declared values and explicit evolution | Schema/codec generation, shared validation and catalog admission |
| Component package author | Install data and behavior together | One component inventory, owner/binding identity and configuration | Matching schema/native paths and atomic contribution admission |
| Ordinary panel author | Edit one persistent field | Resolved panel target, entity and stable component/field identity | Draft, validation feedback, submission correlation and conflict recovery |
| Advanced tool author | Submit existing semantic actions | Owned action and tool-local response policy | Origin routing, owner result and instance retirement |
| Backend/Host author | Prepare and publish native resources | Concrete native operations | Device/source authority, candidate failure and actual retirement |

## First Principles And Design Rules

1. **A common task should have one complete entry.** A convenience method earns its place when it
   hides a real lifecycle, not when it only renames another method.
2. **Display identity and mutation basis are different facts.** A field remains the same field
   when another accepted edit advances the document revision. A submission still needs an exact basis.
3. **A request has a visible failure path.** Parsing, queue admission, document acceptance and
   persistence publication are different outcomes. None implicitly means the next one succeeded.
4. **The owner decides; the adapter explains.** Early validation is useful feedback. It does not
   grant a panel mutable document/World access or replace final validation.
5. **Failure preserves recoverable work.** Rejected input remains available for correction. Conflict
   resolution is deliberate; silently rebasing, discarding or replaying a draft is not a default.
6. **Safe behavior should require less code.** The default field editor manages submission and
   retirement. Advanced callers can submit actions without obtaining a second history authority.
7. **Expose domain decisions, hide process details.** Stable IDs and migrations stay explicit;
   batch indexes, GPU epochs and internal callback routing do not become normal authoring inputs.
8. **Use existing modules before adding frameworks.** Keep `ProductRecipe`, schema owners,
   revisioned patches, batch reports, native panel instances and resource reservations.

## Recommended Shape

Use a complete default field editor over a small correlated action interface. Keep both in the
existing Editor/frontend module; retain UI-neutral semantic models and outcomes in `nara_tooling`.
The default helper first serves the built-in Inspector, then a separately compiled panel.
Do not publish a generic form engine or a public controller state-machine framework.

```mermaid
flowchart TB
    Game[Typed components and one product recipe] --> Schema[Owner catalog and native registration]
    Schema --> Validation[Shared schema and value validation]
    Model[Read-only document and field projections] --> Default[Default field editor]
    Model --> Custom[Custom source-built panel]
    Default --> Submit[Correlated existing EditorAction]
    Custom --> Submit
    Submit --> Owner[Application and document owner]
    Validation --> Owner
    Owner --> Report[Existing ordered action outcomes]
    Report --> Route[Private origin and incarnation routing]
    Route --> Default
    Route --> Custom
    Owner --> History[One document revision and history]
    Owner --> Persistence[Separate save publication owner]
```

### Alternatives Considered

Three independent design lanes explored different seams before this recommendation.

| Alternative | Strength | Material cost | Decision |
| --- | --- | --- | --- |
| A: one pending action per panel plus one callback, no public correlation ID | Very small interface; clear lifecycle | A normal multi-field/tool panel encounters Busy for independent same-frame actions | Keep the callback, remove the arbitrary one-action restriction |
| B: owned tickets and a pollable UI-neutral tool inbox | Flexible multi-action composition; no callback reentrancy | Every ordinary field must retain/poll a ticket; introduces inbox, consumption and long-running-operation policy | Defer until a real non-painting consumer needs pull semantics |
| C: a complete field-edit entry with Host-managed drafts | Smallest common caller; one implementation owns recovery | Limited default interaction/customization | Select as the ordinary path; prove privately before exposing it |
| Selected combination: C over a small submit/result callback with an opaque local ID | One easy path and one coherent advanced seam | Requires exact private routing and clear dispatch-only finality | Recommended |

Keeping today's raw `emit` alone leaves every editing tool to reconstruct acknowledgement. A new
universal event bus, package kernel or transaction DSL would add learning and ownership without
closing a workflow that the existing models cannot express.

### Lessons From The Pinned Reference Engines

The [comparison](../audits/2026-10-19-nara-bevy-godot-architecture-review.md) and
[current audit](../audits/2026-10-19-foundation-next-stage-review.md) contain the pinned Bevy/Godot
source evidence. They are architectural comparisons, not an instruction to copy their public API.

| Reference lesson | Application here | Limit |
| --- | --- | --- |
| Bevy separates component data, plugin composition and reflection facilities | Keep data, installation and persistent schema legible; generate paired registration bookkeeping | Runtime reflection alone does not supply Nara's stable saved-data lineage |
| Godot has targeted Editor extensions, property metadata and explicit revert behavior | Offer a narrow field editor and action result; distinguish authored overrides from effective defaults | Property presentation hints are not automatically persisted validation rules |
| Godot checks resource creation; Bevy retains pipeline preparation states | Failure should terminate or defer inside the existing resource owner | Neither precedent requires a new public render/job framework |

## D1. Ordinary Field Editing

The proposed common usage is intentionally short:

```rust,ignore
impl ImguiEditorPanel for HealthPanel {
    fn show(&mut self, ui: &Ui, cx: &mut ImguiEditorPanelContext<'_, '_>) {
        let Some(entity) = cx.model().selection()
            .filter(|selection| selection.len() == 1)
            .and_then(|selection| selection.top_entity())
            .cloned()
        else {
            ui.text_disabled("Select one entity");
            return;
        };

        cx.edit_field(ui, &entity, &HEALTH_COMPONENT, &CURRENT_FIELD);
    }
}
```

`HEALTH_COMPONENT` and `CURRENT_FIELD` are the component library's stable identities. The helper
binds only to this panel's resolved authoring document. A missing/inactive target is shown as
unavailable; it must not silently fall back to a different active document. The caller does not
construct revisions, patches, tickets, raw paths or defaults. Explicit selection/layout is still
the panel author's decision. One editable control per field per panel paint is the initial contract;
a second presentation can be read-only rather than creating ambiguous duplicate editors.

The helper belongs to the Dear ImGui adapter and is not a universal widget abstraction. Its private
controller is stored in the existing per-instance widget state, beside the panel object. It can
use disjoint borrows for panel state and field controllers while `show` runs. Its shared validator
and document operations remain in their existing toolkit-neutral owners.

### Stable Identity And Exact Basis

Private control/draft identity contains the panel incarnation, document session, authored entity,
stable component and root-field IDs. It does not contain the current document revision. Nested
draft rows use local editor identity; list indexes and temporary map keys are not durable fields.

An edit basis separately captures document revision, admitted schema/catalog basis, component
version and source-binding incarnation. Ordinary Save does not replace that document/source
incarnation. The exact submission still targets the owning document and revision. Existing
`EditorFieldTarget` can remain the revision-bound command target; it should not also be used as
the stable key for all UI state. The document owner must expose the admitted catalog and
source-binding incarnation through a narrow read-only projection; the adapter cannot invent them
by hashing a locator or assuming that equal paths mean equal source authority.

Remove document revision from the adapter's outer widget scope and from unconditional draft
retirement. This requires native focus/IME evidence: stable IDs alone do not prove that toolkit
composition survives. Panel target/resolved-document and project changes still retire local state
under ADR 0118. A selection change inside the same document is not itself a panel retirement.

### Draft State And Conflict Recovery

```mermaid
stateDiagram-v2
    [*] --> Clean
    Clean --> Editing: User changes input
    Editing --> Invalid: Local conversion or schema rejection
    Invalid --> Editing: User corrects input
    Editing --> Submitted: Apply and enqueue succeed
    Submitted --> Clean: Exact owner action accepted
    Submitted --> Rejected: Owner rejects
    Submitted --> Conflict: Basis became stale
    Editing --> Conflict: Accepted document basis changes
    Rejected --> Editing: Correct or retry deliberately
    Conflict --> Editing: Review current and explicitly reapply
    Conflict --> Clean: Discard draft
    Editing --> Incompatible: Schema or target meaning changes
    Incompatible --> Clean: Copy if needed and reopen current field
```

Local invalid text, unfinished container children and empty-but-unconfirmed entries remain drafts.
For an `u8` field, `256` is retained with an immediate range explanation; the owner still validates
the eventual action. Submitted controls keep their candidate and disable another submit for that
field. This first slice does not queue several versions of the same field draft.

A dirty draft survives unrelated document changes. It retains its basis and enters Conflict,
rather than being cleared or silently acquiring the latest revision. The control shows the
accepted current value and proposed value. Explicit reapply captures a fresh basis and reruns
validation; it is a new action, not automatic retry. No per-field read-set rebase protocol is added.
Schema/source meaning changes make the binding incompatible; old input may be copied, but must
not be interpreted under the new schema automatically. Actual instance retirement still discards
its transient draft, as today's contract states.

Not painting a field is not proof that its binding died. Collapsing a component or temporarily
selecting another entity retains a dirty draft within the live panel/document scope. A missing
entity/component/field instead marks the retained draft Incompatible only when precise owner
facts establish that absence. Today's selected-entity Inspector projection does not describe all
unselected fields: lack of projection coverage means temporarily unresolved, not missing. Defer
that classification until the field is resolvable; if timely background classification is needed,
add a narrow read-only owner query keyed by retained binding. Never mutate global selection to
obtain those facts. Clean entries may be reclaimed; dirty, invalid and incompatible entries share
a finite private draft budget and are
never silently evicted. When that budget is full, a new editor explains the capacity limit before
accepting input and offers explicit discard of retained drafts. Do not infer retirement from the
set of controls painted in the latest frame.

Owner acceptance must be correlated to the exact submitted draft before it can retire. If a later
accepted edit already changed the same field, the next clean paint shows the latest projection;
an earlier success is never proof that the old candidate is still current. The helper does not
promise document save, native preview readiness or later-operation completion.

### Defaults, Null And Creation

| User intent | Stored operation | Eligibility |
| --- | --- | --- |
| Set a value | Store an explicit root value | Field is admitted and editable |
| Reset to default | Remove the authored override | Authored state is Present and a default exists |
| Remove optional field | Remove the authored key | Authored state is Present and the key is optional |
| Set explicit Null | Store Null | Nullable shape permits Null |
| Add Component | Copy one complete creation template through the owner | Template exists and creation is admitted |

The remove rule remains `Present && (!required || default_value.is_some())`. Reset is not writing
the default back into the file. A field already Defaulted has no override to remove. Component
creation templates, missing-field defaults and validation constraints retain separate meanings.
All accepted operations use ordinary document Undo/Redo; the control adds no private persistent
undo stack. Native text editing can retain its toolkit-local undo during input.

## D2. Advanced Action Submission And Results

The advanced public sketch remains small:

```rust,ignore
// Proposed additions; names may change with implementation review.
fn submit(&mut self, action: EditorAction)
    -> Result<PanelSubmissionId, PanelSubmitError>;

// On the existing per-instance ImguiEditorPanel trait:
fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {}
```

`submit` is the single result-aware replacement for `emit`; no second mutation route is added.
An opaque process-local ID allows a complex panel to distinguish several actions without creating
its own request identities. It is not serialized, not reusable between incarnations, not authority
to mutate or cancel an action, and not a `Future`. A panel that needs no result can ignore the ID
after handling immediate enqueue failure. Its default callback is empty.

The host accepts several independent actions per panel in existing batch order. Each helper field
can still allow only one pending candidate. A finite private admission budget rejects before
enqueue with a typed capacity error; it never silently drops a result or evicts an unread one.
There is no accumulated result history: dispatch results are delivered once while the original
instance remains live, after all painting borrows release and before its next painting opportunity.
Callback execution receives no UI, frame, Context, World or submit handle, preventing reentrant
dispatch through this seam. Trusted callback panics remain subject to the existing native-code
failure boundary, not a new sandbox promise.

| Stage | Meaning | What the caller may infer |
| --- | --- | --- |
| Immediate `Err` | Action was not enqueued | Keep input; no later callback for this attempt |
| `Ok(id)` | Owned action was queued | No document mutation or save success yet |
| `Dispatched(EditorActionOutcome)` | Existing owner processed that exact action | Interpret its existing typed result; Workspace applied may finish a field draft |
| `NotDispatched(reason)` | Batch/frame/closing/target gate prevented dispatch | No accepted edit for this action; preserve live draft |
| Persistence Accepted within a dispatched outcome | Save/Reopen request was admitted | Still not Saved/Opened or crash-durable publication |

A two-action batch is still ordered, not automatically atomic. A tool needing one history entry
uses an existing single atomic patch/field-edit/gesture action. The action interface must not
silently rebase the second action when the first changes the revision.
In particular, two same-document field writes prepared from the same revision may accept the
first and conflict on the second. Several submissions remove an arbitrary panel-wide Busy limit;
they do not promise automatic multi-field merging. If a real tool needs Apply All, it prepares one
complete atomic patch through the existing owner. Prove the two-field same-basis case explicitly.

### Private Routing And Retirement

The frontend boundary must preserve the exact batch as well as action positions. Its public
`show` path should return an opaque, move-only `ImguiEditorSubmission` containing the prepared
actions and private origin ledger, consumed by the matching frontend's `submit_and_drive`.
It has no public constructor, Clone or conversion that transfers its observation authority to an
arbitrary `EditorActionBatch`. Frontend identity, candidate serial and project epoch are checked
before dispatch; a candidate from a different or superseded frontend cycle is rejected before any
mutation. Frame revision alone is not a correlation identity.

Only one prepared candidate is current per frontend. Preparing another settles any unconsumed
candidate as NotDispatched before the next eligible paint and invalidates late consumption. Dropping
the owned candidate performs no engine work; its pending observation is settled on the next frontend
cycle or released at actual frontend retirement. This bounds retained host bookkeeping without
making Drop run user callbacks. The native presentation path seals its candidate only after its
canvas actions/provenance are final. Direct non-frontend application consumers may still submit an
ordinary `EditorActionBatch`; their reports cannot settle frontend-origin observations.

The adapter records the originating project/workspace epoch, panel incarnation and original action
position in this candidate. Application filtering retains a position mapping and explicit rejected
slots. A compacted closing batch cannot be zipped against original positions. Whole-batch rejection
terminates each queued action. System actions have no panel recipient but retain normal batch ordering.

Each accepted enqueue has one accounted dispatch outcome. Delivery is at most once to the original
live object. If the instance is retired before dispatch, the host makes the not-dispatched decision
where applicable; if mutation already committed, retiring the observer cannot undo it. The host
settles and releases the observation record without delivering to a replacement instance.
No hidden object retention is needed to wait for future persistence or process completion.

Moving/docking and GPU reconstruction preserve panel/controller state. Close, hide, changed resolved
target, layout replacement and project replacement retire its scope under the existing lifecycle.
Collect dispatch results, finish driving the application, and reconcile actual project/target/panel
lifetimes before invoking callbacks. Finding an old object in a widget map is not proof that it is
still live after that drive. Outcome routing runs independently of whether the object paints that
cycle. Every origin has exactly one private recipient: either its field controller or the custom
panel callback. Helper-generated IDs are not broadcast to custom callbacks which never received
them. A controller enters Submitted only after enqueue succeeds; an immediate error retains its
editable candidate and explanation. Neither recipient reconstructs success from global revision,
value equality or the latest single-slot field feedback.

Persistence's final result remains an explicitly separate owner projection in this slice. Do not
present request acceptance as publication. A future tool that genuinely chains a save with another
operation needs owner-correlated persistence completion, not polling the latest global result or
quietly expanding this dispatch callback into a general asynchronous operation system.

## D3. One Component Inventory, One Product Contribution

Keep ordinary consumption unchanged:

```rust,ignore
ProductRecipe::new()
    .add_contribution(agent::package(settings)?)?
```

A component package should expose that single helper; callers do not maintain a parallel provider
list. Direct App, Headless/Desktop, Check, Editor Play and Export continue to use the same recipe.
Do not add another global component installation mechanism.

The package author's mechanical duplication can be reduced with a narrow proposed declaration:

```rust,ignore
nara::persistent_components! {
    pub mod agent_types = [Health, Cooldown];
}

const AGENT_SCHEMA: ComponentSchemaProviderDefinition =
    ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("example.agent.components"),
        PluginSchemaProviderId::new("example.agent.components"),
        ComponentSchemaProviderBindingId::new("example.agent.components.native", 1),
        agent_types::initial_catalog,
        agent_types::register,
    );
```

The one type inventory emits two ordinary existing callbacks: a pure generation-one catalog
source and native registration. It also exposes the fallible pure schema-list helper used by that
initial source, so an explicit successor catalog can reuse today's type membership. It performs
no discovery, World mutation, freeze or installation.
The existing provider admission still compares the declared catalog and native candidate, then
publishes atomically. Plugin declarations can use `AGENT_SCHEMA.id()` instead of duplicating a
provider ID constant; this part is already possible today.

The name `initial_catalog` makes the first-generation limit visible. Evolution switches to the
existing explicit current/predecessor source and registration callback, while reusing the generated
schema list and native register function. A version-two package that later adds a new current
component still updates only this inventory; its predecessor snapshots remain unchanged.
Historical catalogs, tombstones and migrations are not inferred from today's type list. The macro
does not become a package, migration or registration DSL.

Keep owner ID, provider ID, binding versions, component version and catalog generation explicit.
Do not derive stable identities from crate paths, automatically increment every version, or use
native Default as an ordinary Editor callback. A manual codec or advanced migration stays available
through the existing explicit provider path, with no compatibility shim that bypasses admission.

## D4. Finite Shared Constraints And Safe Diagnostics

Start with the actual positive `UiText.logical_size` and one external `Cooldown.seconds` range.
The recommended descriptor extension is an optional finite numeric interval on Scalar shape,
alongside its existing numeric domain. It does not create a new field wrapper or arbitrary rule DSL.
Optional/container traversal remains the existing shape traversal; no new container attribute
grammar is required by this first consumer.

A directional component declaration is:

```rust,ignore
#[derive(Component, PersistentComponent)]
#[nara(id = "example.Cooldown", version = 1, alias = "Cooldown",
       component_capabilities(scene, inspect, edit),
       field_capabilities(scene, inspect, edit), authoring_initial)]
struct Cooldown {
    #[nara(id = "seconds", alias = "Seconds", initial = 0.5_f32,
           range(min_inclusive = 0.0_f32, max_inclusive = 30.0_f32))]
    seconds: f32,
}
```

Only the range syntax is new here. There is one descriptor source, interpreted by bundle admission,
generic patch validation, native preflight and strict Check. UI slider/step/format hints are
presentation metadata if later needed; a slider hint must not silently become a persisted validator.
UI input can be temporarily invalid without becoming accepted document data.

Rules must be complete before adding persisted metadata:

- Bounds have the scalar's admitted signed, unsigned or finite floating domain; no integer-to-float
  coercion. Missing bounds are unbounded; contradictory or empty intervals reject at schema admission.
- F32 intervals compare the deterministic F32-materialized value widened exactly for comparison.
  A tiny positive F64 which rounds to F32 zero must fail a positive F32 rule, matching native decode.
  Defaults, templates and generic/native validation share this rule without rewriting stored bytes.
  `UiText.logical_size` currently has an F64 descriptor without an F32 domain: its explicit
  successor must add both the F32 domain and the exclusive zero lower bound, preserving the
  original predecessor catalog. Merely adding a positive F64 bound does not close the gap.
  Its migration must report invalid historical values for deliberate repair rather than clamp them.
- Nonfinite bounds reject, canonical signed zero is deterministic, and inclusive/exclusive endpoints
  have explicit tags. Constraint traversal consumes existing caller value-work budgets.
- Absent metadata preserves the old fingerprint encoding. New metadata participates in catalog
  identity. Readers which do not support it reject; they cannot silently discard it and claim the
  stronger contract. Adding, tightening, relaxing or removing bounds changes field-shape semantics
  and requires a component successor with explicit predecessor and migration policy, even when
  wire bytes do not change. An identity migration may suffice for a relaxation. Do not build an
  interval-subsumption compatibility prover to bypass existing same-version schema equality.
- Cross-field conditions and arbitrary native invariants remain owner-specific. Generic data
  validity still does not imply complete Runtime readiness. This slice exposes that limit rather
  than claiming every codec can be reproduced from metadata.

Errors need stable identity without revealing private values. A proposed component-domain error
contains a static namespaced code and an optional stable root field ID. The diagnostic bridge
receives an optional admitted component schema and publishes the field only after checking its
membership; without that schema it retains the safe code/summary and omits field identity.
Dynamic paths, map keys, asset locators and raw codec details do not become public diagnostic
payloads. Safe summaries are static and checked.
The diagnostic adapter adds owner/component identity and preserves the domain code, instead of
parsing Display text. Existing free-form native errors remain private fallback detail.
Data-only range rejection uses an engine-defined code and known root field. It does not generate
arbitrary project-native codes, execute a codec or require a metadata code registry. Preserve
custom codes only where a real native/domain failure is already ingested. E6 includes both the
Scene codec-error adapter and generic authoring-decode paths which currently erase those facts.

Dependency direction matters: `nara_diagnostic` already depends on `nara_app`. Therefore app errors
cannot import the diagnostics runtime/report crate. App plugin failures can retain a static
namespaced code beside private free-form detail, lowered and sanitized in the existing product
adapter. For component-domain errors, `nara_reflect -> nara_diagnostic -> nara_app` is acyclic and
can reuse the existing checked code/summary primitives instead of inventing another validator;
review that dependency before adoption. Keep reflect-specific field identity in reflect and
validate it at the bridge. Do not make diagnostic depend on reflect, duplicate the diagnostics
bus or publish an all-engine error enum.

## D5. Resource Failures Stay Inside Their Existing Owners

N01 needs a structural preflight of resolved Prefab source before graph indexing. The public raw
data constructors can remain useful; calling a fallible expansion should return diagnostics rather
than require the caller to memorize a validator order. A new public ValidatedPrefab type family
is unnecessary for this fix. Preserve existing bounded materialization and override semantics.

N08 needs local expected-allocation error capture in Editor target reservation. A failed successor
keeps the old target and registration. The ordinary panel author should never need to preflight
VRAM, inspect device limits or add catch-unwind around a resize. Aggregate target-budget policy
remains separate from correctly reporting allocation failure.

N09 needs a private cache state for deterministic pipeline rejection under the existing key/device
namespace. It must not cache retryable device/allocation failure as a permanent invalid shader.
No public render graph, pipeline-cache controller or polling loop is added to gameplay APIs.

## D6. Predesign The Next Workflow Without Claiming It Exists

| Workflow | Recommended contract to preserve now | Enter implementation when |
| --- | --- | --- |
| Open/Save preparation | Owned attempt and result, cooperative cancel, stale-result rejection, owner-thread commit; no borrowed World/frame across await | A measured phase exceeds the chosen interaction budget |
| Task cancellation | Logical terminality and physical capture/worker retirement are separate; make standalone Drop semantics explicit | G1/G9 characterization, before new external task promises |
| Automatic Rust rebuild | Distinct authored content, observed code inputs, build attempt and immutable executable generation | The product promises code-currentness; choose finite cooperative observation or immutable-source build |
| Restart/restore | Restart same executable is distinct from reset/checkpoint restoration | A concrete game chooses the restoration experience under G6 |
| Multiple same-kind documents | Document-keyed Host persistence/projections, existing per-document history; no active-document mutation fallback | Two-source comparison, cross-document tool or pinned different-document Inspectors |
| Inline domain tools | Narrow source-built field/menu/source-handler contribution using existing actions | One real domain tool cannot use the existing panel seam |

Do not add `async fn` to Plugin build/finish, component codecs, panel painting or World commit to
make the surface appear modern. An asynchronous preparation operation needs explicit pending,
cancel and physical-retirement semantics; it may use existing blocking workers or a Future executor
only after its consumer requires that choice. No generic operation service is needed now.

## Implementation Sequence And Ownership

These units are proposed future work, not an active implementation goal. Reconcile affected ADRs
when adopting the durable contracts. Preserve the existing history and complete real consumers.

| Unit | Delivers | Existing owning files and consumers | Dependencies |
| --- | --- | --- | --- |
| E1 | N01 source graph rejection, N08 allocation-error result, N09 terminal failure cache as separate reviewable changes | `crates/nara_scene/src/prefab.rs`, `validation.rs`, Scene tests; `crates/nara_render_wgpu/src/editor_execution.rs`, `texture.rs`, `backend.rs`, `quad.rs`, native allocation/pipeline tests | None |
| E2 | N02 correct Reset/Remove using authored state | `editor/src/frontend/imgui/authoring.rs`, tooling Inspector projections, native interaction and persistence consumers | None |
| E3 | D2 opaque frontend candidate, correlated dispatch, post-drive lifetime checks and closing-filter settlement | `editor/src/frontend/imgui/mod.rs`, `workspace.rs`, `editor/src/app.rs`, native dispatch path; existing tooling batch reports; independent panel fixture | None; use existing actions |
| E4 | D1 stable draft/widget identity, exact-basis conflict and default field helper | `editor/src/frontend/imgui/mod.rs`, `authoring.rs`, `authoring/recursive_controls.rs`, native IME/interactions and external panel fixture | E2, E3 |
| E5 | D3 one component inventory over current contribution | `crates/nara_reflect`, root macro facade, independent module-consumer and public guides | Explicit first-generation and migration example |
| E6 | D4 interval constraints and safe error identity for the two real consumers | `crates/nara_reflect/src/schema.rs`, validation/codec/provider owners, `nara_reflect_derive`, `nara_scene` diagnostic adapters, `nara_ui` codec and external component fixture | Saved-data lineage and dependency review before writing |
| E7 | Qualification and concise guides | Existing native Editor procedure, external component/panel fixtures, Reference Game and affected guides | Affected implementation units |

E1's three independent owners should land separately; the row groups their user-visible failure
boundary, not a requirement for one large commit. Extract private responsibilities in touched
large files when useful, retaining exports and order. Do not make a whole-tree module shuffle a
prerequisite for a local correctness repair. Build/test commands remain serialized in the shared target.

## Acceptance And Ergonomics Criteria

| Criterion | Required proof |
| --- | --- |
| Ordinary editing does not expose machinery | A separately compiled field panel uses one field-edit entry after selection, with no manual revision, ticket, report matching or native registry |
| Package membership has one source | One type inventory produces both existing catalog/native paths, including a version-two successor followed by a new current component; historical membership remains unchanged |
| Failure preserves user work | u8 overflow, owner rejection, stale revision and batch rejection retain editable input; accepted document/history remain unchanged on rejection |
| Correlation is exact | Two panels and several actions per panel, closing-filtered slots, two same-revision candidates, foreign/stale candidate consumption and custom/helper recipients are distinguished; same-basis field writes do not silently merge; replacement or post-drive retired instances receive none from predecessors |
| Widget identity is stable | A dirty field survives another field's accepted edit with focus/IME composition intact; collapsed or temporarily unselected fields retain input; full draft capacity rejects new input without eviction; revision admission remains strict |
| Stored/default distinction is preserved | Required/default, optional/default, optional/no-default and explicit Null pass Reset/Undo/Redo/Save/Reopen with raw presence asserted |
| Schema/native bounded constraints agree | Positive F32 underflow, endpoints and external cooldown range agree; u64 above 2^53 remains exact; invalid defaults/templates reject admission; absent bounds preserve old fingerprints; every bound change rejects same-version lineage; old catalogs/migrations and unsupported readers behave explicitly |
| Diagnostics remain actionable and private | Data-only range failures preserve engine code/root field; actual native/domain ingestion preserves two custom codes through Check/Editor bridges with schema-checked fields; missing schema omits the field; dynamic keys, paths and value sentinels remain redacted |
| Native failure is recoverable | Invalid Prefab returns diagnostics without mutation; controlled target allocation failure retains old target; repeated invalid shader attempts once per key/namespace |
| No hidden authority or wider readiness claim | Ordinary Open loads no project code; callbacks expose no mutable owner; Save Accepted is never labeled Saved; data-only validation does not assert all native invariants |

Use the existing native qualification procedure for claimed controls, IME, reconstruction and
retirement. Source review or compile-only fixtures do not establish these behaviors. No timings or
performance budgets are claimed from this design. Decision-local measurements for task submission,
Open/Save and edit-to-result precede executor or scaling changes.

## Risks And Alternatives To Avoid

| Risk | Chosen mitigation |
| --- | --- |
| The convenience helper becomes another widget framework | One concrete field entry, private controller, existing native adapter and external consumer |
| Callback results are confused with eventual operation completion | Dispatch-only naming/contract; existing persistence owner remains separate |
| Stable widget identity permits stale writes | Keep revision/schema/source basis separate and strict; conflict requires deliberate reapply |
| Numeric rules silently tighten saved contracts | Explicit metadata identity, rejecting old readers and component successor/migration policy |
| Shorter registration hides durable ownership | Generate only paired first-generation callbacks; explicit IDs/history/provider admission remain |
| Safer diagnostics introduce a crate cycle | Native typed facts stay below diagnostic adapters; app never depends on its diagnostic consumer |
| Advanced APIs dominate the default guide | Lead with Component, derive, contribution and edit_field; put raw callbacks/codecs/Host authority in advanced sections |

The default conflict policy here preserves drafts and requires explicit review/reapply. Product
choices for checkpoint restoration, automatic source freshness and cross-document atomicity remain
deferred to their actual workflows. They do not block the concrete corrections and ordinary field
editing interface proposed above.

# ADR 0102: Renderer-Neutral UI, Text, Widget, and Presented-Frame Topology

**Status**: Accepted
**Date**: 2026-08-04
**Last Revised**: 2026-11-13
**Amended**: 2026-08-05 by the NUI-U8 post-decision checkpoint; 2026-08-06 by the NUI-U17 pre-implementation ownership checkpoint; 2026-08-07 by the NUI-U15A example-first interface-proof checkpoint; 2026-08-08 by the focused U15A focus-identity refreeze and UI Foundation rebaseline; 2026-08-10 by the UIF-U5 public-inventory and Host-authority audit; 2026-08-12 by the UIFD immutable dynamic-text projection correction; 2026-10-25 by the complete source/occurrence/placement key adoption; 2026-11-05 by explicit neutral value consumption and runtime installation
**Owner**: UI authoring, text, widget behavior, input routing, presentation packets, and platform adapters
**Refines**: [ADR 0012](0012-render-crate-boundaries.md),
[ADR 0025](0025-runtime-ui-system.md),
[ADR 0031](0031-text-and-font-strategy.md),
[ADR 0038](0038-scene-prefab-authoring-identity-and-provenance.md),
[ADR 0040](0040-render-resource-lifetime-and-submitter-ownership.md),
[ADR 0041](0041-input-routing-actions-text-focus-and-accessibility.md),
[ADR 0043](0043-scene-prefab-and-patch-document-migration-policy.md),
[ADR 0044](0044-root-facade-and-prelude-layering-policy.md),
[ADR 0056](0056-headless-runtime-and-dedicated-server-readiness.md),
[ADR 0095](0095-plugin-owned-specialized-domains-and-project-configuration.md), and
[ADR 0096](0096-evidence-gated-render-scaling-and-upload-policy.md)
**Refined By**: [ADR 0107](0107-shared-ui-foundation-and-independent-product-layers.md),
[ADR 0108](0108-ui-surface-presented-session-and-atomic-interaction.md), and
[ADR 0109](0109-root-local-ui-compilation-and-presentation-closure.md)
**Implementation Plans**: completed [UI Foundation Correction](../../plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md) lineage and the current [Runtime UI Product Slice](../../plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md); the complete-key replacement follows the [foundation rebaseline](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md); execution continuity belongs in engineering memory rather than this ADR

The [complete persistent-key successor conformance](../../knowledge/engineering/verification/2026-10/2026-10-24T185535Z-complete-persistent-ui-identity-and-neutral-conformance-successor-0cf84bdfd2f54766be84f3ca206f7931.md) binds the current breaking
identity inventory, exact neutral assertions and scoped qualification to implementation `f7ab300`.
Historical inventories retain their recorded scope; this is not whole-toolkit qualification.

The prepared-presentation/input-loss part of the public checkpoint was reopened by the
[Input endpoint replacement](../../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md).
Independent Input/UI source review exposed the prepare/loss/cancel/late-commit case: restoring a
prepared whole-session clone can revive cancelled focus/capture/reducers even if a returned stamp
is stale. ADRs 0041/0108 select total committed/invalidated completion and exact root publication
tails. Runtime, manual neutral presenters, root and Reference consumers are migrated directly.
The [bounded Input/UI qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md)
refreezes the exact changed inventory with owning/public/neutral evidence, four selected actual
Windows/DX12 Editor trajectories and the real Reference desktop observation path. Prior complete-key
and typed-Image qualifications retain their recorded scope. No receipt-returning alias remains.

The same checkpoint covers the independent live UI interaction basis: ordinary release, navigation
or accessibility dispatch can invalidate prepared interaction without revoking Input routing.
Check that basis before publishing the route, and report the UI-owned cause rather than inventing
a platform loss. Unpresented authoring changes remain distinct from committed interaction changes.

Candidate lookup is explicitly named `ui_candidate_frame` / `ui_candidate_frames`. It returns
only current unpresented candidates; successful completion consumes them. Retained presented
frames come from the presentation owner's accepted packet/receipt, which remains independent
of the next candidate. The misleading `latest_ui_frame(s)` names are removed without aliases.
Current presentation is validated through the receipt/session, never by requiring a candidate
to remain installed after commit. This naming and consumer correction belongs to the same
qualified inventory. The Reference observer resolves the retained Runtime/session/frame facts;
ordinary authoring changes do not manufacture a new backend presentation.

### Public native surface presentation

The R11b independent frontend consumer admits a finite `nara_ui_render::UiSurfacePresenter`
adapter for one surface/slot and render target. It composes the existing planner, hosted frame
journal and original backend-outcome custody without installing the game's ECS UI adapter.
Preparation stages the exact frame/journal candidate and its original one-shot outcome permit;
it does not publish a route. Only the matching backend-issued presentation commit can adopt that
candidate. The journal is read-only to its caller, and no public receipt constructor or generic
"mark presented" callback is added.

Each presenter consumes one Host-assigned non-reused surface lifetime and a fixed render target.
Rejected outcome delivery returns the original completion unchanged. Deferred work retains its
exact retry and backend basis. Surface retirement revokes routing before waiting for original
outcomes and terminates local deferred retry authority; late acceptance cannot revive the
surface. Quiescence requires outstanding permits and returned outcomes both to settle. GPU output,
native-use completion and physical resource release remain with the original render Host.
An attempt-local failure can preserve the previous valid route. A target/device fault withdraws
accepted presentation history and closes admission before reporting its terminal outcome, while
keeping the installed Slot for the Host's later retirement generation. Presentation epochs never
substitute for that retirement authority.
Interaction, field reducers, platform input mapping and domain edits remain caller-owned.
The adapter supplies the public backend bridge previously unavailable to independent consumers;
hidden first-party test-presenter factories remain restricted to their named conformance glue.
The presenter interface alone does not qualify toolkit-independent panel attachment, its neutral
dependency closure, native platform-input delivery or a complete independent Editor. The
following value/integration contract owns the dependency separation and its qualification.

### Value consumption and runtime installation

The neutral presentation path consumes the original Render target/resource values, Asset
handles/store revisions, captured Image pixels and backend outcome permits. These values do not
require installing an App, World, filesystem, task service or window backend. Their existing
packages expose CPU/value consumption independently of explicit `runtime` integration features;
`serde` serialization does not implicitly enable runtime installation. Runtime products select
their required features on actual dependency edges rather than relying on workspace unification.

The UI presenter and Runtime adapter share one outcome-custody implementation. Moving that
implementation out of ECS integration changes neither permit identity nor abandonment, retry,
late-result or physical-completion responsibilities. Image snapshots retain the same shared
pixel storage and original external/import-budget lifetime owners. Asset slot revisions preserve
their nominal store and mutation identities; a matching version or hash cannot substitute for
that exact association. Window IDs and Render targets remain the same types in their current
owners. No neutral-only raw-ID namespace or replacement resource allocator is introduced.

The [R11b dependency prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#neutral-presentation-consumption-prerequisite)
owns independent locked dependency, Image custody, presenter and native-consumer qualification.
The selected feature separation is a contract; its implementation evidence does not follow from
the manifest alone. Complete panel hosting and the independent Editor remain separate consumers.

### Captured custom geometry in the original paint stream

An independently authored component may supply immutable bounded triangle meshes without
adding a toolkit-specific widget or a public native render callback. Vertices carry local
content-space positions and straight-alpha colors; indices describe triangle lists. Geometry
does not determine intrinsic size. The compiler resolves padding/origin and clips mesh paint to
the owning content box and its inherited clip. Background, mesh and text participate in the
ordinary node/root painter ordering. Hit, focus and semantic facts retain the existing resolved
layout; geometric curve hit testing needs a separate input contract.

The neutral packet projects the original content transform, retains immutable geometry and
charges actual occurrence/payload costs. The original WGPU UI Feature preserves quad/mesh/quad
barriers in the same attachment transaction and original submission. Mesh preparation uses
the existing device-epoch, reservation, work-observation and completion owners. Repeated shared
payloads remain repeated draw work; clipping or fallback cannot evade candidate-wide geometry
limits. No presentable input route follows from creating a mesh or submitting its buffers.

Path vocabulary and tessellation stay with the component author. This seam requires no Lyon,
GPUI, graph domain or native Device/Queue dependency in the kernel. It does not select a vector
effect language, final-size drawing callback, geometry cache, canvas gesture model or dynamic
code reload. The [current geometry prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#same-stream-custom-geometry-prerequisite)
owns independent/public/native qualification and its limits.

### Original Backend Outcome Ownership

The [render Host transition](../render-host-ownership-design.md) refines the hidden backend
exchange under the existing exact-presentation and bounded-admission invariants. Before a backend
candidate leaves the Runtime, reserve its result slots, collection storage and abandonment outcomes
in one original nominal owner. Each move-only permit retains that storage through Prepared,
Submitted and Completed states. A replacement World resource or equal submission value cannot
receive another owner's permit. Explicit refusal returns the complete original result for retry
or cancellation. Abandonment returns one prebuilt Aborted outcome to its original slot without
allocating or requiring a live World; it cannot manufacture Presented or retry authority.

The original logical-close consumer independently retains the outcome receiver before transfer.
Receiving every backend result is distinct from settling the corresponding Runtime UI/Input
authority. Stop closes new admission, settles buffered and returned attempts through the original
consumer, and waits for outstanding permits before allowing Runtime Stopped. Deferred work is
terminalized during close rather than requeued. Removal of an ingress resource is not completion;
missing or foreign authority retains the close obligation for explicit recovery. GPU submitted-
prefix retirement is a separate owner and may continue after logical settlement.
Statistics and diagnostic observations do not own those obligations; their absence cannot prevent
original UI/Input settlement or manufacture completion.

Ordinary presentation commit also observes the original receiver's close fence, before draining
returned outcomes. A temporarily unavailable close port must not leave a late Presented outcome
free to consume Input authority or revive interaction. Concrete completion entry points derive
presentation versus retirement admission from the exclusively borrowed Runtime on each invocation:
Stopping, CloseIncomplete or Faulted may settle original transport without publishing UI interaction
or world-presentation authority. Invocation refusal returns the original completion, and retry
derives the current lifecycle again. This rule applies before close participants get their first
poll, as well as after their ledger is fenced.
The concrete result distinguishes normal presentation handling from retirement settlement after
transport consumption. A Session clears the matching in-flight claim once; a retired result cannot
report a pending Step as presented or requeue it. Post-operation faults preserve that consumed
result and do not authorize replay.

These are backend lifecycle semantics, not a new ordinary UI authoring/presenter API. The owning,
independent hidden-exchange and actual Root/Editor consumers must qualify them; result-storage
tests alone do not establish interaction settlement or the complete runner-local Host transition.

## Context

The current runtime UI combines persistent ECS components, renderer-owned view discovery, layout,
hit testing, and public extract/queue/batch stages. It has no stable logical widget identity, text
owner, semantic tree, or proof that input targets the frame whose exact packet the backend accepted
and submitted. Editor UI has a
different persistence and command authority, so a shared top-level object model would couple two
products while still leaving their common algorithms shallow.

Neutral layered conformance examples first prove the public text/frame, runtime interaction, and
presentation seams without product facts. The reference-game HUD, terminal overlay, Retry control,
packaged desktop path, and headless path then form one complete product consumer through those
frozen seams. The game may reveal a missing capability, but it cannot alone admit a public provider,
component, layout, or Editor-shell abstraction.

## Decision

UI collection may refuse before acquiring any backend attempt because result capacity is
unavailable, prior outcomes are undrained or original outcome reservations remain pending. The
concrete rendering owner reports that frame as deferred, returns its original transport result
and allows the same managed Runtime to retry after the condition clears. It must not acquire a
replacement permit, discard a pending original sidecar or classify these admission conditions as
invalid render topology. Missing, replaced, closed or mismatched UI owners and malformed attempts
retain their explicit integration-fault behavior.

Nara compiles bounded authoring snapshots into one immutable renderer-neutral frame. Text and
widget behavior are separate deep modules; Runtime and future Editor products use separate
adapters.

The 2026-08-08 rebaseline narrows this broad topology into three deep Interfaces: compile an
immutable candidate, present an owned capability-closed packet, and atomically dispatch against the
exact presented receipt. ADR 0107 owns the product-layer split, ADR 0108 owns Surface Slot/session
atomicity, and ADR 0109 owns root-local compilation and frame-global presentation closure. The
superseded master plan remains implementation history and no longer orders product integration.

```mermaid
flowchart LR
    Text[nara_text]
    Kernel[nara_ui_kernel]
    Widgets[nara_ui_widgets]
    Runtime[nara_ui ECS and Scene adapter]
    Editor[Future tooling UI adapter]
    Packet[nara_ui_render owned packet]
    Backend[Presentation backend]
    Receipt[Presentation receipt]
    Presented[Presented frame behavior receipt session]
    Input[Normalized input timeline]
    Transaction[Interaction transaction]
    Intent[Bounded semantic intent journal]

    Text --> Kernel
    Kernel --> Widgets
    Runtime --> Kernel
    Runtime --> Widgets
    Editor --> Kernel
    Editor --> Widgets
    Kernel --> Packet --> Backend --> Receipt
    Kernel --> Presented
    Runtime --> Presented
    Receipt --> Presented
    Input --> Transaction
    Presented --> Transaction
    Widgets --> Transaction
    Transaction --> Intent
```

- The physical dependency direction is `nara_text <- nara_ui_kernel <- nara_ui_widgets`.
  `nara_ui` is the ECS/Scene runtime adapter. `nara_ui_render` consumes the kernel frame and the
  text raster boundary. WGPU consumes only backend-neutral owned packets.
- `nara_ui_kernel` has no ECS, Scene, Host, Editor, window-backend, or WGPU dependency. It exposes
  one concrete fallible authoring-snapshot builder, compilation, read-only frame access, and
  presented-frame dispatch. Normalized IR, layout nodes, caches, fragments, and lifecycle stages
  remain private.
- Every root and stateful or interactive node has an exact stable logical key. Persistent Scene
  keys include canonical containing-document source, declared logical occurrence and complete
  authored placement, including source-local entity and canonical nested Prefab ancestry. Lookup
  request spelling is diagnostic evidence, not equality. Each node retains its complete address:
  a UI root is not a Scene-context boundary, and legal cross-source/runtime children do not inherit
  or omit identity axes. Runtime-created roots and
  nodes require explicit domain keys. Runtime entity bits, query order, sibling indexes, source
  locations, and `SceneInstanceId` are invalid logical identities.
- Complete Prefab placement includes a tagged distinction between entering an admitted source
  entity and entering the previous anchor's instance-addition namespace, at every ancestry step.
  Source-entity steps encode canonical source and local entity; addition steps encode their own
  local ID without falsely assigning the overridden base as an owner. A same-spelled inherited
  entity and addition do not alias. The adapter consumes encapsulated Scene provenance; the
  opaque UI kernel and its context/placement boundary remain unchanged. This does not qualify
  Prefab overlay editing, Apply/Revert or conflict persistence.
- Kernel consumes bounded opaque binary context and placement values, not Scene/ECS types or a
  public Prefab-path model. Adapters own tagged, length-delimited canonical lowering, preserve
  original UTF-8 and raw UUID bytes, and explicitly share context during a projection. Persistent
  identity requires both values; there is no identity-free or implicit-root fallback. Root/local
  equality is content-based and independent of Arc sharing. Scoping clones complete typed
  addresses without allocating a new segmented local, interning globally or changing extraction
  and reconstruction semantics. The [storage comparison](../../benchmarks/2026-10-25-ui-key-storage.md)
  selects that initial representation without claiming a complete engine performance result.
- Exact key cost uses one canonical framing: root-only costs `3 + R`; scoped costs
  `7 + R + L - P`, with identity tags and two-byte field lengths included in the complete address
  lengths R/L and content-defined longest common prefix P. The 256-byte hard ceiling is unchanged.
  A complete identity encodes its tag, first-field length, first-field bytes, final-field bytes and
  trailing final-field length. The enclosing address length frames that final field. A variable
  leaf length must not precede and destroy otherwise shared placement bytes.
  Admission, `encoded_len` and reduced owner limits use the same function; storage shape cannot
  change eligibility. Canonical cost is not allocator footprint, and this internal representation
  does not promise a stable persistent/wire format or admission of every 32-step Prefab path.
- One immutable `UiFrame` owns resolved logical geometry, root layers, stacking, clipping, paint
  order, reverse hit order, focus/navigation facts, semantics, diagnostics, and its environment
  revision. Root compilation is bounded and failure-atomic.
- A presenter returns a `UiPresentationReceipt` only after the exact packet was accepted and
  submitted by the backend for the matching surface/environment revision. The receipt proves
  backend submission of that immutable packet; it does not claim that a player actually saw the
  pixels. Input dispatch targets one exact retained presented session containing the immutable
  frame, the immutable behavior snapshot compiled with it, and the receipt. Compilation alone never
  grants interaction authority. The adapter retains the current session plus two prior sessions and
  rejects routes that resolve to retired history.
- Runtime UI physically separates authoring projection, per-surface compiled candidates, presented
  history, one interaction transaction coordinator, and bounded intent-journal storage. These are
  concrete private modules inside `nara_ui` until dependency isolation or a second adapter justifies
  another crate. Presented history owns publication only. Intent-journal storage owns entries,
  cursors, acknowledgements, and reclamation only. The transaction coordinator is the sole owner of
  atomic interaction, reducer, journal-admission, and input-finalization commit.
- Every surface has independent candidate, presented-history, interaction, and widget-reducer state.
  A root-local compatible last-good visual may enter a newer candidate only as inert paint with no
  hit, focus, action, or semantic authority. A failed or unpresented candidate leaves the prior
  presented session unchanged and cannot replace its behavior. No hover, press, focus, enabled,
  reducer, or action state is shared across surfaces.
  ADR 0109 refines modal safety: current modal membership/barrier is root-envelope policy rather than
  cached body behavior. An inert retained modal visual still blocks underlay UI and gameplay; if the
  current candidate cannot establish a safe modal barrier/fallback, compilation rejects the complete
  candidate and retains the prior presented session.
- Timeline routing returns one closed disposition. `PassThrough` performs no UI mutation and keeps an
  event eligible for gameplay only when it is proven not to target UI. `Consumed` atomically commits
  interaction state, widget-reducer state, admitted intents, and input finalization.
  `RejectedClosed` commits no UI or intent state, records a bounded rejection, and finalizes the event
  so gameplay cannot consume it. Stale or retired history, route mismatch, invalid projection, route
  conflict, and capacity failure are rejected closed.
- Semantic intents enter a bounded monotonic journal with bounded registered consumers and
  independent cursors. Late registration begins at the current tail unless the owning product
  installs the consumer before input starts; no implicit replay exists. The journal reclaims only the
  prefix acknowledged by every registered consumer, never overwrites an unacknowledged entry, and
  preflights capacity before any interaction or reducer mutation. A slow consumer causes typed
  producer rejection rather than a silent gap. Pause retains both the consumer cursor and unobserved
  intents; explicit unregister releases only that consumer's retention obligation.
- A window/seat endpoint has at most one active surface owner. Republishing the same owner is
  idempotent. A different owner receives a typed conflict without changing the existing route,
  presentation epoch, or revision. Only retirement of the matching surface or endpoint detachment
  releases ownership; no insertion-order or implicit focus-stack policy exists.
- The first authoring vocabulary is closed: root, node/style, visual, display text, controlled
  button, optional semantics, immutable theme, bounded action ID, and typed semantic intent. It has
  no callback with World/Editor authority, property bag, generic Widget component, CSS selector
  language, or second retained object model. A product adapter may replace display content on the
  existing immutable text value, but logical size, brush, alignment, overflow and semantic-text
  policy remain authored facts rather than fields reconstructed from product defaults.
- `nara_text` owns bounded deterministic font admission, shaping, bidi/line layout, measurement,
  glyph keys/raster requests, and diagnostics without system-font discovery. Atlas allocation and
  GPU residency remain presentation-owner responsibilities.
- A node's own text uses the compiler's resolved content box for measurement, final shaping and
  paint origin/clip. Background, hit, focus and semantic facts retain the outer node box. Render
  lowering consumes that published content geometry instead of recomputing layout padding.
  The B10/U9 correction reopens the neutral conformance checkpoint for this invariant, supported by
  the independent Bevy/Godot comparison and the existing padded layout-gallery consumer. Its
  successor evidence must include auto-height Flow/Absolute layout, packet geometry and the
  amended neutral gallery before the shared change is closed.
- One explicit finite ordered set of immutable admitted font faces supplies compiler shaping and
  presentation rasterization. The bundled single-face configuration remains the convenience
  default. Each glyph retains its actual selected face; a planner rejects any face outside its
  admitted set before staged atlas or packet publication, retaining its last-good output. Packets
  remain resource-complete and do not consult a mutable font registry at submission.
- Kernel routing owns capture/target/bubble, pointer capture, focus scopes, modal inertness,
  navigation, semantics, and geometry-coupled session state. `nara_ui_widgets` owns controlled
  behavior reducers and emits typed intents only.
- The R11 editable-text prerequisite adds a bounded single-line field buffer with a distinct live
  key per incarnation and a non-reused token per admitted text-input focus lifetime. Compatible
  redraws preserve that owner. Removal/recreation, blur or disabled input revokes the prior token;
  enabling an otherwise retained logical focus establishes a new input token. Preedit never enters
  the committed value or document history. Buffer Undo/Redo is local; the product adapter alone
  maps explicit Apply/Discard and exact document results. Signed finite position offsets permit
  clipped horizontal text scrolling, while sizes and padding remain nonnegative. Paint, pointer
  coordinates and candidate exclusion geometry share the field's resolved text origin.
- Focus identity is distinct from current navigation and activation eligibility. A stable focused
  key that remains present in its active modal scope survives a temporary disabled state, while
  enabled-only navigation skips it and pointer, keyboard, gamepad, and semantic activation reject
  it as disabled. Hiding or removing the key, retiring its surface, invalidating its modal scope, or
  cancelling its seat still clears or restores focus through the ordinary session rules. This
  preserves pending-control continuity without making disabled controls interactive.
- Public scheduling exposes semantic registration for local command producers, immutable
  current-tick consume/post-consume capture, and Input's documented ordering anchors required by
  independent seat configuration and custom UI adapters. `InputSet` exposes the existing First
  projection/observation/routing/staging order and separate PreUpdate action resolution; it does
  not expose their execution functions or add a scheduler. Ordering applies within one schedule
  and membership alone does not order peers in a set.
- A custom UI adapter may use `InputTimeline::route_pending_ui_input` synchronously in RouteUi.
  The original Input owner supplies pending entries in order and records accepted consume or
  rejected-closed decisions; returning no decision leaves ordinary fallback intact. Raw sequence
  completion stays private/doc-hidden: no delayed or foreign completion authority escapes the
  synchronous borrow. A panic preserves the completed prefix, not batch rollback. This is a
  routing-policy interface, not presentation evidence, a UI receipt issuer or a substitute for
  the actual UI adapter's captured route validation and transaction. Projection/state algorithms,
  local-action mapping, command admission and acknowledgement remain engine-owned implementation.
- Shared UI interfaces are proven before product integration by three thin neutral conformance
  executables plus one independent public consumer: a kernel layout/frame gallery, an ECS/Scene
  runtime interaction lab, a root-facade presentation gallery, and [`ui-module-consumer`](../../../tests/fixtures/ui-module-consumer/). The kernel
  gallery and independent consumer use only ordinary public modules. The runtime and presentation
  galleries isolate non-forgeable prepare/submit/commit authority in local first-party presenter
  glue that may use doc-hidden implementation seams; those seams are excluded from the frozen
  public inventory, cannot be re-exported, and cannot appear in product code. Requiring a public
  fake presenter would weaken receipt meaning and is rejected. The examples introduce no shared
  framework, registry, alternate authoring model, product vocabulary, or duplicate pixel/hash
  matrix.
- The conformance checkpoint freezes a recorded baseline: public-interface inventory, named shared
  semantic invariants, exact example assertions, and the commit that proved them. Product integration
  must reopen and refreeze this checkpoint before changing shared UI behavior or an assertion, even if
  signatures remain unchanged. A post-product neutral case may reproduce a gap but cannot alone admit
  a shared change; the focused amendment must cite a pre-freeze named invariant, independent
  comparative pressure, or a second independent consumer.
- After that checkpoint, the Reference Game consumes the frozen baseline; a shared gap stops for
  independent evidence and a focused plan amendment instead of silently widening the engine.
- Runtime and future Editor adapters share text, kernel, widget state machines, semantic
  vocabulary, canonical logical-address equality and presentation infrastructure. Equal complete
  coordinates lower to equal logical keys; raw Prefab Canvas and Scene-instanced placement are
  different coordinates. Adapters do not share document/source ownership, runtime activation,
  session persistence, Host authority, or domain commands.
- The semantic tree and typed accessibility actions are part of the formal `UiFrame` contract.
  AccessKit integration is a bounded desktop-adapter trial; AccessKit types and availability never
  enter the kernel, widget, packet, or headless contracts.
- Open-GPUI and other engines are behavioral evidence only. Nara does not adopt their App/Window/
  Entity/Element lifecycle or copy source without explicit provenance and license review.

### Unreleased Persistence Baseline

The first-party UI component catalog, component schemas and native binding/codec identity use
the current canonical version 1 baseline. Current typed brush references and bounded logical
text-size semantics belong to that baseline. Retire the prototype brush/text readers, synthetic
catalog predecessors and their historical tombstones; checked-in resources and ordinary consumers
are updated directly. Opening a source document preserves its original declarations and saved
checkpoint rather than invoking those native migrations or making it dirty.

This consolidation does not remove generic component/catalog evolution or the reviewed authored
upgrade contract. Actual project-data evolution, such as EnemyStats v1 to v2, retains its explicit
old/new semantics, history policy and recovery evidence. Do not retain unreleased first-party
prototype formats merely to serve as that mechanism's fixture.

### Image Identity And Typed Asset Binding

The canonical source-location replacement reopened the image-identity part of the shared UI
checkpoint. A complete source locator is not a widget key, and an opaque image name cannot prove
an authored asset selection. The replacement below is implemented. Its
[source/UI qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T015832Z-canonical-source-locations-and-typed-ui-image-binding-qualification-0ce5bafdc65f4893aa0079009668e41a.md)
refreezes the exact public image inventory and neutral assertions with independent manual,
Runtime, Canvas, retained-packet and selected Windows/DX12 consumer evidence. The preceding
conformance evidence retains its original scope.

`UiImageId` distinguishes a bounded Named identity from an exact External identity. `new` always
constructs Named, regardless of spelling. External identity contains a process-local provider type
domain and that provider's exact non-reused numeric resource token. It is an equality key, not a
resource lease, native handle cast, persistent asset identity or authorization. Providers own token
uniqueness and incarnation within their domain. The first-party Image adapter uses the actual
`ImageAsset` domain and process-unique `AssetId`; it introduces no allocator, registry, reflected
payload or asset dependency in the kernel. Rust type identity and these tokens never enter files.

The ECS/Scene adapter retains a private `Handle<ImageAsset>` binding for a persistent Image visual.
Its explicit asset constructor establishes that selection; the ordinary constructor taking an
opaque image ID does not. Replacing an image brush clears the old binding, including when the new
ID compares equal. Export uses that typed binding, the actual `AssetServer` and the explicit encode
scope/policy. A Named or manually supplied External identity without this binding refuses asset
persistence; no diagnostic-string or path-shaped-name inference is permitted. Asset selection is
distinct from actual content/type/residency admission, which remains with its concrete owners.

Native decoding and authoring Canvas compilation use the same prepared Image resolution under
the actual source scope/catalogue or an existing pathless Product binding. Pure authoring edits
preserve typed `AssetRef` leaves rather than manufacturing runtime images. Canvas uses current
admitted Image identities, including later imports, without mutating live asset bindings during
candidate preparation. Complete primary/auxiliary Product selection survives source-free Runtime
decode and reverse export. Canonical paths remain in the Asset owner and are not truncated, hashed
or constrained by the widget-key ceiling.

Immutable kernel frames carry only the selected image identity. The presentation adapter joins
External identities to actual prepared Image keys; explicit `UiPreparedImages::insert` continues
to serve arbitrary opaque/manual consumers. Packets retain the exact snapshot and physical custody
under the existing presentation rules. No path, UUID or Product string alias search participates in
this join. Unsupported image text/borders still refuse at the owning codec/kernel boundary.

The refrozen consumer assertions cover the pure-kernel manual-image consumer and real
Runtime/Canvas paths: a manual Product-shaped ID remains drawable but cannot masquerade as a
persistent asset; long canonical
paths remain usable; same-named roots remain distinct; source-free primary/auxiliary Products
retain actual pixels; changing a brush cannot reuse the previous binding. Shared layout,
interaction, packet and presentation authority contracts remain intact. The linked evidence records
the public image identity inventory. This does not qualify a complete custom-drawn Editor toolkit.

## Alternatives Considered

### Option 1: Continue an ECS execution tree with public layout and render stages

Rejected. It couples compilation to runtime allocation and exposes implementation steps that no
real product needs to orchestrate.

### Option 2: Adopt one shared Runtime/Editor widget object model

Rejected. Game documents and Editor sessions have different identity, persistence, command, and
Host authorities.

### Option 3: Share a private frame kernel and controlled widget reducers through separate adapters

Selected. It concentrates layout/routing/semantic invariants while preserving product boundaries
and a narrow public API.

## Consequences

- The existing UI schema, renderer-bound roots, panels, computed layout resources, pointer routes,
  and public extraction/batching DTOs are replaced rather than wrapped.
- Three workspace-internal crates are admitted early to machine-enforce ownership in this
  AI-developed project. Their APIs remain consumer-driven and may break before 1.0.
- The runtime adapter uses several private lifecycle modules and one transaction coordinator rather
  than one state aggregate or a new public framework. The Reference Game migrates to the bounded
  intent cursor but does not define these shared contracts.
- Frame-local UI action buffers and silent window/seat route replacement are removed. Slow intent
  consumers produce typed backpressure; they cannot lose entries or leak rejected UI input into
  gameplay.
- The bounded text/field owners do not qualify a complete editable Editor, native IME candidate
  behavior or text accessibility. Rich spans, localization, project font assets, virtual collections,
  docking and native multi-window Editor policy retain their separate gates.
- Headless interaction and presentation conformance requires an explicit first-party test presenter.
  Its doc-hidden authority stays local to the conformance executable and is not ordinary consumer
  Interface. A compiled frame by itself is not submission evidence, and a receipt never attests
  human visibility.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Frame authorities for paint/hit/focus/semantics | 1 | Kernel snapshots and routing tests |
| Interaction against unsubmitted or stale geometry | 0 activations | Receipt-journal failure tests |
| Presented behavior taken from a newer unpresented candidate | 0 activations | Presented-session behavior tests |
| UI-targeted rejected input reaching gameplay | 0 edges | Routing-disposition and capacity-failure tests |
| Unacknowledged semantic intents lost across pause or capacity pressure | 0 intents | Journal cursor, backpressure, and resume tests |
| Active window/seat route overwritten by another surface | 0 routes | Route conflict, idempotent republish, retirement, and reclaim tests |
| Kernel dependencies on ECS/Host/Editor/WGPU | 0 | Cargo graph and independent-consumer checks |
| World-mutating widget callbacks | 0 | Public API inspection and compile tests |
| Product code binding to technical input/command phases | 0 systems | Schedule-extension consumer tests |
| Independent/public consumers importing Reference Game or private integration modules | 0 imports | Consumer manifest, public inventory, and focused execution |
| First-party conformance imports of private presentation authority | Named local presenter glue only; 0 re-exports and 0 product imports | Exact source scan and example execution |
| Reference product parity | HUD, terminal, Retry, accessibility, package, and headless gates pass | Reference-game verification contract |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| The neutral kernel becomes a public type warehouse | High | High | Expose only validated snapshot construction, compile, frame access, and dispatch; keep IR private. |
| Stable-key collisions route actions to the wrong node | Critical | Low | Compare bounded exact key content, diagnose duplicates, and exclude invalid nodes from interaction and semantics. |
| Last-good visuals remain accidentally interactive | Critical | Medium | Require a matching presentation receipt and make fallback visuals semantic/action inert. |
| Transaction ownership is split across presented history and intent storage | Critical | Medium | Give one private coordinator the only commit path; history and journal owners expose preflighted storage operations only. |
| A slow or abandoned consumer pins the intent journal | High | Medium | Use bounded registration, explicit unregister, typed producer backpressure, and reclaim only the globally acknowledged prefix. |
| Surface replacement cannot reclaim a window/seat route | High | Medium | Make same-owner republish idempotent and release ownership only on exact matching retirement or endpoint detach. |
| Editor requirements distort game persistence | High | Medium | Keep separate adapters and defer Editor migration until heterogeneous panel evidence exists. |
| Conformance examples become a second framework | High | Medium | Keep each example local to one owner, move reusable behavior into the deep module, and forbid shared scenario registries or alternate authoring lifecycles. |
| Product integration widens frozen seams opportunistically | High | Medium | Keep product facts in Reference Game data/adapters and require independent evidence plus a focused plan amendment for any shared interface change. |
| Text or atlas work becomes unbounded | High | Medium | Enforce owner-local input, glyph, page, upload, retention, and diagnostic limits. |

# nara Architecture Open Questions

**Status**: Living Draft
**Updated**: 2026-07-16

This document contains undecided architecture questions only. Accepted decisions belong in ADRs; implementation evidence belongs in `adr/implementation-status.md` and engineering memory. Each question remains open until its trigger creates enough concrete pressure for an ADR.

## OQ-001: Render Execution Model Trigger

- **Status**: open
- **Owner**: `nara_render`
- **Trigger**: An intermediate logical resource, retained/history lifetime, or cross-target dependency requires scheduling that `RenderPassPlan` cannot express.
- **Related ADRs**: 0017, 0032, 0040, 0094
- **Question**: What is the smallest execution model that satisfies the first static-plan-breaking workflow:
  extended static phases, typed pass providers, a minimal execution kernel, or a logical resource
  graph? Which resource, lifetime, ordering, and inspection semantics does that workflow actually
  require?

## OQ-002: Reusable Material and Shader Specialization

- **Status**: open
- **Owner**: `nara_material`, render domains
- **Trigger**: A project needs a reusable material asset, a custom shader interface, or variants that
  must compile across more than one target capability profile.
- **Related ADRs**: 0012, 0033, 0040, 0054, 0094
- **Question**: Which stable shader interface, parameter/binding schema, variant and fallback policy,
  and logical pipeline/cache key belong above backend-native shader modules and PSOs?

## OQ-003: Runtime UI Layout Model

- **Status**: open
- **Owner**: `nara_ui`
- **Trigger**: Text, image, and child intrinsic measurement exist and at least two real panels need responsive layout.
- **Related ADRs**: 0025, 0041
- **Question**: Should the next retained layout slice use flex, grid, or a smaller nara-specific model, and what is the canonical `Auto`/content sizing contract?

## OQ-004: Platform Accessibility Bridge

- **Status**: open
- **Owner**: input/UI platform adapters
- **Trigger**: Toolkit-independent semantic UI actions are implemented and a desktop accessibility API integration is scheduled.
- **Related ADRs**: 0025, 0041
- **Question**: How should platform accessibility trees and assistive actions map onto nara focus, navigation, activation, and text semantics?

## OQ-005: Physics Integration Authority and Backend Selection

- **Status**: open
- **Owner**: future physics domain
- **Trigger**: The first playable physics vertical slice can name body/control modes, transform
  writers, contact/query freshness, determinism, and deployment requirements.
- **Related ADRs**: 0016, 0018, 0019, 0039, 0042, 0057, 0085
- **Question**: Which 2D backend should be adopted first, when is ECS or the solver authoritative for
  transforms and velocity, and what teleport/kinematic write, query snapshot, contact ordering,
  capability, and event contracts keep backend state coherent without promising solver equivalence?

## OQ-006: Save-Game Snapshot and Restore Contract

- **Status**: open
- **Owner**: future save/persistence domain
- **Trigger**: A real game requires save/restore across process restart or scene travel and can name
  persistent entities, eligible state, service reconstruction, compatibility, and failure behavior.
- **Related ADRs**: 0027, 0043, 0045, 0051, 0058, 0084, 0089
- **Question**: Which snapshot/baseline, component/resource records, runtime-created identity,
  tombstone, migration, service reconstruction, and transactional restore rules form the first save
  format without serializing an ambient `World` or backend-native state?

## OQ-007: Optional Scripting Adapter Contract

- **Status**: open
- **Owner**: a concrete scripting adapter package
- **Trigger**: A real project deliberately chooses a separately reloadable scripting layer and can name workflows that the complete Rust path does not satisfy.
- **Related ADRs**: 0042, 0045, 0093
- **Question**: Which concrete adapter should be trialed first, which lifecycle/data/tooling contracts belong to that adapter, and which contracts have enough independent consumers to move into Nara-owned domain APIs without creating a universal Behavior Host?

## OQ-008: Authoring-to-Runtime Projection and Baking

- **Status**: open
- **Owner**: authoring document domains, `nara_scene`, `nara_tooling`
- **Trigger**: Two independent authoring types need to generate derived runtime components, entities,
  resources, or dependencies rather than spawning one stored component record to one ECS component.
- **Related ADRs**: 0006, 0007, 0026, 0038, 0043, 0047, 0081, 0083, 0087
- **Question**: Which bounded projection/baking contract owns input snapshots, dependency tracking,
  stable source-to-output provenance, generated identity, failure atomicity, diagnostics, and runtime
  admission without freezing a universal baker from a single scene workflow?

## OQ-009: Field-Level Apply Changes

- **Status**: open
- **Owner**: `nara_reflect`, `nara_tooling`
- **Trigger**: The ADR 0034 selected-component Apply Changes baseline is implemented and
  whole-component write-back creates destructive conflicts in a real edit workflow.
- **Related ADRs**: 0034, 0045, 0047
- **Question**: How should field projections, conflict detection, and inverse patches narrow Play Mode write-back?

## OQ-010: Editor Runtime-UI Dogfooding Gate

- **Status**: open
- **Owner**: `nara_ui`, editor adapters
- **Trigger**: Runtime UI has text/IME, keyboard navigation, focus, scroll, accessibility semantics,
  and the layout capabilities required by one complete existing egui panel.
- **Related ADRs**: 0015, 0025, 0041
- **Question**: Which complete editor panel should migrate first, what command/undo/usability/
  performance parity constitutes success, and which later heterogeneous workloads (a virtualized
  hierarchy/table, viewport, timeline, or graph) are required before deciding whether nara UI
  should become the primary editor toolkit? Success with the first panel proves adapter
  replaceability, not final toolkit convergence.

## OQ-011: Platform Export, Signing, and Store Adapter

- **Status**: open
- **Owner**: product build/export hosts and platform adapters
- **Trigger**: A supported platform or store consumes ADR 0086/0088 build artifacts and requires an
  external toolchain, credentials, signing/notarization, or store-specific publication.
- **Related ADRs**: 0020, 0035, 0051, 0055, 0070, 0086, 0088
- **Question**: Which adapter owns toolchain discovery, credential capabilities, signing and store
  steps, progress/cancellation, receipts, and last-good recovery while target planning and package
  identity remain pure engine contracts?

## OQ-013: Typed Event and Request Channels

- **Status**: open
- **Owner**: `nara_app`, domain crates
- **Trigger**: At least two domains need the same producer/consumer/retention/stage metadata beyond existing typed queues.
- **Related ADRs**: 0023, 0036
- **Question**: Is a reusable typed channel wrapper justified, and which lifecycle metadata can be shared without creating a global bus?

## OQ-014: Audio Voice, Mixer, and Device Boundary

- **Status**: open
- **Owner**: future audio domain
- **Trigger**: The first real game or tool schedules an audio slice that needs concurrent voices,
  buses, streaming, spatial playback, or device suspend/reconnect behavior.
- **Related ADRs**: 0016, 0030, 0042, 0079
- **Question**: Which backend, stable voice identity and command model, bus/mix graph, streaming
  ownership, spatial intent, and device-session lifecycle implement the first audio slice without
  placing native handles or callback-thread state in the ECS `World`?

## OQ-015: Text Shaping and Localization Stack

- **Status**: open
- **Owner**: future text/localization domains
- **Trigger**: Runtime UI requires multilingual shaped text with font fallback and deterministic asset import.
- **Related ADRs**: 0025, 0031, 0033
- **Question**: Which shaping, bidi, font rasterization, and localization libraries fit nara's asset/render boundaries?

## OQ-016: GPU Cache Eviction Defaults

- **Status**: open
- **Owner**: `nara_render_wgpu`
- **Trigger**: ADR 0054 instrumentation and a representative project or constrained target expose
  measured GPU-resident pressure and cache-reuse behavior.
- **Related ADRs**: 0037, 0040, 0054, 0068
- **Question**: Which grace-generation and byte-budget defaults balance reuse, memory pressure, and predictable reclamation?

## OQ-017: Advanced Raw Platform Event Access

- **Status**: open
- **Owner**: platform adapters
- **Trigger**: A supported integration cannot be expressed through normalized input/window/text/accessibility events.
- **Related ADRs**: 0013, 0041
- **Question**: How can advanced users observe raw events without making winit types part of gameplay-facing or persistent contracts?

## OQ-018: Persistent Replay Artifact and Checkpoint Policy

- **Status**: open
- **Owner**: future replay domain and participating runtime services
- **Trigger**: A concrete persistent replay workflow has stable identity/envelope evidence, an
  admitted isolated runtime host, a named service-outcome coverage set, and representative
  size/latency measurements.
- **Related ADRs**: 0024, 0042, 0049, 0051, 0057, 0076
- **Question**: What canonical artifact fields, checkpoint coverage registry, service outcome catalog, checksum algorithm, cadence, compression, compatibility fingerprint, and bounded retention defaults satisfy the first measured replay workflow?

## OQ-019: System-Level Stepping and Breakpoint Executor

- **Status**: open
- **Owner**: `nara_app`, `nara_ecs`, `nara_tooling`
- **Trigger**: ADR 0076 exact fixed-tick stepping and an admitted ADR 0084 runtime host exist, and a
  real debugging workflow requires pausing inside a fixed tick rather than observing completed ticks.
- **Related ADRs**: 0002, 0003, 0039, 0057, 0076, 0084
- **Question**: Which stable system identity, topology generation, strict execution mode, open-tick transaction, conditional-breakpoint vocabulary, and failure/discard rules can support system stepping without splitting command acknowledgement or allowing parallel work across a claimed breakpoint?

## OQ-020: Rust Hot-Patch Experiment

- **Status**: open
- **Owner**: optional development tooling and runtime host
- **Trigger**: The reference game has separate P50/P95 measurements for compatible function-body and structural Rust edits, the last-good rebuild/restart path is reliable, and compatible edits still miss the iteration target.
- **Related ADRs**: 0034, 0042, 0076, 0093
- **Question**: Can a pinned Subsecond-like development plugin patch coarse explicit Rust call boundaries only after complete tick/schedule quiescence, classify incompatible layout/signature/static/dependency changes, retain a verified-compatible `World`, and fall back deterministically to the last-good rebuild/restart path across Windows and Linux without claiming a stable native ABI or automatic rollback?

## OQ-021: HDR, Wide-Gamut, and Tone-Mapping Contract

- **Status**: open
- **Owner**: `nara_image`, `nara_render`, material domains, render backends, authoring tooling
- **Trigger**: The first HDR output, wide-gamut asset workflow, tone-mapping/color-grading pipeline,
  or display-profile-aware authoring workflow is scheduled beyond ADR 0092's SDR compatibility mode.
- **Related ADRs**: 0005, 0033, 0040, 0092, 0094
- **Question**: Which scene-referred working space, asset primaries, HDR target formats, exposure,
  tone-mapping and color-grading ownership, paper-white/UI composition, mastering metadata, and
  display capability policy extend the SDR path without silently reinterpreting existing content?

## OQ-022: Editor Render Execution Ownership

- **Status**: open
- **Owner**: render/platform adapters and `nara_tooling`
- **Trigger**: The first offscreen editor viewport or multiple isolated edit/play runtimes need to share a device, caches, or render targets.
- **Related ADRs**: 0015, 0034, 0042, 0078, 0094
- **Question**: Should the editor own or lease a shared process-level render execution authority,
  should each isolated App retain an independent authority, or is another model smaller? How do
  bounded frame transfer, target leases, device epochs, cache budgets, and shutdown rules prevent
  one runtime from invalidating another?

## OQ-023: Platform Application Lifecycle

- **Status**: open
- **Owner**: platform adapters, product hosts, runtime service domains
- **Trigger**: The first non-desktop target, or a supported desktop integration, requires lifecycle
  semantics beyond focus/close: ordered suspend/resume, platform-session loss/recreation, or
  platform-requested termination.
- **Related ADRs**: 0013, 0039, 0041, 0042, 0056, 0082, 0084, 0086, 0088
- **Question**: Which normalized lifecycle transition drafts may platform adapters produce, which
  product-host safe point admits them, and which runtime/service sessions quiesce, survive, rebuild,
  or terminate without confusing operating-system suspension with ADR 0039 gameplay pause or making
  one platform adapter authoritative?
- **Boundary**: Cross-domain response to memory pressure remains OQ-024. Permission requests/results
  and orientation/safe-area changes are separate platform-capability and display-state contracts to
  be admitted by concrete target workflows.

## OQ-024: Cross-Domain Memory-Pressure Coordination

- **Status**: open
- **Owner**: product/executable hosts and residency-owning asset, render, audio, and service domains
- **Trigger**: At least two implemented residency-owning domains publish bounded ADR 0068 resident
  and reclaimable-byte observations, and a named supported target/workload proves that independent
  domain policies cannot meet an explicit process memory ceiling or that an ordered response from
  multiple domains is required before the runtime may continue. A platform pressure notification
  alone is not admission evidence.
- **Related ADRs**: 0037, 0040, 0042, 0054, 0068, 0082, 0088, 0089
- **Question**: Which minimal typed pressure episode and request/result contract may a product host
  admit at a safe point so each domain independently decides what to evict, defer, rehydrate,
  degrade, or refuse; reports bounded resident/reclaimable bytes and live-lease blockers; and
  returns continue, degraded, reject, or graceful-stop outcomes without a global allocator,
  universal priority/fairness policy, or violation of live dependency closures and last-good
  generations?
- **Boundary**: Platform adapters only produce normalized pressure drafts. Per-domain cache modes,
  lease/pin semantics, eviction/rehydration algorithms, and numeric defaults remain domain-owned;
  OQ-016 owns GPU cache defaults. Active scene/content candidate admission and atomic publication
  remain with ADR 0088/0089 or a later residency-closure decision.

## OQ-025: Profiling, Crash Artifacts, and Telemetry Channels

- **Status**: open
- **Owner**: executable hosts, `nara_app`, tooling, and backend adapters
- **Trigger**: A measured performance regression or production crash requires high-frequency CPU/GPU
  timing, schedule/task spans, call stacks, breadcrumbs, or an externally consumable crash artifact
  that the bounded diagnostics bus cannot represent.
- **Related ADRs**: 0009, 0036, 0048, 0068, 0076, 0078, 0084, 0094
- **Question**: Which separate trace, profiler, crash-artifact, and opt-in telemetry contracts provide
  stable correlation, bounded capture, privacy/redaction, retention, and export without turning
  `RuntimeDiagnostics` into a high-volume event stream or process-global policy owner?

## OQ-026: Frame-Critical Job Execution Model

- **Status**: open
- **Owner**: `nara_app`, `nara_ecs`, `nara_tasks`, render domains
- **Trigger**: Profiling shows a fixed-tick, extraction, preparation, or encoding workload cannot
  meet its frame budget through ordinary ECS schedule parallelism or the current background task
  pools without unacceptable latency or synchronization.
- **Related ADRs**: 0003, 0008, 0039, 0052, 0078, 0080, 0084, 0094
- **Question**: Does Nara need a distinct bounded frame-job graph, and if so which dependency,
  work-stealing, affinity, join barrier, panic/cancellation, deterministic-test, and shutdown rules
  separate it from ECS systems, long-running background tasks, and backend-affine workers?

## OQ-027: Network Authority and Replication Contract

- **Status**: open
- **Owner**: future networking/replication domain
- **Trigger**: A playable multiplayer slice can name topology, authority, prediction/reconciliation,
  interest management, latency, bandwidth, player count, and deployment targets.
- **Related ADRs**: 0024, 0028, 0042, 0045, 0056, 0057, 0058, 0089
- **Question**: Which session and entity authority, spawn/despawn, component eligibility, wire
  compatibility, snapshot/delta, interest, prediction/rollback, command validation, and transport
  adapter contracts satisfy that slice without coupling durable records to runtime `Entity` values?

## OQ-028: Animation Evaluation and Write Arbitration

- **Status**: open
- **Owner**: future animation domain and affected component owners
- **Trigger**: The first animation slice writes a field also controlled by gameplay, hierarchy,
  physics, UI, or editor tooling, or requires blending, root motion, markers, or event tracks.
- **Related ADRs**: 0019, 0029, 0039, 0045, 0081, 0085
- **Question**: Which binding/evaluation phases, writer ownership and priority, blend/accumulation
  rules, root-motion handoff, marker/event timing, and fixed-versus-render schedule semantics prevent
  last-writer-wins behavior from becoming the animation contract?

## OQ-029: Gameplay Active and Enabled Semantics

- **Status**: open
- **Owner**: `nara_app`, hierarchy, gameplay, and service domains
- **Trigger**: A real workflow must disable an entity or subtree without despawning it and expects
  defined behavior across systems, physics, animation, audio, input, scripts, and scene travel.
- **Related ADRs**: 0002, 0023, 0036, 0039, 0085, 0089
- **Question**: Is activity local, inherited, or domain-specific; how is it scheduled and queried;
  which enter/exit transitions are emitted; and which identity, hierarchy, service, and authoring
  state remains retained while activity is disabled?

## OQ-030: Navigation and AI Spatial Query Boundary

- **Status**: open
- **Owner**: future navigation/AI domain and spatial data owners
- **Trigger**: A playable AI slice needs a navmesh, grid, pathfinding, dynamic obstacles, crowd
  movement, or asynchronous spatial queries with named 2D/3D and target constraints.
- **Related ADRs**: 0016, 0018, 0042, 0052, 0085, 0089
- **Question**: Which authored/imported navigation data, runtime update/query contract, task and
  safe-point model, stable identities, backend seam, and headless/deterministic guarantees satisfy
  that slice without placing a speculative behavior-tree or global navigation server in core ECS?

## OQ-031: Source Extension Package and Trust Topology

- **Status**: open
- **Owner**: package/build hosts, plugin/editor/importer owners, security adapters
- **Trigger**: An independently versioned module must install a coherent combination of runtime
  plugin, editor tool, importer, content/template, native extension, or user mod, or Cargo-only
  transport creates a measured product-workflow gap.
- **Related ADRs**: 0016, 0042, 0046, 0070, 0079, 0086, 0087, 0088, 0093
- **Question**: Which source-package unit, resolution/lock/source metadata, declared contributions,
  provenance and trust tiers, capability grants, target restrictions, lifecycle/update policy, and
  optional isolation boundary provide one coherent installation experience without inventing a
  second Rust package manager or treating native code like validated data?

## OQ-032: Incremental Authoring Projection

- **Status**: open
- **Owner**: authoring document domains, `nara_scene`, `nara_tooling`
- **Trigger**: A concrete projection/baking path chosen through OQ-008 is correct, but representative
  edit latency or generated-output churn exceeds the editor budget under full projection rebuilds.
- **Related ADRs**: 0026, 0038, 0047, 0081, 0083, 0087
- **Question**: Which projection dependencies, cached outputs, invalidation granularity, specialized
  patch operations, provenance remaps, and atomic fallback rules can make baking incremental while
  preserving document truth, undo, and the exact output of a clean rebuild?

## OQ-033: Structured Data Asset Schema and Authoring

- **Status**: open
- **Owner**: `nara_asset`, `nara_reflect`, data-owning domains, authoring tooling
- **Trigger**: A reference game needs editor-authorable, hot-reloadable, migratable structured data
  such as weapons, enemies, abilities, dialogue, loot tables, or balance configuration shared by
  multiple scenes/components.
- **Related ADRs**: 0007, 0011, 0033, 0043, 0045, 0051, 0081, 0083, 0087
- **Question**: Which typed value/schema boundary, stable asset/subobject identity, inline and
  external representations, references, migration, validation, editor capabilities, and reload
  semantics support reusable data assets without treating every value as an ECS component or
  expanding component reflection into a universal object system?

## OQ-034: Gameplay State Topology and Scoped Lifetime

- **Status**: open
- **Owner**: `nara_app`, gameplay domains, scene/tooling integration
- **Trigger**: A reference game simultaneously needs boot/menu/gameplay/pause/game-over states,
  overlays or orthogonal state domains, and explicit system/entity/resource/message lifetime on
  state entry and exit.
- **Related ADRs**: 0003, 0023, 0036, 0039, 0047, 0084, 0089
- **Question**: Which flat, hierarchical, stacked, or orthogonal typed-state topology is justified;
  how do schedules and run conditions observe transitions; and which scoped entities, resources,
  messages, services, persistence, and scene ownership clean up deterministically without creating
  a hidden global scene tree?

## OQ-035: Spatial World Partition, Streaming, and Origin Policy

- **Status**: open
- **Owner**: scene, asset, spatial, render, physics, navigation, and runtime host domains
- **Trigger**: One active scene exceeds memory or frame budgets, coordinate precision becomes
  visible, or content must stream around a camera/player while cross-region references remain live.
- **Related ADRs**: 0018, 0037, 0053, 0068, 0083, 0085, 0088, 0089
- **Question**: Which cell/layer selection, authored and cooked partition data, cross-cell identity and
  reference rules, residency budgets, activation safe points, hierarchy boundaries, and origin
  shifting coordination satisfy the first large-world workflow across rendering, physics,
  navigation, audio, and networking?

## OQ-036: Panic, Abort, and Native Callback Fault Containment

- **Status**: open
- **Owner**: `nara_app`, executable hosts, `nara_tasks`, and native service/backend adapters
- **Trigger**: A production executable, task worker, system adapter, or native callback can panic or
  fault and the product must choose between process termination, runtime-generation failure and
  fresh restart, or an explicitly proven containment domain.
- **Related ADRs**: 0003, 0008, 0009, 0042, 0048, 0052, 0068, 0078, 0084, 0086, 0093
- **Question**: Which build-profile panic strategy, unwind/abort rules, catch boundaries,
  FFI/native-callback guards, worker failure propagation, invariant invalidation, runtime/process
  terminal states, crash handoff, and fresh-restart policy contain faults without relying on unwind
  across FFI or resuming a generation whose `World`, service, or native state may be compromised?
- **Boundary**: Typed recoverable errors remain the normal contract. Until an admitted containment
  design proves otherwise, catching a panic does not authorize continued gameplay in the same
  runtime generation; OQ-025 owns profiling, crash-artifact, and telemetry channels rather than
  containment semantics.

# AGENTS.md

This file provides repo-local guidance for agents working on nara.

## Start Here

Before changing code or architecture, establish the repository state and read authority in this
order:

1. Inspect `git status --short --branch` and preserve every concurrent user or agent change.
2. Read `STRATEGY.md` when present for product scope and non-goals.
3. Read [`docs/architecture/README.md`](docs/architecture/README.md) for document authority and
   activation rules.
4. Read the relevant Accepted ADRs for durable boundaries, then read
   [`docs/architecture/adr/implementation-status.md`](docs/architecture/adr/implementation-status.md)
   for what is actually implemented and what remains open.
5. Execute only a plan whose frontmatter says `execution_state: active` and whose registration is
   active in engineering memory. The current execution contract is
   [`docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md`](docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md).
6. Treat [`docs/architecture/open-questions.md`](docs/architecture/open-questions.md), Proposed ADRs,
   design harnesses, appendices, and inactive plans as evidence or candidate input, never as
   implementation authority.

[`docs/knowledge/engineering/current-state.md`](docs/knowledge/engineering/current-state.md) is a
derived navigation view, not an independent source of truth. Check its active registration,
fingerprint, cited evidence, and repository revision before relying on it. Do not infer authority
from the newest filename, the most detailed document, or the value
`artifact_readiness: implementation-ready`. When sources disagree, stop the affected implementation
path and reconcile the authority, ledger, active plan, and engineering-memory evidence first.

## State Vocabulary

- **Implemented** means repository evidence exists in the implementation ledger at the reviewed
  revision.
- **Accepted target** means an Accepted ADR constrains future implementation; it does not prove the
  current code already has that shape.
- **Proposed or candidate** means evaluation only. It cannot authorize a public API, crate, or
  product claim.
- **Active plan** owns execution order only. It may gather evidence for a Proposed ADR but cannot
  silently make that ADR Accepted.

The Project Direction rules below are durable constraints and accepted targets unless a sentence
explicitly says that an implementation is transitional or a candidate. Use the implementation
ledger, tests, and current source for implementation truth.

## Task Router

| Task | Required context before action |
|---|---|
| Implementation | Active plan unit, its entry trigger and gates, related ledger rows and Accepted ADRs, current code and tests |
| Architecture decision | Product strategy, architecture map, relevant Accepted ADRs, implementation gaps, and the owning open question |
| Research or design | Owning open question or active-plan evidence request; `repo-ref/` remains read-only and design drafts remain non-normative |
| Code review | Fixed point plus dirty-worktree fingerprint, active unit/spec, relevant AGENTS rules and Accepted ADRs; report findings before summaries |
| Handoff or integration | Active registration, exact source revision, dirty/staged ownership, verification evidence, remaining blockers, and next admitted unit |

## Project Direction

- nara is an open-source game engine written in Rust. Rust is the official and complete game-authoring language; public Rust APIs must support the full production path.
- nara is an integrated product built from explicit modules. The first-party default combination prioritizes a coherent workflow; crate and plugin boundaries support documented reuse and explicit replacement, including source/data migration when contracts differ, without promising arbitrary cross-engine compatibility.
- Runtime simulation is ECS-backed. Components are data and systems own behavior, but project documents, process-level hosts, native backends, package metadata, and editor workspace state keep their own authorities outside the simulation `World`.
- `nara_ecs` uses `bevy_ecs` as the ECS substrate. Do not reintroduce a custom ECS unless an ADR explicitly replaces ADR 0002.
- Gameplay-language and scripting runtimes are optional plugin/package Adapters, not a required second author language. OQ-007 records a Godot/Unity-like first-party C#/.NET gameplay experience as the preferred product hypothesis and leading trial candidate, but it does not authorize CoreCLR/Roslyn dependencies, a managed build or public SDK, a script capability, dynamic non-Rust ECS storage, or a default product feature. Treat OQ-007's evidence ladder only as candidate research routing, never as implementation authority: bounded feasibility research requires both a named Rust workflow gap and the named Schema, first-playable, Editor Host, and candidate-package baselines, while production Adapter work requires an active separate Trial plan plus the applicable Accepted ADR constraints. `Plugin` remains a Rust-side engine/game module; Adapter-loaded assemblies or script modules are not `Plugin` implementations. Do not freeze a universal Behavior Host, default Wasm ABI, or core VM dependency before a concrete Adapter and game workflow prove the shared contract.
- Rust iteration is layered: reload assets/scenes/data directly; permit compatible code patching only behind explicit capability detection; use incremental rebuild plus a fresh isolated runtime for structural changes; restore state only through declared, validated contracts. Never claim that every Rust edit hot-reloads in place.
- `nara_app` owns nara's product-facing `App`, fallible `Plugin`, stage, runner, and lifecycle boundary. Do not adopt `bevy_app`.
- `nara_app` owns pause/resume/time-scale execution and exact complete fixed-tick stepping. One step from Paused must run the whole fixed Prepare/Simulate/Finalize and gameplay Admit/Consume/Capture/Acknowledge transaction exactly once, then return to Paused. Render-frame stepping and future system stepping are separate contracts.
- Third-party participation and ordering may depend only on schedule labels and system sets explicitly documented as public semantic anchors. The first-playable inventory contains `CoreStage::FixedUpdate` and the frame-end `CoreStage::Cleanup` schedule labels plus joinable `FixedUpdateSet::Simulate`, `GameplayCommandSet::Consume`, and `GameplayCommandSet::Capture` for membership/before/after ordering; unlisted public variants are not promises. `Cleanup` is an observation/retirement point after the render pipeline and runs while paused without proving a present. Do not describe ordering against a schedule label, concrete first-party system, private subset, or incidental registration order as compatible. Every public anchor must document its participation mode, inputs/completion, deferred flush, skip, App/domain fault, and cleanup semantics; composition owns validated cross-domain set edges. Before `App::seal`, build and validate each owning schedule and required set graph, require automatic deferred insertion, reassert final deferred application, then expose no public schedule-configuration mutation on the sealed App. An ignore-deferred relation is a trusted advanced opt-out that cannot claim public-anchor compatibility; do not add a scheduler wrapper solely to police it. This does not promise a total order among otherwise unordered phase peers.
- Product-capability admission returns `CompositionError`, pure plugin group/slot/dependency planning returns `PluginPlanError`, preparation returns `PluginPrepareError`, and App-level plugin hooks return `PluginError`; do not flatten these phases or reintroduce panic-based prerequisite helpers.
- Plugin composition targets static `Plugin::declaration()`, stable definition keys, data-only groups, sealed plugin/group/tuple inputs, pure resolution, private preparation, and closed commit. `Plugin::build`/`finish` must not install plugins/groups or select the process runner, including on the direct App path.
- Plugins should expose stable IDs and only the capabilities, requirements, conflicts, services, and schema facts that real composition consumers use. A `PluginServiceId` proves presence only; it is not provider selection, exclusivity, or runtime dispatch. Default plugin groups are explicit product bundles; `MinimalPlugins` must stay headless and minimal. Ordinary external plugins must be appendable without definition IDs, fingerprints, or slot anchors, while code-first and file-backed products should share one inspectable, editable official top-level recipe.
- Root Cargo features are coarse compiled product-capability ceilings. The required product capabilities of a resolved plugin plan must be a subset of the normalized project request, which must be a subset of the compiled ceiling; validate plugin service requirements/conflicts as a separate closure before mutating `App`. `default` stays `runtime-core`, `serde` weak-forwards only into already enabled domains, and placeholder crates require a real production consumer plus the ADR 0079 admission evidence.
- Headless/server profiles must not install window, render, audio-device, editor, or UI-toolkit adapter plugins by default. Server-ready gameplay should run through deterministic-friendly simulation stages and consume semantic gameplay commands rather than raw keyboard/mouse/window input.
- `HeadlessRuntimePlugins` is for local headless tests/AI drivers and may include low-level input observations; `ServerPlugins` is the stricter server-ready bundle and must not install raw input resources unless the app explicitly adds them.
- `nara_gameplay` owns gameplay command drafts/submissions/envelopes, stable command target vocabulary, bounded action-to-command mapping, and the authoritative fixed-tick queue/batch lifecycle. Public producers use `GameplayCommandIngressSource`; `LocalAction` is reserved for the engine-owned action mapper. It must not introduce networking transports or runtime `Entity` values into durable command data.
- Gameplay command consumers join `GameplayCommandSet::Consume`; replay/debug taps join `Capture`; only the engine-owned `Acknowledge` set retires a current healthy batch. Do not reintroduce frame cleanup, direct pending-queue reads, or consumer-owned acknowledgement.
- Gameplay pending plus active commands share one retained budget. On a lifecycle invariant failure, the first fault is sticky, active work moves into queue-owned quarantine, consumers are gated, and pending plus quarantine remain budgeted until the owning runtime is discarded. Do not add in-place poison recovery.
- File, replay, network, and package adapters must enforce ADR 0049 encoded byte/depth/count budgets before deserializing gameplay submissions; serde's semantic limits are not an allocation boundary.
- `nara_tasks` owns bounded threaded task pools, typed terminal handles, cancellation tokens, ordered integration helpers, finite shutdown, and an explicitly test-only inline driver. Do not expose Tokio or async-std as nara's gameplay-facing async contract, and do not reintroduce a production deterministic/inline execution mode.
- `nara_app::CoreStage::TaskUpdate` is the explicit main-thread integration point for background results, but `nara_app` owns no business-domain integration set and `nara_tasks` configures none. `nara_asset` owns `AssetTaskUpdateSet::{Poll, ResolveSourceChanges, SpawnJobs, ApplyResults}`; each poller captures one ready membership or queue prefix at entry, eligible predecessor-unblocked outcomes must apply in that frame, stale/superseded outcomes retire, and only later-ready or eligible missing-predecessor work waits.
- File-backed projects use `nara.toml` as the project settings authority. Runtime embedding may override settings through explicit resources/plugin configuration, but engine domains must not invent separate persistent project config files for asset roots, startup scenes, task pools, window defaults, or input-map sources. The manifest may request semantic capabilities mapped by a compiled Rust recipe; it cannot introduce Rust code or select an arbitrary plugin/provider by string identity. Every accepted capability must map to actual composition behavior rather than parse-only success.
- `nara_project` owns `ProjectManifest` parsing, validation diagnostics, profile overlays, and `EffectiveProjectSettings` lowering. It must stay side-effect-free: no window creation, no task thread creation, no GPU resources, no direct `World` mutation.
- Transient event/message/resource queues must document producer, consumer, retention, cleanup stage, and replay/diagnostic role. Typed resource queues are allowed when their lifecycle is explicit; do not introduce an untyped global event bus.
- Runners pass real elapsed time into `nara_app`; the app lowers it into explicit real, virtual/game, fixed, and render-interpolation time domains. Pause, time scale, max delta, fixed catch-up, runtime state transitions, background/redraw policy, and frame-transient cleanup follow ADR 0039.
- Specialized domains such as physics, audio, text shaping, animation, input policy, and editor toolkits are plugin-owned by default. They may own their semantic components/schema, public sets, queries/events, native state, faults, and configuration. Nara shares App/ECS/time/schedule/schema/asset/diagnostic/lifecycle substrate, not a universal Service/Command/Result state machine or provider Interface.
- Persistent data must exclude native handles, pointers/closures, runtime `Entity`/`AssetId`/`Handle<T>`, Rust/Bevy runtime IDs, Host capabilities/absolute paths, solver/session/worker indices, callbacks/native-object tokens, transient caches/manifolds, unbounded opaque backend blobs, and other process-local identity. Plugin-owned records must be bounded, canonical, versioned, and migration-aware. External threads never mutate the `World` directly, and long-lived native/thread owners must reach a finite truthful terminal state; queues, batches, acknowledgements, poisoning, main-thread stages, and retryable close are domain choices justified by real ownership.
- A concrete first-party integration may ship through one plugin-owned API. Require production-shaped variation pressure, an independent implementation challenge, and a second real Adapter only before claiming a Nara-owned cross-implementation Interface or unchanged-data portability; fakes prove conformance and faults only.
- Keep `wgpu`, `winit`, egui, and dear-imgui behind their owning adapter crates because they cross accepted renderer/platform/toolkit boundaries. A leaf plugin may directly own a concrete physics/audio/text library when no second internal adapter boundary has been proven.
- `nara_winit` owns all `winit` imports and desktop event-loop integration.
- `nara_render` owns backend-neutral render concepts, frame lifecycle, views, targets, stock phases, `RenderPassPlan`, `RenderBackendStatus`, `RenderBackendState`, and skipped-frame reasons. `RenderPassPlan` is the current engine-owned static phase-order contract, not a persistent recipe, partial graph, or arbitrary external pass API. Public dependency edges, arbitrary phase/plan constructors, or a submitter SPI require a complete external feature tracer before admission; OQ-001 chooses a future execution/graph shape only after a concrete intermediate, history, or cross-target use case proves static planning insufficient.
- wgpu is nara's only RHI. Exact wgpu limits, features, downlevel/surface capabilities, handles, encoders, and queues stay in `nara_render_wgpu`. Do not add a mirrored nara GPU API, speculative generic backend trait, or WebGL2 compatibility pipeline.
- Pipeline families, renderer profiles/recipes, public feature/pass catalogs, retained render scenes, `CompiledPipelineTemplate`, `FrameExecutionPlan`, scoped raw-wgpu access, native interop, and replacement Render Host roles are candidate mechanisms under ADR 0094, not current implementation requirements. Each requires a production-shaped tracer, alternatives review, and separate admission before a public API is added.
- The current owned `RenderFramePacket` freezes admitted topology only; do not claim it already owns all sprite/UI/resource payloads. Resource-complete extraction may converge on an owned backend-neutral packet that does not borrow the gameplay `World` or carry surfaces/devices/encoders, but must not add a public packet-section/provider registry without a concrete external consumer. A second render ECS world and native render worker remain optional internal optimizations.
- `nara_render_wgpu` owns all `wgpu` imports and GPU surface/device lifecycle through one serialized execution authority. Browser WebGPU is JavaScript-agent/local-executor affine and uses local asynchronous initialization; native placement is declared by its platform adapter. Do not use `fragile-send-sync-non-atomic-wasm`, unsafe Send/Sync, or a blocking render-stage initializer to satisfy ECS resource bounds. Surface loss and unexpected device loss are separate; Device/Queue-dependent objects, physical caches, backend realizations, and device-dependent async/encoded results use a non-reused host/device epoch.
- A native window target admits only one live surface binding. Its non-cloneable handle source owns
  the native guard and acknowledges release from `Drop`; the paired `WindowSurfaceLease` may only
  verify that release or request retirement. Platform runners invoke the registered surface-
  retirement driver only for targets they registered; they must not use global plugin cleanup as a
  local retirement pump. Device-loss detection and local invalidation do not prove epoch-
  correlated recovery. An `Unavailable` backend must not re-enter device initialization every frame;
  recovery requires a future explicit retry or reconstructed backend contract.
- `nara_render_wgpu` reports backend initialization, skipped frames, and backend errors through `RenderBackendStatus`.
- Backend GPU resource caches own textures, buffers, samplers, bind groups, pipelines, and intermediate targets. Cache invalidation must be generation/device/budget aware; do not make one-frame-unused pruning the product contract.
- Sprite/UI/text/3D domains own extraction and batch data. The stock wgpu integration may encode concrete feature batches through private feature-gated modules; independent submitter plugins or provider traits are not required until a second encoder/lifecycle owner proves the SPI.
- `nara_sprite_render` owns backend-neutral 2D extraction, queueing, sorting, and batching. GPU backends should consume `SpriteBatches`, not gameplay `Sprite` or `Tilemap` components.
- `nara_ui` owns runtime UI authoring components, computed layout resources, and pointer interaction state. It must not depend on egui, dear-imgui, winit, or wgpu.
- `nara_ui_render` owns backend-neutral runtime UI extraction, queueing, clipping, sorting, and batching. GPU backends should consume `UiBatches`, not gameplay/editor UI state directly.
- Input is layered: platform adapters produce normalized input events/state; `nara_input` resolves retained button/pointer state into `ActionOutcomes` through `ActionMap` and declared input schedule sets. UI focus, pointer capture, text/IME, richer context priority, and accessibility remain future layers on the same route.
- Client physical input lowers into gameplay commands through `nara_gameplay::ActionCommandMap` before crossing replay, AI-driver, or future server-authoritative boundaries.
- `nara_scene` owns persistent `SceneDocument` / `PrefabDocument`, stable `SceneEntityId`, validation, and world spawn/export. Scene/prefab documents must not store runtime `Entity`, runtime `AssetId`, or backend handles.
- After prefab expansion, the explicit stable-ID component records are the complete persistent composition. Bevy `#[require]`, component hooks, and observers do not implicitly define Scene/Prefab/Inspector/undo/migration/export semantics and are not covered by rollback claims. RGF-U29 uses two eligibility phases: provider freeze rejects required-component and intrinsic `ComponentHooks` metadata, while every persistent target-World apply flushes deferred work, records the post-flush rejection baseline, and takes an exclusive borrow. A fresh-target apply rechecks real `ComponentInfo` hooks plus matching event-global/component-global `Add`/`Insert`/`Discard`/`Remove`/`Despawn` observers before allocation and holds exclusivity through allocation and persistent insertion, so no entity-scoped observer can appear after the target exists. An apply to an already existing or reserved target additionally checks entity and entity+component scopes before its first persistent mutation. Rejection leaves the applicable post-flush baseline unchanged. Any version-coupled hook-presence probe stays private to `nara_ecs`. Runtime-only ECS and post-publication World-local hooks/observers remain valid runtime behavior, but every later persistent apply repeats the applicable check, rejects while a matching hook remains installed, or waits for an explicit Host safe point to disable a matching observer. RGF-U12 proceeds independently and certifies only World-independent document/schema truth; U12 and U29 converge before RGF-U26 first materializes content.
- Scene/prefab authoring edits should use `ScenePatchDocument` transactions. Patch operations serialize as `op + args`, validate against schema-aware `ComponentFieldPath`, and return inverse patches for undo.
- Continuous slider, gizmo, curve, drag, and text-composition edits use one toolkit-neutral `Begin -> bounded Preview -> Commit(one patch + inverse) / Cancel` transaction. Preview creates no undo entries; focus/capture loss, tool close, target deletion, and revision conflict must cancel or reject explicitly.
- Scene, prefab, and patch document shape changes require document-level migrations before component-value migrations and validation. Runtime loading must not silently rewrite source files.
- `SceneAuthoringSession` is the first authoring/live sync boundary. It treats `SceneDocument` as truth, stores undo/redo as inverse patches, and rebuilds its managed live `World` projection instead of mutating arbitrary ECS storage directly.
- `nara_tooling` owns UI-agnostic editor/debug models such as `WorldSnapshot` and `SceneInspectorState`. UI adapters should render tooling models and send tooling commands instead of inventing editor-only mutation paths.
- Tooling observations, diffs, timelines, breakpoints, and future replay/checkpoint records must use the U8 stable identity vocabulary, never runtime `Entity`, Bevy `NodeId`, backend handles, or process pointers. `WorldSnapshot` is transitional and should be deleted rather than preserved when the stable schema-aware observation model lands.
- Component observation requires schema eligibility plus an independent disclosure/redaction policy. `Inspect` does not automatically authorize remote transport, logging, or persistent capture, and runtime diagnostics must not become a high-frequency component trace.
- Timeline models must distinguish proven provenance, temporal correlation, and explicitly instrumented direct causality. A command and component diff in the same tick do not alone prove causation.
- AI, script, behavior-tree, and other interpreter domains own optional `ExecutionCursor` program/instruction/source-map state. Cursor held-data and source-map references require the host observation allowlist/redaction policy and must not expose absolute host paths. nara must not infer a source line for ordinary Rust systems or ECS entities.
- Backwards debugging means restoring a completed-tick checkpoint into a fresh isolated runtime and replaying commands plus recorded outcomes forward. Do not implement inverse world diffs or claim reverse machine execution.
- Asset/data reload, optional Rust function hot-patching, runtime reconstruction, and script-module reload are separate contracts. A Subsecond-like development plugin may patch explicitly integrated compatible Rust bodies only at an engine-owned quiescent boundary; structural or unknown changes rebuild and start a fresh isolated runtime. Keep last-good recovery independent, and do not claim native state migration, dynamic ABI stability, or post-commit rollback without direct evidence.
- Editor workspace state belongs in `nara_tooling`: open documents, active scene/document, selection set and top-selected target, document dirty/saved revisions, external reload conflicts, per-document undo/redo, and workspace commands must remain UI-toolkit agnostic.
- `nara_tooling_egui` owns all `egui` imports and early egui debug/editor panels. Core runtime crates and `nara_tooling` must remain UI-toolkit agnostic.
- Play Mode must use an isolated runtime `World` fork spawned from a validated edit document snapshot. Stop Play discards runtime changes by default; persistent write-back must be an explicit Apply Changes flow that produces `ScenePatchDocument` and goes through normal validation/undo. The first supported Apply Changes subset is selected `SceneEntityId` plus explicitly requested registered scene/edit-capable component IDs; whole-scene diffing, prefab-expanded write-back, and edit-while-playing merge are still unsupported.
- Prefab overrides are `ScenePatchDocument` values applied relative to source prefab IDs before expansion. Do not reintroduce whole-component prefab override maps.
- Nested prefab source resolution goes through `PrefabSourceResolver`; expanded prefab IDs use the `anchor/source_entity` namespace rule.
- Scene/prefab tooling must preserve authoring provenance: scene-local entities patch the scene, prefab source entities patch the prefab, prefab anchors patch the scene instance, and prefab-expanded projections must write back only through explicit override or convert-to-local flows.
- `nara_scene` must keep scene/prefab spawn two-phase: preflight first, then mutate the target `World`. Asset-aware spawn uses a scratch `AssetServer` and only writes it back after the full preflight succeeds.
- `nara_reflect` owns `ComponentValue`, schema metadata, `ComponentFieldPath`, component preflight/apply codecs, migrations, `ComponentDecodeContext`, and `ComponentEncodeContext`. Keep its value, schema, path, codec, migration, and registry modules focused. Domain crates register their own built-in component codecs through their plugins.
- A schema owner's durable identity/version/tombstone lineage is distinct from one runtime recipe's composed catalog fingerprint. Omitting an optional plugin does not tombstone its schemas or forbid later compatible reactivation. Runtime requires complete frozen bindings; missing-schema authoring remains fail-closed until ADR 0090 is accepted and implemented.
- Canonical-v1 component/field capabilities are exactly `scene`, `inspect`, `edit`, `asset_ref`, and `entity_ref`. Save, animation, replication, script read/write, and diagnostic projection remain domain-owned future values; add one only after a concrete consumer and Accepted format decision prove its behavior, tests, and migration impact. Capabilities gate eligibility, not domain behavior or disclosure authority.
- `nara_asset` persistent references use semantic `AssetRef::Path` or `AssetRef::StableId`; `Handle<T>` and `AssetId` are runtime-only and must not serialize as project data.
- `nara_asset` owns source asset identity, `.meta` records, importer registry metadata, typed import job contracts, imported artifact records, dependency graph data, load states, reload generations, source change coalescing, and reload request scheduling. It must not own GPU resources, file watchers, or depend on render backend crates.
- File-backed project input is untrusted. Scene, prefab, patch, component value, image, asset metadata, import artifact, and schema catalog loaders should follow ADR 0049 parse/decode budgets before mutating runtime or project state.
- Logical `AssetPath` validation is not filesystem containment. File-backed asset roots, watcher paths, import-cache paths, symlinks, and Windows junctions must follow ADR 0050.
- Native filesystem authority belongs in `nara_fs`: domains consume opaque host-issued capabilities, opened handles, scoped identity evidence, and typed guarantee receipts. Canonicalize-then-open is not strict authorization; unsupported or unproven platform/filesystem guarantees fail closed.
- Persistent project files should converge on the ADR 0051 envelope: `kind`, `format_version`, `engine_min_version`, and `generator`, with golden fixtures for migration-sensitive formats.
- `SourceChangeResolver` must keep reload scheduling generation-stamped, expected-version guarded, and dependency-aware. Same-frame source changes coalesce by logical path with the last semantic event winning; do not make `Removed` unconditionally dominate atomic-save modify sequences.
- Asset source-change translation or scheduling failures must be observable through structured diagnostics. Do not discard `SourceChangeResolver` errors.
- Asset reload policy keeps runtime `Handle<T>` stable across reloads, preserves the last good typed asset value on failed reload when one exists, records first-load failure without inventing values, and treats GPU objects as backend cache entries rather than imported artifacts.
- `nara_asset_watch` owns all `notify` imports. Filesystem watcher events must be translated into semantic `AssetSourceChange` values before asset reload logic sees them. Keep this crate optional behind the root `asset-watch` feature.
- `nara_image::ImagePlugin` owns typed image importer registration, async image reload jobs, runtime `Assets<ImageAsset>`, and image render-resource preparation. `ImagePreparePlugin` is prepare-only and must not become a second asset loading path.
- `nara_image::ImageAsset` and `PreparedImageResource` describe image content/import identity only: source metadata, extent, format, color space, hashes, and pixels. Do not reintroduce image-owned sampler or material policy.
- `nara_material` owns backend-neutral 2D material intent: `FilterMode`, `AddressMode`, `SamplerDescriptor`, `AlphaMode2d`, `Material2dDescriptor`, semantic image references, and material keys. Sprites, tilemaps, UI images, and future 2D material users should route sampler/alpha/tint through this domain.
- Texture upload, atlases, materials, UI images, and future 3D assets must flow through the asset import + render resource preparation seam in ADR 0033 instead of direct path-to-wgpu shortcuts.
- `nara_render_wgpu` owns backend GPU resource caches. Gameplay/domain crates store typed handles or backend-neutral descriptors, never `wgpu` handles.
- GPU upload, dynamic buffer allocation, and backend resource stats follow ADR 0054 as refined by ADR 0096: backend ownership, finite admission, observability, and epoch invalidation are durable; ring/staging buffers, pending queues, universal deferral, and fairness are measurement-gated mechanisms. Per-frame buffer creation may remain while the reference workload stays within budget.
- Sprite/tilemap render batches are material-aware. `nara_sprite_render` resolves runtime image handles into `SpriteMaterialKey` values containing image resource key, sampler, alpha mode, and tint; backend caches consume those keys instead of image-resource-only batch keys.
- Runtime UI image panels are material-aware and flow through the same image prepare, sampler/material key, and backend texture cache path as sprites.
- `nara_render_wgpu` consumes `RenderPassPlan` plus sprite/UI batches for clear/world/UI/gizmo ordering. Do not bury new pass ordering rules in the wgpu draw loop.
- `nara_render_wgpu` must keep image texture upload cached separately from sampler/material bind-group identity so sampler changes do not require image reimport or texture reupload.
- Large tilemaps follow ADR 0096: measure a committed workload first, avoid known invisible work where correct, and trial the smallest private mechanism. Do not revive ADR 0053's fixed backend-neutral chunk cache without new admission evidence.
- Keep render modules split by responsibility: `nara_sprite_render::{types,extract,queue}`, `nara_ui_render::{types,extract,queue}`, and `nara_render_wgpu::{surface,sprite,ui}` should stay narrow instead of growing monolithic backend or render-bridge files.
- `DiagnosticReport::push` only collects diagnostics. Use `Diagnostic::emit_to_tracing` or `DiagnosticReport::emit_to_tracing` explicitly when logs are desired.
- Diagnostic code/domain/producer/field keys and summaries are engine-owned static identities/text.
  Dynamic values enter diagnostics only through classified fields: validated public identifiers or
  project-relative paths, numeric/boolean values, or value-free sensitive/secret redaction markers.
  Do not stringify errors, paths, environment values, URLs, component payloads, or credentials into
  summaries, identities, tracing fields, serialization, or dedupe keys.
- `DiagnosticReport` severity is sticky across bounded rejection and eviction. Merge reports through
  its bounded report merge API so observed severity and drop/truncation accounting are preserved;
  never rebuild a report by copying only retained entries.
- The root `nara` facade must keep `winit` and `wgpu` optional; default features stay backend-free.
- `nara::prelude` should stay gameplay-first and backend-free. Move backend/tooling/debug/render extraction, queue, batch, GPU cache, and other advanced extension types to `advanced_prelude` or module-specific preludes.
- `repo-ref/` contains reference source trees. Treat it as read-only reference material and keep it out of git.

## Architecture Rules

- Record durable architecture decisions under `docs/architecture/adr/`.
- Keep `docs/architecture/nara-foundation.md` aligned with implemented crate boundaries.
- Use `docs/knowledge/engineering/` for session memory, subagent findings, verification, and handoff state.
- Runtime diagnostics should be inspectable structured data first. Domain diagnostics may remain rich locally, but runtime problems that matter to tools should bridge into the ADR 0048 observability bus instead of relying on logs or private queues only.
- Server diagnostics and future metrics are first-class runtime outputs and must be observable without editor UI, windowing, rendering, or a tracing subscriber.
- Task pool changes must preserve main-thread integration, cooperative cancellation, deterministic testability, and ADR 0052 backpressure/diagnostics vocabulary.
- Prefer fearless refactoring before compatibility layers. This project is pre-1.0; remove obsolete scaffolding instead of preserving it.
- Keep scene/prefab/save data independent from runtime `bevy_ecs::Entity` values, runtime `AssetId`, and backend-native handles.
- Runtime UI is expected to be nara-owned long term. egui/dear-imgui are acceptable for debug/editor tooling, not as the runtime UI foundation.

## Rust Workflow

- Use Rust 2024 and the workspace MSRV in `Cargo.toml`.
- Format with `cargo fmt --all`.
- Prefer `cargo nextest run --workspace` for tests.
- Run `cargo check --workspace` before considering implementation work complete.
- Check optional backend examples with `cargo check -p nara --no-default-features --features desktop-winit,render-wgpu --example windowed_clear`, `cargo check -p nara --no-default-features --features runtime-2d,desktop-winit,render-wgpu --example windowed_sprites`, and `cargo check -p nara --no-default-features --features runtime-ui,desktop-winit,render-wgpu --example runtime_ui_panel` when touching platform, input, UI, or render backend code.
- Use dependency boundary searches when touching backend crates: `rg -n "winit::|winit =" crates src Cargo.toml` and `rg -n "wgpu::|wgpu =" crates src Cargo.toml`.
- Use precise commits with Conventional Commit messages.
- Do not discard or rewrite user changes. Never use `git reset --hard`, `git checkout --`, `git restore`, `git clean`, or stash to remove work unless the user explicitly asks.

## Subagent Guidance

- For architecture research, prefer read-only subagents with an explicit instruction not to spawn nested subagents.
- Subagents may inspect `repo-ref/`, docs, and source files, but the orchestrating agent owns edits, staging, commits, and final verification unless the user says otherwise.

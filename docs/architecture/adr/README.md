# Architecture Decision Records

This directory records durable architecture decisions for nara. An ADR's decision status and its implementation status are deliberately separate:

- `Accepted` means the project selected the decision.
- `Implemented` means repository evidence proves the selected contract is present and verified.
- An accepted ADR may be `not-started` or `partial` without weakening the decision.

Implementation state lives in [implementation-status.md](implementation-status.md), never in the ADR `Status` field.

## Decision Lifecycle

ADR decision status is one of:

- `Proposed`: under active review and not authoritative.
- `Accepted`: authoritative until another ADR supersedes it.
- `Superseded`: replaced by a named ADR; its number is never reused.
- `Rejected`: evaluated and deliberately not selected.

New ADRs use the next unassigned monotonic number; IDs reserved by an active accepted plan are not reused for a different decision. ADRs must contain Context, Decision, at least two Alternatives Considered, Consequences, Success Metrics, and Risks and Mitigations. Cross-domain or stateful decisions include a Mermaid flow or state diagram. Refinement and supersession links must be bidirectional.

Mutually dependent proposals may declare an atomic admission group. Every member remains
non-authoritative until one review accepts the complete group; partial promotion is invalid. Use
`Refines`/`Refined By` for decisions that replace only part of an ADR. Reserve `Superseded` for a
replacement that makes the complete earlier decision non-authoritative.

## Implementation Evidence

The implementation ledger uses these states:

- `not-started`: no implementation claim and no invented anchors.
- `partial`: existing code and verification anchors plus an explicit remaining gap and trigger.
- `implemented`: at least one existing code/configuration anchor and one existing verification anchor prove the complete decision.
- `superseded`: implementation evidence is owned by the replacement decision.

Each implementation unit updates its ADR and ledger row before code, then records final code and verification anchors after the unit passes. Historical classification outside an active implementation slice is non-blocking and belongs to the final integration unit.

## Pre-1.0 Replacement Policy

nara is unreleased. When a contract is wrong, remove it and make the corrected design canonical:

- Do not keep compatibility wrappers, deprecated aliases, or parallel `V1`/`V2` Rust APIs.
- Use the canonical unsuffixed Rust name for the replacement.
- Superseded draft persistent shapes and fixtures are deleted; the corrected shape becomes canonical `format_version = 1` after every in-repository source and fixture is updated.
- Add a migration-guide entry describing the intentional break, source rewrite, and cache rebuild/quarantine action. Runtime loading never silently rewrites source files.
- Preserve a compatibility reader or migration chain only when a new ADR names a real compatibility window and its owner.

See [the July 2026 migration guide](../../migrations/2026-07-engine-foundation.md).

## ADR Catalogue

- [ADR 0001](0001-runtime-workspace-boundaries.md): Runtime Workspace Boundaries
- [ADR 0002](0002-use-bevy-ecs-as-ecs-substrate.md): Use Bevy ECS as the ECS Substrate
- [ADR 0003](0003-own-app-plugin-and-schedule-lifecycle.md): Own App, Plugin, and Schedule Lifecycle
- [ADR 0004](0004-use-bevy-reflect-backed-component-metadata.md): Use Bevy Reflect-Backed Component Metadata
- [ADR 0005](0005-dimension-aware-runtime-with-2d-first-authoring.md): Dimension-Aware Runtime with 2D-First Authoring
- [ADR 0006](0006-scene-and-prefab-data-model.md): Scene and Prefab Data Model
- [ADR 0007](0007-asset-identity-and-import-pipeline.md): Asset Identity and Import Pipeline
- [ADR 0008](0008-runtime-concurrency-and-task-pools.md): Runtime Concurrency and Task Pools
- [ADR 0009](0009-diagnostics-errors-and-logging.md): Diagnostics, Errors, and Logging
- [ADR 0010](0010-plugin-lifecycle-dependencies-and-failure.md): Plugin Lifecycle, Dependencies, and Failure
- [ADR 0011](0011-component-schema-ids-and-migrations.md): Component Schema IDs and Migrations
- [ADR 0012](0012-render-crate-boundaries.md): Render Crate Boundaries
- [ADR 0013](0013-platform-window-and-runner-boundaries.md): Platform, Window, and Runner Boundaries
- [ADR 0014](0014-testing-ci-and-compatibility-policy.md): Testing, CI, and Compatibility Policy
- [ADR 0015](0015-editor-tooling-and-dogfooding-boundary.md): Editor, Tooling, and Dogfooding Boundary
- [ADR 0016](0016-extension-seams-for-backends-and-domain-modules.md): Extension Seams for Backends and Domain Modules
- [ADR 0017](0017-render-graph-policy.md): Render Graph Policy
- [ADR 0018](0018-coordinate-units-and-time.md): Coordinate, Units, and Time
- [ADR 0019](0019-physics-strategy.md): Physics Strategy
- [ADR 0020](0020-project-layout-and-package-format.md): Project Source Layout
- [ADR 0021](0021-scripting-and-wasm-boundary.md): Scripting and WASM Boundary (Superseded)
- [ADR 0022](0022-3d-coordinate-system.md): 3D Coordinate System
- [ADR 0023](0023-event-message-and-command-model.md): Event, Message, and Command Model
- [ADR 0024](0024-determinism-fixed-update-and-replay-policy.md): Determinism, Fixed Update, and Replay Policy
- [ADR 0025](0025-runtime-ui-system.md): Runtime UI System
- [ADR 0026](0026-editor-command-patch-and-undo-model.md): Editor Command, Patch, and Undo Model
- [ADR 0027](0027-save-game-and-runtime-persistence.md): Save Game and Runtime Persistence
- [ADR 0028](0028-networking-and-replication-scope.md): Networking and Replication Scope
- [ADR 0029](0029-animation-strategy.md): Animation Strategy
- [ADR 0030](0030-audio-strategy.md): Audio Strategy
- [ADR 0031](0031-text-and-font-strategy.md): Text and Font Strategy
- [ADR 0032](0032-render-backend-integration-boundary.md): Render Backend Integration Boundary
- [ADR 0033](0033-asset-import-and-render-resource-preparation-seam.md): Asset Import and Render Resource Preparation Seam
- [ADR 0034](0034-editor-play-mode-world-boundary.md): Editor Play Mode World Boundary
- [ADR 0035](0035-project-manifest-and-runtime-settings-authority.md): Project Manifest and Runtime Settings Authority
- [ADR 0036](0036-event-message-and-resource-queue-lifetime.md): Event, Message, and Resource Queue Lifetime
- [ADR 0037](0037-asset-load-request-cache-and-lifetime-policy.md): Runtime Asset Acquisition, Reload, and Lifetime Policy
- [ADR 0038](0038-scene-prefab-authoring-identity-and-provenance.md): Scene/Prefab Authoring Identity and Provenance
- [ADR 0039](0039-main-loop-time-pause-and-runtime-state.md): Main Loop, Time Domains, Pause, and Runtime State
- [ADR 0040](0040-render-resource-lifetime-and-submitter-ownership.md): Render Resource Lifetime and Submitter Ownership
- [ADR 0041](0041-input-routing-actions-text-focus-and-accessibility.md): Input Routing, Actions, Text Input, UI Focus, and Accessibility
- [ADR 0042](0042-runtime-service-and-backend-boundary.md): Runtime Service and Backend Boundary
- [ADR 0043](0043-scene-prefab-and-patch-document-migration-policy.md): Scene, Prefab, and Patch Document Migration Policy
- [ADR 0044](0044-root-facade-and-prelude-layering-policy.md): Root Facade and Prelude Layering Policy
- [ADR 0045](0045-component-schema-capability-metadata.md): Component Schema Capability Metadata
- [ADR 0046](0046-plugin-metadata-and-default-plugin-groups.md): Plugin Metadata and Default Plugin Groups
- [ADR 0047](0047-editor-workspace-and-scene-document-state.md): Editor Workspace and Scene Document State
- [ADR 0048](0048-runtime-diagnostics-and-observability-bus.md): Runtime Diagnostics and Observability Bus
- [ADR 0049](0049-untrusted-project-input-and-parse-budget-policy.md): Untrusted Project Input and Parse Budget Policy
- [ADR 0050](0050-asset-root-symlink-junction-and-package-trust-policy.md): Asset Root, Link, Mount, and Package Trust Policy
- [ADR 0051](0051-persistent-file-envelope-migration-and-golden-fixtures.md): Persistent File Envelope, Migration, and Golden Fixtures
- [ADR 0052](0052-task-backpressure-cancellation-and-long-running-diagnostics.md): Task Backpressure, Cancellation, and Long-Running Diagnostics
- [ADR 0053](0053-visibility-culling-and-tilemap-render-cache.md): Visibility, Culling, and Tilemap Render Cache
- [ADR 0054](0054-gpu-upload-budget-and-buffer-allocation-policy.md): GPU Upload Budget and Buffer Allocation Policy
- [ADR 0055](0055-feature-matrix-boundary-checks-and-compatibility-fixtures.md): Feature Matrix, Boundary Checks, and Compatibility Fixtures
- [ADR 0056](0056-headless-runtime-and-dedicated-server-readiness.md): Headless Runtime and Dedicated Server Readiness
- [ADR 0057](0057-authoritative-fixed-tick-and-command-ingress.md): Authoritative Fixed-Tick and Command Ingress
- [ADR 0058](0058-stable-runtime-identity-and-entity-references.md): Stable Runtime Identity and Entity References
- [ADR 0068](0068-global-resource-budgets-metrics-and-diagnostic-privacy.md): Global Resource Budgets, Metrics, and Diagnostic Privacy
- [ADR 0070](0070-capability-oriented-filesystem-substrate.md): Capability-Oriented Filesystem Substrate
- [ADR 0076](0076-play-runtime-debug-control-and-observation.md): Play Runtime Debug Control and Observation
- [ADR 0077](0077-render-pipeline-recipes-graph-compilation-and-backend-encoding.md): Render Pipeline Recipes, Graph Compilation, and Backend Encoding
- [ADR 0078](0078-render-host-affinity-webgpu-initialization-and-device-recovery.md): Render Host Affinity, WebGPU Initialization, and Device Recovery
- [ADR 0079](0079-root-product-capabilities-and-placeholder-domain-retirement.md): Root Product Capabilities and Placeholder Domain Retirement
- [ADR 0080](0080-domain-owned-task-update-integration-sets.md): Domain-Owned TaskUpdate Integration Sets
- [ADR 0081](0081-schema-source-stable-identity-catalog-and-runtime-binding.md): Schema Source, Stable Identity, Catalog, and Runtime Binding
- [ADR 0082](0082-process-host-authority-and-runtime-construction-topology.md): Process Host Authority and Runtime Construction Topology
- [ADR 0083](0083-durable-project-asset-and-document-entity-identity.md): Durable Project Asset and Document Entity Identity
- [ADR 0084](0084-executable-runtime-ownership-and-isolation.md): Executable Runtime Ownership and Isolation
- [ADR 0085](0085-hierarchy-transform-and-visibility-semantics.md): Hierarchy, Transform, and Visibility Semantics
- [ADR 0086](0086-rust-project-build-and-executable-generation.md): Rust Project Build and Executable Generation
- [ADR 0087](0087-asset-dependency-import-product-and-artifact-publication-graph.md): Asset Dependency, Import Product, and Artifact Publication Graph
- [ADR 0088](0088-target-build-cook-package-and-runtime-content-catalog.md): Target Build, Cook, Package, and Runtime Content Catalog
- [ADR 0089](0089-runtime-scene-instance-loading-activation-unload-and-travel.md): Runtime Scene Instance Loading, Activation, Unload, and Travel
- [ADR 0090](0090-unavailable-schema-and-lossless-authoring.md): Unavailable Schema and Lossless Authoring
- [ADR 0091](0091-editor-persistence-recovery-and-concurrent-writer-policy.md): Editor Persistence, Recovery, and Concurrent Writer Policy
- [ADR 0092](0092-sdr-color-space-alpha-and-output-encoding.md): SDR Color Space, Alpha, and Output Encoding
- [ADR 0093](0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md): Rust Authoring, Hot Iteration, and Optional Scripting Adapters

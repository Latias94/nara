# nara Architecture Open Questions

**Status**: Living Draft
**Updated**: 2026-07-11

This document contains undecided architecture questions only. Accepted decisions belong in ADRs; implementation evidence belongs in `adr/implementation-status.md` and engineering memory. Each question remains open until its trigger creates enough concrete pressure for an ADR.

## OQ-001: Full Render Graph Trigger

- **Status**: open
- **Owner**: `nara_render`
- **Trigger**: A second pass requires explicit transient resource lifetime or cross-target dependency scheduling that `RenderPassPlan` cannot express.
- **Related ADRs**: 0017, 0032, 0040
- **Question**: What resource and scheduling model should replace phase ordering when the first concrete graph-only use case arrives?

## OQ-002: Reusable Material and Shader Specialization

- **Status**: open
- **Owner**: `nara_material`, render domains
- **Trigger**: A project needs shared material files or more than the current inline 2D descriptor can represent.
- **Related ADRs**: 0012, 0033, 0040
- **Question**: Which stable material asset and shader-specialization vocabulary belongs above backend pipelines?

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

## OQ-005: Physics Backend Selection

- **Status**: open
- **Owner**: future physics domain
- **Trigger**: The first playable physics vertical slice has collision, query, determinism, and deployment requirements.
- **Related ADRs**: 0016, 0019, 0042
- **Question**: Which 2D backend should be adopted first, and what stable component/event boundary keeps the backend replaceable?

## OQ-006: Save and Replication Record Shape

- **Status**: open
- **Owner**: future save/network domains
- **Trigger**: U8 stable runtime identity is proven and either save restoration or replication becomes an implementation slice.
- **Related ADRs**: 0027, 0028
- **Question**: Which component/field records, authority metadata, and tombstones belong in durable save or replication data?

## OQ-007: Guest Scripting Runtime

- **Status**: open
- **Owner**: future scripting domain
- **Trigger**: A real game needs reloadable untrusted behavior that cannot be accepted as in-process native Rust.
- **Related ADRs**: 0021, 0042, 0045
- **Question**: Which WASM runtime, capability model, scheduling contract, and component API should form the first guest boundary?

## OQ-008: Incremental Authoring Projection

- **Status**: open
- **Owner**: `nara_scene`, `nara_tooling`
- **Trigger**: Measured rebuild-style live projection exceeds the editor interaction budget for a representative scene.
- **Related ADRs**: 0026, 0038, 0047
- **Question**: Which patch operations need specialized incremental world commands while preserving document-as-truth and atomic undo?

## OQ-009: Field-Level Apply Changes

- **Status**: open
- **Owner**: `nara_reflect`, `nara_tooling`
- **Trigger**: Whole-component Apply Changes creates destructive conflicts in a real edit workflow after U16 is complete.
- **Related ADRs**: 0034, 0045, 0047
- **Question**: How should field projections, conflict detection, and inverse patches narrow Play Mode write-back?

## OQ-010: Editor Runtime-UI Dogfooding Gate

- **Status**: open
- **Owner**: `nara_ui`, editor adapters
- **Trigger**: Runtime UI has text, keyboard navigation, focus, scroll, and the layout capabilities required by one existing egui panel.
- **Related ADRs**: 0015, 0025, 0041
- **Question**: Which real editor panel should migrate first, and what usability/performance evidence constitutes success?

## OQ-011: Project Export Side-Effect Adapter

- **Status**: open
- **Owner**: future CLI/editor export consumer
- **Trigger**: A concrete CLI command or editor workflow consumes U23 export manifest values.
- **Related ADRs**: 0020, 0035, 0051, 0055
- **Question**: Which adapter owns package publication, signing hooks, target toolchains, and user-facing recovery once a real consumer exists?

## OQ-013: Typed Event and Request Channels

- **Status**: open
- **Owner**: `nara_app`, domain crates
- **Trigger**: At least two domains need the same producer/consumer/retention/stage metadata beyond existing typed queues.
- **Related ADRs**: 0023, 0036
- **Question**: Is a reusable typed channel wrapper justified, and which lifecycle metadata can be shared without creating a global bus?

## OQ-014: Audio Backend and Mixing Boundary

- **Status**: open
- **Owner**: future audio domain
- **Trigger**: The first real game or tool schedules an audio vertical slice with stable intent plus one decoder, mixer, or playback backend consumer.
- **Related ADRs**: 0016, 0030, 0042, 0079
- **Question**: Which backend and stable command/component boundary should implement the first audio vertical slice?

## OQ-015: Text Shaping and Localization Stack

- **Status**: open
- **Owner**: future text/localization domains
- **Trigger**: Runtime UI requires multilingual shaped text with font fallback and deterministic asset import.
- **Related ADRs**: 0025, 0031, 0033
- **Question**: Which shaping, bidi, font rasterization, and localization libraries fit nara's asset/render boundaries?

## OQ-016: GPU Cache Eviction Defaults

- **Status**: open
- **Owner**: `nara_render_wgpu`
- **Trigger**: U22 exposes resident-byte pressure and representative projects provide cache reuse/memory measurements.
- **Related ADRs**: 0040, 0054
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
- **Trigger**: U8 stable identity, U9 canonical schema/envelope, and U16 isolated runtime host are implemented, and a concrete persistent replay workflow has representative size/latency measurements.
- **Related ADRs**: 0024, 0042, 0049, 0051, 0057, 0076
- **Question**: What canonical artifact fields, checkpoint coverage registry, service outcome catalog, checksum algorithm, cadence, compression, compatibility fingerprint, and bounded retention defaults satisfy the first measured replay workflow?

## OQ-019: System-Level Stepping and Breakpoint Executor

- **Status**: open
- **Owner**: `nara_app`, `nara_ecs`, `nara_tooling`
- **Trigger**: U16 exact fixed-tick stepping is implemented and a real debugging workflow requires pausing inside a fixed tick rather than observing completed ticks.
- **Related ADRs**: 0002, 0003, 0039, 0057, 0076
- **Question**: Which stable system identity, topology generation, strict execution mode, open-tick transaction, conditional-breakpoint vocabulary, and failure/discard rules can support system stepping without splitting command acknowledgement or allowing parallel work across a claimed breakpoint?

## OQ-020: Native Rust Code Reload Boundary

- **Status**: open
- **Owner**: future native module/runtime host domain
- **Trigger**: Measured full rebuild plus isolated Play-host restart misses a real iteration-latency target and a concrete native module boundary can own ABI and state migration.
- **Related ADRs**: 0021, 0034, 0042, 0076
- **Question**: Can a narrow native module ABI prove code quiescence, thread/task/callback retirement, native-handle ownership, versioned state extraction/migration, two-phase publication, and rollback strongly enough to justify hot replacement over rebuild-and-restart?

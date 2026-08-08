---
title: Editor Foundation Workflow - Plan
type: feat
date: 2026-08-08
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
execution_state: inactive
product_contract_source: docs/architecture/ui-product-boundaries-editor-dogfood-and-porting-strategy.md
origin: docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md
depends_on: docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md
plan_id: editor-foundation-workflow-2026-08
unit_namespace: EFI
---

# Editor Foundation Workflow - Plan

## Goal Capsule

Deliver the first usable single-window Editor workflow without making a toolkit the owner of
documents, selection, undo or Host lifecycle.

## Scope

- deepen the existing `EditorWorkspace`, command, Inspector, selection, undo, persistence and egui
  adapter path rather than introducing a parallel Editor authority;
- separate action preconditions by ownership: document mutations carry exact document ID and base
  revision; continuous gestures additionally carry gesture identity and
  `Begin -> Preview* -> Commit | Cancel`; workspace/navigation actions such as Open, SetActive,
  Close and Exit do not invent a document revision;
- one selection authority, bounded clean-aware undo, per-document persistence and conflict outcome;
- explicit Open/Switch/Save/Reopen/Close/Exit multi-document lifecycle and a fixed logical shell with
  Scene Tree, Inspector, Play/Stop and Diagnostics;
- egui Adapter owned by the Editor Host, with no egui memory or generated IDs in durable data;
- one numeric slider and one text field exercising gesture coalescing, IME/clipboard ingress and
  exact document conflict handling;
- a Viewport lease whose picking and selection actions bind document revision, lease generation,
  presented render generation, camera generation and input-transform revision;
- keyboard navigation, focus recovery, semantic output and one real OS accessibility vertical.

## Non-goals

No docking graph, detached windows, multi-viewport, Nara UI/egui convergence decision, generic
EditorPlugin registry, or game-domain binding language. Those require a complete panel workload and
a focused successor decision.

## Existing baseline and delta

The current Editor workspace, commands, Inspector adapter, save/reopen path and egui tooling are
the baseline. This successor preserves their ownership and tests. Its delta is limited to verified
gaps: exact multi-document targeting, base-revision preconditions, one selection authority,
clean-aware history, per-document persistence, a launchable fixed shell, and presented-generation-
bound Viewport actions. It must delete or replace conflicting state rather than wrap it with a
second command/session model.

The workspace owns ordered selection, primary/top item and provenance. Inspector drafts are keyed by
document, target and source revision. Monotonic document revision is separate from the clean
checkpoint/history cursor. Undo is bounded by record count and inverse-byte budget; one continuous
gesture produces exactly one undo record. Stale, conflicting and oversized mutations have typed
outcomes, and cross-document atomic mutations are rejected in this first workflow.

## Gate

This plan remains inactive until UIF-U5 publishes a Foundation Conformance Baseline and the Editor
domain action/persistence ADRs have been accepted. It is technically independent from the Runtime UI
Product Slice; repository policy still permits only one active plan at a time.

The egui Inspector gate proves the Editor document/action/gesture/undo/persistence/IME/accessibility
contract. It does not prove Nara UI widgets. A later Nara UI Inspector parity successor must consume
the same immutable model and ordered action trace before shared widget breadth, responsive layout or
Editor toolkit convergence is admitted.

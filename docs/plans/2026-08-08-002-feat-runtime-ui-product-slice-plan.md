---
title: Runtime UI Product Slice - Plan
type: feat
date: 2026-08-08
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
execution_state: inactive
product_contract_source: docs/architecture/reference-game-ui-product-slice.md
origin: docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md
depends_on: docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md
plan_id: runtime-ui-product-slice-2026-08
unit_namespace: RUP
---

# Runtime UI Product Slice - Plan

## Goal Capsule

Consume one pinned UI Foundation Conformance Baseline to prove one data-authored Runtime UI product
slice and publish the first Product-Validated Baseline. The Reference Game is an acceptance carrier,
not an engine-design oracle.

## Scope

- data-authored HUD and a read-only bounded terminal/Retry overlay; editable terminal text and IME
  are not implied;
- the canonical `960 x 540` logical design space and standard 16:9 physical targets;
- one representative fractional input-scale case and one closed letterbox/pillarbox Host mapping
  during transient non-16:9 resize; this does not admit a 4:3 target or adaptive-layout matrix;
- six capture states plus paused lifecycle, keyboard/pointer/gamepad and bounded accessibility
  action coverage;
- headless parity, packaged arbitrary-CWD launch and source/package asset closure;
- semantic, layout and exact presented-receipt assertions without Cartesian smoke or opaque hashes.

## Required product traces

The product proof is not six independent static fixtures. One executable state trajectory must:

1. start from the same Scene/Prefab-authored structure used by desktop, headless and package modes;
2. update HUD facts from gameplay without rebuilding the structure in Rust;
3. present the terminal/Retry modal;
4. activate Retry through pointer, keyboard, gamepad and the bounded accessibility ingress;
5. emit exactly one typed game command for each activation attempt;
6. show the new run generation only on the next matching presented receipt; and
7. use the same Runtime UI adapter in headless and desktop products.

A separate authoring roundtrip changes HUD geometry, label/theme role or modal structure in source
data, then proves restart/reload, desktop, headless and packaged arbitrary-CWD consumption without a
Rust change or test-private injection hook.

## Non-goals

No new shared UI Interface, generic binding DSL, full component catalogue, editor shell, docking,
multi-window policy, localization, IME or virtualization unless a focused admission record is
created from a real product failure.

## Gate

The plan becomes active only after UIF-U5 publishes a Foundation Conformance Baseline and this plan
pins its immutable record. A shared defect suspends RUP; a focused UIF correction publishes the next
baseline; RUP explicitly rebases and reruns affected evidence before resuming. Historical baseline
and product evidence are never rewritten.

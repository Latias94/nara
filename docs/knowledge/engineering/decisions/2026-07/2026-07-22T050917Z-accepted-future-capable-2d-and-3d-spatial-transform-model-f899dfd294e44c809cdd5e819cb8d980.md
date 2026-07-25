---
type: "Decision"
title: "Accepted future-capable 2D and 3D spatial transform model"
description: "Decision for Accepted future-capable 2D and 3D spatial transform model."
timestamp: 2026-07-22T05:09:17Z
record_id: "f899dfd294e44c809cdd5e819cb8d980"
producer_id: "codex-root"
run_id: "spatial-model-2026-07-22"
---

# Decision

Accept ADR 0097 as Nara's durable spatial-model target: separate 2D and 3D authored transform
domains, normal TRS base values, optional typed post-affine residuals, and derived opaque global
affines. This is an Accepted target, not a claim that the current 2D-only implementation has the
model.

`Transform2d` and future `Transform3d` are mutually exclusive persistent spatial roles. UI layout
remains independent. `KeepWorldExact` may materialize a post-affine residual; `KeepWorldTrs` is the
explicit strict alternative; low-level structural movement remains `KeepLocal`.

# Context

The product goal is a Godot/Unity-class capability ceiling without turning normal 2D authoring into
matrix manipulation or adding a speculative transform-provider layer. Bevy proves useful local TRS
plus derived affine/freshness mechanics; Godot proves Node2D, Node3D, and UI layout have distinct
semantics; Unity Entities proves a post-transform layer is a practical way to retain simple local
authoring while representing shear, non-uniform scale, and pivot-relative translation.

# Alternatives

- One universal 3D transform for all entities: rejected because it conflicts with 2D-first and UI
  semantics.
- TRS-only local transforms: rejected because precise affine import/reparenting would force a
  future persistent-format break.
- Full raw matrices for all authoring: rejected because it makes normal authoring and animation
  shallow numerical APIs.
- TRS base plus typed post-affine residual: accepted.

# Consequences

ADR 0097 refines ADRs 0005, 0018, and 0022. ADR 0085 remains Proposed but now delegates its
transform/reparent representation to ADR 0097. The implementation ledger records ADR 0097 as
`not-started`; an active transform slice must add codecs, migration, propagation, exact/strict
reparent transactions, render/camera extraction, and concrete physics/import evidence before any
implementation claim.

# Citations

- [ADR 0097](../../../../architecture/adr/0097-future-capable-2d-3d-spatial-transform-model.md)
- [Unity Entities transform concepts](https://docs.unity3d.com/Packages/com.unity.entities@1.3/manual/transforms-concepts.html)
- `repo-ref/bevy/crates/bevy_transform/src/components/global_transform.rs`
- `repo-ref/godot/scene/2d/node_2d.cpp`
- `repo-ref/godot/scene/3d/node_3d.cpp`

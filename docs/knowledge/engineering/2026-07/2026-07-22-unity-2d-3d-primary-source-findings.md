---
type: "Research"
title: "Unity 2D and 3D transform primary-source findings"
description: "Official Unity evidence for Nara's future-capable 2D and 3D spatial transform model."
timestamp: 2026-07-22T05:12:00Z
record_id: "6d1d48fefe294364ae9c2a0285beec2b"
producer_id: "codex-root"
run_id: "spatial-model-2026-07-22"
---

# Unity 2D/3D Transform Primary-Source Findings

> **Status:** Non-normative research draft.
>
> This note records Unity primary-source observations for Nara architecture work. It does not change an ADR, authorize an API, or imply that Unity's object model should be copied.

## Scope

This research is limited to Unity-owned documentation. It covers classic `Transform`, hierarchy and sibling order, `SetParent(worldPositionStays)`, `RectTransform`, and Unity Entities transforms.

## Primary-source observations

### Classic Transform unifies Unity's 2D and 3D object model

Unity 6 states that every `GameObject` has a non-removable `Transform`, which stores position, rotation, scale, and parenting state. Its editor documentation describes manipulating only X/Y in 2D mode and X/Y/Z in 3D mode, but both workflows use the same component model.

- [Unity Manual: Transforms](https://docs.unity3d.com/6000.0/Documentation/Manual/class-Transform.html)

The same manual says local position, rotation, and scale are measured relative to the parent; a root has world-space values. A child follows parent movement, rotation, and scale; hierarchy is a tree with multiple children but one parent. It also treats sibling order as visible editor state.

- [Unity Manual: Transforms](https://docs.unity3d.com/6000.0/Documentation/Manual/class-Transform.html)
- [Unity API: Transform.SetSiblingIndex](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/Transform.SetSiblingIndex.html)

### World preservation is a first-class reparenting intent

`Transform.SetParent(parent, worldPositionStays)` uses `true` by default. When true, Unity adjusts the parent-relative position, scale, and rotation to retain the former world-space values; `false` retains local orientation instead. Unity exposes this as a boolean command-side choice.

- [Unity API: Transform.SetParent](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/Transform.SetParent.html)

The documentation also explicitly warns that a zero scale can produce undefined math and NaNs, and that a rotated child under a non-uniformly scaled parent can appear sheared. Some components, including example collider classes, do not behave correctly under that skew.

- [Unity Manual: Transforms, scale and non-uniform scale](https://docs.unity3d.com/6000.0/Documentation/Manual/class-Transform.html)

### UI layout is not merely a 2D point transform

Unity's `RectTransform` inherits from `Transform`, but its contract is position, size, anchors, and pivot for a rectangle. If its parent is also a `RectTransform`, it can define position and size relative to the parent rectangle. Anchor values are fractions of the parent rectangle, which is a layout rule rather than ordinary world-space transform composition.

- [Unity uGUI Manual: Rect Transform](https://docs.unity3d.com/6000.0/Documentation/Manual/class-RectTransform.html)
- [Unity API: RectTransform](https://docs.unity3d.com/6000.0/ScriptReference/RectTransform.html)

Unity also documents that some `RectTransform` calculations occur at frame end before UI vertices are generated. Consumers that require the resulting geometry earlier must explicitly force a Canvas update. This is an observable freshness boundary, not an implementation detail callers can ignore.

- [Unity uGUI Manual: Rect Transform](https://docs.unity3d.com/6000.0/Documentation/Manual/class-RectTransform.html)

### Entities separates authored local values from derived world data

Unity Entities 1.3 uses `Parent` as the source hierarchy declaration and maintains a `Child` buffer from it. `LocalTransform` holds local position, quaternion rotation, and **uniform** scale; `LocalToWorld` is the derived matrix used by rendering. Non-uniform scale, shear, or a pivot offset are represented through an additional `PostTransformMatrix`.

- [Unity Entities 1.3: Transform concepts](https://docs.unity3d.com/Packages/com.unity.entities@1.3/manual/transforms-concepts.html)

The same documentation says `LocalToWorld` can be stale or contain graphical smoothing offsets while simulation is running. Simulation that requires an exact current world transform must use a separate world-transform computation rather than assume the cached matrix is authoritative at all times.

- [Unity Entities 1.3: Transform concepts](https://docs.unity3d.com/Packages/com.unity.entities@1.3/manual/transforms-concepts.html)

## Implications for Nara

### Borrow

1. **Make persistent order explicit.** Unity treats sibling order as meaningful enough to expose `GetSiblingIndex` and `SetSiblingIndex`; Nara should retain the proposed ordered document hierarchy rather than reconstruct order from entity IDs or ECS iteration.
2. **Persist local authoring intent and derive world transforms.** A local `Transform2d` or `Transform3d` is the durable scene value. Global matrices/affines are runtime projections for rendering, queries, and other consumers.
3. **Publish a freshness contract.** Transform propagation needs an explicit schedule completion point and a precise rule for obtaining an exact world transform before that point. A cached `GlobalTransform*` must not silently promise immediate freshness after arbitrary writes.
4. **Treat hierarchy and transform participation as distinct facts.** Entities demonstrates a useful direction of authority (`Parent` is authored; `Child` is derived). Nara can keep one structural hierarchy while allowing 2D, 3D, UI, physics, and render domains to opt into their own projections.
5. **Preserve affine global results.** Non-uniform parent scale plus child rotation can yield shear. Global transform caches should therefore retain affine/matrix information instead of assuming every computed world transform can be losslessly decomposed back to local TRS.

### Do not copy without a Nara-specific contract

1. **Do not require a universal transform on every ECS entity.** Unity's universal `Transform` follows from the `GameObject` product model. Nara should keep explicit `Transform2d` and `Transform3d` participation so data-only, tooling, command, and UI-layout entities do not gain false spatial semantics.
2. **Do not make UI a specialization of `Transform2d`.** Unity's `RectTransform` itself proves that anchors, size, pivot, layout timing, and clipping require a different authority. Nara should keep `nara_ui` layout data and computed geometry independent from world-space 2D transforms, with an explicit adapter only for world-space UI.
3. **Do not expose a boolean-only reparent API.** `worldPositionStays` is ergonomic but hides representability, inverse, and failure policy. Nara should model preserve-local versus preserve-world as named operation intent, reject non-invertible or unrepresentable conversion before mutation, and persist the resulting local transform rather than the transient intent.
4. **Do not import `PostTransformMatrix` merely for feature parity.** Unity uses it to compensate for a deliberately uniform-scale ECS local transform. Nara should first decide whether local per-axis scale is needed for its production authoring workflow, then add an explicit affine or skew contract only when a real consumer proves it.
5. **Do not let physics inherit every visual transform by default.** Unity's own documentation warns that non-uniform and sheared hierarchy results can be incompatible with physics-related components. Nara should retain a separate physics participation and scale policy.

## Questions this evidence does not answer

- Whether Nara 3D local scale should be vector-valued from its first public schema or use a more restrictive initial policy.
- The exact Nara public representation for `Transform3d` rotation and its canonical persistent encoding.
- Whether a future authoring workflow needs local 2D skew or a fully affine local transform.
- Which transform propagation data structures meet Nara's target workloads.

Those questions require Nara's existing ADR constraints, source-level Bevy/Godot research, and a concrete production tracer; Unity documentation alone is insufficient to decide them.

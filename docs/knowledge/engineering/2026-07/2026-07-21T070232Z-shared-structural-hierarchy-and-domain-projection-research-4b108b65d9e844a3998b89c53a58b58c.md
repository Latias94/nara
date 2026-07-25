---
type: "Engineering Research"
title: "Shared structural hierarchy and domain projection research"
description: "Non-normative source audit of Nara, Bevy, and Godot hierarchy, transform, visibility, lifecycle, cloning, and domain-projection boundaries."
timestamp: 2026-07-21T07:02:32Z
record_id: "4b108b65d9e844a3998b89c53a58b58c"
resource: "docs/architecture/adr/0085-hierarchy-transform-and-visibility-semantics.md"
tags: ["architecture", "hierarchy", "transform", "visibility", "ui", "scene", "bevy", "godot"]
status: "research"
producer_id: "codex-hierarchy-research-record"
run_id: "20260721-shared-hierarchy-audit"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0a87503b43e1d6abc8b23404789dafc1a7cfe22b"
---

# Summary

Nara should have one canonical structural parent relation for the Nara-authored runtime ECS entity
topology, but it should not turn that relation into a universal object tree or a lifetime owner.
The recommended runtime representation is a Nara-defined, non-`linked_spawn`
`StructuralParent`/`StructuralChildren` relationship built on the `bevy_ecs` relationship
substrate. The persistent source remains stable scene/prefab identity plus explicit parent and
durable sibling order.

This conclusion is narrower than the current ADR 0085 proposal in four important ways:

1. Built-in Bevy `ChildOf`/`Children` couples structure to recursive despawn and optional recursive
   linked cloning. Both behaviors bypass Nara's Scene Instance ownership, state-scope cleanup,
   stable identity, sibling-order, prefab-provenance, and duplicate semantics.
2. A structural relation does not prohibit private domain projections. 2D transform propagation,
   future 3D transform propagation, Nara runtime UI layout, rendering, picking, input, and
   accessibility may each maintain derived indexes or trees that can be rebuilt from canonical
   ECS/document facts.
3. The scope is Nara runtime ECS entity authoring topology. egui, dear-imgui-rs, third-party game
   UI, editor toolkit internals, skeleton bones, physics joints, render graphs, and virtual layout
   nodes are not required to use this relation.
4. Bare `bevy_ecs::Entity` values do not contain World identity. A raw relationship insertion can
   only be interpreted against the current World's entity bits and cannot prove the origin World
   or Runtime Generation. Nara-owned hierarchy transactions must resolve World-bound tokens or
   runtime references before claiming cross-World or cross-generation validation.

The user-facing result can still be simple: the Editor hierarchy panel shows one structural tree;
Editor Delete can default to an explicit `DespawnSubtree` command; and 2D/3D drag-reparent can
default to requesting `KeepWorld`, with atomic failure when the authored transform cannot represent
the result. At the lower ECS layer, parent despawn only detaches surviving children and direct
relationship changes have `KeepLocal` semantics.

This record is research evidence only. It does not accept ADR 0085, authorize public component
names, or change the active implementation plan.

# Question

Should 2D, future 3D, and Nara runtime UI share one hierarchy authority, and if so, which facts,
lifecycle behavior, projections, mutation guarantees, and extension boundaries may safely be
shared without forcing unrelated domains into one tree?

# Repository Baseline

The audit used these fixed source baselines:

- Nara commit `0a87503b43e1d6abc8b23404789dafc1a7cfe22b` for tracked implementation
  evidence. The working-tree ADR 0085, ADR 0025, ADR 0089, and OQ-034 text was also reviewed as
  current Proposed/Accepted/open-question evidence; dirty proposal text is not implementation
  authority.
- Bevy commit `f6c6e6eebb94e81c090614f19039319e9acb3c85` under `repo-ref/bevy`.
- Godot commit `c939bf3791ce40ff70e0ee29f06486da1ebb6a84` under `repo-ref/godot`.

The current Nara implementation is transitional:

- `Parent(Entity)` is mutable application state, while `sync_children` clears every `Children`
  vector and reconstructs it from a query each `PostUpdate`
  (`crates/nara_scene/src/hierarchy.rs:28-40`, `:67-95`, `:146`).
- `Transform2d` is persistent local TRS and `GlobalTransform2d` exists, but `TransformPlugin::build`
  only registers the component schema and installs no propagation system
  (`crates/nara_transform/src/lib.rs:12-50`, `:71-96`).
- Sprite and camera extraction read local `Transform2d`, so nested ancestors currently do not affect
  extracted world placement (`crates/nara_sprite_render/src/extract.rs:14-18`, `:62-70`;
  `crates/nara_render/src/lib.rs:793-821`).
- Runtime UI computes a private `ComputedUiLayouts` projection from `Parent`, retains
  `UiNode.visible`, and uses the computed result for hit/focus eligibility
  (`crates/nara_ui/src/layout.rs:57-105`, `:120-214`;
  `crates/nara_ui/src/interaction.rs:141-176`, `:209-236`).
- Scene documents already persist stable `SceneEntityId` plus parent separately from runtime
  `Entity` (`crates/nara_scene/src/document.rs:100-134`).

# Primary-source Evidence

## Bevy: relationship maintenance is reusable, built-in hierarchy policy is not

Bevy explicitly calls `ChildOf` the source of truth and immediately maintains `Children` through
component hooks on insert, replace, and remove (`repo-ref/bevy/crates/bevy_ecs/src/hierarchy.rs:28-50`,
`:105-152`). This is useful substrate evidence: Nara does not need an end-of-frame full rebuild to
keep the reverse collection consistent.

The built-in relation also opts into `linked_spawn`
(`repo-ref/bevy/crates/bevy_ecs/src/hierarchy.rs:147-152`). `RelationshipTarget::LINKED_SPAWN`
means that related entities participate in recursive despawn and, when linked cloning is enabled,
recursive clone (`repo-ref/bevy/crates/bevy_ecs/src/relationship/mod.rs:268-278`, `:366-399`). The
same flag therefore embeds two product policies, not merely a convenient parent index.

For a relationship target without `linked_spawn`, the derive default is false, and target discard
removes the relationship component from its sources rather than deleting those source entities
(`repo-ref/bevy/crates/bevy_ecs/src/relationship/mod.rs:271-278`, `:305-337`). This supports the
required low-level Nara behavior: despawning a structural parent can detach children while leaving
their lifetime to another authority. An explicit subtree operation can still traverse and retire a
validated closure at a higher layer.

Bevy's `Entity` contains only index and generation, and equality compares its bit representation
(`repo-ref/bevy/crates/bevy_ecs/src/entity/mod.rs:424-442`). This is source-level evidence for the
cross-World limitation: no World identity is carried by a bare `Entity`. Nara already documents the
same fact in `WorldEntityToken` and binds the token to a `WorldIdentityDomainId`
(`crates/nara_identity/src/domain.rs:143-150`). Its resolution path checks the actual World binding
before returning an entity (`crates/nara_identity/src/domain.rs:393-409`, `:499-525`).

Consequently, raw `StructuralParent(Entity)` insertion cannot reliably diagnose that the caller
copied the bits from a different World. If those bits name a live entity in the current World, the
raw substrate has no provenance with which to distinguish the alias. Only a Nara-owned transaction
that accepts or resolves `WorldEntityToken`, `WorldEntityLocator`, `RuntimeEntityReference`, or an
equivalent World-bound value may promise cross-World/generation rejection.

## Bevy: one structural relation can feed non-identical projections

Bevy visibility separates local `Visibility`, hierarchy-derived `InheritedVisibility`, aggregate
`ViewVisibility`, and per-camera `VisibleEntities`
(`repo-ref/bevy/crates/bevy_camera/src/visibility/mod.rs:80-95`, `:161-226`, `:341-346`). This
supports a layered Nara visibility model, but not Bevy's exact override semantics: Bevy allows
explicit `Visible` to override a hidden ancestor (`:91-95`, `:650-716`). Nara must choose and test
its own semantic rule rather than cite this as ecosystem consensus.

Bevy UI consumes ECS hierarchy facts but builds and caches a separate Taffy layout projection.
`UiSurface` maps entities to Taffy nodes, sets layout children, and computes layout in the private
tree (`repo-ref/bevy/crates/bevy_ui/src/layout/ui_surface.rs:44-72`, `:113-180`, `:219-240`). Its
optional `GhostNode` path even flattens selected structural nodes when deriving UI parents and
children (`repo-ref/bevy/crates/bevy_ui/src/experimental/ghost_hierarchy.rs:1-25`, `:55-105`).

This is a direct counterexample to ADR 0085 language that could be read as prohibiting every
domain-specific topology copy or requiring one shared traversal/dirty-root implementation. One
canonical authoring relation and several disposable domain projections are compatible.

## Godot: one Node tree with domain-specific child caches and boundaries

Godot's Node parent relation is the common structural tree, but spatial domains project only
compatible nodes:

- `Node3D` casts its Node parent to `Node3D`, records a private `node3d_children` list, and computes
  global transform only through that compatible parent unless `top_level` breaks inheritance
  (`repo-ref/godot/scene/3d/node_3d.cpp:151-168`, `:649-659`).
- `CanvasItem` similarly casts the Node parent, maintains `canvas_item_children`, exposes an
  explicit top-level boundary, and derives visibility from its compatible parent
  (`repo-ref/godot/scene/main/canvas_item.cpp:72-119`, `:380-418`, `:588-640`).

The transferable lesson is not to copy Godot's object tree. It is that a stable structural
authority can coexist with private domain child indexes, domain participation boundaries, and
different transform/layout math.

Godot also shows why Nara must not infer every ownership axis from structure. `Node::owner` is
stored separately from parent, maintained in a separate owned list, and constrained to an ancestor
for scene ownership (`repo-ref/godot/scene/main/node.cpp:2240-2248`, `:2300-2326`). Nara has a
stronger reason to keep ownership separate: ADR 0089 makes scene membership explicit and forbids
inferring it from hierarchy (`docs/architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md:110-124`).

Godot's default object deletion recursively deletes Node children
(`repo-ref/godot/scene/main/node.cpp:295-312`), while `remove_child` detaches the child and clears
its parent without deleting the object (`:1747-1800`). That proves both user operations are useful;
it does not prove that recursive deletion should be an unavoidably implicit ECS hook. Nara can make
the destructive choice explicit at its Editor/scene transaction boundary.

## Nara: lifecycle and topology have already diverged conceptually

ADR 0089 proposes multiple active Runtime Scene Instances in one World and explicitly says scene
membership is independent from parent hierarchy
(`docs/architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md:36-39`,
`:68-76`, `:110-141`). OQ-034 similarly records Runtime Scene Instance, hierarchy, prefab
provenance, region residency, and Gameplay State as orthogonal relations, with at most one Scene
Instance lifecycle owner per runtime entity (`docs/architecture/open-questions.md:584-604`).

ADR 0025 accepts Nara-owned runtime ECS UI while allowing runtime projections, incremental indexes,
measurement caches, and virtualized materialization
(`docs/architecture/adr/0025-runtime-ui-system.md:17-35`, `:55-62`, `:111-115`). Sharing structural
authoring facts therefore does not require the ECS tree to remain the final UI execution tree.

# Counterexamples/Scope Limits

The recommendation deliberately excludes these cases:

| Case | Why it is not the canonical structural relation |
|---|---|
| egui, dear-imgui-rs, or a third-party game UI | These toolkits own their own widget/window trees and may coexist as independent UI layers. Requiring a `StructuralParent` component would reduce ecosystem freedom without improving Nara scene truth. |
| Editor toolkit widget tree | The Editor hierarchy panel displays game/runtime structure; its own controls are presentation and need not mirror the inspected structure. |
| Virtualized or generated UI layout nodes | They may not have one persistent ECS entity each. They belong to a rebuildable UI projection over stable authoring identity. |
| Skeleton bones or animation graphs | A bone/program identity may be sub-entity data with different serialization, evaluation, and update frequency. It is not necessarily an ECS entity parent. |
| Physics joints, constraints, navigation links, and render graphs | These are graphs, often many-to-many or cyclic, and do not express structural containment. |
| Scene Instance, prefab provenance, Gameplay State scope, region residency | These are lifecycle, provenance, activation, or residency relations. They must not become aliases for parent/child, and this research does not authorize competing cleanup owners. |
| Per-view culling and accessibility/input indexes | They are consumer-specific, frame- or policy-derived projections. They may be trees or indexes but are not authoring truth. |
| Cross-World or cross-generation references | A structural relationship is local to one World. Cross-boundary coordination uses explicit stable/runtime identity and reconstruction, not an ECS parent edge. |

The shared scope is therefore: entities in one Nara runtime ECS World that opt into the Nara
structural authoring topology. An entity may have zero or one structural parent. A domain may
participate continuously along that structure, stop at an incompatible parent, explicitly flatten
authoring-only nodes such as UI ghost nodes, or use an explicit bridge such as a future world-space
UI adapter. Any flatten/skip behavior is domain-owned derived projection policy; a domain must not
silently skip an incompatible ancestor and invent spatial inheritance.

# Recommended Direction

## Structural facts and ownership

- Keep `SceneEntityRecord.parent` and a versioned durable sibling-order representation as persistent
  scene/prefab authority. Runtime `Entity` order is never durable order.
- Runtime-only roots and children also require deterministic insertion/move semantics. Their order
  may remain generation-local, but it must come from an explicit structural transaction rather
  than `Entity::to_bits()`, query iteration, or hook timing; promotion into authoring data assigns
  durable order explicitly.
- Define Nara-owned names such as `StructuralParent` and `StructuralChildren` using Bevy's
  relationship substrate, without `linked_spawn`. Exact public names remain an ADR/API decision.
- Treat the parent component as runtime source of truth and the children collection as
  relationship-maintained derived state. Application code must not mutate the reverse collection
  independently.
- Keep lifecycle and cleanup effects in their explicit, separately admitted contracts. Runtime
  Scene Instance membership, future state scope, and other lifecycle axes must define their
  composition without competing deletion authority. Structural topology grants none.
- Low-level parent despawn detaches children. A high-level, fallible `DespawnSubtree` transaction
  validates structural closure plus actual ownership before retiring entities.
- Raw despawn of a scene-managed entity still bypasses stable-identity and Scene Instance
  retirement and remains an invariant violation even though the non-linked relationship safely
  detaches structural children.
- Editor Delete may default to `DespawnSubtree` because that is the expected hierarchy-panel
  workflow, but the command remains visible, undoable where applicable, and distinct from raw ECS
  despawn.
- Scene Instance unload always retires exact membership. It must not call `DespawnSubtree`, expand
  retirement through structural descendants, or cross an ownership scope implicitly. Version 1
  may reject cross-scope parent edges until an explicit detach, reparent, or adoption transaction
  proves a broader contract.
- Duplication/cloning is a Nara-owned transaction that allocates/remaps stable `SceneEntityId`,
  preserves or deliberately changes sibling order, remaps references, retains prefab provenance,
  and assigns Scene Instance ownership. Do not delegate these semantics to linked cloning.

## Validation boundary

- Nara-owned scene/editor/runtime hierarchy commands accept stable document IDs or World-bound
  entity evidence. They preflight missing target, self-parent, ownership-scope rules, cycles,
  sibling order, and transform conversion before one atomic commit.
- Public diagnostics may claim cross-World or cross-generation rejection only when the input carries
  and validates `WorldIdentityDomainId`/runtime-generation evidence.
- Advanced direct ECS insertion remains a same-World, raw-entity-bits operation. Hooks may reject a
  missing current-World target or self-reference, and a later barrier may detect cycles, but Nara
  must not invent origin provenance that the input did not contain.

## Domain projections

- 2D owns `Transform2d -> GlobalTransform2d`, its math, dirty tracking, diagnostics, and consumer
  schedule barrier.
- Future 3D owns `Transform3d -> GlobalTransform3d` independently. Sharing parent facts does not
  force a universal 3D authoring transform onto 2D or UI.
- Nara runtime UI owns computed layout, clipping, source order, hit testing, focus, accessibility,
  virtualization, and any private layout tree. It may derive structural participation from the
  canonical relation without exposing its internal tree as scene truth.
- Dynamic physics bodies remain domain roots with identity effective scale in the first physics
  slice, consistent with the existing physics research. A later parented-body contract must name
  the fixed-tick world-pose authority, the safe point that lowers solver world pose back into local
  authored transform, and the singular/non-representable failure policy before it is admitted.
- Render, picking, gizmos, physics, and other consumers read the admitted global/effective
  projection; they do not each recompute an incompatible parent chain.
- Do not freeze one shared traversal implementation, dirty-root storage, or universal projection
  registry before measured workloads prove that sharing it removes more complexity than it adds.

## Reparent and visibility UX

- A low-level relation change means `KeepLocal`; it changes topology without silently rewriting
  authored transform/layout values.
- Editor drag-reparent for 2D and 3D may default to requesting `KeepWorld`. The transaction captures
  the old global transform, checks the new parent and domain boundary, derives a candidate local
  transform, and atomically rejects singular or non-representable results. UI reparent remains a
  layout operation unless a concrete UI workflow defines another policy.
- Use one hierarchy-level persistent local visibility intent for participating Nara entities and a
  runtime-only effective visibility projection. A Godot-like rule in which a hidden ancestor
  suppresses descendants is the recommended Nara UX; Bevy proves that another rule exists, so this
  remains an explicit decision and test obligation.
- A spatial top-level marker does not automatically break structural visibility. Transform,
  layout, inherited visibility, clipping, and culling boundaries remain separately named domain
  policies.
- Per-view culling remains frame-local and separate. Hidden UI must also cancel or reject stale
  hover/press/focus/capture state at a named barrier; hiding is not gameplay disable, scene unload,
  or layout deletion.

# ADR 0085 Revision Checklist

Before ADR 0085 can be considered for acceptance, revise it to:

1. Replace built-in `ChildOf`/`Children` as the target with a Nara-defined non-`linked_spawn`
   relationship over the Bevy substrate.
2. Remove recursive parent despawn as a low-level invariant. Specify detach-on-parent-despawn and a
   separate validated `DespawnSubtree` transaction; state that Editor Delete may invoke the latter
   by default.
3. Specify Nara-owned duplicate/clone behavior for stable identity, sibling order, entity-reference
   remapping, prefab provenance, and Scene Instance ownership. Do not inherit linked-cloning policy.
4. Define deterministic runtime root/child insert and move behavior for runtime-only entities; do
   not use `Entity` bits or query order as a tie-break.
5. Replace an unconditional claim that raw hierarchy insertion rejects cross-World edges with the
   honest split between World-bound Nara transactions and raw current-World `Entity`-bit semantics.
6. State explicitly that Scene Instance, Gameplay State scope, prefab provenance, and region
   residency remain orthogonal and own their respective lifecycle/provenance behavior.
7. Narrow "one shared topology" to Nara runtime ECS entity authoring topology. Exclude external UI
   toolkits, editor toolkit internals, bones, constraints, and other domain graphs.
8. Permit private derived domain trees/indexes and remove any requirement that all domains share one
   traversal implementation, dirty-root representation, or physical topology cache.
9. Retain separate 2D/3D/UI local and derived projections, define participation boundaries, and
   permit explicit domain-owned flatten/skip and top-level rules while requiring consumers to read
   the relevant completed projection.
10. Define low-level `KeepLocal`, Editor 2D/3D `KeepWorld` request behavior, atomic failure, and UI's
   separate layout semantics.
11. Choose visibility override semantics explicitly, separate effective from per-view visibility,
    and define hidden UI focus/capture cleanup.
12. Present one structural tree in the Editor while making destructive subtree operations and
    ownership consequences inspectable.
13. Update risks and admission tests so they test these boundaries rather than treating absence of
    all domain-specific topology copies as success.
14. Keep parented dynamic physics bodies outside the first contract until solver world-pose
    writeback, local conversion, scale/shear limits, ordering, and faults are proven.

# Admission Evidence

Acceptance should require all of the following evidence, not merely a flat sprite demo:

1. Relationship tests for insert, replace, detach, stable reverse collection, arbitrary-depth cycle
   rejection, parent despawn with surviving detached children, and explicit deterministic subtree
   despawn.
2. A raw cross-World numeric-alias test demonstrating why bare `Entity` cannot prove provenance,
   plus World-bound transaction tests that reject wrong domain/generation before mutation.
3. Duplicate/clone fixtures that remap stable scene identity and references, preserve deterministic
   sibling order and prefab provenance, assign the intended Scene Instance owner, and keep
   runtime-only insert/move order stable without an `Entity`-bit tie-break.
4. Scene unload and Gameplay State-scope fixtures proving that topology does not become accidental
   lifecycle ownership, that exact membership rather than structural closure drives unload, and
   that one scope cannot delete another scope's surviving entities.
5. Three-level 2D transform propagation on startup and same-frame mutation, followed by a focused
   3D spike using the same structural facts but independent math and diagnostics.
6. Nara runtime UI layout, render, hit-test, focus, clipping, and virtualized/generated-node fixtures
   showing a private projection derived from canonical authoring structure, including one explicit
   ghost/flatten rule and one explicit layout-root boundary.
7. Reparent tests for `KeepLocal`, representable `KeepWorld`, singular-parent failure,
   non-representable/shear failure, domain boundaries, and atomic no-change on error.
8. Visibility tests for ancestor suppression, root/default behavior, per-view divergence, and
   hidden UI hover/press/focus/capture retirement without deleting layout or simulation state.
9. Editor workflow tests showing one hierarchy panel, explicit subtree Delete, undo/redo through
   normal scene patches, and drag-reparent behavior with structured failure feedback.
10. Coexistence fixtures proving egui/dear-imgui-rs or a custom game UI can operate without adopting
    `StructuralParent` and without changing Nara core.
11. Measured deep-tree, wide-tree, mutation-heavy UI, and extraction workloads before admitting a
    shared dirty-index optimization or making incremental-cache behavior public.
12. A physics fixture that keeps version-1 dynamic bodies at a supported domain root, plus a later
    admission tracer for parented world-pose writeback and singular/non-representable local
    conversion.

# Citations

## Nara

- `docs/architecture/adr/0085-hierarchy-transform-and-visibility-semantics.md:3-10`, `:35-99`,
  `:101-209`, `:284-338`.
- `docs/architecture/adr/0025-runtime-ui-system.md:17-35`, `:55-62`, `:96-124`.
- `docs/architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md:16-39`,
  `:68-76`, `:100-141`, `:209-253`.
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md:19-28`, `:70-150`,
  `:401-425`.
- `docs/architecture/open-questions.md:584-604` (OQ-034).
- `crates/nara_scene/src/document.rs:27-40`, `:100-134`.
- `crates/nara_scene/src/hierarchy.rs:28-95`, `:137-147`.
- `crates/nara_transform/src/lib.rs:12-50`, `:71-96`.
- `crates/nara_identity/src/domain.rs:143-150`, `:393-409`, `:435-469`, `:499-525`.
- `crates/nara_ui/src/layout.rs:57-105`, `:120-214`.
- `crates/nara_ui/src/interaction.rs:8-29`, `:141-176`, `:209-236`.
- `crates/nara_sprite_render/src/extract.rs:14-18`, `:62-70`, `:98-110`.
- `crates/nara_render/src/lib.rs:793-821`.
- `docs/knowledge/engineering/2026-07/2026-07-20T045206Z-physics-2d-replacement-model-and-first-backend-recommendation-d5a0577581384e439bcddb6636fd3efd.md:167-184`,
  `:256-284`.

## Bevy at `f6c6e6eebb94e81c090614f19039319e9acb3c85`

- `repo-ref/bevy/crates/bevy_ecs/src/hierarchy.rs:28-50`, `:105-152`.
- `repo-ref/bevy/crates/bevy_ecs/src/relationship/mod.rs:268-337`, `:366-410`.
- `repo-ref/bevy/crates/bevy_ecs/src/entity/mod.rs:19-78`, `:226-246`, `:424-442`.
- `repo-ref/bevy/crates/bevy_ecs/src/entity/clone_entities.rs:476-499`, `:884-888`.
- `repo-ref/bevy/crates/bevy_camera/src/visibility/mod.rs:80-95`, `:161-226`, `:341-346`,
  `:650-716`.
- `repo-ref/bevy/crates/bevy_ui/src/layout/ui_surface.rs:44-72`, `:113-180`, `:219-240`.
- `repo-ref/bevy/crates/bevy_ui/src/experimental/ghost_hierarchy.rs:1-25`, `:55-105`,
  `:137-164`.

## Godot at `c939bf3791ce40ff70e0ee29f06486da1ebb6a84`

- `repo-ref/godot/scene/main/node.cpp:289-312`, `:1747-1800`, `:2240-2248`, `:2300-2326`.
- `repo-ref/godot/scene/3d/node_3d.cpp:151-168`, `:649-659`, `:682-688`, `:1022-1071`,
  `:1131-1139`.
- `repo-ref/godot/scene/main/canvas_item.cpp:72-119`, `:258-263`, `:380-418`, `:451-482`,
  `:588-640`.
- `repo-ref/godot/scene/2d/physics/rigid_body_2d.cpp:153-156`.
- `repo-ref/godot/scene/2d/node_2d.cpp:388-395`.

# ADR 0029: Animation Strategy

**Status**: Accepted
**Date**: 2026-07-08
**Last Revised**: 2026-11-18
**Refined By**: ADR 0045: Component Schema Capability Metadata; ADR 0095: Plugin-Owned Specialized
Domains and Project Configuration

## ADR 0095 Refinement

The durable-identity and transient-pose distinctions below remain constraints if animation data is
persisted. They do not authorize a `nara_animation` crate, a universal component-field animation
Interface, or one controller/graph model before a real animation workflow selects them. The first
integration may own its complete clip, target, evaluation, and playback API as a plugin.

## Context

Animation affects 2D-first experience, component schemas, reflection, assets, and editor timelines. nara should support sprite animation early while leaving room for field animation, skeletal animation, and future 3D animation.

## Decision

nara freezes only the persistence and ownership constraints needed by future animation. The first
concrete animation plugin selects its clip/controller/target/evaluation model from a real workflow.

Rules:

- Phase 1 may support simple sprite/frame animation as its first 2D workflow.
- A concrete plugin may represent clips as assets; this ADR does not require every animation domain
  to use one universal clip type.
- Any persistent animation target uses stable identity appropriate to its owning domain. Generic
  field animation is a candidate that would use a stable entity selector plus `ComponentTypeId` and
  `ComponentFieldId`; it is not the default binding model before a tracer. Skeleton, bone, morph,
  material, or other domains may define their own stable target IDs.
- Author-visible controller, playback, parameter, and gameplay-relevant result state is stable
  animation-domain data and may be represented by ECS components or resources. Evaluated pose
  buffers, blend scratch, skeleton caches, skinning palettes, and backend acceleration state are
  transient animation/render-domain implementation data; this ADR does not require one ECS
  component per pose, bone, or evaluation intermediate.
- Timeline curves, skeletal 2D/3D, and animation graphs remain candidate plugin-owned mechanisms.
- Animation should run in deterministic-friendly schedules when it affects gameplay state; presentation-only animation may run in frame update.

### First Concrete Game Workflow

The Two-Level Game's first animation workflow introduced a game-owned frame API over its existing
logical time and held movement intent. Four evaluated atlas regions are derived presentation data.
They do not add a second clock, persistent playback cursor or universal clip/controller model.
The game plugin projects them after its fixed simulation and after asynchronous Scene/resume reconstruction,
before original render extraction, including paused maintenance. Source/cooked Image identity,
Sprite sampling and retained captures remain with their existing owners. The
[complete runtime/delivery unit](../../plans/2026-11-03-joint-motion-execution-plan.md#explorer-atlas-animation-through-runtime-and-delivery)
owns implementation and evidence. This finite workflow does not qualify independent playback,
frame events, generic field targets, blending or skeletal animation.

### Authored Sprite Clip Resources

The directly consumed `nara_sprite_animation` module owns immutable named clips,
normalized Sprite regions, positive per-frame durations and explicit loop/one-shot sampling.
The selected game workflow supplies its existing logical clock and owns clip selection; the
module does not install another clock, player/controller state, ECS writer or universal binding
interface. The original Sprite retains Image and sampler ownership. Normalized clip geometry
does not imply a cross-domain prepared Image dependency or certify arbitrary atlas content.
Source import, tracked build and explicitly selected Runtime decode share the same admitted
clip invariants. Sampling reads immutable data without repeating complete resource validation.
The [authored resource unit](../../plans/2026-11-03-joint-motion-execution-plan.md#authored-sprite-clips-through-runtime-and-delivery)
owns implementation and qualification. Game-clock-derived projection remains reconstructible
from existing captured semantics and compatible immutable content; independent playback cursors,
events, blends and other animation domains require their own selected state policy.

## Alternatives Considered

### Option A: Hardcode sprite animation only

**Pros**: Fast 2D results.

**Cons**: Poor path to transform/material/UI/3D animation.

**Decision**: Rejected as the long-term model.

### Option B: Full animation graph from day one

**Pros**: Mature high-end feature set.

**Cons**: Too large before basic runtime/rendering.

**Decision**: Rejected for Phase 1.

### Option C: Universal Asset Clips Targeting Registered Component Fields

**Pros**: Works for sprite animation now and grows toward timelines/3D later.

**Cons**: Requires stable schema IDs, a binding phase, and explicit interpolation rules.

**Decision**: Deferred; one field-targeted workflow does not prove a universal binding model.

### Option D: Concrete Plugin First with Stable Persistence Constraints (Chosen)

**Pros**: Lets the first workflow select the smallest complete model while preserving durable IDs
and transient evaluation ownership.

**Cons**: Later animation domains may expose different APIs or require explicit conversion.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| 2D usefulness | One concrete sprite animation workflow completes through a plugin-owned public API | [Explorer atlas workflow](../../plans/2026-11-03-joint-motion-execution-plan.md#explorer-atlas-animation-through-runtime-and-delivery) |
| Schema integration | Persistent targets use stable component/field IDs and survive field rename | Design review |
| Future growth | 3D/skeletal animation can add domain-owned targets without inheriting a false field-binding contract | Architecture review |
| Schedule clarity | Gameplay-affecting animation can run in fixed update | [Fixed/paused/restored game controls](../../../samples/two-level-game/tests/animation_gameplay.rs) |
| Storage honesty | High-level controller state remains inspectable without exposing pose, blend, or GPU caches as persistent ECS data | Future animation tracer and type review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Target binding drifts across schema changes | High | Medium | Persist stable IDs, resolve paths only during binding, and validate migrations/tombstones |
| Animation scope explodes | High | Medium | Start with sprite/frame clips |
| Interpolation rules are unclear | Medium | Medium | Define per-field animation value traits later |
| Generic component-field animation becomes the universal binding model | High | Medium | Let skeleton, material, and other domains own stable subtarget IDs proven by their first tracer |
| Transient pose data is forced into authoring ECS storage | High | Medium | Keep controller/result state stable while pose and evaluation caches remain domain-private |

## Consequences

- Display names and authoring paths can change without rewriting animation target identity.
- If generic field animation is later admitted, loading binds stable IDs against a frozen schema
  catalog before writes become active and reports missing/tombstoned targets. Domain-specific target
  catalogs require equivalent stable-identity and diagnostic properties without being forced
  through `ComponentFieldId`.
- Write arbitration, blend order, root motion, event timing, and gameplay-versus-presentation
  scheduling remain separate decisions for the first non-trivial animation slice.

# ADR 0046: Plugin Metadata and Default Plugin Groups

**Status**: Accepted
**Date**: 2026-07-09
**Last Revised**: 2026-07-14
**Refines**: ADR 0010, ADR 0035, ADR 0040, ADR 0044
**Refined By**: ADR 0056: Headless Runtime and Dedicated Server Readiness; ADR 0079: Root Product
Capabilities and Placeholder Domain Retirement

## Context

nara owns its `App` and `Plugin` lifecycle, with fallible plugin installation and duplicate
protection. That is the right product boundary. The next risk is composition: as rendering, input,
asset import, UI, physics, tooling, and editor adapters grow, ad hoc plugin ordering and convenience
installation will obscure what a project actually enabled.

There is already pressure:

- Before this ADR was implemented, `WgpuRenderPlugin` installed sprite and UI render submitters for
  convenience.
- `MinimalPlugins` can drift from a truly minimal runtime base.
- Generated docs, diagnostics, editor views, and AI agents need to inspect installed capabilities.
- File-backed project settings need to lower into predictable plugin groups.

## Decision

nara plugins expose lightweight metadata and nara provides explicit default plugin groups.

```mermaid
flowchart TD
    Group[Data-only PluginGroup] --> Entries[Stable slots and registrations]
    Declaration[Static Plugin declaration] --> Definition[Repeatable typed definition]
    Definition --> Entries
    Entries --> Plan[Pure resolved plugin plan]
    Plan --> Prepare[Private prepared instances]
    Prepare --> Build[Closed fallible lifecycle commit]
    Build --> App[App resources/schedules/systems]
    App --> Diagnostics[plugin graph diagnostics]
```

The plugin metadata contract is:

- Every plugin type owns one static `Plugin::declaration()` containing a stable `PluginId` suitable
  for diagnostics, generated docs, and project inspection. Constructed instances and registration
  records do not provide competing metadata authorities. Rust `TypeId` may support private typed
  erasure, but it is not a public or persistent identity.
- Plugins declare provided capabilities, required capabilities/plugins, conflicts, and a short
  category such as core, asset, render, platform, input, tooling, service, or backend. These facts
  do not vary with instance configuration.
- Every `PluginId` is unique in a resolved plan. The unused open-ended non-unique flag is removed;
  a future proven multi-instance lifecycle requires explicit stable entry identity.
- A repeatable `PluginDefinition` combines a stable versioned `PluginDefinitionId`, the canonical
  declaration, explicit immutable configuration, its canonical representation/versioned digest,
  and one admitted private typed factory binding. A raw direct plugin instance remains one-shot and
  cannot enter a replayable product blueprint.
- Metadata is declarative and diagnostic first. Composition closes declared capabilities,
  requirements, conflicts, and group membership into one inspectable plan; this is validation, not
  a general-purpose dependency solver.
- Plugin groups are data-only ordered product bundles. Their builder records stable slots,
  repeatable definitions, nested groups, disable/configure/relative-order intent, and no `App`
  mutation. One resolved registration collection derives membership, order, slot state, and full
  group provenance; there is no parallel static member array.
- Duplicate registrations may merge provenance during pure resolution only when their stable
  slot occurrence identity (or the same un-slotted unique `PluginId`) and complete admitted
  definition key match. Configuration
  equality compares private canonical representations after a digest match; hash equality alone is
  insufficient. Divergent IDs/configurations/bindings are errors; public `add_plugin_if_missing`
  and first/last-wins selection are not composition policy.
- Duplicate group IDs converge only when an intrinsic group-definition fingerprint over canonical
  expanded occurrences, definition keys, order intent, and nested group fingerprints matches. The
  fingerprint excludes outer edits and accumulated provenance, which merge only after equality.
- Missing required capabilities and conflicts produce structured diagnostics before any `App`
  mutation. Pure plan failures use `PluginPlanError`; product capability failures use
  `CompositionError`; repeatable factory preparation uses `PluginPrepareError`; App-level plugin
  hook failures use `PluginError`. Panic-based prerequisite checks remain invalid.
- `App::add_plugins` preserves a Bevy-like single/group/tuple call through a sealed input trait, but
  all inputs lower through collection, resolution, optional private preparation, and closed commit.
  Plugin build/finish hooks cannot install hidden dependencies.
- Optional backend adapters stay feature-gated. Enabling a feature exposes the plugin, but it does
  not silently install it unless a chosen group includes it.
- Render submitter ownership follows ADR 0040: device/surface backend plugins are separate from
  sprite, UI, text, gizmo, and future 3D submitter plugins. Convenience groups may combine them.

## Default Plugin Groups

Initial group vocabulary:

| Group | Purpose |
|---|---|
| `CorePlugins` | App scheduling, diagnostics, tasks, core runtime resources |
| `AssetPlugins` | Asset server, import registry, reload scheduling, optional watch adapter by feature |
| `Runtime2dPlugins` | Transform, render-domain basics, image/material, sprite, and tilemap; no runtime UI |
| `RuntimeUiPlugins` | Runtime UI authoring, layout, interaction, and UI submission without sprite/tilemap ownership |
| `HeadlessRuntimePlugins` | Core runtime plus asset/scene/gameplay-domain systems that can run without window, render, audio-device, editor, or UI toolkit adapters |
| `ServerPlugins` | Dedicated-server-ready headless composition with deterministic-friendly gameplay stages, diagnostics/metrics, and no networking transport unless an optional networking crate is explicitly added |
| `DesktopWinitPlugins` | `nara_window` plus desktop `nara_winit` adapter |
| `WgpuBackendPlugins` | Base wgpu target/backend operation; sprite and UI submitters are available only under their compiled domain/backend features and join a resolved plan only when requested |
| `ToolingPlugins` | UI-agnostic tooling models and optional adapter groups |

`MinimalPlugins` should remain small and headless. It should not grow into "everything a sample game
might want." Examples can use richer groups when they need rendering, windowing, asset import, or UI.
The fixed `DesktopWgpuPlugins` bundle is removed; project composition combines runtime presets and
additive product/adapter capabilities after validating the compiled/requested/required product
subsets and the separate plugin service requirement/conflict closure.

## Alternatives Considered

### Option A: Keep ad hoc plugin installation

**Pros**: Simple and flexible while the engine is small.

**Cons**: Dependencies and installed capabilities become invisible. Convenience plugins can
accidentally hardwire unrelated domains together.

**Decision**: Rejected.

### Option B: Copy Bevy's full plugin group implementation model

**Pros**: Mature Rust precedent and familiar ergonomics.

**Cons**: `TypeId` keys, one-shot boxed instances, panic-oriented edits, and public
`PluginGroupBuilder::finish(App)` do not support stable package inspection, replayable generations,
or Nara's candidate containment.

**Decision**: Rejected.

### Option C: Lightweight metadata plus explicit nara groups

**Pros**: Gives diagnostics and product bundles now without committing to a full dependency solver
or Bevy-compatible API surface.

**Cons**: The implemented static member arrays plus imperative group build methods create two
authorities. Instance-owned metadata also requires construction before product planning.

**Decision**: Superseded by implementation evidence.

### Option D: Keep Bevy ergonomics over a stable data-only plan

**Pros**: Preserves `add_plugins(plugin/group/tuple)` and chained group editing while making one
registration collection authoritative, replayable, inspectable, deterministic, and closed before
App mutation.

**Cons**: Requires a breaking migration to static declarations, repeatable definitions, pure group
builders, typed plan errors, and explicit dependencies.

**Decision**: Chosen. Nara adopts Bevy's caller ergonomics, not its process-local identity and
immediate-install semantics.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Inspectable plugin graph | Resolved/installed entries expose stable IDs, declarations, slots, and provenance from one source | Unit/API tests |
| Minimal stays minimal | `MinimalPlugins` remains headless and backend-free | Dependency review |
| Backend decoupling | wgpu device/surface setup does not permanently auto-own sprite/UI/text submitters | Plugin tests |
| Diagnostics | Missing prerequisites/conflicts produce structured errors with plugin IDs before mutation | Unit tests |
| Docs/tooling | Plugin groups can be listed for docs/editor/AI tooling | Snapshot/API test |
| Product separation | Runtime 2D installs no runtime UI; runtime UI pulls no sprite/tilemap group | Group and dependency tests |
| Deterministic plan | Identical inputs produce identical order and fingerprint | Repeated/property tests |
| Closed dependencies | No build/finish hook installs a plugin/group | Static and ignored-error contract tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Definition key is rebound or transferred inconsistently | High | Medium | Admit one typed factory binding per stable definition ID/version and preserve the complete key into the private prepared carrier. |
| Trusted factory ignores canonical config | High | Low | Do not claim runtime self-verification; require domain conformance tests and keep native/authority work outside the factory. |
| Group names overfit early architecture | Low | Medium | Keep groups product-level and allow pre-1.0 breaking cleanup. |
| Users expect automatic dependency solving | Medium | Low | Document that composition validates a declared closure; it does not choose arbitrary plugins. |
| Convenience examples get more verbose | Low | Medium | Provide clear group presets instead of hidden installs. |
| Group membership drifts from installation | High | Medium | Derive both resolved group snapshots and committed installation from one ordered registration collection. |

## Consequences

- `Plugin` exposes one static declaration with stable ID, category, capabilities, requirements, and
  conflicts; group membership is resolved provenance rather than plugin-owned metadata.
- `PluginGroupBuilder` is data-only, and `App::add_plugins` lowers plugin/group/tuple inputs through
  the same resolver and closed lifecycle commit.
- `WgpuRenderPlugin` no longer unconditionally installs or compiles sprite/UI submitters; product
  capability closure selects the base backend and each submitter independently.
- Runtime 2D and runtime UI are separate groups, and desktop window plus wgpu composition is
  additive rather than fixed in `DesktopWgpuPlugins`.
- Root facade/prelude cleanup exposes group names deliberately rather than exporting backend/plugin
  internals through the gameplay prelude.

## Open Questions

- Which canonical configuration encoding/derive gives third-party definition authors deterministic
  fingerprints with the smallest advanced authoring surface?
- Which concrete cross-plugin replacement proves the need for a public slot-conformance evidence
  carrier beyond same-plugin configuration and optional-slot disable?
- Which named project preset, if any, should examples use for the common 2D desktop capability set?

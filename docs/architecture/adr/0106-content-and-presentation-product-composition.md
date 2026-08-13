# ADR 0106: Content and Presentation Product Composition

**Status**: Accepted
**Date**: 2026-08-04
**Last Revised**: 2026-08-13
**Owner**: Root product features, project capability normalization, Host recipes, and plugin groups
**Refines**: [ADR 0005](0005-dimension-aware-runtime-with-2d-first-authoring.md),
[ADR 0040](0040-render-resource-lifetime-and-submitter-ownership.md),
[ADR 0044](0044-root-facade-and-prelude-layering-policy.md),
[ADR 0046](0046-plugin-metadata-and-default-plugin-groups.md),
[ADR 0056](0056-headless-runtime-and-dedicated-server-readiness.md),
[ADR 0079](0079-root-product-capabilities-and-placeholder-domain-retirement.md),
[ADR 0082](0082-process-host-authority-and-runtime-construction-topology.md), and
[ADR 0095](0095-plugin-owned-specialized-domains-and-project-configuration.md)
**Implementation Plan**: [Ordered UI Kernel and Engine Boundary Correction](../../plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md), NUI-U5 and NUI-U15; [Product Composition Contribution Correction](../../plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md)

**Composition Baseline**: [PCC-U4 Reviewed Product Composition Conformance Baseline](../../knowledge/engineering/verification/2026-08/2026-08-13T073115Z-pcc-u4-reviewed-product-composition-conformance-baseline-56264181873249c18d5477ce2170c8b1.md)

## 2026-08-12 Product-Contribution Amendment

The compiled Cargo ceiling, requested product and installed plugin closure remain three separate
authorities. A compile feature may make a concrete implementation available, but it must not alter a
base plugin declaration, register systems, create platform sessions, drain a domain queue or publish
platform semantics.

```mermaid
flowchart LR
    Cargo[Compiled implementation available] --> Admission{Requested intersection available?}
    Request[Normalized product request] --> Admission
    Admission -->|no| Reject[CompositionError; App unchanged]
    Admission -->|yes| Base[Base adapter plugin]
    Admission -->|presentation-2d + render-wgpu| Sprite[WGPU sprite contribution]
    Admission -->|presentation-ui + render-wgpu| UI[WGPU UI contribution]
    Admission -->|presentation-ui + desktop-winit| A11y[Winit accessibility contribution]
```

- `WgpuRenderPlugin` has one feature-invariant declaration and owns only the base WGPU backend
  lifecycle plus sealed frame submission. Concrete first-party Sprite and UI contribution plugins
  install the systems that contribute their domain packets to the private backend frame. They do
  not admit a public submitter trait, registry, dynamic phase or RenderGraph.
- The WGPU Sprite contribution is installed only for the requested
  `presentation-2d + render-wgpu` intersection. The WGPU UI contribution is installed only for the
  requested `presentation-ui + render-wgpu` intersection.
- Winit platform accessibility is one explicit product contribution installed only for the
  requested `presentation-ui + desktop-winit` intersection. `nara_winit` receives an opaque
  installed-contribution fact and never interprets the root `ProductCapabilitySet`.
- Ordinary file-backed Headless, Desktop and Editor actions accept one `ProductRecipe` plus
  execution policy. Non-replayable Host test probes use a recipe-anchored API under
  `project_host::advanced`; raw direct-App plugins remain a separate explicit path. Neither ordinary
  intents nor advanced Host overlays carry an independent schema-provider list. A schema-owning
  file-backed extension uses `SchemaContribution`.
- Capability normalization, concrete-contribution selection, plugin-plan resolution and schema
  resolution reject before candidate App creation. Plugin build and Host bridge capture remain
  fallible candidate work: failure poisons and retires the unpublished candidate rather than
  claiming arbitrary Rust side effects were rolled back.
- These concrete contribution plugins are first-party closed topology. They do not make backend
  replacement, dynamic plugin discovery or a general contribution registry supported contracts.

## Context

Nara currently treats `runtime-2d` as both game content/simulation and presentation: selecting it
installs render, image preparation, and sprite submission. Runtime UI has the same coupling between
authoring/interaction and UI rendering. As a result, a project cannot run the same Scene, gameplay,
sprite/tilemap facts, UI intent, and Retry authority in a truly headless Host without compiling or
resolving presentation owners.

Desktop presentation is a product choice made by the Host under a compiled capability ceiling. It
is not an intrinsic property of 2D content, UI authoring, or a project document. Editor tooling is a
separate Host-owned product under ADR 0104 and must not enter either content or presentation by
implication.

## Decision

Nara separates product composition into content/simulation, renderer-neutral presentation, and
platform/backend adapter closures. Hosts select and validate those closures explicitly before
mutating an `App`.

```mermaid
flowchart TD
    Project[Normalized project content request]
    Host[Host product recipe]
    Content[2D and UI content/simulation groups]
    Present[Explicit 2D and UI presentation groups]
    Adapters[Explicit window and WGPU adapters]
    Headless[Headless runtime World]
    Desktop[Desktop presented product]
    Tooling[Separate Host-owned Editor tooling]

    Project --> Host
    Host --> Content
    Content --> Headless
    Host --> Present --> Desktop
    Host --> Adapters --> Desktop
    Content --> Desktop
    Host -. deliberate Editor product only .-> Tooling
```

- `runtime-2d` is redefined as content/simulation-safe. It owns the 2D transform, image/content,
  material, sprite, tilemap, and gameplay-facing facts required by the runtime, but installs no
  render coordinator, image-preparation owner, sprite submitter, WGPU adapter, or window adapter.
- `runtime-ui` is redefined as UI authoring, text/kernel/widget behavior, normalized input intent,
  and ECS/Scene adaptation. It installs no UI renderer, GPU glyph/image owner, WGPU adapter, or
  window adapter.
- Renderer-neutral 2D presentation is an explicit product capability/group that installs the render
  coordination, image preparation, and sprite/tilemap packet-producing owners required by a real
  presented product.
- Renderer-neutral UI presentation is a separate explicit capability/group that installs UI paint
  packet construction and bounded glyph/image preparation. It does not imply 2D sprite/tilemap
  presentation.
- Window/surface ingress and WGPU submission remain separate platform/backend adapter capabilities.
  Presentation groups cannot choose an operating-system window backend or a GPU backend.
- A desktop Host recipe explicitly combines the required content groups, presentation groups,
  window adapter, and WGPU adapter exactly once. There is no hidden monolithic desktop bundle that
  bypasses normalized capability and plugin-closure validation.
- A headless Host resolves the same game content, Scene, gameplay state, and Retry command authority
  with content groups only. Its resolved plugin plan contains no window, render, image-preparation,
  sprite-submission, UI-render, WGPU, or Editor tooling owner.
- Focused headless UI-kernel tests may construct an explicit test presenter to prove receipt-gated
  interaction. That test utility is not installed by the headless product recipe.
- Compiled Cargo capabilities remain an upper bound, normalized project needs remain declarative,
  and the Host owns adapter selection. Missing or contradictory content, presentation, or adapter
  closure fails before any resource, schedule, plugin, or lifecycle mutation.
- Because Nara is pre-1.0, the old meanings of `runtime-2d` and `runtime-ui` are corrected in place.
  No legacy feature alias, duplicate plugin group, or transitional project-capability spelling keeps
  presentation implied.
- Tooling is selected only by an Editor Host recipe and remains independent from both runtime
  content and presentation composition.

## Alternatives Considered

### Option 1: Keep content features implying their default presentation stack

Rejected. It makes headless execution a subtractive special case, hides compile and runtime cost,
and prevents the same content recipe from proving product parity without presentation owners.

### Option 2: Add one monolithic desktop feature and plugin bundle

Rejected. It couples windowing, WGPU, 2D, UI, and tooling choices, obscures invalid combinations,
and recreates the fixed bundle removed by ADR 0079.

### Option 3: Compose content, presentation, and adapters explicitly

Selected. It keeps content portable, makes desktop cost and authority inspectable, and lets Hosts
choose coherent products without exposing every internal crate as a capability.

### Option 4: Let additive Cargo features activate matching runtime behavior

Rejected. Cargo features are a compiled ceiling and are unified across the dependency graph. Using
them as runtime installation switches makes an unrelated consumer silently change another product's
plugin requirements and platform behavior.

### Option 5: Install concrete first-party contributions from requested capability intersections

Selected. It keeps the backend and platform adapters deep, preserves exact product inspection and
avoids inventing a generic submitter or contribution SPI.

## Consequences

- Existing root feature, project capability, plugin-group, reference-game recipe, and facade
  expectations change together with no compatibility aliases.
- Independently compiled consumers can select 2D content, UI content, each presentation slice, and
  desktop adapters without importing unrelated product domains.
- Headless gameplay and Retry tests use the same content and command authorities as desktop while
  proving the absence of presentation and tooling owners.
- Presentation crates become optional consequences of a presented product rather than mandatory
  dependencies of 2D or UI data.
- Base backend and platform plugins no longer change behavior when unrelated integrations are
  compiled. Requested products install concrete first-party contributions explicitly.
- Ordinary file-backed Host actions have one recipe authority; advanced direct-plan editing remains
  available only through its explicitly advanced API.
- This decision does not define remote rendering, browser/mobile backends, automatic backend
  selection, Editor docking/multi-window policy, or dynamic capability acquisition.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Presentation/tooling owners in the reference-game headless resolved plan | 0 | Plugin-plan snapshot and dependency checks |
| Desktop content, presentation, window, and WGPU group installation | Exactly once per required group | Desktop composition snapshot |
| `runtime-2d` dependencies on render preparation/submission or window/WGPU adapters | 0 | Locked independent-consumer Cargo tree |
| `runtime-ui` dependencies on UI rendering or window/WGPU adapters | 0 | Locked independent-consumer Cargo tree |
| Compatibility aliases preserving old implicit semantics | 0 | Feature, manifest, facade, and stale-symbol inspection |
| Headless/desktop game and Retry authority parity | Identical accepted gameplay outcomes | Host parity and reference-game integration tests |
| Invalid capability-pair mutation | 0 resources, schedules, or plugins changed | Retryable composition-failure tests |
| Base plugin requirements changed by unrelated compiled features | 0 | All-features requested-subset matrix |
| Unrequested WGPU/Winit contribution plugins | 0 | Resolved-plan absence assertions |
| Ordinary file-backed Host composition inputs | 1 `ProductRecipe` plus execution policy | Public inventory and Host parity tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Desktop recipes omit a required presentation owner | High | Medium | Normalize and validate product/service closure before App mutation and test the complete desktop recipe. |
| Content crates retain hidden render dependencies | High | High initially | Enforce locked independent-consumer trees and source-level dependency audits. |
| Capability vocabulary mirrors every crate | Medium | Medium | Keep capabilities product-sized and admit a new slice only for a real independently selectable consumer. |
| Headless and desktop gameplay behavior diverge | High | Medium | Run the same content fixtures and Retry authority through Host parity tests; vary only presentation/adapters. |
| Correcting feature meanings surprises early users | Medium | High | Publish one explicit pre-1.0 migration entry and provide no alias that can silently retain the wrong closure. |
| Concrete contribution plugins are mistaken for a public SPI | High | Medium | Keep their frame sets and bridge receipts private; expose no trait, registry or dynamic phase contract. |
| Compiled accessibility code creates hidden native behavior | Critical | Medium | Gate every AccessKit session, event and publication path on the installed opaque contribution fact. |
| Recipe migration moves test-only raw edits into product API | High | Medium | Convert real product plugins to typed recipes and keep raw plan editing in explicit advanced fixtures only. |

---
title: Readable Runtime UI and Deterministic Text - Plan
type: feat
date: 2026-08-04
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution_state: active
product_contract_source: reference-game-product-gap
execution: code
origin: docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md
supersedes: docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md
plan_id: readable-runtime-ui-deterministic-text-2026-08
unit_namespace: RUI
operator_execution_authority: external-control-plane
---

# Readable Runtime UI and Deterministic Text - Plan

## Goal Capsule

- **Objective:** Replace the reference game's unlabeled bars and terminal color block with a
  readable HUD, a clear victory/defeat overlay, and a real Retry button. Prove one deterministic,
  Unicode-capable runtime-UI text path from typed font bytes through shaping, glyph caching,
  backend-neutral primitives, and the existing wgpu UI pass.
- **Authority:** Accepted ADR 0025 owns runtime UI, ADR 0031 owns the text/font constraints, ADR
  0041 owns input routing, and ADR 0095 rejects a speculative text backend. OQ-015 remains open for
  project font import, fallback families, localization, editable text, and a possible future shared
  text domain. No new ADR is required for the private Parley/Swash implementation choice.
- **Inherited evidence:** SRT-U1 through SRT-U6 remain complete at their recorded revisions. ADR
  0089 remains Proposed; this plan does not reopen scene lifecycle or Retry authority.
- **Execution profile:** Fearless pre-1.0 refactoring is authorized. Remove entity-ID painter-order
  fallbacks and the reference game's rectangle-only terminal presentation rather than preserving
  parallel compatibility paths.
- **Stop conditions:** Stop and re-plan if correctness requires project font import, a generic
  importer/provider registry, a new text or render crate, a public text backend, a new render phase,
  a RenderGraph, full gameplay-state topology, general widget callbacks, or a persistent hierarchy
  redesign outside the UI component schema.

---

## Product Contract

### Problem

The reference game starts immediately but does not tell the player how to move, that firing is
automatic, how much health remains, what the score is, or how to retry. Runtime UI renders panels
only, and equal-order layout and hit testing still use runtime `Entity` allocation as a product
ordering fallback. The current system therefore cannot provide readable product feedback or a
stable button contract across rebuild, Retry, save, and reopen.

### Requirements

#### Authority And Scope

- R1. Exactly one implementation-ready plan and one engineering-memory registration are active.
  The SRT plan is reciprocally superseded without changing its immutable completion evidence.
- R2. OQ-015 is activated only for the first concrete runtime-UI shaping/raster path. Font import,
  localization, IME/editable text, world text, fallback-family policy, and a shared `nara_text`
  domain remain unresolved and outside this plan.
- R3. The runtime UI path remains owned by `nara_ui`, `nara_ui_render`, and
  `nara_render_wgpu`. No `TextBackend`, `UiBackend`, provider registry, or third-party type enters
  the ordinary Nara API.

#### Deterministic UI Stack

- R4. `UiRoot::order` is the explicit order among roots for one render target. `UiNode` gains a
  persistent sibling-order value used only within its structural parent. Equal root orders for the
  same target or equal sibling orders under one parent are invalid; runtime entity IDs and query
  iteration never resolve the conflict.
- R5. Layout builds a complete candidate tree first, validates every root and sibling order, then
  publishes one last-good `ComputedUiLayouts` generation. Each visible node receives a derived
  stack index from root order, z-index, and explicit depth-first source order. Failure publishes no
  partial stack and reports one bounded UI diagnostic.
- R6. Panel extraction, text extraction, painter order, clipping, and hit testing consume the same
  stack index. Draw walks the stack forward and hit testing walks it backward. Material sorting may
  batch only adjacent compatible primitives and cannot reorder visible primitives.
- R7. The `UiNode` schema version and codec change are explicit. Current first-party fixtures and
  examples assign sibling order deliberately; migration never claims to recover an order that the
  prior format did not store.

#### Typed Font And Text

- R8. `UiFontAsset` is an immutable typed asset constructed through one fallible byte-validation
  boundary. Retained font bytes, faces, text bytes, shaped glyphs, atlas dimensions, and atlas
  pixels have documented fixed ceilings. Invalid or oversized input fails before publication.
- R9. `UiPlugin` installs one deterministic bundled default font from a repository-owned asset with
  recorded source and license. It performs no system-font discovery and needs no current working
  directory. A Direct App may install a typed runtime font override; file-backed project font
  import is not claimed by this slice.
- R10. `UiText` is a bounded single-line persistent component with private fields and fallible
  construction/update. It owns UTF-8 text, finite positive font size, and color. A separate
  runtime-only `UiTextFont` may select a typed font handle; absence selects the bundled default.
- R11. `nara_ui_render` privately owns Parley 0.11 shaping/layout and Swash 0.2 rasterization with
  system discovery disabled. The architecture accepts Unicode text and advanced shaping; missing
  fonts or glyphs produce bounded diagnostics rather than silent empty output or ASCII-only
  substitution.
- R12. The first implementation supports one line, explicit node bounds, and clipping. Intrinsic
  measurement, wrapping, rich text, fallback families, locale switching, and editable text are not
  hidden assumptions or partial public promises.
- R13. A private bounded CPU glyph atlas uses an established rectangle allocator and publishes an
  owned immutable generation snapshot. Glyph cache/atlas data is runtime render data, never scene
  data and never an `ImageAsset` with fabricated import provenance.

#### Render Submission

- R14. `nara_ui_render` emits one ordered primitive stream containing panel and glyph quads. A
  primitive key contains the node stack index and an intra-node primitive order, so a node's panel
  precedes its glyphs while text and panels from different nodes remain correctly interleaved.
- R15. The wgpu frame packet owns the exact glyph-atlas snapshot needed by its UI primitives.
  `nara_render_wgpu` uploads and caches atlas generations under the existing device epoch, scissor,
  quad shader, UI phase, acquire, submit, and present contract. Packet submission does not borrow
  the gameplay World.
- R16. Atlas upload and cache retention are bounded and observable through focused UI render stats.
  Device loss or generation change cannot reuse stale glyph pixels. No new render phase, public
  graph node, generic resource provider, or mirrored image-asset pipeline is added.

#### Button And Input Semantics

- R17. `UiButton` owns enabled state only. `UiInteractionState` exposes one frame-transient
  activation target and no callback. A left press starts capture only on a visible enabled button;
  release activates exactly once only on the same still-eligible target.
- R18. Ordered mouse transitions are consumed in order. Release outside, hidden/disabled/removed
  targets, pointer-route changes, and focus loss cancel activation. The interaction system runs in
  a named `PreUpdate` UI set after physical action resolution and uses the last completed UI stack.
- R19. The reference game's desktop adapter observes Enter and Retry-button activation, coalesces
  them to at most one submission per frame, and submits the existing `retry_draft()` into the
  existing bounded `GameplayCommandQueue`. It does not add a second reset path, mutate wave state,
  add a semantic-input bus, or expose a widget callback.

#### Reference-Game Product Proof

- R20. Running HUD visibly presents wave, `HP current / maximum`, score, enemies remaining/planned,
  and `WASD MOVE | AUTO-FIRE`. It projects only `WaveSnapshot` and does not become gameplay
  authority.
- R21. Terminal state presents a dim overlay, `VICTORY` or `DEFEAT`, final score, a real Retry
  button, and `ENTER TO RETRY`. Pending Retry disables the button and displays `RETRYING...`;
  rejection keeps the terminal generation and restores a bounded visible reason; success removes
  the terminal UI with the new generation.
- R22. HUD, terminal panel, text, button visuals, and hit target preserve explicit stack order over
  rebuild, Retry, resize, save, reopen, and unrelated entity allocation. The ordinary desktop and
  packaged product paths use the same recipe and renderer.
- R23. The selected bundled font and its license/provenance are committed and embedded in the
  product. Source and packaged desktop products render the same glyph pixels from arbitrary
  current working directories and home directories.

#### Public Boundary And Closure

- R24. Ordinary public API is limited to the typed font asset, bounded `UiText`, runtime font
  override, `UiButton`, explicit sibling order, activation observation, and the semantic UI schedule
  anchor required by the real product. Shaping contexts, glyph IDs, allocator state, atlas pages,
  primitive internals, and wgpu cache types remain private or advanced.
- R25. Every behavior-bearing unit uses focused proof-first or characterization-first tests. No
  script parses Rust source, mirrors job topology, or grows a special evidence protocol.
- R26. The slice closes only after real text pixels and click-to-Retry are observed through the
  packaged ordinary desktop product, with no unresolved P0/P1 review finding.

### Key Technical Decisions

1. **One concrete text owner first.** Runtime UI owns shaping through submission. A shared text
   crate waits for a second real consumer.
2. **Parley plus Swash, kept private.** They provide mature Unicode shaping/layout and rasterization
   without exposing their types. System font discovery is disabled.
3. **Bundled default before project font import.** This closes the product workflow without
   misclassifying fonts in the current image-only Project Content closure or inventing a generic
   importer seam.
4. **Explicit UI order, not hierarchy order by accident.** UI sibling order is a UI authoring fact;
   the generic `Children` collection remains a runtime relationship projection.
5. **One primitive stream.** Panels and glyphs are sorted together before adjacent batching, so
   material grouping cannot violate painter order.
6. **Product adapter, not input framework expansion.** The reference game coalesces Enter and
   button activation into its existing Retry command at `PreUpdate`.
7. **No title menu yet.** Direct startup remains. A full boot/menu/gameplay/pause/game-over state
   topology waits for the simultaneous workflow pressure described by OQ-034.

### High-Level Flow

```text
UiRoot + UiNode sibling order
          |
          v
validated ComputedUiLayouts generation -----> reverse-order hit testing
          |
          +---- UiPanel -------------------+
          |
          +---- UiText -> Parley -> Swash -+--> ordered UI primitives
                                               + owned atlas generation
                                                         |
                                                         v
                                      existing WGPU UI phase / scissor / quad pass

Button activation + Enter action
          |
          v
reference-game desktop adapter -> retry_draft() -> GameplayCommandQueue -> atomic Retry
```

### Risks And Mitigations

| Risk | Mitigation |
|---|---|
| Text work expands into localization or editor text | Freeze single-line runtime display text; retain OQ-015 for other workflows. |
| Font fallback becomes platform dependent | Bundle one validated font and disable system discovery. |
| Material batching changes painter order | Sort one primitive stream first and batch only adjacent compatible items. |
| Runtime entity allocation changes visual or hit order | Reject duplicate explicit order and remove every entity-ID tie-break. |
| Glyph atlas masquerades as imported content | Keep an owned UI-render atlas snapshot and a wgpu cache; never create an `ImageAsset`. |
| Click creates a second Retry authority | Submit the existing command draft and leave all reset logic in the current fixed-tick consumer. |
| UI interaction observes half-built layout | Publish complete last-good layout generations and use the prior completed generation in `PreUpdate`. |

---

## Implementation Units

### RUI-U1. Activate The Readable Runtime UI Slice

- **Goal:** Establish one truthful successor authority and retire stale SRT-era documentation
  anchors before code changes.
- **Requirements:** R1-R3, R25.
- **Dependencies:** Verified SRT-U6 completion at `4f5fb6f` and current baseline `8bcec9f`.
- **Files:** this plan; SRT frontmatter; architecture map; ADR implementation ledger; one
  verification shard and one work registration; derived engineering-memory indexes.
- **Approach:** Keep ADR 0089 Proposed, retain OQ-015 for unimplemented workflows, record the narrow
  runtime-UI trigger, and correct ADR 0011's old reference-game generation-3 anchors.
- **Verification:** Direct plan reciprocity and authority audit, ledger/source anchor inspection,
  engineering-memory validate/render/check, link and whitespace checks; no Cargo documentation test.

### RUI-U2. Publish One Deterministic UI Stack

- **Goal:** Replace entity-ID tie-breaking with explicit persistent UI source order shared by
  layout, rendering, and hit testing.
- **Requirements:** R4-R7, R24-R25.
- **Dependencies:** RUI-U1.
- **Files:** `crates/nara_ui/src/{lib.rs,layout.rs,interaction.rs,codec.rs,tests.rs}`;
  `crates/nara_ui_render/src/{types.rs,extract.rs,queue.rs,tests.rs}`; affected examples and schema
  fixtures.
- **Approach:** Add `UiNode` sibling order, validate root/sibling uniqueness into a candidate
  generation, derive one stack index, remove `Entity::to_bits()` from painter and hit semantics,
  and make panel extraction carry the exact index. Preserve only adjacent material batching.
- **Test scenarios:** Stable order after unrelated spawn/despawn and entity reallocation; nested
  siblings and z-index; duplicate root/sibling rejection with last-good generation; draw-forward and
  hit-reverse parity; clip behavior; schema version/codec roundtrip and legacy disposition.
- **Verification:** Focused serial `nara_ui` and `nara_ui_render` nextest suites, affected examples,
  locked checks, strict changed-target Clippy, fmt, API review, and unit evidence.

### RUI-U3. Add The Typed Bundled Font And CPU Text Pipeline

- **Goal:** Shape and rasterize bounded single-line UI text into a private owned glyph-atlas
  generation without platform font discovery.
- **Requirements:** R8-R13, R24-R25.
- **Dependencies:** RUI-U2.
- **Files:** workspace dependencies and lockfile; `crates/nara_ui` font/text modules, codec, bundled
  font and license/provenance; `crates/nara_ui_render` text pipeline, atlas, stats, and tests.
- **Approach:** Add private Parley 0.11, Swash 0.2, and a proven rectangle allocator. Install a
  validated default typed font, expose bounded `UiText` plus runtime override, shape advanced UTF-8
  text, rasterize/cache glyphs, and publish an immutable bounded atlas snapshot. Do not use system
  fonts, `ImageAsset`, a file importer, or a public backend trait.
- **Test scenarios:** Bundled font parses; corrupt/oversized font rejection; empty, boundary, and
  oversized text; invalid size/color; repeated glyph cache reuse; non-ASCII shaping; missing glyph
  diagnostic; exact glyph/atlas limits; deterministic glyph positions and atlas bytes; codec
  roundtrip.
- **Verification:** Focused serial UI/UI-render nextest suites, locked checks, dependency review,
  strict Clippy, fmt, data-integrity/performance/API review, and unit evidence.

### RUI-U4. Submit Ordered Glyphs Through WGPU

- **Goal:** Render the exact CPU glyph-atlas generation through the existing owned frame packet and
  UI phase while preserving panel/text painter order.
- **Requirements:** R14-R16, R23-R26.
- **Dependencies:** RUI-U3.
- **Files:** `crates/nara_ui_render` primitive/batch ownership; `crates/nara_render_wgpu/src/{lib.rs,
  ui.rs,backend.rs}` and focused GPU tests; root feature wiring if required.
- **Approach:** Replace panel-only extraction with one ordered primitive stream, capture the required
  atlas snapshot into `WgpuFramePayload`, cache atlas textures by generation and device epoch, and
  reuse the existing UI shader/scissor/pass. Add bounded upload/cache stats without a second render
  phase or World borrow during submit.
- **Test scenarios:** Panel/glyph/panel interleaving; text clipping; unchanged atlas reuse; changed
  generation single upload; stale generation/device-epoch rejection; missing atlas fail-closed;
  bounded upload and eviction; nonblank glyph pixel readback at desktop viewport sizes.
- **Verification:** Focused serial UI-render and wgpu nextest suites, locked checks, constrained GPU
  pixel proof, strict Clippy, fmt, correctness/performance review, and unit evidence.

### RUI-U5. Deliver The Readable HUD And Retry Button

- **Goal:** Replace the reference game's rectangle-only status display with an understandable,
  mouse-operable product flow that reuses the existing Retry command.
- **Requirements:** R17-R23, R25-R26.
- **Dependencies:** RUI-U4.
- **Files:** `crates/nara_ui` button/interaction scheduling and tests; `reference-game/src/{ui.rs,
  input.rs,lib.rs}`; desktop flow/render tests, probes, docs, and package policy as needed.
- **Approach:** Add minimal enabled-button activation semantics, move interaction to the named
  `PreUpdate` set using the last completed stack, and install one product adapter that coalesces
  Enter and click into `retry_draft()`. Project HUD and terminal text exclusively from
  `WaveSnapshot` and `WaveRetryStatus`.
- **Test scenarios:** Running HUD values and controls; victory/defeat text; press/release same target
  activates once; drag-out/hidden/disabled/removed/focus-loss cancellation; same-frame ordered
  transitions; Enter/click equivalence and coalescing; Pending disabled state; Rejected reason and
  retained old generation; Applied clears terminal UI; resize without overlap or hit drift.
- **Verification:** Reference-game default/desktop/flow/render/parity suites, source desktop smoke,
  bounded pixel proof, Clippy/fmt, product/correctness review, and unit evidence.

### RUI-U6. Prove Packaged Product Readability And Close

- **Goal:** Verify the ordinary packaged desktop journey, classify the new public surface, and
  update architecture status without claiming the deferred text ecosystem.
- **Requirements:** R2-R3, R22-R26.
- **Dependencies:** RUI-U5 and a reviewed executable revision.
- **Files:** package smoke/policy and reference-game docs; API boundary tests; ADR 0025/0031/0041
  ledger rows; foundation ownership summary; engineering-memory evidence and registration.
- **Approach:** Run the real packaged binary from arbitrary cwd/home, visually verify HUD and both
  Retry inputs, inspect glyph pixels and device/cache behavior, and classify every new symbol as
  ordinary, advanced, or private. Record ADR 0031 as partial, not fully implemented, and retain
  OQ-015's deferred workflows.
- **Test scenarios:** No-checkout/no-source font availability; deterministic source/package glyph
  output; full play-to-terminal-to-click-Retry journey; Enter fallback; repeated Retry and resize;
  default/headless feature isolation; public-surface negative audit.
- **Verification:** Focused serial workspace/reference-game/package gates, real manual desktop
  journey, independent correctness/testing/API/maintainability review, memory validate/render/check,
  final diff review, and `git diff --check`.

---

## Verification Contract

### Focused Gates

| Unit | Required verification |
|---|---|
| RUI-U1 | Authority reciprocity, ledger anchors, memory validate/render/check, link and whitespace inspection; no Cargo documentation test. |
| RUI-U2 | UI layout/interaction/render ordering suites, schema/codec migration evidence, affected examples, locked serial checks and Clippy. |
| RUI-U3 | Font validation, shaping/raster, cache/atlas limits, Unicode/missing-glyph behavior, locked serial checks and review. |
| RUI-U4 | Unified primitive ordering, owned atlas packet, wgpu cache/device epoch, clipping and glyph pixel proof. |
| RUI-U5 | Reference-game HUD/terminal/button behavior, Enter/click command parity, desktop flow/render/parity suites and smoke. |
| RUI-U6 | Packaged arbitrary-environment journey, feature isolation, public API audit, independent review and memory closure. |

### Regression Rules

- Never run Cargo concurrently in this checkout. Reuse `target`, use `CARGO_BUILD_JOBS=1` and
  `-j 1` for substantial work, and expand from focused `cargo nextest run` gates by blast radius.
- Run `cargo fmt --all -- --check`, affected locked checks/tests, changed-target strict Clippy with
  explicit pre-existing allowances, and `git diff --check`.
- Keep headless/server products free of winit, wgpu, toolkit, font-raster, and raw-input runtime
  requirements unless their selected features explicitly include runtime UI.
- Do not run or extend `tests/architecture_docs.rs`. Inspect authority links directly and use the
  engineering-memory validator and renderer for document state.
- Do not add a reference-game-specific script or evidence protocol. Rust tests, existing package
  smoke, and one bounded manual desktop journey own the evidence.
- Every unit receives final diff review before precise staging and a Conventional Commit.

---

## Definition Of Done

- One active RUI plan/registration supersedes SRT while preserving all SRT completion evidence and
  keeping ADR 0089 Proposed.
- Explicit UI root/sibling order produces one validated stack consumed identically by layout,
  panel/text drawing, clipping, and reverse hit testing; no runtime entity ID is a product-order
  tie-break.
- One immutable typed bundled font, bounded single-line `UiText`, mature Unicode shaping/raster,
  and a private bounded glyph atlas reach the existing wgpu UI phase through an owned frame packet.
- Panel and glyph primitives remain correctly interleaved; atlas generation and device epoch cannot
  display stale pixels; missing/invalid/over-budget text data fails with bounded diagnostics.
- The reference game visibly explains controls and live status, shows victory/defeat/final score,
  and exposes a real Retry button. Click and Enter coalesce into the existing Retry command and
  preserve the SRT atomic replacement authority.
- Source, Editor, ordinary desktop, and packaged no-checkout paths retain deterministic font and UI
  behavior across resize and Retry. Headless behavior remains unchanged.
- ADR 0025, ADR 0031, ADR 0041, the ledger, foundation summary, reference-game docs, and engineering
  memory state only the implemented slice. OQ-015 remains open for project font import, fallback,
  localization, editable text, world text, and any shared text module.
- Every changed public contract has English API documentation, migration guidance where persistent
  schema changed, focused behavioral and negative tests, precise commits, and no unresolved P0/P1.
- No `nara_text` crate, public text/backend provider, system-font discovery, fake font
  `ImageAsset`, generic widget callback, semantic-input bus, title-state framework, RenderGraph, or
  reference-game-specific evidence framework remains.

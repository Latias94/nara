---
title: Runtime UI Product Slice - Plan
type: feat
date: 2026-08-08
deepened: 2026-08-10
amended: 2026-08-10
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution: code
execution_state: active
product_contract_source: docs/architecture/reference-game-ui-product-slice.md
origin: docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md
depends_on: docs/knowledge/engineering/verification/2026-08/2026-08-10T145838Z-uif-u5-foundation-conformance-baseline-70b35505e8da4cb388c36d3ede602259.md
supersedes: docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md
baseline_record: 70b35505e8da4cb388c36d3ede602259
plan_id: runtime-ui-product-slice-2026-08
unit_namespace: RUP
operator_execution_authority: external-control-plane
---

# Runtime UI Product Slice - Plan

## Goal Capsule

- **Objective:** Consume UIF-U5 baseline `70b35505e8da4cb388c36d3ede602259` to prove one
  data-authored Runtime UI workflow from gameplay state through backend-accepted presentation,
  exact input activation, restart, headless execution and packaged launch.
- **Authority:** ADR 0102 and ADR 0107-0109 own shared UI decisions. This plan owns product
  integration only. It is the canonical successor for the still-open NUI-U15B product obligations;
  the superseded NUI plan is not reactivated.
- **Execution profile:** Use the Reference Game as an acceptance carrier. Fix product adapters and
  data first. A neutral reproduction must prove an owning Foundation defect before shared contracts
  change.
- **Stop condition:** Suspend this plan if a required product behavior cannot be expressed through
  the pinned public inventory. Publish a focused UIF correction and explicitly rebase this plan to a
  successor baseline before resuming.
- **Tail ownership:** This plan closes Runtime UI product validation only. Editor, new shared widgets,
  dynamic asset APIs and generalized authoring infrastructure remain separate work.

## Product Contract

### Summary

Prove one complete data-authored Runtime UI product slice without turning the Reference Game into the
owner of shared interfaces. The proof uses one real state trajectory, one bounded authoring
roundtrip and exact desktop, headless and packaged consumers instead of Cartesian smoke or opaque
hashes.

### Problem Frame

UIF-U5 proves that the shared compile, present and dispatch boundaries are internally coherent and
independently consumable. It does not prove that a game can author, present, activate, restart and
package the same UI through ordinary product paths. Existing Reference Game fixtures are useful
component regressions, but several construct independent runtimes or use test presenters; they do
not by themselves establish a Product-Validated Baseline.

### Requirements

#### Baseline and authority

- R1. All product evidence pins UIF-U5 record `70b35505e8da4cb388c36d3ede602259`
  and commit `e307cb3` until an explicit correction lineage replaces it.
- R2. Product code uses the frozen public Runtime UI and presentation composition; hidden Host
  authority is permitted only inside the named first-party backend presenter.
- R3. A shared contract may change only after a renderer-neutral reproduction fails against the
  owning module and this plan is suspended.

#### Product trajectory and interaction

- R4. One runtime starts from the Scene/Prefab-authored HUD and terminal structure, changes gameplay
  facts, reaches a terminal modal, activates Retry and presents the successor run generation.
- R5. Pointer, keyboard, gamepad and platform accessibility activation each emit exactly one typed
  Retry command against the exact presented receipt.
- R6. Retry pending disables repeat activation while retaining the same focus identity; rejection
  restores the same focused action, and application removes obsolete paint, hit, focus and semantic
  facts before the successor run appears.
- R7. Paused delivery remains bounded and cursor-retained. Input backpressure or stale generations
  reject closed without leaking activation to gameplay.
- R8. Completed, defeated, Retry pending, Retry rejected and Retry applied behavior is observed as
  one ordered product trace where applicable, not reconstructed solely as independent static worlds.

#### Surface and visual contract

- R9. The authored design space is fixed at `960 x 540`; `1280 x 720`, `1920 x 1080` and
  `2560 x 1440` prove invariant logical layout with target-specific transform and raster facts.
- R10. Ordinary behavior cases use input scale `1.0`; exactly one representative pointer case uses
  fractional scale `1.25` so scale does not multiply the state and input matrix.
- R11. One non-16:9 transient resize proves letterbox or pillarbox mapping, safe margins and
  non-clamped black-bar input. It does not admit a 4:3 authored target or general responsive layout.
- R12. Structural assertions cover safe margins, modal coverage and centering, non-overlap, text
  bounds, state facts, focus, semantics and exact packet/receipt identity. Opaque image hashes and a
  full state-by-target Cartesian product are not completion evidence.

#### Authoring and delivery

- R13. Scene/Prefab data owns structure, order, layout, labels, theme roles and action identity;
  Rust supplies typed gameplay facts and consumes typed intents only.
- R14. An ordinary source-data edit changes one visible geometry, label, theme role or modal
  structure fact and is consumed after restart or supported reload without a Rust source change or
  test-private injection hook.
- R15. Desktop/WGPU, presentation-free headless and packaged arbitrary-CWD launch consume the same
  project data and Runtime UI adapter. Package evidence proves resource and font closure.
- R16. At least one real platform accessibility ingress maps an exposed node/action to the exact
  presented session; direct internal numeric action injection remains component evidence only.
- R17. Verification records representative compile, plan and presentation latency plus packet,
  atlas and retained-resource observations. This plan gathers a baseline and does not freeze an SLA.

### Acceptance Examples

- AE1. A run starts with authored HUD facts, reaches Completed or Defeated, presents Retry, accepts
  one activation, enters pending, and shows a new run generation only on the next matching receipt.
  Covers R4-R8.
- AE2. A rejected replacement keeps the terminal visible, restores the same focused Retry identity,
  emits no second command and does not expose underlay UI or gameplay. Covers R5-R8.
- AE3. The same authored frame remains inside the 24 logical-pixel safe margin at all three 16:9
  targets. A 4:3 physical window letterboxes it, and black-bar clicks never clamp into Retry. Covers
  R9-R12.
- AE4. A source-data label or geometry edit is visible in desktop, headless semantic/layout facts
  and an arbitrary-CWD packaged launch without a Rust edit. Covers R13-R15.
- AE5. A real platform accessibility action for the presented Retry node produces the same typed
  command trace as keyboard activation. Covers R5 and R16.

### Scope Boundaries

#### Included

- The bounded HUD and read-only terminal/Retry overlay in
  `docs/architecture/reference-game-ui-product-slice.md`.
- Product adapter, first-party presenter, source-data, package and focused verification changes
  needed to close R1-R17.
- Removal or migration of obsolete product fixtures, scripts and hash/matrix assertions that no
  longer own an independent contract.

#### Deferred to Follow-Up Work

- Editable text, IME, localization, project-font authoring, scroll, virtualization, drag/drop and
  animation require separate product slices.
- Editor Domain, egui shell, Viewport and Nara UI Inspector parity remain independent successor
  lanes.
- Generic binding DSLs, widget registries, docking, native multi-window policy, RenderGraph and
  importer/provider registries are not admitted.

## Planning Contract

### Key Technical Decisions

- KTD1. Use one product trajectory plus focused boundary cases rather than a state-by-target test
  matrix. (session-settled: user-directed — chosen over game-shaped interface design and broad smoke:
  examples should prove modules without becoming their authority.) Governs R4-R12.
- KTD2. Keep component fixtures that isolate a real contract, but label them component evidence and
  require the ordinary first-party backend path for Product-Validated claims. Governs R2, R4 and
  R15-R16.
- KTD3. Assert structured layout, semantics, focus, typed outcomes and exact identity instead of
  opaque digests. Small image or snapshot artifacts may aid diagnosis but cannot close a requirement.
  (session-settled: user-approved — chosen over SHA-heavy proof because hashes hide the violated
  contract.) Governs R9-R12.
- KTD4. Keep Scene/Prefab as the static authoring source and use game-specific typed fact projection;
  do not introduce a generic binding language in this slice. (session-settled: user-approved — chosen
  over premature shared binding because a second complete consumer has not exposed the pattern.)
  Governs R13-R14.
- KTD5. A shared defect follows `RUP suspended -> focused UIF correction -> successor baseline ->
  explicit RUP rebase`. Historical evidence remains immutable. Governs R1 and R3.
- KTD6. Collect low-overhead timing and resource observations in the existing product path; do not
  add a benchmark framework or CI performance gate before the first representative samples exist.
  Governs R17.

### High-Level Design

```text
Scene/Prefab source
        |
        v
Reference Game typed facts -> Runtime UI adapter -> immutable candidate
        |                                             |
        |                                             v
        +------------------------------------ backend-accepted packet
                                                      |
                                                      v
platform input/accessibility -> exact receipt -> typed Retry command
                                                      |
                                                      v
                                           successor run generation

The same source and adapter feed desktop, headless and packaged products.
```

### Sequencing

RUP-U0 reconciles the existing work-in-progress fixture and freezes a truthful product test surface.
RUP-U1 closes the behavioral trajectory. RUP-U2 closes authoring and delivery. RUP-U3 publishes the
Product-Validated evidence and metrics. Later units may not compensate for an earlier missing
authority invariant with broader smoke.

## Implementation Units

### RUP-U0 — Product authority and fixture reconciliation

**Status:** Active. This is the sole active unit.

**Goal:** Convert the existing Reference Game UI work-in-progress into a compilable, focused product
test surface whose claims match R1-R12.

**Requirements:** R1-R3, R8-R12.

**Files:** `reference-game/tests/ui_product.rs`,
`reference-game/tests/support/ui_product_fixture.rs`,
`reference-game/tests/support/ui_product_fixture/**`, `reference-game/README.md`, and the product
contract or migration documentation when a claim changes.

**Approach:** Preserve useful structural and behavior helpers. Remove full Cartesian expansion,
opaque hashes, duplicate packet/glyph construction and debug-string semantic matching. Use scale
`1.0` for normal cases and one named `1.25` pointer case. Keep direct test presenters labeled as
component fixtures; do not count them as backend product proof.

**Test scenarios:** Six bounded visual/behavior states at the canonical target, three target
readability captures for one representative state, one fractional pointer case, structured HUD and
terminal facts, safe margin and modal geometry, focus/semantic identity, pending repeat rejection
and applied removal.

**Verification:** `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test
ui_product --test-threads=1` and direct diff review for hash, Cartesian and hidden-presenter claims.

**Exit:** The product test compiles, every assertion maps to a named requirement, and no fixture is
misrepresented as desktop/backend or platform-accessibility evidence.

### RUP-U1 — One exact gameplay-to-Retry trajectory

**Depends on:** RUP-U0.

**Goal:** Prove one runtime trajectory from authored HUD through terminal activation to a successor
run generation with exact presented-session semantics.

**Requirements:** R4-R8, R13 and R16.

**Files:** `reference-game/src/runtime_ui.rs`, gameplay Retry owners, the focused product fixture,
first-party desktop/accessibility presenter glue and owning tests.

**Approach:** Drive one runtime through gameplay facts and terminal state. Route pointer, keyboard,
gamepad and platform accessibility through the same typed intent boundary. Make pending, rejection
and application observable without constructing a new world for each transition.

**Test scenarios:** Completed and Defeated terminal entry, initial focus, concurrent activation,
pending repeat activation, rejected replacement, applied replacement, pause-safe journal retention,
stale receipt rejection and underlay/gameplay blocking.

**Verification:** Focused Reference Game nextest, one first-party backend-presented desktop probe and
the platform accessibility test path. Component fixtures remain supplementary.

**Exit:** Each input modality emits exactly one typed command against a matching receipt and the
next run generation appears only after its successor presentation commits.

### RUP-U2 — Authoring, surface and packaged delivery closure

**Depends on:** RUP-U1.

**Goal:** Prove that ordinary project data and one Runtime UI adapter survive source editing,
physical mapping, headless execution and packaging.

**Requirements:** R9-R17.

**Files:** `reference-game/prefabs/reference-ui.prefab.json`, source-data roundtrip fixtures,
`reference-game/src/bin/desktop_render_probe.rs`, `reference-game/src/bin/desktop_support/**`,
`reference-game/packaging/package-layout-v1.json`, `reference-game/tools/package.py`, package policy
tests and focused product tests.

**Approach:** Use an isolated project copy for the authoring roundtrip. Change source data through
the ordinary loader/canonical writer path, then restart or reload. Reuse the same adapter for
desktop, headless and package modes. Exercise all three 16:9 targets, one 4:3 physical resize and one
fractional pointer case without multiplying the full state matrix.

**Test scenarios:** Safe-area readability, non-overlap and text bounds; letterbox black-bar input;
source label or geometry change; desktop/WGPU glyph and packet residency; presentation-free
headless facts; arbitrary-CWD packaged launch; missing resource and font closure failure.

**Verification:** Serial focused nextest, one real desktop/WGPU presentation probe, the existing
cross-platform package tool, arbitrary-CWD launch and headless product gate. Record exact commands
and which assertions are automated or bounded observations.

**Exit:** Desktop, headless and package consume the same changed source data and adapter, and each
target has structured evidence rather than a digest-only pass.

### RUP-U3 — Product-Validated baseline and handoff

**Depends on:** RUP-U2.

**Goal:** Publish immutable Runtime UI product evidence and close the preserved NUI-U15B obligation.

**Requirements:** R1-R17.

**Files:** engineering-memory verification/registration shards, ADR implementation ledger, current
product contract and derived state.

**Approach:** Record the exact commit, baseline lineage, automated gates, bounded desktop
observations, public/private boundary inventory, authoring edit, package layout and representative
timing/resource samples. Delete abandoned experiments and obsolete fixture/script paths before the
final commit.

**Verification:** Rerun the focused product, desktop, headless and package gates from a clean index;
run `cargo fmt --all -- --check`, scoped strict clippy and engineering-memory validation/render
checks. Perform simplification and Standards/Spec code review on the final delta.

**Exit:** One immutable Product-Validated evidence record closes Runtime UI product acceptance,
updates the high-authority ledger and activates no unrelated successor automatically.

## Verification Contract

- Cargo commands run serially with `CARGO_BUILD_JOBS=1`; focused nextest precedes broader gates.
- V1: `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test ui_product
  --test-threads=1` proves the focused product assertions.
- V2: `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test first_wave
  --test runtime_drive --test headless_snapshot --test headless_cli --test-threads=1` proves the
  gameplay trajectory, typed command boundary and presentation-free headless path.
- V3: `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --features desktop
  --test desktop_flow --test desktop_render --test desktop_parity --test-threads=1` plus one bounded
  `desktop_render_probe` run proves backend acceptance, packet/resource closure and matching receipt;
  an internal direct presenter cannot satisfy V3.
- V4: one platform accessibility path proves node/action mapping into the exact presented session;
  direct internal action injection cannot satisfy V4.
- V5: `python reference-game/tools/package.py create --repository-root . --platform
  windows-x86_64 --version 0.1.0 --source-revision <git-sha> --headless-binary <headless-binary>
  --desktop-binary <desktop-binary> --output <candidate.zip>` creates the candidate, which is then
  launched from an unrelated working directory while the V2 headless path consumes the same source
  data.
- V6: the authoring roundtrip changes source data without Rust changes and is observed by desktop,
  headless and package paths.
- V7: `cargo fmt --all -- --check`, scoped strict clippy, `git diff --check`, engineering-memory
  validation and deterministic rollup rendering pass.
- V8: structural assertions and exact identities are authoritative. Cartesian state/target smoke and
  opaque hashes are prohibited as completion gates.

## Definition of Done

- R1-R17 are traced to executable evidence and one immutable Product-Validated record.
- The same authored structure and Runtime UI adapter drive the real trajectory, desktop, headless and
  package paths.
- Pointer, keyboard, gamepad and platform accessibility each produce exactly one typed command for
  an exact presented receipt.
- Three 16:9 targets, one fractional pointer case and one non-16:9 physical resize are proven without
  a Cartesian matrix.
- One ordinary source-data edit reaches desktop, headless and arbitrary-CWD package output without a
  Rust edit or test-private hook.
- The shared UIF-U5 public inventory remains unchanged, or a focused correction lineage explicitly
  supersedes it before product work resumes.
- Obsolete tests, scripts, hashes, compatibility paths and abandoned attempts in this slice are
  removed rather than left as uncompiled or misleading evidence.
- Final formatting, focused nextest, desktop/headless/package gates, simplification review and
  Standards/Spec review pass on the committed scope.

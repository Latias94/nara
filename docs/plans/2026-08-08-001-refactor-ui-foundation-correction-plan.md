---
title: UI Foundation Correction - Plan
type: refactor
date: 2026-08-08
deepened: 2026-08-08
amended: 2026-08-09
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution_state: active
product_contract_source: docs/architecture/ui-foundation-design.md
origin: docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md
supersedes: docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md
plan_id: ui-foundation-correction-2026-08
unit_namespace: UIF
operator_execution_authority: external-control-plane
---

# UI Foundation Correction - Plan

## Goal Capsule

- **Objective:** Re-establish a deep, renderer-neutral UI Foundation whose compile, present and
  dispatch Interfaces preserve one exact presented revision across layout, paint, hit testing,
  focus, semantics, resources and typed intent.
- **Scope:** Correct only shared contracts already claimed by the landed UI architecture. Do not
  add a general component catalogue, binding language, docking framework, multi-window policy,
  virtualization engine, or second public renderer topology.
- **Product order:** Runtime UI product acceptance is paused until this plan publishes a new
  Foundation Conformance Baseline. The Reference Game remains a late acceptance carrier and may
  provide neutral reproductions, but it cannot design a shared Interface.
- **Authority:** This is the sole active UI execution plan. The predecessor remains immutable
  historical evidence. The 2026-08-09 contract audit reopened focused corrective units without
  rewriting the immutable UIF-U2/UIF-U3 evidence. Successor plans are inactive until UIF-U5
  publishes completion evidence.
- **Execution profile:** Nara is pre-1.0. Prefer owning-module fearless refactors and deletion of
  obsolete paths over compatibility wrappers. Keep Cargo commands serial with one build job.

## Product Contract

### Shared Interface

```text
compile(authoring_snapshot, environment, limits)
    -> Compiled(UiFrameCandidate) | CompileRejected(diagnostics)
plan(candidate, target, capability_generation)
    -> Planned(ResourceCompletePacket) | ContentRejected
prepare_submission(packet, surface_slot_generation, presentation_epoch)
    -> PreparedPresentationAuthority | AuthorityRejected
submit(PresentationRequest {
    packet, prepared_authority,
    environment_revision, capability_generation, gpu_generation,
})
    -> Presented(BackendPresentationCommit)
     | Deferred(retry_identity)
     | BackendFault
commit(BackendPresentationCommit) -> UiPresentationReceipt // infallible
dispatch(presented_receipt, input_event)
    -> Consumed | PassThrough | RejectedClosed + typed intents
```

The exact Rust names may evolve during implementation, but the following facts are durable:

1. A candidate is immutable and revisioned. Root-local failures are candidate outcomes; global
   identity, snapshot-profile or shared-capacity failures return `CompileRejected` without a
   candidate. An unsafe modal fallback is the explicit safety exception: it rejects the candidate
   rather than publishing a visible-but-penetrable modal.
2. The snapshot records the exact authoring profile used to validate roots. The compiler rejects a
   different profile before frame revision, cache clock, retention or least-recently-used state can
   change. Compile-only and session-only budgets are not part of that profile.
3. Candidate text input work is reserved before a unique shaping attempt. Failed deterministic
   layouts consume that work once and are cached within the candidate; shaped-glyph output is
   accounted separately from source bytes or Unicode scalar count.
4. All fallible UI authority staging and input-route reservation complete before backend submit. A
   receipt is returned only by consuming the backend-issued, non-cloneable commit for the exact
   packet, target, Surface Slot, environment, capability and GPU generations. Its final authority
   swap is infallible; consumers and journals cannot construct or advance receipt authority directly.
5. Only that receipt may route pointer, keyboard, gamepad or platform accessibility actions.
6. `RejectedClosed` changes no session, widget reducer or intent-journal state. Geometry/capture
   reconciliation is represented as a successful consumed transition when it must mutate state.
7. `PassThrough` leaves UI state unchanged and explicitly permits gameplay routing.
8. `ContentRejected` and `Deferred` do not replace last-presented state. A deferred retry is valid
   only for its exact generations and becomes `Superseded` when a newer attempt for that slot is
   admitted. Target/surface faults retire only their target or slot; device loss retires the GPU
   generation and every retry bound to it.

### Foundation invariants

- Root-local authoring failure cannot discard valid sibling roots. Global snapshot limits and
  identity errors remain whole-snapshot failures.
- A compatible last-good root stores relative root-local facts. Current layer and structural order
  are rebound from the current root envelope; stale global paint coordinates are never reused.
- Pointer position, focus, capture, semantic target and route stamps are tied to a presentation
  and geometry revision. A resize or scale change cancels stale capture and cannot reuse old
  coordinates.
- A semantic snapshot is read-only. It cannot invent or publish focus that the presented session
  did not commit.
- Glyph, primitive, node, atlas and upload limits are enforced without resetting the allowance for
  each text node or root. The owning budgets are deliberately split: kernel compilation owns
  candidate-wide node, shaped-glyph and logical-paint admission; the presentation planner owns
  target-plan-wide expanded primitives, referenced resources, atlas memory and packet retention; the
  concrete backend owns device upload, instance-buffer and residency work. First-party authoring
  capabilities close before packet acceptance. This baseline admits one bounded uniform corner
  radius for fill, image and border; the planner closes it into renderer-neutral rect facts and the
  first-party WGPU adapter must render it. Asymmetric radii remain deferred.
- Each Surface Slot has a stable identity, endpoint binding, route/presentation/geometry generation,
  retirement generation and one exact `SurfaceContentTransform`. UIF exposes only the current
  all-roots authoring scope and current theme/font environment; generic root selection and per-surface
  theme choice remain private/deferred until render-target UI or an Editor Viewport consumes them.
  Multiple surfaces do not overwrite one another through map iteration.
- During this Foundation baseline one presentation target admits at most one active Surface Slot.
  Multiple slots are supported only when they have distinct presentation targets; target aggregation,
  compositing and viewport fan-in are deferred to a named successor.
- A Surface Slot follows `Active(transform) -> Suspended(reason, generation) -> Active(successor)`
  or `Retired(tombstone)`. Zero extent, missing endpoint and invalid scale suspend packet, route,
  pointer/capture and accessibility authority instead of silently retaining an old receipt. A
  suspended native endpoint may retain physical held-edge ownership so real releases remain balanced;
  retirement consumes that ownership and cannot be resumed.
- Platform accessibility actions, if enabled for a product, enter the same exact presented-session
  transaction as normalized input. A platform-neutral semantic action remains a valid Foundation
  proof without claiming OS adapter acceptance.
- A current modal barrier is envelope state, not cached root-body state. If a modal body fails but a
  compatible last-good visual exists, the visual is inert while the current barrier still blocks
  underlay UI and gameplay. A modal failure without a valid barrier/fallback rejects the candidate
  and preserves the prior presented session.

### Budget failure domains

Budget dimensions are not silently truncated and are not re-granted per root or packet. Allocation
and failure scope are deterministic in current structural order, then paint/resource key order:

| Owner | Dimensions | Admission point | Failure disposition |
|---|---|---|---|
| `nara_ui_kernel` authoring profile | root count, source-node count, depth and key bytes | snapshot construction and compiler profile preflight | whole-candidate `CompileRejected` before revision/cache/LRU mutation |
| `nara_ui_kernel` compile budget | attempted unique text bytes/requests, candidate nodes, shaped-glyph output, logical paint, diagnostics/cache | reserve attempted work before shaping; account output after successful layout; tentative candidate compilation | whole-candidate `CompileRejected` before revision/cache/LRU mutation |
| `nara_ui_render` planner | expanded primitives, referenced images/glyphs, atlas pages/bytes, retained packet bytes | target-plan preflight and staged resource planning | `ContentRejected`; last-presented packet and planner state remain unchanged |
| concrete backend | device upload bytes, instance-buffer bytes, residency/queue work | backend frame submission | `Deferred` when retryable, `BackendFault` for device failure; no receipt advances |

Root-local authoring errors remain isolated until a shared candidate budget is exhausted. There is
no partial target presentation and no order-dependent silent truncation. A retryable backend budget
may make residency progress, but it never publishes a receipt until the complete packet is accepted.

### Root failure matrix

| Root state | Compatible retained visual | Barrier required | Result |
|---|---|---|---|
| non-modal local failure | yes | no | inert `RetainedVisual` root outcome |
| non-modal local failure | no | no | `Absent` root outcome |
| modal local failure | yes | yes | inert retained visual plus current envelope barrier |
| modal local failure | no | yes | whole-candidate `CompileRejected::ModalFallbackUnavailable` |
| identity/profile/shared-capacity failure | irrelevant | irrelevant | whole-candidate `CompileRejected` |

### Foundation baseline lineage

UIF-U5 publishes a **Foundation Conformance Baseline**, not a claim of product validation. Runtime
and Editor successors pin its immutable record. If either exposes a shared defect, that product plan
is suspended, a focused UIF correction publishes a successor baseline, and the product explicitly
rebases before resuming. Historical evidence is never edited. A **Product-Validated Baseline** may be
published only after a complete product workflow consumes a pinned conformance baseline.

## Reference pressure policy

The [UI Foundation Design](../architecture/ui-foundation-design.md) pressure matrix is required
evidence, not implementation authority. Open-GPUI supplies stable-key, frame-journal, focus,
virtualization and logical-docking pressure; Dear ImGui supplies Editor adapter and viewport
failure pressure; Godot supplies Control and author workflow pressure; Bevy supplies layout/text/
controlled-widget/render algorithm pressure. Unity remains a secondary organization analogy until a
primary source/version and neutral assertion are pinned. Nara rejects external lifecycle, identity,
global-service and public render-topology models.

## Planning Contract

### Execution order

```mermaid
flowchart LR
    Evidence[Immutable UIF-U0 through UIF-U3 evidence]
    Audit[2026-08-09 contract audit]
    U2C[UIF-U2C Profile and text-work correction]
    U3C[UIF-U3C Submission authority correction]
    U4C[UIF-U4C Surface suspension and platform closure]
    U5[UIF-U5 Refreeze]
    Runtime[Inactive Runtime UI product successor]
    Editor[Inactive Editor successor]

    Evidence --> Audit --> U2C --> U3C --> U4C --> U5
    U5 -. product priority .-> Runtime
    U5 -. product priority .-> Editor
```

Execution remains operationally serial. The original UIF-U2/UIF-U3 evidence stays immutable, while
the corrective sequence supersedes only their completion claims. UIF-U2C is complete and UIF-U3C is
the sole active unit; UIF-U4C and UIF-U5 remain inactive. UIF-U5 admits exactly one product successor as the next
active plan; the other remains independently eligible against the same pinned conformance baseline.

### Alternatives considered

1. **Continue the 16-unit master plan.** Rejected because Foundation correction, product
   acceptance and final cleanup share one closure point and repeatedly reopen one another.
2. **Finish U15B and record Foundation defects as debt.** Rejected because the confirmed defects
   violate the exact-session, root-local failure and resource-bound claims that U15B would consume.
3. **Create one public crate per lifecycle owner now.** Rejected because internal seams are enough
   until an independent dependency closure or second adapter proves a real crate seam.
4. **Sequential Foundation plan with inactive product successors.** Accepted because it preserves
   one active authority while making each shared Interface independently reviewable.

### Risks and mitigations

| Risk | Mitigation |
|---|---|
| Foundation scope expands into a widget framework | UIF units admit only existing claimed invariants; product breadth remains in successors |
| Refactor invalidates useful product WIP | Preserve the WIP unstaged, migrate only after UIF-U5, and do not claim it as evidence |
| Internal module split becomes shallow indirection | Test through the three external Interfaces and keep private seams replaceable |
| Resource closure introduces excessive work | Use one target-frame budget and one representative backend proof, not repeated product matrices |
| Authority drifts again | Every unit publishes an immutable registration/evidence successor and renders derived rollups |

## Verification Contract

| Gate | Command or evidence | Applies to |
|---|---|---|
| V1 Interaction owners | `cargo nextest run --locked -p nara_ui_kernel -p nara_ui_widgets -p nara_ui --test-threads=1` | UIF-U1 |
| V2 Root compiler | `cargo nextest run --locked -p nara_ui_kernel --test compile_order --test failure_atomic --test validation --test profile --test text_budget --test-threads=1` | UIF-U2/UIF-U2C |
| V3 Presentation owners | `cargo nextest run --locked -p nara_ui_kernel -p nara_ui -p nara_ui_render -p nara_render_wgpu --all-features --test-threads=1` | UIF-U3/UIF-U3C |
| V4 Surface/platform adapter | Serial focused nextest: `cargo nextest run --locked -p nara_input --lib --test-threads=1`; `cargo nextest run --locked -p nara_ui_kernel --test interaction --test-threads=1`; `cargo nextest run --locked -p nara_ui_widgets --test button --test-threads=1`; `cargo nextest run --locked -p nara_ui --lib --test-threads=1`; `cargo nextest run --locked -p nara_winit --features accessibility --test accessibility --test-threads=1`; root Host bridge tests for suspend/restore and submit/commit races | UIF-U4/UIF-U4C |
| V5 Neutral examples | `cargo run --locked -p nara_ui_kernel --example layout_gallery -j 1`; `cargo run --locked -p nara_ui --example interaction_lab -j 1`; `cargo run --locked --example ui_presentation_gallery --features presentation-ui,desktop-winit,render-wgpu -j 1` | UIF-U5 |
| V6 Independent consumer | `cargo nextest run --manifest-path ui-module-consumer/Cargo.toml --locked --test-threads=1` | UIF-U5 |
| V7 Compile/lint/format | serial locked check, changed-target `cargo clippy --no-deps` with `-D warnings`, and `cargo fmt --all -- --check` | Every code unit; full shared gate at UIF-U5 |

Documentation-only UIF-U0 uses link/frontmatter inspection plus engineering-memory validation. No
Reference Game product matrix is required by UIF-U0 through UIF-U4.

### Unit index

| Unit | Owning seam | Depends on | Stop condition | Successor |
|---|---|---|---|---|
| UIF-U0 | Architecture authority | predecessor evidence | More than one active plan/registration remains | UIF-U1 |
| UIF-U1 | Presented-session dispatch | UIF-U0 | A rejection must mutate uncommitted UI state to stay correct | UIF-U2 |
| UIF-U2 | Snapshot/compiler boundary | UIF-U0 | Root-local failure cannot coexist with global identity safety | UIF-U3 |
| UIF-U3 | Presentation planner/backend | UIF-U2 | Legal authoring requires submit-time live World/asset reads | UIF-U4 |
| UIF-U4 | Host Surface Slot/platform adapter | UIF-U1 and UIF-U3 | Platform identity must enter the kernel or two surfaces share mutable state | UIF-U5 |
| UIF-U2C | Snapshot profile and attempted text work | 2026-08-09 audit | Builder/compiler profile can disagree or failed shaping work escapes the shared budget | UIF-U3C |
| UIF-U3C | Prepared submission authority | UIF-U2C | GPU submission can succeed before receipt/route authority is guaranteed commit-ready | UIF-U4C |
| UIF-U4C | Surface suspension and platform closure | UIF-U1, UIF-U3C and started UIF-U4 work | Zero/missing endpoints silently retain old receipt or platform authority | UIF-U5 |
| UIF-U5 | Public neutral consumers | UIF-U1, UIF-U2C, UIF-U3C and UIF-U4C | A conformance Interface still has an unresolved P0/P1 owner defect | one product successor |

## Implementation Units

### UIF-U0 — Authority rebaseline

**Depends on:** completed NUI evidence and the superseded master plan.

**Goal:** Replace the 16-unit cross-domain execution authority with this Foundation plan and register
the Runtime UI and Editor successor plans without pretending they are active.

**Files:** active plan, roadmap, architecture map, ADR 0102 and focused successor ADRs, ledger,
engineering-memory registration/evidence.

**Verification:** Direct frontmatter/link inspection, no stale active registration and reciprocal
`supersedes` links. The predecessor plan permits this one controlled frontmatter authority transition
to `superseded`; its body and all historical evidence remain immutable.

**Exit:** One active UIF registration points here; the predecessor is `superseded`; successor plans
are inactive and reference this plan.

### UIF-U1 — Atomic presented-session interaction

**Depends on:** UIF-U0.

**Goal:** Fix the owning transaction/session contract and refreeze disabled-focus identity.

**Files:** `crates/nara_ui/src/interaction/**`, `crates/nara_ui_kernel/src/interaction/**`,
`crates/nara_ui_widgets/**`, neutral interaction lab, focused tests and the migration guide for
the intentional transaction/input break.

**Required cases:** rejected disabled/retired/stale routes leave session, reducer and journal
unchanged; consumed reconciliation commits capture/hover cleanup atomically; resize/scale cancels
capture; disabled focus remains the navigation origin but not a destination; re-enable of the same
key restores the visual focus state, including the focused-plus-disabled visual composition; semantic
snapshots report only committed focus. A disabled current item remains the navigation origin while
enabled candidates alone are destinations, including middle-item sequential and directional cases.
Publishing a newer presented frame reconciles removed or changed controls before their stable keys can
be reused, so capture, focus and reducer state cannot survive a remove/reinsert cycle. When a route has
pending lifecycle reconciliation, that cleanup commits before an otherwise rejected input is
classified; unsupported input cannot postpone required cleanup.

**Verification:** owning nextest, the public interaction lab, and a focused regression for each
disposition. No Reference Game matrix.

**Exit:** A new immutable evidence record names the exact transaction and focus assertions.

### UIF-U2 — Root envelopes and order-safe fallback

**Depends on:** UIF-U0. It executes after UIF-U1 but does not consume interaction implementation.

**Goal:** Make root-local validation and fallback independent while preserving current structural
order.

**Required cases:** one malformed root does not suppress valid roots; an invalid root body contributes
no hit/focus/semantics/action; a last-good root rebinds current layer/sequence after reorder or layer
change; global identity/limit errors remain fail-closed. Current modal membership and barrier policy
belong to the envelope rather than cached body facts: an inert retained modal still blocks underlay UI
and gameplay, while a modal with no valid barrier/fallback rejects the candidate and preserves the
previous presented session. Every whole-candidate rejection retires any older unpresented candidate
for that surface; only an already committed presented session may remain authoritative.

**Verification:** kernel compile/order nextest, a neutral multi-root example, and an explicit
reorder-while-invalid regression.

**Exit:** The compiler exposes root-local outcomes through the existing candidate boundary without
leaking normalized IR or cache types.

### UIF-U3 — Presentation planner and capability closure

**Depends on:** UIF-U2.

**Goal:** Make resource ownership and limits truthful at the presented-frame boundary.

**Required cases:** kernel candidate-wide node/shaped-glyph/logical-paint budgets and planner
target-plan-wide primitive/resource/atlas/retention budgets are each enforced once; packet-owned
image and glyph snapshots survive backend submission; bounded uniform radii for fill/image/border are
closed by the planner and rendered by WGPU; staged planner state and the last-presented packet are
unchanged on `ContentRejected`; backend upload/residency work may return `Deferred` only with an exact
retry identity and without advancing receipt authority; `BackendFault` retires its GPU generation.
Text measurement cannot multiply expensive shaping across callbacks before applying the shared
candidate allowance. Only a backend-issued one-shot presentation commit can publish a receipt.

**Verification:** `nara_ui_render`/WGPU focused nextest, one neutral presentation gallery, and one
representative readback. No repeated product hash matrix.

**Exit:** Presentation planner owns the complete packet/resource/capability decision and the backend
does not re-read live gameplay assets.

### UIF-U4 — Surface slots and accessibility ingress

**Depends on:** UIF-U1 and UIF-U3.

**Goal:** Make surface identity and platform action ownership explicit without coupling the kernel to
Winit or AccessKit.

**Required cases:** a precise `SurfaceContentTransform` defines platform-logical, physical-surface,
physical-content and authored-logical coordinates, safe-area ordering, forward/inverse mapping and
outside-content behavior; one active slot per presentation target; transform changes cancel stale
pointer/capture state without tearing held physical edges, and a release that began before logical
route invalidation cannot be claimed by a successor route. True endpoint retirement synthesizes
balanced releases and clears all endpoint state. A coordinated retirement admits its complete
lifecycle batch before changing either input or UI ownership; capacity/sequence failure leaves the
Surface Slot, route and physical state unchanged, while a platform-observed endpoint loss remains
fail-closed. Geometry invalidation is scoped to the exact
Surface Slot lineage, so a newer slot may restart its presentation epoch without inheriting an old
stale-geometry watermark. Multiple distinct targets do not overwrite focus or routes. Platform
accessibility actions become receipt-stamped typed events before dispatch; semantic actions are
advertised only when a controlled behavior owner resolves them. Platform tree publication and its
callback mapping commit form one exclusive synchronous transaction with an infallible platform
commit arm: a stale, foreign or retired candidate cannot reach the platform or replace the last
committed mapping. Generic root selectors, per-surface theme selection and target aggregation remain
deferred.

**Verification:** kernel surface lab, Winit adapter test where the feature is enabled, and a two-
surface route isolation regression.

**Exit:** Host owns endpoints and platform lifecycles; kernel sees only backend-neutral Surface Slot
facts and exact receipts.

### UIF-U2C — Snapshot profile and attempted text-work correction

**Status:** Complete at commits `e6070bc` and `3dab804`, with immutable verification in
`docs/knowledge/engineering/verification/2026-08/2026-08-09T035823Z-uif-u2c-snapshot-profile-and-text-work-correction-dda1efec999b40e98d6f38fac067503b.md`.
This unit supersedes the affected completion claim in UIF-U2 evidence without editing that evidence.

**Depends on:** the 2026-08-09 contract audit and immutable UIF-U2 evidence.

**Goal:** Make authoring admission and compiler interpretation share one exact, owner-specific
profile while bounding every unique shaping attempt, including deterministic failures.

**Required cases:** `UiAuthoringSnapshot` records the exact root/source-node/depth/key profile used
by its builder; the compiler compares that profile before revision/cache/LRU mutation; stricter-to-
wider and wider-to-stricter mismatches both reject; diagnostics, paint, cache and session budgets do
not create false profile mismatches. Candidate text work reserves unique source bytes before calling
the text engine, caches deterministic failed requests for the candidate, and accounts successful
shaped-glyph output separately. Multiple distinct `LineLimitExceeded` roots eventually exhaust the
text-work budget, while repeated identical failure does not repeatedly shape or charge. Rejection
preserves frame revision, compatible last-good roots and cache order. The modal failure matrix above
is the only whole-candidate exception to ordinary root-local content isolation.

**Verification:** V2 plus focused failure-atomic regressions for both profile mismatch directions,
distinct failed text requests, repeated identical failure, and last-good retention after rejection.

**Exit:** A snapshot cannot be interpreted under a different authoring profile, and no expensive
text attempt can bypass the candidate-wide work budget through a root-local error.

### UIF-U3C — Prepared submission authority and infallible commit

**Status:** Active. This unit supersedes the affected completion claim in UIF-U3 evidence without
editing that evidence.

**Depends on:** UIF-U2C and immutable UIF-U3 planner/backend evidence.

**Goal:** Ensure that successful backend submission can only be followed by an infallible authority
commit for the exact packet and generations the backend accepted.

**Required cases:** before GPU submit, the UI runtime stages the exact candidate/session/reducer/
journal replacement and reserves every fallible input-route capacity, conflict and generation fact.
The backend receives a prepared attempt bound to packet, target, slot, environment, capability and
GPU generations. A non-cloneable backend commit consumes that exact attempt; the final presented-
session and route swap cannot fail. Public consumers cannot call a frame-plus-epoch receipt minting
function, and the kernel journal cannot independently mint receipt authority. Each slot follows
`Prepared -> Submitted -> Accepted -> Committed`, with `Aborted` and `Superseded` terminal paths;
a deferred older attempt cannot revive after a newer attempt succeeds. Target/surface failure and
device loss retire only their owning fault domain.

**Verification:** V3 plus a Host bridge race regression proving that backend acceptance has no
fallible post-submit arm, superseded deferred work cannot commit, and target failure does not retire
an unrelated GPU generation or Surface Slot.

**Exit:** A submitted pixel generation and the input/semantic receipt generation cannot diverge.

### UIF-U4C — Surface suspension and platform authority closure

**Status:** Inactive until UIF-U3C closes. The already committed UIF-U4 endpoint-retirement and
recording-accessibility work remains valid input to this unit.

**Depends on:** UIF-U1, UIF-U3C and commit `e0948b5`.

**Goal:** Complete the Host-owned Surface Slot lifecycle for temporarily unavailable endpoints and
finish platform publication semantics without retaining stale authority.

**Required cases:** the Host owns a unique target-to-slot index. Zero extent, endpoint missing or
invalid scale moves an active slot to `Suspended`, removes its presentation route and accessibility
tree, invalidates stale receipt/geometry/capture facts, and does not submit an old packet. Restore
allocates successor generations and requires fresh presentation before input or semantics resume.
Suspension preserves only the physical held-edge ownership needed for balanced real releases; true
retirement synthesizes the appropriate releases exactly once and writes a permanent tombstone.
When a slot migrates from endpoint A to endpoint B, every held edge remains owned by the consumer
that received its press on A until a balanced release or explicit retirement cleanup completes; B
cannot silently absorb or clear predecessor ownership. Distinct targets remain isolated.
`WinitAccessibilityPublisher` proves only exact Host-owned tree and mapping record commit. An incoming
platform action must carry the exact publisher, Surface Slot, publication generation and presented
receipt that produced its node mapping; stale-generation and foreign-publisher node identities fail
closed instead of being restamped onto current behavior. Any future real `accesskit_winit` adapter
stays internal and may strengthen, but never weaken or bypass, that receipt meaning.

**Verification:** V4, minimize/restore and missing-window Host bridge tests, stale receipt rejection,
endpoint A-to-B held-edge balancing, distinct-target isolation, exact accessibility record retirement,
stale/foreign accessibility action rejection and non-16:9 letterbox mapping.

**Exit:** Every active, suspended and retired endpoint has one explicit authority state; no skipped
Host branch leaves a stale route, receipt, pointer transform or accessibility tree authoritative.

### UIF-U5 — Neutral refreeze and handoff

**Depends on:** UIF-U1, UIF-U2C, UIF-U3C and UIF-U4C.

**Goal:** Publish a Foundation Conformance Baseline that is safe for product integration without
claiming product validation.

**Verification:** `layout_gallery`, `interaction_lab`, `ui_presentation_gallery`, UI module consumer,
focused shared nextest, serial formatting/lint, and direct public-inventory audit. The baseline
record includes the exact commit, public inventory, invariants, assertions, reference pressure
matrix revision, and predecessor record it supersedes.

**Exit:** Close UIF and activate exactly one product successor against the pinned conformance record.
Runtime UI is the default next delivery priority; Editor remains independently eligible and is not a
technical descendant of Runtime UI.

## Non-goals and admission triggers

- No complete Editor shell, docking, detached native windows, or multi-viewport implementation.
- No generic binding/expression language, importer/provider registry, RenderGraph, RenderApp,
  virtualization framework, or second official author language.
- No broad widget implementation merely to fill a catalogue. A new shared widget needs a named
  invariant, a real workload, and either independent comparative pressure or a second heterogeneous
  consumer.

## Definition of Done

- UIF-U0/UIF-U1 plus UIF-U2C through UIF-U5 have immutable evidence and one active successor
  registration; original UIF-U2/UIF-U3 evidence remains immutable historical input.
- Three deep Interfaces remain free of ECS, Host, WGPU and toolkit identity leakage.
- Every UIF-U1, UIF-U2C, UIF-U3C and UIF-U4C failure scenario is fail-closed and tested through the
  owning seam.
- No stale active plan, orphan test, compatibility alias, duplicate order authority, live submit-
  time asset read, or redundant Cartesian/hash smoke remains in the affected path.
- The Runtime UI and Editor plans can pin the conformance baseline. A shared defect follows the
  suspend/correct/rebase lineage instead of rewriting the baseline or silently widening a product plan.

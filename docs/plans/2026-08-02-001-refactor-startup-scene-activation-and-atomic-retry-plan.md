---
title: Startup Scene Activation and Atomic Retry - Plan
type: refactor
date: 2026-08-02
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution_state: superseded
product_contract_source: active-plan-stop-condition
execution: code
origin: docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md
supersedes: docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md
successor: docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md
plan_id: startup-scene-activation-atomic-retry-2026-08
unit_namespace: SRT
operator_execution_authority: external-control-plane
---

# Startup Scene Activation and Atomic Retry - Plan

## Goal Capsule

- **Objective:** Close the concrete seam that stopped RGS-U4: retain the actual startup scene and
  instance receipt through candidate Startup, extend the existing scene transaction with bounded
  product initialization and exact extra retirement, then finish the reference game's one-spatial-
  authority and authoring/desktop proof. First close the independently confirmed image-revision and
  unconsumed-invalidation defects so the product proof cannot silently render stale content or
  retain an ever-growing event log.
- **Authority:** Accepted ADRs and the implementation ledger remain higher authority. ADR 0089 stays
  Proposed; this plan authorizes only its focused one-startup-scene/Retry Trial.
- **Inherited evidence:** RGS-U1 through RGS-U3 remain complete at their recorded revisions. This
  successor does not reopen runtime hierarchy ownership or completed 2D transform propagation.
- **Execution profile:** Fearless pre-1.0 refactoring is authorized. Delete the reference game's
  hand-copied reset template, duplicate spatial fields, inferred projectile ownership, and any
  superseded helper instead of preserving parallel paths.
- **Stop conditions:** Stop and re-plan if correctness requires a raw `World` callback, a provider
  registry, a general scene session/manager, a persistent format redesign beyond the reference-game
  schema reset, a fallible branch after scene authority mutation, a new schedule-abort mechanism, or
  broad acceptance of asynchronous/additive/travel semantics from ADR 0089.
- **Tail ownership:** The active execution driver owns focused implementation, serial verification,
  review follow-up, precise commits, engineering-memory closure, and push. External platform
  credentials and protected-environment authority remain outside repository text.

---

## Product Contract

### Problem

Project Host owns the exact expanded startup document while it materializes an unpublished runtime
candidate, and `nara_scene` owns a failure-atomic hierarchy-aware replacement kernel. The product
cannot currently receive the successful startup receipt and source, cannot add runtime-only values
to replacement candidates by stable scene ID, and cannot retire its exact runtime-created entities
inside the same replacement proof. The reference game therefore copies a partial live-World
template and infers projectile ownership from component shape. That loses persistent topology and
presentation, makes first startup differ from Retry, and can remove unrelated entities.

### Requirements

#### Authority And Trial Scope

- R1. Exactly one implementation-ready plan and one engineering-memory registration are active.
  The superseded RGS plan keeps its completed U1-U3 evidence and records this successor
  reciprocally.
- R2. ADR 0089 remains Proposed. The focused Trial accepts only retained startup activation input,
  a scoped replacement overlay, exact additional retirement, and the reference game's existing
  fixed-step safe point.

#### Startup Activation

- R3. Runtime materialization retains the exact expanded `SceneDocument` that was actually spawned
  and the matching successful `SpawnedSceneInstance`; it never reconstructs either from the live
  `World` or silently substitutes bundled content for the Editor's current Play document. Only one
  sealed materialize operation may consume a retained source and produce this unforgeable pair.
- R4. Retained source memory is explicit and bounded by the deterministic logical accounting model.
  Bundled startup reuses the snapshot-owned
  allocation and lease. Editor expansion receives its own checked retained-memory charge and lease
  before runtime admission; rejection publishes no candidate or uncharged retained document. A
  root-product activation resource owns the private lease guard for the complete candidate/run
  lifetime, including failed Startup, incomplete cleanup, Stop, and retirement; `nara_scene` never
  depends on Project Content budgeting.
- R5. Project Host inserts one private engine-owned activation input into the unpublished candidate
  after scene materialization and before Startup. The root product adapter promotes it into the
  read-only active startup authority; fallible product Startup systems consume that authority as an
  input without taking its retention guard. Failure prevents managed runtime publication and
  retires the candidate. Promotion, dependent initialization, and pending-input finalization use
  three ordered provisional sets inside the existing `StartupStage::Runtime`; publication never
  retains the private one-shot input, while the active authority retains the exact source and
  receipt for the complete run.
- R6. The startup activation boundary is a provisional advanced scene/product seam, not an ordinary prelude
  type, global resource locator, plugin provider, or `ProductRecipe` callback. Direct App callers
  construct a logically bounded retained source through the same advanced root materialize operation; they
  cannot spawn document A and later pair its receipt with document B. Their explicit source limit
  does not claim a `ProjectContentBudgetHost` lease.

#### Atomic Replacement

- R7. Scene spawn, authoring replacement, and product replacement share one prepare/commit kernel.
  The product path does not make raw `SceneSpawner::replace` or a mutable candidate `World` public.
- R8. A bounded owned overlay targets candidate entities only by `SceneEntityId` and accepts typed
  runtime components through a scoped writer. It exposes no raw entity map or retainable candidate
  handle and rejects missing targets, duplicate component writes, existing components, and
  lifecycle hooks/observers before old authority changes. Every accepted component type is
  registered during plugin build/seal; the runtime transaction resolves an existing `ComponentId`
  and rejects unregistered types without mutating the ECS component registry. A required
  `SceneProductTransactionLimits` value separately caps overlay writes and additional retirements
  under documented engine ceilings; zero, exact-limit, and limit-plus-one behavior is deterministic
  and capacity is checked before scratch spawn or any `World` mutation.
- R9. Product replacement declares an exact bounded set of additional runtime entities to retire.
  Duplicate, missing, scene-owned, persistent-identity-owned, hierarchy-linked, lifecycle-active,
  or otherwise ineligible entities reject before scene authority changes; component-type sweeps
  are not accepted. `nara_identity` owns the authoritative per-entity identity-axis preflight.
- R10. Any document, component decode, overlay, hierarchy, identity, membership, observer, or extra
  retirement failure leaves the old scene, identities, hierarchy, product resources, projectiles,
  and renderable state unchanged and releases scratch candidates and retained charges exactly once.
- R11. After the first authority mutation, commit has no recoverable failure branch. Candidate
  runtime insertions, relationship detach, identity replacement, exact old membership retirement,
  exact extra retirement, and prepared product-resource moves complete under exclusive `World`
  access through infallible commit tokens. A post-mutation error is not routed as a sticky schedule
  fault because later systems could observe partial publication; if implementation cannot remove
  every such branch, this plan stops instead of claiming rollback or inventing schedule abort.

#### Reference-Game Completion

- R12. Direct App, tests, headless, desktop, and Editor Play consume equivalent activation and
  replacement semantics. Editor Play proves the current expanded document, including unsaved
  in-memory edits admitted by Play, remains the Retry source for that runtime.
- R13. The reference game alone owns a private bounded run generation containing retained source,
  current receipt, exact runtime-created projectile entities, and prepared Wave resource values.
  No Nara crate exports game-specific ownership or a derived-entity registry.
- R14. One product initializer derives runtime-only health, velocity, cooldown, lifetime,
  allocator, movement, Retry, and run-generation values from authored facts. Candidate Startup and
  Retry call the same derivation; only their publication adapters differ.
- R15. `Transform2d` remains the only gameplay spatial authority. Role aggregates lose position and
  velocity, runtime/derived facts leave persistent schema, movement writes transforms directly, and
  one authored weapon child proves hierarchy through the same scene and render graph.
- R16. Existing stable asset, entity, prefab, schema, and retained field identities remain unchanged
  where semantics remain unchanged. Replaced aggregate IDs and fields are explicitly migrated,
  tombstoned, or rejected; an ID is never reused for a marker or different payload.
- R17. Every recoverable pre-commit Retry rejection publishes one bounded game-owned reason through
  `WaveRetryStatus`, consistently observable in headless, desktop, and Editor Play while the prior
  generation remains authoritative. Engine diagnostics may add context but do not replace this
  product outcome or create another Retry status authority.

#### Public Boundary And Closure

- R18. The Trial adds no async loader, additive scene, multi-instance active set, general unload,
  travel, retained-entity migration, scene service scope, Scene Manager, or public safe-point API.
- R19. Every public/provisional contract has English API documentation, bounded misuse tests, and an
  explicit ordinary/advanced/private disposition before this plan closes.
- R20. Every mutation of an image asset slot, including direct replacement with unchanged source
  metadata, dimensions, format, color space, and byte length, changes backend-neutral prepare
  identity through the existing `AssetSlotRevision`; stale GPU pixels cannot be reused.
- R21. `ImageAsset` has one fallible construction/deserialize validation path that rejects zero or
  overflowing extents and any RGBA byte length other than `width * height * 4` before render prepare.
- R22. Snapshot-key cache state is the sole image prepare invalidation authority. The unconsumed,
  unbounded `RenderPrepareInvalidations` event surface is removed rather than gaining a duplicate
  consumer or another version system.
- R23. Managed Startup and runtime faults preserve a bounded privacy-safe static diagnostic code,
  safe summary, and producer origin when an engine-owned fallible system classifies an error;
  unknown third-party errors retain the generic fallback and arbitrary dynamic text is discarded.
- R24. Complete hierarchy validation visits only entities with `Parent` or `Children` plus declared
  additions, while preserving the unchanged-generation fast path and all reverse-edge validation.

### Acceptance Examples

- AE1. A bundled project publishes one active startup authority whose source pointer shares the retained
  snapshot allocation and whose receipt resolves every spawned stable scene ID.
- AE2. Editor Play changes a child local transform in memory, starts without saving, retries, and
  restores the edited Play source rather than the bundled source.
- AE3. A product Startup initializer rejects invalid authored configuration; Project Host publishes
  no runtime and candidate retirement reaches a finite terminal state.
- AE4. A replacement overlay adds runtime-only components to player, enemy, and weapon candidates
  by stable ID without exposing their runtime `Entity` values to the caller.
- AE5. Missing IDs, duplicate overlay components, lifecycle observers, invalid hierarchy,
  zero/exact/limit-plus-one transaction inputs, and scene- or persistent-identity-owned extra
  retirements each preserve the complete prior scene and product state on rejection.
- AE6. Retry retires only projectiles recorded by the current game generation; a projectile-shaped
  entity owned by another fixture survives.
- AE7. Successful Retry restores the authored `Transform2d`, Sprite, parent relation, stable
  identity, and derived gameplay state, then publishes the next generation before simulation
  resumes.
- AE8. Initial startup and Retry produce the same normalized runtime-only values from the same
  authored source while allocating a fresh non-reused scene instance identity.
- AE9. A player movement tick changes `Transform2d`; the authored weapon local offset remains fixed
  and its completed global/extracted pose follows in the same tick.
- AE10. Scene/prefab/Export/Apply Changes contain authored configuration only; current health,
  velocity, cooldown, lifetime, projectile ownership, and `GlobalTransform2d` never serialize.
- AE11. Source and packaged headless/desktop products run from arbitrary cwd/home through ordinary
  `ProductRecipe` and run facades; no first-party smoke assembles a private Host path.
- AE12. Public-surface review finds no recipe callback, raw replacement port, candidate `World`
  callback, provider registry, scene session, or broad ADR 0089 claim.
- AE13. A rejected Retry reports one bounded `WaveRetryStatus` reason in headless, desktop, and
  Editor Play, preserves the complete prior generation, and can be retried after the cause is
  corrected without a second status authority.
- AE14. Replacing a loaded image through `Assets::insert` with identical metadata and byte length but
  different pixels changes the prepare snapshot and causes exactly one GPU reupload.
- AE15. Zero extent, overflow, undersized RGBA, and oversized RGBA fail at the `ImageAsset` boundary;
  valid direct construction, importer output, and serde input share one invariant check.
- AE16. Repeated image modification/removal leaves no retained invalidation records after prepare;
  prepared-resource snapshot replacement and frame-age eviction remain authoritative.
- AE17. A rejected startup initializer reaches Project Host with its stable engine diagnostic code
  and safe summary, while an unclassified third-party error remains the generic execution fault.
- AE18. A hierarchy with many unrelated entities validates in work proportional to parent/children
  participants and declared additions, without missing a `Children`-only invalid state.

### Out Of Scope

- Async preparation, cancellation, additive scenes, general travel, active-set revisions, multiple
  simultaneous instances, persistent-entity adoption, and scene-scoped service retirement.
- Ordered persistent hierarchy, `KeepWorld`, inherited visibility, 3D, physics, save games, C# or
  scripting, a package manager, and release-evidence infrastructure.
- A generic initializer trait, behavior host, universal transaction graph, public scene manager, or
  new crate for this product seam.

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Use a successor plan, not an in-place exception.** The RGS-U4 stop condition fired. This
  plan supersedes the remaining RGS order while preserving U1-U3 evidence and finishing its product
  objective after the focused Trial.
- KTD2. **Keep `ProductRecipe` pure.** Recipe identity, configuration, plugin and schema closure stay
  replayable before content loading. Runtime source/receipt delivery uses a one-shot candidate
  resource, not a Host callback or provider contribution.
- KTD3. **Retain and materialize one inseparable source/receipt pair.** Project Host chooses one
  `Arc`-backed, leased expanded source before spawn. A root-defined sealed materialize operation
  consumes that source, invokes scene spawn, and returns a provisional activation resource that
  privately owns the Project Content lease while exposing only matching source/receipt views to
  product Startup. Direct App uses the same operation with an explicit retained-byte limit. Retry
  never re-expands bundled content or reverse-exports the live `World`; `nara_scene` remains
  independent of root budgeting.
- KTD4. **Use Startup as the initial product boundary.** Candidate admission installs the scene and
  a private activation input; existing managed Startup runs while the runtime is still unpublished.
  The root product adapter promotes that input in `Consume`, fallible game initialization reads the
  active authority in `Dependents`, and `Finalize` rejects any private input that was not promoted.
  The active authority remains root-owned so an empty recipe or Editor session does not need a
  boilerplate ownership-transfer system and cannot accidentally drop the retention guard. No new
  App stage is required.
- KTD5. **Expose intent, not the candidate World.** A scoped overlay writer offers only typed owned
  insertion by stable scene ID. It lowers immediately into the Scene transaction's owned insertion
  plan and cannot escape the call. Overlay types must already be registered by sealed plugin
  composition; transaction preflight never changes the component registry. One explicit
  per-transaction limits value bounds overlay writes and exact additional retirements before
  scratch allocation.
- KTD6. **Extend the existing Scene kernel.** Factor one internal prepare/commit implementation used
  by spawn, authoring replace, and the advanced product transaction. Do not create a second identity
  or hierarchy replacement path.
- KTD7. **Retire an exact declared set.** Additional entities are product-owned inputs, bounded and
  validated alongside scene retirement. Any active scene or persistent identity axis rejects the
  entity through an identity-owned preflight. Hierarchy remains structural only and never implies
  ownership or recursive lifetime.
- KTD8. **Prepare everything before authority changes.** Scene components, overlay values, relation
  work, identity, membership, extra retirement, and replacement resource values all validate while
  the prior generation is authoritative. The commit token owns every insertion, detach, identity,
  retirement, and resource move and exposes no `Result` after its first authority mutation. Sticky
  fault reporting is not a substitute for this guarantee because the current schedule continues
  running after it records a fault.
- KTD9. **Reuse the existing fixed-step boundary.** Reference-game Retry remains at the start of its
  fixed simulation set before command/gameplay mutation. This does not establish a general runtime
  scene safe-point API.
- KTD10. **Keep the Trial provisional.** Activation and product replacement stay outside ordinary
  prelude and compatibility promises. SRT-U6 either classifies the minimum proven surface or makes
  it private; broad ADR 0089 remains Proposed.
- KTD11. **Budget retained Editor source explicitly.** The Editor's expanded Play source is a new
  retained allocation, not a free clone. Its checked charge lives exactly as long as the activation
  source/run and remains owned through incomplete candidate cleanup until finite retirement.
- KTD12. **Finish the game slice before further architecture.** Once the seam is proven, complete
  component/schema/content migration, visible weapon hierarchy, editor workflow, and desktop play.
  Do not start another horizontal ADR or evidence framework first.

### Reference-Game Schema Disposition

Schema generation 4 has generation 3 as its direct predecessor. Committed generation 1-3 catalog
bytes remain unchanged. Because the current migration API cannot split one aggregate record into
multiple component records, unreleased current fixtures are rewritten atomically while legacy
aggregate records fail with a stable tombstone/unsupported diagnostic rather than silently losing
fields.

| Existing ID | Disposition in generation 4 | Authoritative successor |
|---|---|---|
| `reference_game.Player` v1 | Type tombstone; do not reuse the ID as a marker. | New `reference_game.PlayerRole`, `reference_game.InitialHealth`, and `reference_game.InitialVelocity2d`; authored position moves to engine `Transform2d.translation`. |
| `reference_game.Enemy` v2 | Type tombstone; retain the historical `target` field tombstone. | New `reference_game.EnemyRole`, `reference_game.InitialHealth`, and `reference_game.InitialVelocity2d`; authored position moves to engine `Transform2d.translation`. |
| `reference_game.Projectile` v1 | Type tombstone; remove the authored startup projectile and reject legacy records explicitly. | Runtime-only projectile role/id, `Velocity2d`, damage, lifetime, and authoritative `Transform2d`; no replacement persistent projectile aggregate. |
| `reference_game.Weapon` v1 | Version-migrate in place to v2. | Retain `cooldown-ticks` and `damage`; tombstone and remove `remaining-ticks`; runtime cooldown is a non-persistent component. |
| `reference_game.WaveSpawn` v1 | Retain unchanged. | Same type and `tick` field identity. |
| Runtime health, velocity, cooldown, damage, lifetime, projectile ownership, run resources, and `GlobalTransform2d` | Intentionally unsupported by persistent schema. | Derived by the private initializer or transform pipeline for each run generation. |

The new type IDs above are new identities, not aliases for deleted aggregates. Stable asset,
scene-entity, prefab, retained field, `Name`, `Visibility`, and engine transform IDs stay unchanged.

### High-Level Design

```mermaid
sequenceDiagram
    participant Host as Project Host admission
    participant Scene as nara_scene transaction
    participant Startup as Product Startup
    participant Run as Private run generation

    Host->>Scene: materialize exact retained source
    Scene-->>Host: successful instance receipt
    Host->>Startup: promote source + receipt into root authority
    Startup->>Run: read authority; derive and publish runtime-only state
    Note over Host,Run: managed runtime is still unpublished
    Run->>Scene: Retry(source, receipt, overlay, exact retirements)
    Scene->>Scene: preflight while old instance is authoritative
    Scene-->>Run: commit new instance and retire exact old generation
    Run->>Run: infallible resource/generation publication
```

### Risks And Mitigations

| Risk | Mitigation |
|---|---|
| A generic callback mutates unrelated World state during preparation | Expose a scoped typed overlay writer only; reject lifecycle-active component insertion. |
| Editor Play retains uncharged duplicate content | Compute retained bytes before admission and let a root-private guard retain the lease through candidate/run cleanup. |
| Extra retirement invalidates hierarchy/identity proof | Reject any scene/persistent identity or structural link through owner-defined preflight and validate the union under one exclusive transaction. |
| Product inputs exceed retained transaction memory | Require separate overlay/retirement limits, enforce engine ceilings before scratch spawn, and test zero/exact/plus-one boundaries. |
| Scene commit gains a second implementation | Refactor spawn/replace around one private kernel; public wrappers supply policy only. |
| Startup authority becomes a general scene service locator | Keep the private input one-shot, expose only source/receipt views through the provisional advanced surface, and reserve replacement ownership for the focused transaction. |
| Scratch entities are mistaken for async World-free candidates | Document that they are exclusive unpublished-World state; retain ADR 0089's broader admission trigger. |
| Reference-game schema reset silently loses old fields | Classify every old ID first and add explicit migration/tombstone/rejection fixtures before rewriting content. |

---

## Implementation Units

### SRT-U1. Activate the Focused ADR 0089 Trial

- **Goal:** Replace the stopped RGS execution order with this narrow, reviewable Trial authority.
- **Requirements:** R1-R2, R18; AE12.
- **Dependencies:** Verified RGS-U3 completion at its recorded revision and independent seam audits.
- **Files:** this plan; predecessor frontmatter; ADR 0089; architecture map; implementation ledger;
  one work registration and verification record; derived engineering-memory indexes.
- **Approach:** Keep ADR 0089 Proposed, record only the Trial boundary and exclusions, point every
  active authority source to this plan, and preserve prior verification records unchanged.
- **Verification:** Direct pointer/reciprocity audit, ADR/ledger status inspection, engineering-memory
  validation/render, link checks, and `git diff --check`; no Cargo documentation test.

### SRT-U2. Correct Image Revision And Invalidation

- **Goal:** Remove two independently confirmed render-product defects before using desktop output as
  evidence: stale direct image replacement and an unconsumed invalidation log.
- **Requirements:** R20-R22; AE14-AE16.
- **Dependencies:** SRT-U1.
- **Files:** `crates/nara_asset/src/storage.rs`; `crates/nara_render/src/prepare.rs`;
  `crates/nara_image/src/{lib.rs,prepare.rs,import/**}` and tests; affected
  `crates/nara_render_wgpu` texture tests and public exports.
- **Approach:** Add existing `AssetSlotRevision` to backend-neutral prepare snapshot identity, make
  every `ImageAsset` construction/deserialize route use one fallible extent/RGBA validator, and
  delete `RenderPrepareInvalidations` plus its reason/event exports because snapshot comparison and
  frame-age eviction already own invalidation. Do not hash pixels each frame or add another version.
- **Test scenarios:** Same asset version/metadata/extent/length with changed pixels re-prepares and
  uploads once; ordinary file reload remains correct; zero/overflow/mismatched RGBA rejects across
  direct/import/serde paths; repeated modify/remove retains no invalidation records; unchanged
  snapshots reuse the GPU resource.
- **Verification:** Focused serial `nara_asset`, `nara_render`, `nara_image`, and
  `nara_render_wgpu` nextest suites; locked checks, strict changed-target Clippy, fmt, diff review,
  and unit evidence.

### SRT-U3. Deliver Bounded Startup Activation Input

- **Goal:** Carry the exact retained source and successful spawn receipt into product Startup before
  managed runtime publication.
- **Requirements:** R3-R6, R10, R12, R23; AE1-AE3, AE17.
- **Dependencies:** SRT-U2.
- **Files:** `src/project_content.rs`; `src/project_host/runtime.rs` and Editor runtime path; a
  root-defined provisional activation resource; `crates/nara_app/src/runtime/{fault_route.rs}` and
  fault types; focused root/scene/App tests and API documentation.
- **Approach:** Introduce one sealed root materialize operation that consumes an `Arc`-backed retained
  startup source with a private lease/direct-limit guard, spawns that exact source, and inserts the
  inseparable one-shot activation only after success. Install provisional ordered Startup consume,
  dependent, and finalizer sets. Carry engine-classified static diagnostic code/summary/origin
  through managed Startup and runtime fault conversion while retaining a generic third-party
  fallback. Derive the executable component registry only from the candidate World, permanently
  close materialization at Startup finalization, and prove that the guard survives candidate
  failure and incomplete cleanup until finite retirement.
- **Test scenarios:** Bundled pointer sharing; Editor retained-byte exact limit/limit-plus-one and
  release; Direct App source/receipt cannot be mismatched or materialized against a foreign
  component registry; late materialization after Startup is rejected without World mutation;
  private input is promoted before the
  dependent set and absent by finalization; dependent ordering; invalid product Startup prevents publication and
  preserves its safe diagnostic detail; generic third-party fallback; lease retention through
  cleanup-incomplete/retirement; no recipe callback or ordinary-prelude export.
- **Verification:** Focused project-content, project-runtime boot, workspace Play, and scene suites;
  locked serial checks for root and affected crates; Clippy, fmt, diff review, and unit evidence.

### SRT-U4. Compose Atomic Scene Replacement Extras

- **Goal:** Extend the existing hierarchy-aware scene replacement kernel with scoped candidate
  runtime values and exact additional retirement.
- **Requirements:** R7-R11, R24; AE4-AE6, AE18.
- **Dependencies:** SRT-U3.
- **Files:** `crates/nara_ecs/src/transaction.rs`; `crates/nara_identity/src/domain.rs`;
  `crates/nara_hierarchy/src/validation.rs`; `crates/nara_scene/src/spawn.rs` and focused tests;
  minimal advanced exports/API docs; affected identity/hierarchy tests.
- **Approach:** Factor a single private Scene prepare/commit kernel. Add a bounded scoped overlay
  writer keyed by stable ID, a required two-axis `SceneProductTransactionLimits`, and a bounded
  exact extra-retirement input. Require sealed pre-registration of every overlay component, check
  limits before scratch spawn, lower values using existing component IDs before any old-authority
  mutation, and ask `nara_identity` to reject every extra with a live scene or persistent identity
  axis. Replace complete hierarchy `World` scans with separate retained Parent/Children queries.
  Validate the combined retirement contract, then execute one infallible commit token. Keep raw
  replacement and candidate World private.
- **Test scenarios:** Missing/duplicate overlay targets; duplicate/existing components; Add/Insert/
  Remove/Despawn observer and hook eligibility; unregistered type leaves the component registry
  unchanged; zero/exact/limit-plus-one overlay and retirement counts; duplicate/missing/scene-owned/
  persistent-identity-owned/parented extra retirements; persistent locator sentinel survival;
  Parent-only and Children-only invalid worlds among many unrelated entities; scratch rollback; old
  scene/identity/hierarchy/resources unchanged on every failure; success inserts all runtime values,
  moves prepared resources, and lets the next same-tick sentinel observe one coherent generation.
- **Verification:** Focused serial `nara_ecs`, `nara_identity`, `nara_hierarchy`, and `nara_scene`
  nextest suites; locked checks and strict changed-target Clippy; API review and unit evidence.

### SRT-U5. Complete the Reference-Game Spatial Authority

- **Goal:** Replace the partial live-World reset template with one private canonical run owner and
  finish the product behavior originally assigned to RGS-U4.
- **Requirements:** R12-R17; AE6-AE10, AE13.
- **Dependencies:** SRT-U4.
- **Files:** `reference-game/src/{components.rs,lib.rs,resources.rs,systems.rs,ui.rs}`; schema,
  scene/prefab/content fixtures; reference-game tests; root project-content closure fixtures; docs.
- **Approach:** Implement the authoritative generation-4 disposition table above, split
  authored/runtime/derived facts, use `Transform2d` directly, add the authored weapon child, and
  install one private initializer for Startup and Retry. The run owner records exact projectiles and
  calls the advanced Scene transaction with prepared overlay, exact retirement, and infallible
  resource replacements. Delete `WaveResetTemplate`, position projection, and component-type
  ownership inference.
- **Test scenarios:** Deterministic headless outcomes; startup/Retry normalized parity; exact
  projectile ownership and unrelated sentinel survival; full topology/component/presentation
  restoration; rejected initializer atomicity; weapon local/global behavior; runtime-state schema
  exclusion; explicit legacy migration/tombstone/rejection; arbitrary cwd/home content closure.
- **Verification:** Locked serial reference-game default/desktop/authoring suites, source headless
  run, bounded desktop smoke, local no-checkout package smoke, Clippy/fmt, review, and unit evidence.

### SRT-U6. Prove The Authoring Journey And Close The Trial

- **Goal:** Prove Editor and desktop product workflows, classify the provisional API, and record the
  next product decision without broadening ADR 0089.
- **Requirements:** R12, R18-R19; AE2, AE7-AE12.
- **Dependencies:** SRT-U5 and a reviewed executable revision.
- **Files:** focused Editor persistence/Play tests; reference-game docs/fixtures; public-surface
  tests and API docs; ADR 0089/ledger status; engineering-memory verification/registration indexes.
- **Approach:** Run Edit/Play/Retry/Stop/Reopen using current Editor content and the ordinary desktop
  facade. Classify activation/replacement symbols as advanced or private from actual use. Record
  focused Trial evidence as partial implementation only; do not accept broad scene lifecycle.
  Resolve the provisional activation feature boundary from actual consumers: serialization must not
  alter product plugin composition, and any retained Direct App surface that remains public must be
  available from its semantic runtime ceiling rather than through an unrelated `serde` gate.
- **Test scenarios:** Unsaved admitted Editor edit retained through Retry; saved offset survives
  close/reopen; Play state does not dirty authoring; packaged headless/desktop arbitrary cwd/home;
  overlapping input and focus release; terminal/Retry presentation; public-surface negative audit.
- **Verification:** Focused Editor and reference-game suites, real manual desktop journey, package
  smoke, independent correctness/API/data-integrity/maintainability review, memory render/validate,
  and `git diff --check`.

---

## Verification Contract

### Focused Gates

| Unit | Required verification |
|---|---|
| SRT-U1 | Authority reciprocity, ADR/ledger status, memory render/validate, link and whitespace checks; no Cargo documentation test. |
| SRT-U2 | Asset slot revision, image validation, snapshot reuse/reupload, invalidation removal, affected locked serial checks and Clippy. |
| SRT-U3 | Project-content/runtime boot/Editor Play/scene/App fault-detail tests, exact budget release, locked serial checks and changed-target Clippy. |
| SRT-U4 | ECS/identity/hierarchy/scene transaction suites, component-registry and failure matrix, public-boundary review, locked serial checks and Clippy. |
| SRT-U5 | Reference-game default/desktop/authoring suites, canonical content, source and packaged smoke, focused review. |
| SRT-U6 | Editor journey, real desktop journey, package smoke, public-surface audit, independent review and memory closure. |

### Regression Gates

- Never run Cargo concurrently in this checkout. Reuse `target`, use `CARGO_BUILD_JOBS=1` and `-j 1`
  for substantial work, and expand from focused `cargo nextest run` suites according to risk.
- Run `cargo fmt --all -- --check`, affected locked checks/tests, strict Clippy for changed targets
  with explicit pre-existing allowances, and `git diff --check`.
- Preserve backend isolation and server/headless exclusion of window, GPU, toolkit, raw-input, and
  watcher adapters.
- Do not run or extend `tests/architecture_docs.rs`; inspect authority links directly and use the
  engineering-memory validator/render commands for documentation state.
- Every unit receives final diff review before precise staging and a Conventional Commit.

### Review Gates

- SRT-U2: correctness, API, and performance review of slot revision identity, fallible image
  validation, snapshot reuse, and invalidation ownership.
- SRT-U3: correctness, data-integrity, and API review of leases, inseparable source/receipt
  ownership, ordered Startup consumption, diagnostic privacy, and product/recipe separation.
- SRT-U4: adversarial correctness and API review of atomicity, hooks/observers, component registry
  immutability, scratch rollback, exact retirement, no-failure tail, hierarchy complexity, and
  absence of raw World/provider surfaces.
- SRT-U5: ECS/product/schema review of one spatial authority, component granularity, identity
  lineage, exact projectile ownership, and absence of desktop-only gameplay.
- SRT-U6: independent product journey, correctness, API, data-integrity, and maintainability review;
  reviewers must not turn focused evidence into broad ADR 0089 acceptance.

---

## Definition Of Done

- SRT-U1 leaves one active plan and registration, reciprocal supersession, ADR 0089 still Proposed,
  and truthful focused-Trial ledger language.
- SRT-U2 makes direct image mutation observable to render prepare, validates all image bytes at the
  construction boundary, and removes the unconsumed invalidation authority.
- SRT-U3 delivers the exact leased source plus matching receipt before product Startup across
  bundled and Editor materialization, carries bounded diagnostic detail, and publishes no runtime on
  managed startup failure.
- SRT-U4 provides one hierarchy-aware replacement kernel whose advanced product path validates
  bounded typed candidate overlays and exact extra retirements before authority change, commits
  through an infallible token afterward, keeps the component registry unchanged on rejected input,
  and exposes no raw candidate World or provider registry.
- SRT-U5 removes duplicate reference-game spatial/runtime persistence, uses one initializer for
  startup and Retry, owns exact projectiles, restores complete authored topology/presentation, and
  renders one real weapon child through the shared transform graph.
- SRT-U6 proves Editor and desktop workflows, assigns every provisional symbol to advanced/private
  or removes it, records focused partial evidence without accepting broad scene lifecycle, and
  selects the next concrete product slice.
- Every changed public contract has English API docs, migration guidance where identities changed,
  behavioral/negative tests, focused serial verification, a precise commit, and no unresolved
  P0/P1 finding.
- No `WaveResetTemplate`, component-type projectile sweep, duplicate position authority, raw scene
  replacement port, `ProductRecipe` Host callback, general scene session/manager, provider registry,
  benchmark framework, or unrelated staged change remains.
- Async/additive/travel, ordered hierarchy, `KeepWorld`, inherited visibility, 3D, physics, save,
  scripting, and release infrastructure remain outside production promises.

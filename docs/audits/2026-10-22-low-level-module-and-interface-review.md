# Low-Level Module And Interface Review

Review started: 2026-10-22; completed: 2026-10-23, Asia/Shanghai.
Reviewed source: `c28f8d8b28dc8b08b42e5086185907a8dcecc32e`, branch
`test/compiled-asset-domain-export`.

## Findings

The lower-level ownership direction is worth retaining, but several interfaces do not yet carry
all the facts needed to use them correctly. The most important gaps concern failed ownership
transfer, revocation of derived-state authority, and the distinction between desired and published
asset configuration. These are not reasons to replace Bevy ECS or introduce a universal engine
transaction framework.

This review records ten concrete defects and six interface/design debts. A defect is not a claim
that an ordinary shipped workflow has already encountered it. Reproduction and source-only evidence
are distinguished below. The earlier [workflow review](2026-10-22-post-refactor-ownership-and-ergonomics-review.md)
remains separate; its six performance/consumer follow-ups are not reclassified as new defects here.

| ID | Priority | Finding | Evidence level |
|---|---|---|---|
| L01 | P1 | Failed close-participant registration can discard the transferred owner | Rust probe plus source |
| L02 | P2 | First identity adoption skips target observer admission | Rust probe plus source |
| L03 | P2 | Raw hierarchy changes leave stale completion tokens accepted by extraction predicates | Rust probe plus source |
| L04 | P2 | Moving an immutable Global component bypasses projection invalidation | Two-frame Rust probe plus source |
| L05 | P2 | Cross-frame Image reload can lose the desired metadata recipe | Source-confirmed; gated runtime regression needed |
| L06 | P2 | Watcher directory events lose subtree scope | Source-confirmed; native event frequency unmeasured |
| L07 | P2 | Multiple lock guards can share one native lock state | Unix primitive probe plus source |
| L08 | P2 | Shape preflight copies an oversized map key before checking its budget | Allocation-observing Rust probe |
| L09 | P2 | Public ECS derives are not consistently usable through renamed dependencies | Positive/negative rustc probes |
| L10 | P3 | Finite rotation endpoints can overflow interpolation | Numeric probe plus source |

Priority reflects impact and repair order, not an estimate of occurrence frequency. P1 is the
failure-path owner-custody gap; P2 covers correctness/admission and supported-interface failures;
P3 covers a low-likelihood numeric boundary. No memory-unsafety finding is asserted.

## Concrete Defects

### L01: Failed Registration Must Retain Or Return The Close Owner

[`App::register_plugin_runtime_close_participant`](../../crates/nara_app/src/lib.rs#L1449)
accepts `participant: P` by value. The non-Build rejection and the call to
`register_plugin_shutdown_obligation` return before the participant reaches the obligation ledger.
An undeclared or duplicate obligation therefore drops the participant and returns only `PluginError`.
Only the later duplicate-participant-ID branch explicitly retains its owner for retirement.

The actual consumers already use acquisition followed by ownership transfer:
[`TaskPlugin`](../../crates/nara_tasks/src/runtime.rs#L3502) takes a close owner, and
the [checkpoint plugin](../../samples/two-level-game/src/gameplay/storage.rs#L48) opens storage
before registration. Their current declarations are valid; this is not evidence that those normal
paths have leaked. It is evidence that the public failure contract cannot protect a legitimate
participant whose cleanup lives in `begin_close`/`poll_close`, rather than `Drop`.

A probe confirmed both non-Build rejection and an undeclared obligation inside Build:
`dropped=true`, `begin_close=false`; the latter App becomes `Poisoned`. Poisoning cannot retire an
owner that was never retained. The lower
[`RuntimeObligationLedger::register`](../../crates/nara_app/src/runtime/close.rs#L102)
already returns the participant on rejection.

**Repair owner:** `nara_app`. Make every transfer path retain the owner for failed-construction
retirement or return it in an ownership-carrying error. An illegal phase must not silently destroy
the caller's only protocol owner. Test undeclared obligation, duplicate obligation, illegal phase,
and duplicate participant ID, including incomplete close and retry. Do not delegate this recovery
protocol to every plugin.

### L02: First Identity Adoption Must Admit Target Lifecycle Effects

[`validate_identity_support_topology`](../../crates/nara_identity/src/domain.rs#L204) performs the
target-specific check only when that target already contains `IdentityEntityMarker`.
[`adopt_entity`](../../crates/nara_identity/src/domain.rs#L705) inserts the marker precisely when
it is absent. Global validation with `target=None` cannot inspect an entity-component observer.

A probe first obtained the registered marker ID from an already adopted control entity through
public ECS archetype metadata. It then installed an entity-component `Add<()>` observer on an
unmarked target. First adoption succeeded and the observer ran once. The private Rust marker name
does not prevent a trusted ECS caller from observing the registered component ID.

This is a lifecycle-admission defect, not an untrusted-code sandbox escape. The existing test in
[`domain.rs`](../../crates/nara_identity/src/domain.rs#L251) starts from an already marked target;
the [Scene adoption loop](../../crates/nara_scene/src/spawn.rs#L1666) uses the same owner operation.
The initial entity-only probe did not fire an observer and is not claimed as a reproduction;
the successful probe explicitly targeted the registered marker component.

**Repair owner:** `nara_identity`. Validate each existing target's relevant observers regardless
of marker presence before inserting or reusing it. Test first adoption with Add and Insert
entity-component observers, zero observer effects on rejection, unchanged identity indexes,
and legal idempotent adoption. The existing ECS validator need not be replaced.

### L03: Completion Is A Revocable Fact, Not Agreement Between Old Tokens

[`hierarchy_component_changed`](../../crates/nara_hierarchy/src/lib.rs#L252) marks private
generation state dirty but leaves `CompletedHierarchyProjection` installed. The supported writer
also removes the token; the raw ECS hook does not. The authoritative
[`completed_topology_generation`](../../crates/nara_hierarchy/src/lib.rs#L80) sees the dirty state,
but [Sprite extraction](../../crates/nara_sprite_render/src/extract.rs#L65) compares only retained
hierarchy, Transform and order generations. All three old values can still agree.

A probe removed a live child's `Parent` after Extract propagation. The authoritative query
returned `None`, while the exact generation predicate used by extraction returned `true`.
This probe exercised the predicate, not a renderer/readback trajectory. With the default
non-interpolated presentation path, a consumer can accept an old world-space matrix in this window.
The [existing raw-removal test](../../crates/nara_hierarchy/src/tests.rs#L955) checks the full query,
not the predicate used by the downstream consumer.

**Repair owner:** `nara_hierarchy`, with Transform/render consumer integration. Every supported raw
mutation must invalidate the externally readable completion fact immediately. Provide one
owner-authored validity projection rather than asking consumers to reconstruct private-state
checks. Preserve schedule-based rebuilding. Test raw Parent/Children removal between completed
projection and actual Sprite/camera extraction; no derived publication may accept a stale receipt.

### L04: Immutable Components Are Not Owner-Bound Components

[`GlobalTransform2d`](../../crates/nara_transform/src/lib.rs#L50) has a private matrix and is
immutable, but safe ECS operations can take an existing component and insert it on another entity.
It has no insertion/removal invalidation hook. The
[`static propagation fast path`](../../crates/nara_transform/src/propagation.rs#L203) checks local
change ticks, participant count, hierarchy generation and token presence, not raw Global changes.

A probe created two roots with local x positions 10 and 20, completed startup, and swapped their
Global components in Update without changing their locals. Two subsequent successful frames left
the first entity's Global x at 20. The bug is persistent incorrect derived data, not just a missing
component rejected by extraction. [Sprite extraction](../../crates/nara_sprite_render/src/extract.rs#L106)
and cameras consume those values.

**Repair owner:** `nara_transform`. Include raw insertion, replacement and removal of owned derived
components in invalidation. Distinguish owner publication from external mutation so an unchanged
frame retains its fast path. Test swap, removal and reinsertion before propagation, followed by
actual extraction, and assert that an unchanged frame does not rebuild the forest.

### L05: Desired Import Settings Must Survive Supersession Across Frames

[`capture_with_settings`](../../crates/nara_image/src/import/publication.rs#L143) takes settings
from the last usable Image. [`accepted_reload_settings`](../../crates/nara_image/src/reload.rs#L687)
reads metadata for `MetaModified`, but normally uses those published settings for `Modified`.
[`drain_coalesced`](../../crates/nara_asset/src/reload/source_changes.rs#L387) retains metadata
dirtiness only within one drained batch.

With an sRGB publication, admit a Linear metadata reload and hold its worker. On a later frame,
admit a PNG modification. That successor supersedes the pending generation but captures sRGB from
the old publication. It can publish new pixels with the old recipe, with no remaining metadata
event to correct it. Before the first usable publication, the analogous successor can reject with
`MissingImportSettings`. The [current latest-recipe test](../../crates/nara_image/src/tests.rs#L2283)
puts both events in the same frame.

Generation/state/slot checks correctly reject stale completions; they do not establish which
configuration the newest request should execute. This finding is source-confirmed, not a newly
executed asynchronous reproduction.

**Repair owner:** `nara_image`. Retain admitted desired settings or unresolved metadata dirtiness
independently from last-good pixels and carry it through request supersession. Keep metadata
identity admission. Use the existing external [I/O gate](../../tests/fixtures/module-consumer/tests/support/image_reconciliation.rs#L311)
for two-frame metadata-to-source, first load, failure/retry and late predecessor completion tests.

### L06: Watcher Transport Must Preserve The Scope Of Invalidation

[`watch_events_from_notify`](../../crates/nara_asset_watch/src/backend.rs#L56) lowers directory
remove/create and rename notifications to path-only events. The
[`translator`](../../crates/nara_asset_watch/src/lib.rs#L196) produces an exact asset path, while
the [resolver](../../crates/nara_asset/src/reload/resolution.rs#L152) performs exact-path lookup.

If a backend reports removal of `textures` without separate notifications for every descendant,
the `textures/player.png` record receives no removal. The directory path itself is valid, so the
watcher's [translation-failure rescan path](../../crates/nara_asset_watch/src/observability.rs#L65)
does not activate. Existing published child content can remain authorized.

This is a defect in the optional watcher path. This review did not identify a normal product
installing that plugin and does not claim an observed current Editor failure. Native event
frequency and platform-specific notification patterns were not measured.

**Repair owner:** `nara_asset_watch` at transport classification, with asset-owner invalidation.
Preserve directory/subtree scope and bound invalidation over known records, or require the existing
sticky `RescanRequired` path when the event's scope cannot be trusted. Do not recursively scan from
the native callback. Qualify directory deletion, cross-root move and populated-directory arrival,
including child publication and derived-resource invalidation.

### L07: Native Lock Ownership Does Not Match Borrowed Guard Ownership

[`FileCapability::try_lock(&self)`](../../crates/nara_fs/src/file.rs#L281) permits multiple guards
over the same file. Each [`FileLock`](../../crates/nara_fs/src/lock.rs#L28) independently unlocks
the full range on release/drop. The [Unix adapter](../../crates/nara_fs/src/platform/unix.rs#L476)
uses `flock`, whose state belongs to the open file description rather than to each Rust guard.

A Linux primitive probe acquired Exclusive twice on one file description, unlocked once and then
successfully acquired Exclusive through an independent open description. The first logical guard
would still be alive. An Exclusive-to-Shared second acquisition can also change the first guard's
effective mode. This reproduces the native mechanism used by the adapter, not a Linux build of
the Nara crate. Windows repeat-acquisition behavior is not inferred from it.

The real [owner-marker consumer](../../editor/src/supervised_play/generation/store/scavenge.rs#L281)
also uses `mem::forget(lock)` to retain the native lock for the file capability's lifetime. That
workaround is intentional and is not itself reported as a leak; it demonstrates a missing owned
lock shape.

**Repair owner:** `nara_fs`. Make one native lock have one explicit owner. Prefer a consuming
`LockedFileCapability` for long-lived use, or enforce one outstanding borrowed lock with an
appropriately synchronized state. Simply cloning the underlying descriptor is not an independent
Unix lock owner. Test nested same-mode acquisition, mode changes, independent opens, explicit
release, drop, and ordinary owner-marker lifetime on supported platforms.

### L08: Structural Limits Are Checked After A Map-Key Allocation

[`MapKeyVisitor`](../../crates/nara_core/src/serde_shape.rs#L467) calls `to_owned` for borrowed
strings; only afterwards does [`MapKeySeed`](../../crates/nara_core/src/serde_shape.rs#L458) check
per-string and aggregate string limits. A rejection therefore does not bound the allocation it
is supposed to precede.

A JSON probe supplied an unescaped 1 MiB key with a 32-byte per-string limit and a 64-byte total
string limit. Allocation observation was enabled only after the input and parser were constructed.
The preflight rejected correctly, but requested a 1,048,576-byte allocation first. This is not an
unbounded whole-product allocation claim: the [persistent-file byte gate](../../crates/nara_core/src/format.rs#L579)
still limits input before preflight, and parsers can have separate allocation behavior.
The extra allocation is specifically owned by this shared map-key visitor. Actual users include
[Scene file decoding](../../crates/nara_scene/src/format.rs#L63), asset metadata and schema files.

**Repair owner:** `nara_core`. Give the key visitor access to the tracker, check borrowed key bytes
before copying and charge them once. Retain duplicate detection. Document that generic serde
preflight cannot promise to prevent allocations already made by the syntax adapter, especially
for escaped strings. Add allocator-observing tests for borrowed oversized keys and small accepted
keys; do not build a replacement parser.

### L09: The ECS Facade's Derive Contract Is Incomplete

[`nara_ecs`](../../crates/nara_ecs/src/lib.rs#L8) adapts four derives to Nara's crate-path resolver,
but also reexports upstream `Bundle` and `system::SystemParam`. Those macros still look for
`bevy_ecs`/`bevy`. The [Reference Game alias](../../reference-game/src/systems.rs#L3) and
[`ProjectScenes` alias](../../src/project_host/runtime/scene_requests/api.rs#L6) expose the resulting
upstream naming knowledge to ordinary consumers.

In the same temporary manifest declaring only a renamed `ecs = { package = "nara_ecs", ... }`,
the adapted Component derive compiled and spawned successfully. Bundle and SystemParam failed
with E0433 because `bevy_ecs` was absent. These were direct rustc probes with the manifest environment
set for proc-macro resolution, not fresh Cargo fixture builds. Existing fixtures with a direct
Bevy dependency can mask this gap.

**Repair owner:** `nara_ecs`/`nara_ecs_derive`. Define the supported public derive set and adapt the
already exposed Bundle plus the actually consumed SystemParam, including complete hidden
expansion support. Reuse upstream injectable macro logic where possible. Do not independently
reimplement ECS derive semantics. Qualify both renamed-root and renamed-ECS consumers with no
direct Bevy dependency, and execute the derived bundles/parameters rather than checking imports.

### L10: Finite Rotation Inputs Can Produce A Non-Finite Interpolant

[`interpolate`](../../crates/nara_transform/src/presentation.rs#L186) subtracts two `f32` rotations
before wrapping the difference. Opposite-sign finite endpoints around `3e38` overflow the
subtraction; `rem_euclid` then yields NaN. Both endpoint matrices can be finite. The projection
validator protects downstream output, but valid endpoint states can fault the interpolated frame.

The numeric expression was reproduced; a complete interpolation/render trajectory was not run.
This low-likelihood boundary is not evidence that normal small-angle interpolation is broken.

**Repair owner:** `nara_transform`. Compute the wrapped angular difference in a domain wide enough
for all admitted f32 endpoints, then lower deliberately. Test opposite-sign extremes, crossing
plus/minus pi, equal angles and alpha 0/0.5/1, including finite candidate matrices. Preserve the
shortest-arc and exact endpoint behavior.

## Interface And Design Debts

These are not additional confirmed data-corruption defects in protected first-party paths.

### E01: Erase Codec Types After The Public Registration Boundary

The [advanced codec registration](../../crates/nara_reflect/src/registry.rs#L1232) names `T`, but
returns an untyped `PreparedComponentCandidate` and gives the encoder `World + Entity` rather than
the selected `&T`. The [existing counterexample](../../crates/nara_reflect/src/tests.rs#L724)
registers ApplyProbe, prepares Velocity, and freezes successfully; first value preflight rejects
the mismatch. The [runtime guard](../../crates/nara_reflect/src/persistent_apply.rs#L732) is effective.
[Sprite](../../crates/nara_sprite/src/lib.rs#L335) and
[Tilemap](../../crates/nara_tilemap/src/lib.rs#L638) repeat type selection and missing-component
handling in their encoders.

**Direction, P2:** Keep a typed candidate relationship at registration and erase only in private
storage. Supply the already selected component to the encoder, retaining read-only World/context
where reference encoding genuinely needs them. The simpler typed codec convenience already does
this selection internally. Do not attempt to analyze arbitrary closures at freeze time.

### E02: Single-Task Physical Return Belongs Below Domain Preparation

[`TaskHandle`](../../crates/nara_tasks/src/runtime.rs#L896) correctly distinguishes first terminal
publication from physical retirement, but exposes no per-task physical-return observation.
[Scene preparation](../../src/project_host/runtime/scene_requests/preparation.rs#L31),
[Browser scanning](../../src/project_host/authoring/source_index/preparation.rs#L124), and
[document opening](../../src/project_host/runtime/editor/document_open/preparation.rs#L137)
therefore implement their own returned flags, capture-drop ordering and cancellation/result races.
The current safeguards are not shown to retire capabilities prematurely.

**Direction, P2:** Add a narrow executor-authored retirement receipt for closure/capture disposal
and executor-owned late-result cleanup. Domain owners still own publication, request slots,
external clones and transferred results. Pool-wide close is not a substitute during ordinary
running. Migrate Scene and Browser to prove reuse; cover queued/running cancellation, panic and
blocked late-result destruction. This is distinct from the earlier capture-drop cost measurement.

### E03: Asset Mutation Needs A Shared Captured Baseline, Not Repeated Caller Checks

[`commit_reload`](../../crates/nara_asset/src/storage.rs#L164) checks only the version;
failure recording has no captured baseline, and
[`remove_with_state`](../../crates/nara_asset/src/storage.rs#L205) can advance a supplied state
store even if this value store has no value. A valid handle combined with empty store B and states
from store A can revoke A's publication without removing A's value. Global ID uniqueness does not
bind a particular state/value-store pair.

First-party Image publication already checks binding plus state/slot revisions, and TileSet has
its own slot guard. Those guards protect their actual paths. The debt is that the shared mutation
interface requires each asynchronous owner to rediscover the same store/revision proof.

**Direction, P2:** Reuse existing store identities and revision values in a small typed mutation
baseline, validated for success/failure/removal. Keep source metadata and recipe policy domain-owned.
Raw code-first mutation can remain explicit. Test cross-store removal, same-version ABA and late
failure after a successor wins; avoid inventing a generic importer executor or residency service.

### E04: Spatial Participation Needs A Complete Temporal Contract

[`add_fixed_transform_projection_systems`](../../crates/nara_transform/src/lib.rs#L75) adds only
`after(Propagate)`. It does not imply before Finalize, nor does it offer the equivalent actual
Extract-stage consumer interface. The
[Reference Game](../../reference-game/src/lib.rs#L152) supplies its own before-Capture relation;
the [workspace consumer](../../tests/workspace_play_runtime.rs#L163) reaches into `__private` to
observe completed Extract values. Existing game ordering is therefore not reported as broken.

**Direction, P2:** Specify the fixed projection-consumer window and expose only the additional
phase entrypoint demanded by the real caller. Test consumer-to-finalization ordering, zero/multiple
fixed steps and paused Extract. An `after` relation is not a complete interval; do not expose every
internal set or introduce a universal phase DSL.

### E05: Identity Mutation Should Own Its World Borrowing Choreography

The identity domain is usually a resource in the World it mutates. Adoption, registration and
retirement take different combinations of domain and World borrows, so an ordinary consumer needs
[`resource_scope`](../../tests/fixtures/module-consumer/tests/durable_identity.rs#L309) or manual
remove/reinsert machinery. Existing identity checks remain valid; this is caller knowledge, not
a demonstrated corrupted index.

**Direction, P3:** Provide a small World-oriented writer/operation surface for the actual
adopt/register/retire workflow, retaining detached-domain use as an advanced path. Error returns
must leave the domain installed and indexes unchanged. Avoid another general World wrapper.

### E06: Schedule Ordering Is Not Success Ordering

The [managed error handler](../../crates/nara_app/src/runtime/fault_route.rs#L330) records a sticky
fault and returns. The [owner](../../crates/nara_app/src/lib.rs#L1901) observes it after the whole
Schedule returns. Same-schedule successors and deferred commands can still run. In particular,
[Gameplay Capture/Acknowledge](../../crates/nara_gameplay/src/lib.rs#L135) follow Consume, while
their current condition checks tick membership rather than successful consumption.

**Direction, P2:** Document the detection granularity and deferred effects explicitly. A domain
that requires success before capture/acknowledgement needs its own accepted-operation state or one
fallible operation, not merely `.chain()`. The
[external fault test](../../tests/fixtures/schedule-extension/renamed-root/src/lib.rs#L285) checks
the final fault but not these later effects. Add that oracle before selecting any Gameplay policy
change. The current ADR does not promise intra-schedule abort or rollback, so this is not called
an ADR violation, and no new scheduler is recommended.

## Architecture To Retain

- **Keep the ECS substrate and one execution owner.** A thin Bevy substrate is appropriate;
  trusted Rust systems retain World access without a sandbox claim. App configuration, sealed
  schedules and runtime execution should not become competing lifecycles.
- **Keep distinct identities and authorities.** Durable asset ID, local handle, source locator,
  runtime incarnation and World locator identify different facts. Do not merge them into one ID.
- **Keep structural hierarchy separate from spatial math and lifetime.** Explicit ordered roots,
  non-linked parent relations, guarded writers and exact retirement sets earn their complexity.
- **Keep full candidate validation and publication.** Iterative Transform projection, finite
  matrices and lifecycle-free transaction guards protect real invariants. Repair revocation rather
  than deleting those guards to reduce source size.
- **Keep persistent validity separate from native reconciliation.** Scene's preparation, first-effect
  revalidation and final retirement checks prove different facts. The Reflect/Scene lane found no
  additional defect in that protocol; arbitrary native rollback remains unpromised.
- **Keep filesystem proof axes separate.** Handle authority, content identity, conflict protection,
  atomic publication and durability cannot be replaced by one `safe` flag. L07 concerns lock
  ownership, not an argument to weaken path or publication checks.

## First-Principles Refactor Order

1. **Close ownership and admission gaps:** L01, L02, L07. A rejected operation must not lose its
   resource owner or execute unadmitted lifecycle effects. Land focused regression tests first.
2. **Repair revocable spatial authority:** L03/L04 together, then E04. Establish one trustworthy
   completion fact before polishing consumer helpers. Preserve unchanged-frame behavior.
3. **Repair asset intent and invalidation:** L05/L06, then E03. Separate desired configuration,
   published value and in-flight attempt; retain subtree scope until the correct owner consumes it.
4. **Move representable obligations into types:** L09 and E01, followed by E05. Qualify ordinary
   external consumers without Bevy aliases or handwritten resource extraction.
5. **Centralize actual repeated lifecycle knowledge:** E02 with two existing consumers. Do not
   create an async framework for every domain. Handle L08/L10 as bounded focused corrections;
   clarify E06 before changing command acknowledgement semantics.

The common design test is not the number of types or methods. Ask whether a fact the lower module
already knows must be reconstructed by every caller. Retain type identity until erasure is needed;
retain custody until physical work returns; revoke completion when its premise changes; keep
desired state distinct from last successful publication. Each improvement should remove caller
knowledge without discarding independent admission evidence.

The pinned local Bevy source at `85503941e488152711a4695bbe248e9ed9f453fc` illustrates
[typed registration before erased storage](../../repo-ref/bevy/crates/bevy_reflect/src/type_registry.rs#L344)
and [component reflection operations](../../repo-ref/bevy/crates/bevy_ecs/src/reflect/component.rs#L81).
These are mechanism comparisons, not evidence of the latest upstream design or a recommendation
to copy its whole application/asset model. The Godot checkout pin was checked but its mechanisms
were not used as evidence in this review.

## Explicit Non-Findings And Limits

- Missing exact KeepWorld, PostTransform2d, 3D transforms or a general resource-acquisition service
  are capability boundaries, not newly discovered regressions. Require a concrete consumer before
  accepting their architecture.
- A changed Transform currently rebuilds a complete candidate forest, and interpolation derives
  presentation matrices. The [existing scale report](../benchmarks/2026-10-18-foundation-scale.md)
  remains prior evidence; no new product tail-latency, CPU, memory or scaling result is claimed.
- `Color`'s linear-sRGB/straight-alpha meaning is not documented at its constructors, and no named
  sRGB conversion is present. This is the already recorded partial
  [ADR 0092 implementation](../architecture/adr/implementation-status.md#L208), not a novel color
  pipeline defect. Do not reinterpret stored colors during an ergonomics change.
- Diagnostic field-aware dedupe uses retained fields by explicit
  [ADR 0048 policy](../architecture/adr/0048-runtime-diagnostics-and-observability-bus.md#L40).
  A probe confirmed that dropping the distinguishing field merges two drafts, but that is not
  counted as a bug. Manual diagnostic retention without arbitrary clear is also deliberate.
- No whole-workspace qualification, new native Editor acceptance, MSRV qualification, or release
  claim was performed. Existing successful cohorts were not rerun or relabeled as current evidence.

## Execution Evidence

Four read-only Codex lanes covered App/ECS, Tasks/Asset, Reflect/Scene and Identity/Hierarchy/Transform.
The parent reviewed Core/FS/Diagnostic and cross-module callers, checked candidate findings against
source, and executed the following bounded probes. Every lane was closed after collection.
An additional read-only Codex reviewer checked the final report's classification and ownership,
lifecycle and spatial claims against source and returned `no blocking report findings`.

| Probe | Actual result | Scope |
|---|---|---|
| Close registration outside Build and undeclared obligation in Build | Participant dropped; no begin-close callback; Build App poisoned | Cached Nara App, Windows, Rust 1.99 |
| First identity adoption with marker-specific target observer | Accepted; observer ran once | Cached Identity/ECS |
| Swap two completed Global components in Update | Incorrect x=20 retained for local x=10 across two successful frames | Cached App/Hierarchy/Transform |
| Remove Parent after Extract propagation | Authoritative completion None; extraction's generation predicate true | Predicate oracle, not GPU rendering |
| 1 MiB borrowed JSON key with 32/64-byte shape limits | Rejected after a 1,048,576-byte allocation request | Allocator-observing cached Core/serde_json |
| Renamed ECS derive control and negative probes | Component passed; Bundle/SystemParam failed E0433 for bevy_ecs | rustc with manifest environment, no Cargo fixture build |
| Two flock acquisitions, one unlock, independent acquisition | Independent exclusive acquisition succeeded | Python fcntl under Ubuntu-26.04/WSL; native mechanism only |
| Extreme finite f32 angular difference | Wrapped difference NaN | Numeric expression, no presentation trajectory |
| Retained-field dedupe control | Second distinct draft deduplicated after distinguishing field dropped | Matches ADR 0048; excluded finding |

Probe source files were created outside the repository in the Windows temporary directory using
`apply_patch`. Rust probes linked existing `target/debug/deps` artifacts and followed current source
control flow; they corroborate the findings but are not fresh builds from this revision. No Cargo
commands, formatting, product source edits or build cleanup were performed. A running unrelated
Cargo process was observed and left alone. The preexisting untracked `vendor/` was not read or changed.

Tool failures were bounded: the initial four subagents failed before execution with routing 404;
replacement default-model agents ran. One Reflect lane ended without text and was replaced by a
narrower read-only lane. The allocation probe initially selected an incompatible cached serde
feature cohort and was relinked against the matching artifact. Spatial probe corrections covered
entity-only observer non-triggering and tuple/debug-name assumptions before the successful
component-ID-based test. No such failed attempt is presented as successful acceptance.

Only this report and its architecture-map link are repository deliverables. These findings do not
change Accepted ADRs, close implementation tasks or authorize a release. Follow-up implementation
should preserve the source baseline and qualify its own changed behavior with focused nextest
and external-consumer tests.

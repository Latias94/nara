# Post-Boundary Redesign: Pinned Bevy and Godot Comparison

**Date:** 2026-10-23. **Nara design baseline:** `88b7fb3`.

This study checks the [proposed replacements](../architecture/post-boundary-contract-redesign.md)
against the actual read-only local reference sources. It supplements the
[earlier low-level study](2026-10-23-low-level-contract-mechanisms.md); that study does not by itself
qualify the newer PB1-PB7 paths. Findings and probes remain owned by the
[post-boundary audit](../audits/2026-10-23-post-boundary-architecture-and-interface-review.md).

## Baselines and Method

| Repository | Exact local HEAD | Version label in that checkout |
| --- | --- | --- |
| Bevy | `85503941e488152711a4695bbe248e9ed9f453fc` | `0.20.0-dev` in root Cargo.toml |
| Godot | `81ab961f91066a63bcbf3ec340962bb1a1ada558` | `4.8.0-beta` from version.py fields |
| Nara | `88b7fb3` | Proposed replacement design; no implementation adopted in this study |

Both reference worktrees were clean when inspected. The checked-out Bevy reference is not assumed
to be identical to Nara's crates.io `bevy_ecs = 0.20.0`; compare mechanisms, not interchangeable
source revisions. No fetch, checkout, reference edit, Cargo, SCons, runtime test or benchmark was
performed. Selected immutable raw GitHub files were also retrieved as a supplementary source check;
the local pinned files and their line references own the detailed trace. These are fixed-source
findings, not statements about latest releases. Recommendations are explicitly Nara inferences.

Comparison categories:

- **Similar mechanism:** the source implements a relevant owner/operation pattern.
- **Different contract:** it avoids or accepts a condition rather than solving Nara's exact problem.
- **No equivalent established:** the inspected path does not demonstrate Nara's stronger guarantee.
  This is not a whole-engine absence proof or a bug report against the reference project.

## Verdict Matrix

| Nara unit | Bevy observation | Godot observation | Disposition |
| --- | --- | --- | --- |
| R1: executed output plus fault | Registered systems retain a result through bookkeeping/deferred application; schedule errors do not imply rollback | Object invocation returns a Variant and a separate CallError; not a Rust ownership-return protocol | Keep lossless owned output, but do not attribute the sticky-fault envelope to either engine |
| R2: disposal custody/outcome | Scope waits on tasks/cancellation; pool Drop closes and joins synchronously | Worker task completion, explicit reclamation and pool finish have different phases | Keep Nara's admitted-disposal tracking because its concurrent pollable close promises more |
| R3: capability write cursor | Asset readers/writers open separate stream objects | FileAccess explicitly shares one seek/read/write stream position | Keep Nara's explicit logical write cursor; do not copy a weaker stream contract |
| R4: post-flush admission | World::flush executes commands; reflection inserts/applies with live callbacks | PackedScene constructs and sets node properties sequentially | Keep the actual post-flush baseline; neither supplies Nara's lifecycle-free persistent commit guarantee |
| R5: prepared identity/content | Typed RenderAsset transforms source to GPU value, but RenderAssets::insert remains public | ImageTexture owns a RID and updates/replaces its associated backend content | Keep Image-owned construction, while avoiding claims that upstream prevents arbitrary manual mismatch |
| R6: recovery continuity | Source add/modify triggers reload; source removal is not Nara-style immediate Image revocation | Import params are read from and written back to .import, including failed-import validity | Retain bounded owner continuity; diagnostic-only rejection does not settle superseded Loading |
| E1: exclusive Scene access | Domain World/Commands Scene extensions; SystemState is also a normal public advanced tool | SceneTree provides current_scene and semantic change operations | Keep domain-owned access; stop treating all SystemState use as an architecture defect |
| E2: complete ECS name | Bevy exports bevy_ecs as ecs, no selected-wrapper/full-substrate distinction inside that path | No directly analogous external ECS re-export | bevy_ecs remains a reasonable Nara naming choice, not an upstream-derived necessity |

## 1. Operation Output Is Not Rollback (R1 / PB1)

Bevy's [World::run_system_with](../../repo-ref/bevy/crates/bevy_ecs/src/system/system_registry.rs#L673)
looks up and temporarily takes the registered system, initializes it if necessary, runs it without
immediate deferred application, keeps the returned result, queues deferred work, restores the system,
then flushes before returning `Ok(result?)` (especially lines 725-739). Errors before invocation and
system execution errors can both occur. Nothing in this sequence says that an error undoes prior
World mutation or makes a consumed input available again. Schedule executors likewise allow an
error handler to return and continue bookkeeping, as documented in the earlier pinned study.

Godot's [Object::callp](../../repo-ref/godot/core/object/object.cpp#L851) returns a `Variant` and
updates a separate `Callable::CallError`. For several script-call error categories it returns the
script's returned Variant rather than reconstructing an uncalled operation. The function also has
real side effects, including the special `free` path. It is not an atomic transaction or a Rust
move-only input-return interface.

**Nara inference:** R1 is justified by the actual Host's ownership requirement, not by an assertion
that Bevy or Godot already has the same sticky-fault scope. Preserve normal returned output while
recording a post-operation fault separately. Keep the typed owned-port admission return: a generic
uncalled FnOnce can drop captures, so a generic execution envelope alone cannot guarantee input
custody. Do not grow this into a universal execution transaction or conflate a normal return with
unwind recovery.

## 2. Completion, Disposal, Joining and Cleanup Success Differ (R2 / PB2/PB4)

The native threaded Bevy
[scope implementation](../../repo-ref/bevy/crates/bevy_tasks/src/task_pool.rs#L368) retains spawned
tasks, drives/awaits their results and resumes task panic payloads when collecting them. Its
[Scope Drop](../../repo-ref/bevy/crates/bevy_tasks/src/task_pool.rs#L697) drains retained handles and
blocks on `task.cancel().await`. The
[TaskPool Drop](../../repo-ref/bevy/crates/bevy_tasks/src/task_pool.rs#L616) closes the shutdown sender
and joins its worker handles, expecting successful joins unless already panicking. Bevy re-exports
[async_task::Task](../../repo-ref/bevy/crates/bevy_tasks/src/lib.rs#L79); this study does not infer
all of that dependency's cancellation/destructor semantics from the re-export.

Those are useful examples of retaining work until its owner can finish it. They are not a bounded,
retryable close API callable concurrently with submission through a shared standalone facade.
Dropping a last shared owner, scoped blocking cancellation and Nara's `shutdown_blocking(&self)`
are different lifetime shapes. No equivalent to Nara's replaced-pending capture destruction on an
in-flight submitter was established in the inspected Bevy pool path.

Godot's [task processing](../../repo-ref/godot/core/object/worker_thread_pool.cpp#L97) invokes work
and, for template callbacks, deletes the template userdata before publishing task/group completion.
For normal Callable work, the task record can remain after completion. The
[wait/reclaim path](../../repo-ref/godot/core/object/worker_thread_pool.cpp#L403) waits if needed and
frees the task record when waiters permit; `is_task_completed` only observes the record's completion
flag. [finish](../../repo-ref/godot/core/object/worker_thread_pool.cpp#L857) switches runlevel, joins
threads and frees remaining task records. Thus even its completion vocabulary has a concrete scope,
not a universal claim that every captured object's lifetime has ended.

Godot's build defaults to disabling exceptions and explicitly says it does not use exceptions in
[SConstruct](../../repo-ref/godot/SConstruct#L935). Do not project Rust destructor-unwind reporting
onto this C++ protocol, or conclude that its lack of `CleanupFailed` means Nara does not need one.

**Nara inference:** R2's queue-locked transfer to private disposal custody is appropriate. A shared
counter/obligation and a bounded cleanup-failure summary are enough; no per-task worker or new
global executor is needed. Admitted work in caller-thread disposal must delay close; submissions
rejected after close cannot reopen its finite work set. Cleanup return can end physical custody while
failure evidence remains. Reference mechanisms support explicit ownership, not a drop-in proof of
Nara's PB2/PB4 fixes.

## 3. File Cursor Semantics Must Match the Public Abstraction (R3 / PB3)

Bevy's [file asset reader](../../repo-ref/bevy/crates/bevy_asset/src/io/file/file_asset.rs#L74) opens
a fresh file stream for a read. Its
[writer](../../repo-ref/bevy/crates/bevy_asset/src/io/file/file_asset.rs#L174) creates a writer stream
through a separate file open/create operation. This is not a shared exact-handle capability whose
borrowed readers promise independent positions and whose writer must continue across borrows.

Godot deliberately exposes a stream contract:
[seek/get_position](../../repo-ref/godot/drivers/windows/file_access_windows.cpp#L293) use the FILE
position, [get_buffer](../../repo-ref/godot/drivers/windows/file_access_windows.cpp#L336) uses `fread`,
and [store_buffer](../../repo-ref/godot/drivers/windows/file_access_windows.cpp#L384) uses `fwrite` with
read/write transition handling. A read moving the subsequent write position is consistent with that
single-stream interface. It is not an implementation of independent capability cursors.

**Nara inference:** Keep R3. A small logical write offset and positional native writes are necessary
to implement the contract Nara already advertises. Reopening by path, duplicating lock ownership or
copying Godot's stream semantics would evade rather than fix PB3. This does not justify unrelated
append/truncate/seek feature expansion or imply Nara's filesystem is generally superior.

## 4. Persistent Admission Is a Stronger Nara Contract (R4 / PB5)

Bevy [World::flush](../../repo-ref/bevy/crates/bevy_ecs/src/world/mod.rs#L3208) applies queued
registrations and commands; it is not a read-only metadata barrier. The ordinary
[ReflectComponent function table](../../repo-ref/bevy/crates/bevy_ecs/src/reflect/component.rs#L309)
specializes operations for C, uses live entity insertion/reflected application and component
registration, and explicitly has panic paths for reflective mutation of immutable components.
It does not implement Nara's stable-ID receipt and no-lifecycle-effect persistent publication tail.

Godot [SceneState::_instantiate](../../repo-ref/godot/scene/resources/packed_scene.cpp#L226)
constructs node classes, then applies properties and later resolves node references/connections.
[Node construction](../../repo-ref/godot/scene/resources/packed_scene.cpp#L389) and
[property assignment](../../repo-ref/godot/scene/resources/packed_scene.cpp#L566) are actual object
operations, not a side-effect-free target admission phase. This does not prove arbitrary effects
can be rolled back after a later problem.

**Nara inference:** R4 remains necessary because Nara intentionally accepts a stronger persistent
write contract. Derive existence/insertion facts after the permitted flush, but revalidate the
original asset baseline instead of recapturing it. Do not replace the contract with a copy of live
reflection mutation, or require users to remember a manual pre-apply flush. Keep frozen codecs and
target admission distinct; reference similarity cannot qualify the regression.

## 5. Owner-Directed Preparation, Not Universal Unforgeability (R5 / PB6)

Bevy's [RenderAsset](../../repo-ref/bevy/crates/bevy_render/src/render_asset.rs#L42) associates a
source type, a SystemParam and a preparation operation. Its
[preparation driver](../../repo-ref/bevy/crates/bevy_render/src/render_asset.rs#L393) supplies a
specific source/id pair and stores the returned GPU asset. However,
[RenderAssets::insert](../../repo-ref/bevy/crates/bevy_render/src/render_asset.rs#L220) publicly accepts
an ID and prepared value, and `GpuImage` has public fields. Do not claim that Bevy's complete public
surface statically prevents every mismatched manual insertion.

The current driver also removes an existing GPU asset when processing a newly extracted source
before the replacement is ready, even if upload is deferred. Its
[GpuImage prepare](../../repo-ref/bevy/crates/bevy_render/src/texture/gpu_image.rs#L62) may write new
pixels into a compatible previous texture and reuse it. Those are valid choices in its render-World
workflow, not the same semantics as retained immutable Nara packet snapshots across old/new content.

Godot [ImageTexture::set_image/update](../../repo-ref/godot/scene/resources/image_texture.cpp#L67)
owns a texture RID and changes the associated backend object. Replacement preserves the high-level
resource identity; update validates size, format and mipmaps, then updates the texture. The RD
[texture storage](../../repo-ref/godot/servers/rendering/renderer_rd/storage_rd/texture_storage.cpp#L1632)
looks up the owned RID, validates shape and sends the pixels to the rendering device. A compatible
RID can therefore refer to changing contents, rather than an immutable content revision.

**Nara inference:** Keep an Image-owned coherent prepared capture and eliminate arbitrary
identity/pixel assembly in ordinary packet construction. Do not justify it with an inaccurate claim
of equivalent upstream restrictions. Bevy has real multiple RenderAsset implementations; its generic
store is earned there. That does not justify retaining Nara's generic store if its only production
consumer remains Image. Conversely, deletion remains conditional on the actual consumer inventory,
not a general argument that generic stores are bad. Avoid per-frame pixel hashing and new generic
traits solely to simulate the association that Image can preserve at capture time.

## 6. Recovery Authority Needs a Different Lifetime (R6 / PB7)

Bevy's [source-event routing](../../repo-ref/bevy/crates/bevy_asset/src/server/mod.rs#L1985) triggers
reload on source add/modify or metadata modification; removed source/folder observations refresh
parent folders rather than immediately executing Nara's source-Image revocation protocol. Its
[reload path](../../repo-ref/bevy/crates/bevy_asset/src/server/mod.rs#L776) uses current path handles
and loads again. Failed loading is recorded in
[AssetInfos](../../repo-ref/bevy/crates/bevy_asset/src/server/info.rs#L685). This does not establish
successful Remove followed by bounded resolver-rejection recovery, because the selected removal
contract is different.

Godot [EditorFileSystem::_reimport_file](../../repo-ref/godot/editor/file_system/editor_file_system.cpp#L2806)
reads existing `.import` params before combining defaults/overrides. After calling the importer, it
[writes the sidecar](../../repo-ref/godot/editor/file_system/editor_file_system.cpp#L2935), recording
`valid=false` on import error and writing `[params]` regardless of that outcome. Desired import
settings thus have a durable owner separate from successful output. This is not proof that all
sidecar races, watcher admission or whole-graph recovery are solved.

The comparison prompted an alternative worth challenging: in
[resolver admission](../../crates/nara_asset/src/reload/resolution.rs#L304), a queue item/byte capacity
failure currently goes through `reject_asset_reload`, which changes the stored load state and its
revision even though Image never received an attempt. An idle Removed baseline could retain the
same exact receipt if that rejection only emitted the existing event/diagnostic. That is useful to
consider before inventing identity machinery, but it does not close the complete recovery workflow.

Independent challenge found the decisive wider sequence: successful Remove, recovery attempt A
admitted into Loading, then source-only Modified B rejected by resolver capacity. Routing advances
generation before capacity checking. Leaving state unchanged can strand Loading; terminating A with
Failed advances the exact revision and again erases its recovery receipt. A's stale result skips the
[normal failure continuation](../../crates/nara_image/src/reload.rs#L962). Retaining A or moving
supersession behind queue admission changes stale-result policy and expands the admission protocol.

This does not prove every such retry fails to load: Image may already have retained an admitted
[DesiredImageRecipe](../../crates/nara_image/src/reload.rs#L698), which can independently supply
settings. Tests must observe recovery eligibility itself, not just eventual load success that another
mechanism can rescue. Pair the positive transition with same-version external state-write rejection.

There is also an existing intentional visible contract: Nara's
[modified-publication rejection test](../../crates/nara_asset/src/reload/tests_requests.rs#L160)
expects Failed while retaining a usable last-good value. Source comparison does not automatically
justify changing that contract or treating all unadmitted observations as harmless.

**Selection after challenge:** Retain the bounded private continuity-witness design, not a new queue
reservation/predecessor-retention protocol. Separate pixel-free removal recovery eligibility from
one attempt's exact baseline, while preserving current Failed/generation/diagnostic semantics.
Only enumerated owner transitions carry eligibility forward; actual external state/slot writes,
replaced stores, invalid metadata and real revocation must still invalidate it. Each concrete
publication uses the unchanged exact admission guard. This is Nara-specific design work, not a
mechanism proved by upstream similarity.

Diagnostic-only capacity rejection remains an alternative with unresolved broader state-policy
costs, not the selected easier repair. The witness likewise needs an exact finite transition table
and negative tests; an unchanged slot, error string or numeric revision delta is insufficient.
No generic writer-provenance log, persistent recovery framework or unconditional new token family
is warranted.

## 7. Domain Ergonomics and Honest Dependency Names (E1/E2)

Bevy exposes the complete dependency as
[`bevy::ecs`](../../repo-ref/bevy/crates/bevy_internal/src/lib.rs#L48), with a feature-selected
[prelude](../../repo-ref/bevy/crates/bevy_internal/src/prelude.rs#L1). This is not Nara's selected
authoring facade plus separately complete dependency path. It neither proves nor disproves the
specific names `upstream` and `bevy_ecs`; that remains a Nara API clarity decision.

Bevy also uses [cached SystemState](../../repo-ref/bevy/crates/bevy_render/src/render_asset.rs#L256)
for exclusive-world work. Its [SystemState API](../../repo-ref/bevy/crates/bevy_ecs/src/system/function_system.rs#L298)
checks World identity, validates access, and separates get/get_mut from explicit deferred apply.
Manual SystemState is not inherently broken or inappropriate Rust engine design. Cache it when
the actual parameter state requires persistence; do not casually recreate stateful message cursors.

At the product operation level Bevy provides
[WorldSceneExt](../../repo-ref/bevy/crates/bevy_scene/src/spawn.rs#L13), with immediate and queued
Scene operations. Godot provides
[SceneTree current/change APIs](../../repo-ref/godot/scene/main/scene_tree.cpp#L1681). Godot's
change-to-node detaches the old scene, keeps the pending new scene identity, and only later attaches
it and emits success; it is not an all-old-or-all-new atomic transition or Nara's source/receipt
contract.

**Nara inference:** Keep E1 for the repeated actual consumer choreography, not as a claim that
SystemState should never appear. Choose a borrowed current-Scene view and scoped ProjectScenes
entry sharing the existing domain rules, with no resource removal/reinsertion or implicit flush.
Preserve the original types and six adapted derives. Rename full access to `bevy_ecs` only for
explicit dependency identity; do not add a vendor abstraction, broaden the prelude, or claim that
Bevy/Godot prescribes the name.

## Consequences for Adoption

- R1/R2/R3/R4 remain justified by Nara's actual contracts. Upstream mechanisms inform owner shapes,
  not equivalent qualification or performance guarantees.
- R5 remains owner-directed and must preserve code-first and retained immutable packet consumers.
  Do not copy mutable GPU reuse into immutable-snapshot identity without a separate revision policy.
- R6 retains bounded recovery continuity after challenge of a diagnostic-only alternative. Cover
  Loading supersession and distinguish receipt continuity from retained-recipe rescue.
- E1/E2 remain distinct and lower priority than correctness repair. Explicit full dependency access
  is not a replacement for complete ordinary domain workflows.
- No production fix or Accepted ADR change is implemented here. Adopt with current-source
  regressions, public consumers and affected native evidence; no comparative benchmark was run.

## Verification Limits

The detailed review reads local pinned source and Nara design documents, with selected fixed-commit
GitHub source retrieval as a supplement. Reference `git status` and HEAD were checked; both reference
trees stayed read-only. No Rust or C++ implementation was executed.
The study does not establish race-freedom, destructor failure recovery, security parity or relative
performance of either engine. Local links are review aids; the exact hashes above define the
comparison even if the ignored reference checkouts later move.

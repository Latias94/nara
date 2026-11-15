# Persistent Two-Level Game And Content Lifecycle

## Outcome

Prove one small Rust-authored top-down exploration game with cross-level travel, pickups and
an explicit single-slot checkpoint save. It loads a second level, returns to the first, preserves
explicit game state across a fresh process, and runs from a source-carrying export outside the
checkout. Use ordinary public product interfaces. Preserve the existing Reference Game's wave
CLI, deterministic regression output and fixed candidate layout.

The user selected this exploration consumer rather than extending the wave-combat fixture.
Use a separate `samples/two-level-game` consumer after P1's focused public delivery tests.
Checkpoint saves reconstruct transient simulation; arbitrary-tick quicksave is a separate contract.

The user authorizes implementation and fearless refactoring. This plan does not reopen completed
foundation/locality units or create a new Goal. Root owns integration and serial Cargo/fmt/Git;
research and implementation lanes have disjoint live file ownership. Preserve untracked vendor.

## Existing Contracts And Pressure

- Strategy selects a Rust-first integrated production path, generalized from complete consumers.
- ADRs 0037/0117 distinguish asset identity, immutable published values, dependencies and retention.
  A copied handle does not acquire or retain content.
- ADR 0110 originally delivered exactly the startup closure. P1 extends that decision explicitly
  for a finite declared Scene-root set; a second Scene on disk alone is insufficient export authority.
- ADR 0027 keeps game saves separate from authoring Scene/Prefab files and backend-native state.
- ADR 0089 remains Proposed. Adopt only the concrete lifecycle behavior the consumer proves;
  do not implicitly accept its complete additive-loading or world-partition design.

At this plan's starting source, content loading is hard-wired to `startup.default_scene`. A generic selected-Scene
snapshot must bind its exact source and retain only its own dependency closure. Existing runtime
activation retains the startup source; changing a lower Scene receipt alone would leave Retry
and content retention pointing at the old level. Correct those owners before exposing travel.

## Ordered Units

### P1: Selected Scene Content And Exact Delivery

Declare project-wide `[content] scenes = ["level-b.scene.json"]`, relative to the effective
`paths.scenes`. Startup is an implicit delivery root. Bound the explicit list to 64 entries,
reject duplicate/invalid entries and preserve strict unknown-field admission. This is a finite
declaration ceiling, not a gameplay capacity or performance claim. Profiles do not independently
override the list.

Generalize `ProjectContentSnapshot` to one selected Scene and its dependencies. `load` selects
startup; `load_scene` uses an explicit admitted path without changing manifest bytes or settings
lineage. Rename startup-only data getters to Scene getters rather than preserving misleading
compatibility aliases. Keep startup attestation as a distinct proof available only for the actual
startup selection. Source/closure facts retain exact root, manifest, schema and member authority.

Produce a separately named delivery receipt from the sorted union of startup and declared roots.
Use the same loader/traversal for every root. Load and validate sequentially, retain only bounded
source facts after each snapshot is dropped, and keep the existing two-generation budget for
active plus prepared content. Charge union work and retained facts to the shared content budget.
Identical path/kind/digest facts deduplicate; conflicting facts reject. Export reopens these exact
members and does not discover dependencies or include arbitrary unused content. Build and startup
agreements remain distinct from the delivery union.

Acceptance: independent public consumer loads different roots under the same authority; failed
loads preserve prior snapshots; source-path/digest/schema mismatches reject; budgets release after
the owning snapshots/receipts retire. Export includes the second Scene's transitive Prefab/Image/
TileSet/metadata dependencies, excludes unused content, rejects changes before publication, and
loads both roots from the published tree under a different working directory.

### P2: One Committed Level Owner And Travel

Unify the currently committed source retention and Scene-instance receipt under one owner.
Startup is its first installation. Prepare the next admitted source without mutating the active
level, then commit at the existing owner/simulation safe point. Use the current protected Scene
transaction for lifecycle-free data and explicit product resources; do not allow arbitrary native
effects inside a rollback promise. Retry uses the committed level's source, not the original
startup source. Failure before commit retains the active level; later domain reconstruction has
an explicit degraded/retry result rather than pretending native rollback.

Retire the Host's decoded startup snapshot when its live ownership has transferred; retain
startup admission facts separately for Editor freshness comparisons. A legitimate current level B
must not make the Editor compare B's content revision against the original startup A. Products
must not maintain a second independently updated instance receipt for Retry.

Track decoded-generation lifetime with a shared, acyclic lease held by the snapshot, retained
Scene source and escaped Image values. Source-only closure/delivery facts do not retain it.
Old render packets can legitimately keep A alive after B commits: preparation of C must respect
the same two-generation ceiling until those consumers retire. Do not make Image retention hold
the containing snapshot or silently expand the generation budget. Scene replacement and resource
residency are distinct contracts; preflight identity/dependency changes and explicitly retire
only the old level's unshared resources under their existing owners.

Implement this in three cohesive slices: first the current Scene source/receipt owner, acyclic
generation retention and Reference Game Retry migration; then concrete project-content residency
preflight/commit/retirement; finally owned background preparation and the two-level consumer.
Keep data-only replacement usable without importing the filesystem or a graphics backend. Do not
expose full hosted travel after only the first slice, when destination resources cannot yet be
installed and retired correctly.

The residency slice uses one staged AssetServer for component decoding and destination resource
bindings. A narrow nominal Scene asset-bindings input may carry an otherwise empty component batch;
it must not accept unrelated staged component writes or expose scratch entities across calls.
Prepare Image versions, exact owned catalog changes, TileSet publication/retirement and aggregate
retention before the existing Scene commit. The protected exclusive operation then has no recoverable
resource-publication failure after the Scene changes. Do not implement World rollback or copy the
startup publisher's incremental reconciliation into travel. Native GPU/domain reconstruction keeps
its separate accepted-but-degraded semantics.

Keep a concrete current-level residency receipt, not a universal asset refcount service. Retire
only publications still matching that receipt, preserve foreign catalog records, and keep historical
AssetServer identity bindings. Empty destinations still retire old content. Shared identities may
keep their handles, but publish the destination's admitted decoded Image values so the current level
does not indefinitely retain its predecessor's generation. Preserve TileSet atlas-layout barriers;
old packets retain their own immutable values until physical retirement.

The owned preparation entry must use the original Host authority and shared loader budget, admit
only startup or declared delivery roots, and expose the prepared target read-only before commit.
Games validate logical checkpoint/progress references and build ordinary owned product state there,
then reuse the protected overlay writer. Do not resolve a second independent plan in gameplay or
convert project content into direct sources to bypass residency. Preparation completion is distinct
from playable reconstruction; deletion facts and dynamic objects become usable only after the
consumer's finite reconstruction succeeds.

Use current tasks only where blocking preparation requires them. Reject stale project/runtime/
request results and preserve cancellation and physical retirement. Keep the logical level path,
source retention and actual Scene identity distinct. Release is proven by owner/resource/packet
retirement, not by copied handle counts.

Acceptance: A -> B -> A; rejected B keeps A; Retry in B restarts B; repeated travel does not leak
identities or source budgets; two instances of one source remain distinct; pending preparation
across runtime close cannot publish late. Exercise actual product Hosts and ordinary game controls.

### P3: Versioned Game-Owned Save And Restore

Build the first format in the game consumer before extracting a general persistence interface.
Use ordinary private gameplay and save DTO/I/O modules behind a thin library and the supported
headless/desktop entries; existing local Cargo crates remain available when real reuse warrants
them. Run save/travel acceptance through exported headless arguments rather than an unpackaged
third probe binary. Test a deep gameplay-module change through Cargo without overstating the
separate closed five-file build-input observation as a complete source snapshot.

Capture an explicit selection of level identity, progress/resources, persistent dynamic entities
and deletion facts. Use durable identities and versioned migration. Do not serialize an ambient
World, runtime Entity values, task state or backend handles; do not infer save eligibility from
Scene/Inspector metadata or add an unproven universal derive flag.

Restore validates data and required content before committing game state. Native/resource state
is reconstructed from logical data under its existing owner. Preserve a usable current game on
preparation rejection and report reconstruction failures explicitly. Save I/O uses existing local
publication/conflict contracts, with application-owned location and bounded data.

Acceptance: save in B, terminate, restore in a fresh process, return to A, retain picked/deleted
objects and progress; cover two source instances, dynamic identity, old-format migration, corrupt
or missing content and interrupted/conflicting writes. Exported runs use the same game interfaces.

## November 13 marker reconstruction recovery

The existing two-level consumer exposes `Degraded` after a committed marker loses its runtime
representation or is legally retired. Retry previously reused live markers without rebuilding
their components, and retained dead tokens when allocating replacements. Both paths therefore
remained degraded without any failed Scene preparation to retry.

Repair this consumer's existing reconstruction owner. Validate each retained token's original
World identity domain and exact runtime-only live identity before reuse. Remove physically absent
owned tokens, reject ambiguous live identities, and restore checkpoint-backed Transform/Sprite
values and root placement through the existing identity and hierarchy writers. Keep one bounded
owned token set and validate the resulting all-World marker set. A missing authored Scene object
or native reconstruction failure retains its explicit failure behavior.
Later repair failure may retain partial game-owned component/entity/hierarchy effects and returns
the caller to `Degraded`; this consumer does not promise rollback.

Two complete managed HeadlessRun regressions first create markers across Meadow, Cavern 0 and
Cavern 1, return to Cavern 0, then remove its marker representation or legally retire the marker.
They observe natural degradation and issue the real Retry command. Recovery must preserve the
exact original committed `CurrentScene` receipt/source, Progress and ActiveLevel; restore exact
components, game identity and root membership; retain stable running frames and an unrelated
runtime entity; then travel again and retire the old marker. No replacement Scene transaction,
second runtime owner, checkpoint mutation or reconstruction of other levels is admitted.

The pre-fix release-mode run `1264b200-a5ff-405d-824a-5cb94e1875e1` passes all 30 existing tests
and fails both new cases at Retry remaining `Degraded`. This is a combined existing-control and
RED run, not a separate pristine-source baseline. Native physics/audio reconstruction, general
rollback, arbitrary missing authored components and other platform execution remain open.

The corrected release-mode run `acd1f0dc-0021-40c1-a41f-3e55b2f9719a` passes 32/32 with no
skips: both recovery trajectories and all original save, fresh-process restore, travel and
retained-source controls. It uses Rust 1.95.0, locked offline dependencies, the original root
target, one build job and serial nextest execution on Windows. Independent review finds no
confirmed P1/P2; foreign-token refusal, Parent-to-Root repair and later repair failure remain
source-reviewed branches rather than separately executed scenarios. This is CPU/headless
evidence, not native desktop or cross-platform qualification.
All-target correctness Clippy and sample formatting checks pass. Existing dependency and
headless-entry warnings remain; no warning-free or desktop-feature qualification is claimed.

## Source-Free Two-Level Player Successor

Status: implemented at `bb02fed` and locally qualified on Windows x64, continuing from
`a3ee53e`. Carry the actual
Two-Level Game through the existing cooked
content, captured Cargo Player and whole-directory publication owners. The current source
entry and reconstruction Host remain controls. Do not substitute another toy game, hand-built
catalog, test-only executable or manually copied pack for the ordinary game-owned cooker and
Player path.

Preserve the project's `ProjectLocation` Scene identities. Assign explicit complete shipping
Scene Products in the finite game level catalog; both levels are delivery/travel roots. Source
travel keeps its declared authoring paths; processed travel selects its complete Products from
the actual committed Scene representation. Use the existing Image importer, verified groups,
Schema compiler, reference resolver, Runtime Image codec and deep catalog validation. Do not
add another cooker, package format, loader, Scene or native-process authority.

The cooker and Player use one fixed game recipe with explicit Runtime Image codec selection.
The existing configured `RunOptions` values participate in the plugin-plan fingerprint, so a
Player must not reinterpret a fixed catalog with a different Save/Restore recipe. Admit its
finite launch intent through the original gameplay queue at the first fixed tick only, before
ordinary gameplay. Keep the original Storage/SaveSlot and close participant when opening the
explicit application save path; reject malformed, duplicate, late or incompatible intent.
Do not weaken configuration fingerprinting or expand the exact reconstruction inventory.

Generate the original independently expected bootstrap before building the Player, declare it
and the actual game/build inputs in Cargo's existing source inventory, retain content/source
observations through build and publication, and handle original rejected/orphaned/unresolved
package custody. A normal build without a generated expectation must refuse to run as a cooked
Player. No command-line/manifest override may authorize the opened pack. Reuse the original
root target, locked offline Rust1.95, one build job and serial Cargo.

Qualify the production cooker against both actual levels and Image assets. Publish and relocate
the exact three-file package. Remove only the test's owned copied source/cache/build-input tree,
run from an unrelated empty directory with Cargo/rustc unavailable, and execute the existing
Travel and Save trajectories. A fresh process restores the same checkpoint, independent level
instances, pickup progress and dynamic marker identities before returning to Meadow. Assert
actual processed resource content and original Scene receipts, not only decoded save DTOs.
Corrupt the later Cavern payload after a valid publication: startup must remain possible, the
actual travel must reject and preserve the original Meadow state, and no source fallback may
occur. Reject a foreign/corrupt catalog at startup and retain actual native exit/cleanup evidence.

Keep product launch policy, gameplay, checkpoint compatibility and finite packaging inventory
in this sample. The existing public production owners remain authoritative. Independently review
the launch/travel adapter, content/code fences and actual-process acceptance; run owning sample
and affected original processed/package controls. Record exact failures, platform/toolchain and
dependency scope before local Conventional Commits. This headless native delivery does not
qualify desktop presentation, server stripping, processed exact-state reconstruction, foreign
execution or full R7/R10/R12/Foundation. Editor/Dialogue product work remains paused.

### Cooked Player Implementation And Initial Controls

The sample now supplies ordinary `cook` and `player` binaries and one fixed `cooked_recipe`.
Its Image codec is explicitly selected as `two-level-game.cooked-image` revision one. The
shipping primary Scene Products are Meadow `624ebd5d-b7b1-4da9-94f8-86d689caeac1` and
Cavern `0c9fa5da-20ef-4c87-a559-c733bc231350`; original Image metadata and `ProjectLocation`
Scene ownership remain unchanged. The assembler consumes the actual saved Scene envelopes,
native Schema and real PNG imports, with exactly two Scene and two Image members. No engine
source, dependency or lockfile changes are required at this checkpoint.

Configured `RunOptions` do enter the original plugin-plan fingerprint; that comparison is not
relaxed. A versioned first-tick gameplay submission selects the finite Player scenario and
explicit application save path after fixed-recipe startup. It uses the original Storage Arc,
SaveSlot and registered Runtime close participant. Exact-state reconstruction remains source-only
and rejects this unrepresented processed placement. The source entry and development Host keep
their existing configurable recipes and command/lifecycle owners.

The first production library check passes. Original library controls, including the new launch
cases, pass 80/80 in `d5af7dc3-0fd0-4de6-8377-209a7c227c39` (5.380 seconds). Independent
review then identifies a real mixed-command rejection flaw: a late launch returns before normal
same-batch releases and saves. RED `9155fbd0-f56f-41e9-bb55-ddca429c2615` fails the new real
Headless trajectory with held movement still true, a changed second-tick pose and no save. The
receiver now rejects only launch intent, continues ordinary commands and preserves the launch
error after the batch. That regression passes in the first full package run.

The first full package run `d9bc2910-4cb0-40ff-af65-8b786e4ca9bd` passes 133/135, with four
actual-build/private-worker cases outside ordinary selection. Its two new fence tests correctly
expose comparison of identities from different capability sessions. The correction uses the
original live-directory `same_object` operation, then opens and compares file identities within
the retained original session. Code, build script and generated bootstrap identities/digests and
the exact code inventory remain checked through publication. Rebinding code does not replace
the earlier content baseline. The complete final selected-source run remains required.

Source review separately corrects publication failure reporting, unreaped Cargo custody and
Player output. An orphan now reports its exact original staging name/identity and cleanup
failure for explicit recovery; no guessed recursive cleanup occurs. Pending Cargo cleanup keeps
the original child and target lease until actual reaping, with a notice rather than a false
timeout completion. Player diagnostics and result writes use fallible `Write`, so a broken pipe
cannot replace an already captured original panic. The current resident Image set is observed
from actual `Assets<ImageAsset>` on every observation, separately from historical pixel proofs.
Independent review of the game adapter, cooker, Player and actual-process acceptance closes its
reported P1/P2 findings; source review is not actual generated-Player execution evidence.

The ordinary fallback Player compiles without a generated bootstrap and refuses execution.
The real generated branch is included only after the cooker writes its original expected bytes;
using `include!` permits formatting a clean source checkout without inventing placeholder
constants. `build.rs` is now part of the original development source fence, copied actual-build
workspace and declared Cargo inventories. Final native delivery, generated-branch compilation,
remaining checks and actual source/platform limits are still pending at this checkpoint.

### Cooked Player Native Qualification

The final ordinary selected package passes 135/135 in
`46b86a59-a52c-4fff-b9c8-69b2fdcc1270` (7.617 seconds), with the actual-build and private-worker
cases outside default selection. It retains all source gameplay, checkpoint, exact resume,
development Host and command/protocol controls alongside the new launch, source-fence and
fallback-Player checks. Broken stdout/stderr writes are exercised through the actual output
writer functions. These CPU results do not compile or execute the conditional generated Player.

The first actual production delivery run `89f5492f-f4e3-464e-bdb0-f3d021b72f13` fails before
Cargo: the Image importer receives the project root instead of the original assets directory
capability and correctly refuses absent metadata. The correction opens the admitted assets
subdirectory for physical import while retaining the unchanged logical `assets` scope and
original project-root content observations. No importer fallback or relaxed identity is added.

Actual release run `fca444e5-b81b-4847-b4f7-6c1035658d33` then passes 1/1 in 616.083 seconds,
including the initial explicit-target dependency/Player build. This executes the production
cooker and genuinely compiles the generated Player branch. It verifies exact captured code,
bootstrap/catalog/pack digests, the four-member closure and original three-file publication,
relocates the directory, then deletes only the checked owned source/cache/build-input copy.
The original engine source and shared target remain. Normal process, containment, output-reader
and drained-output receipts pass with no forced kill.

Six fresh sealed Player processes cover ordinary startup, complete Travel, Save, fresh Restore,
postpublication Cavern corruption and independent catalog corruption. Their actual executable
location supplies the pack, an unrelated working directory stays empty and an invalid PATH
makes Cargo/rustc unavailable. Actual processed Scene representations preserve the original
`ProjectLocation` source universe, six live entity receipts and level transitions. Both Image
Products retain their original 16x16 sRGB RGBA bytes and complete hashes; current residency is
distinct from the historical Image proofs. Save/Restore checks 30 points, three pickups, marker
IDs 1/2/3 in distinct logical level instances, next ID four and continued return to Meadow.
The later corrupted Cavern rejects only its demand: Meadow's original receipt, ten points and
marker one remain. Catalog corruption refuses before a gameplay snapshot. Neither failure can
fall back to the removed source.

This is Windows x64, Rust1.95, locked offline release, jobs one and the original target. The
616-second acceptance duration includes compilation and all scenarios; it is not a gameplay
frame time, startup distribution or build-performance improvement. Owning all-target
`cook,cooked-player` correctness Clippy and format checks pass; Clippy's clean-checkout Player
selection is the fallback branch, while the actual delivery separately compiles and runs the
generated branch. Existing non-correctness/dependency warnings remain. The original source
development Host's actual edit/adoption regression also passes 1/1 in
`135f346f-659f-481a-801d-48898c524a3b` (306.235 seconds, including a cold dev-profile dependency
build). It retains real body/private-structure edits, probe disposal, canonical restore, source
drift and compile-failure preservation, route exchange and original physical retirement after
adding `build.rs` to its copied/frozen inventory. Its intentional compiler rejection remains
part of that passing control, not a failed acceptance run. Final diff review removes one
redundant identical launch-skip check; affected launch/level controls pass 11/11 in
`53cf1c98-dcff-41e7-a5f7-b0c8086ea564` (0.459 seconds). Final selected correctness Clippy,
formatting, added local links/headings, manifest/lock/YAML parsing and Git whitespace checks
pass. These overlapping source selections are recorded separately rather than summed into an
aggregate test count. All ten scoped research/implementation/review agents are closed.

CI's existing ordinary Two-Level headless step now enables `cook,cooked-player` to cover these
CPU/fallback contracts. The expensive actual-delivery test stays explicitly selected by its
documented filter; its private ignored worker is not a standalone test. Remote CI has not run.
No dependency/lock version, engine source, runtime format or accepted owner changes. Desktop
processed presentation, server stripping, processed exact resume, foreign platforms and full
R7/R10/R12/Foundation remain open. Paused Editor/Dialogue work is preserved.

## Follow-Up Triggers

The first required physics/audio domain proves its own native acquisition, teardown and
reconstruction workflow before a portable provider abstraction. Multi-document Editor tooling,
Tile authoring, new job kernels, broad async traits, dynamic native ABI and browser execution
remain outside these units. Extract touched private responsibilities when implementation warrants
it, preserving one authoritative lifecycle rather than distributing its state across modules.

CI correctness continues independently. Observe hosted results at implementation boundaries;
do not idle waiting for the complete matrix or infer success from an earlier source. Cache/profile
changes remain measured work under the separate CI plan.

After P1-P3, audit compiler deprecations against the supported Rust toolchain and inspect the
actual enabled dependency graph of `bevy_ecs`, `bevy_tasks` and `nara_tasks`. Measure build and
runtime costs and distinguish ECS-internal scheduling from product task custody before selecting
reuse, removal or a narrower implementation. In particular, investigate the reported atomic
`fetch_update` to `try_update` rename. Coexisting package names alone do not prove duplicate
execution or justify replacing a scheduler.

The [preliminary dependency and deprecation study](../research/2026-10-21-task-dependency-and-deprecation-preliminary.md)
records the supported rename and enabled Bevy feature graph. It does not establish actual duplicate
worker pools or select a replacement; measure those costs after the complete consumer is qualified.

## Sources

- [Strategy](../../STRATEGY.md)
- [Module/goal review](../audits/2026-10-20-module-goal-alignment-review.md)
- [ADR 0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
- [ADR 0037](../architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md)
- [ADR 0027](../architecture/adr/0027-save-game-and-runtime-persistence.md)
- [Proposed ADR 0089](../architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md)
- [CI repair](2026-10-20-ci-portability-repair-plan.md)

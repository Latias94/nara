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
- ADR 0110 currently delivers exactly the startup closure. A second Scene on disk is insufficient
  export authority. Extend that decision explicitly for a finite declared Scene-root set.
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

## Sources

- [Strategy](../../STRATEGY.md)
- [Module/goal review](../audits/2026-10-20-module-goal-alignment-review.md)
- [ADR 0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
- [ADR 0037](../architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md)
- [ADR 0027](../architecture/adr/0027-save-game-and-runtime-persistence.md)
- [Proposed ADR 0089](../architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md)
- [CI repair](2026-10-20-ci-portability-repair-plan.md)

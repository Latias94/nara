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

## Source-Free Desktop Player Successor

Status: implemented at `4267672`, with native-cleanup corrections `1fa730d`/`b92fd2e`, and locally
qualified on Windows x64/DX12, continuing from `cf68a77`.
Extend the actual cooker/Player game delivery above to a separately selected native desktop
placement. Retain the qualified headless path. Select
headless versus desktop explicitly in the game-owned cooker; Cargo features alone must not
silently choose a content configuration or change the fixed headless recipe. Desktop content
uses the project's actual desktop profile and the existing native game input plugin, Scene
materialization, Winit session and external WGPU Host. Code-first keyboard declarations remain
game-owned executable configuration unless a consumed authored Input asset requires the
original immutable Input codec. Do not add a second mutable mapping owner.

Use one fixed desktop cooked recipe in cooker and Player, preserving all fingerprint, source,
bootstrap, exact Cargo selection and whole-directory publication checks. The Player opens only
its compiled expected pack. Start the original DesktopRun initially paused, complete genuine
native presentation/paused maintenance, submit any finite first-tick launch intent through the
existing port, then resume through the original control ticket. Ordinary mode remains running
until normal native close. An explicit absolute save slot supports the same K/L actions.
Scripted Travel/Save/Restore and first-presentation verification are finite optional entry modes;
they do not replace the ordinary playable path or authorize a source fallback.

An opt-in bounded stdout observation stream may report actual post-Host game state and matching
Runtime/App/render/window/surface/device presentation facts for external qualification. It is
not another mutation channel, mandatory per-frame production telemetry, screenshot or GPU
duration proof. Correlate original facts rather than label an arbitrary current snapshot as an
older presented frame. Preserve original failure payload and physically retire Runtime, native
window, presentation resources and pump before terminal success. Incomplete cleanup remains
explicit; no independent window/GPU/input manager or Editor UI is introduced.

Qualify a package built by the production cooker, relocated with only its three declared files
and no copied source/cache/Cargo/rustc at execution. On Windows, use matched actual process/
thread/window authority for native key Down/Up and normal Close. Drive the ordinary running
game through movement/pickup, marker, travel/instance, checkpoint Save and fresh-process Restore;
prove restored independent-instance progress and continued play. Restore any injected holds and
the previously observed foreground window during success/failure cleanup. Do not operate on
unmatched external windows. Require genuine presentations after distinguishable game changes,
both exact processed Images and Scene receipts, normal original process containment/readers/
output retirement, and original native window destruction. Explicit adapter selection must fail
when unavailable. Separate these facts from any existing offscreen/source replay pixel evidence.

Retain malformed/bootstrap/fingerprint/source-drift refusal and the original headless delivery
control. Independently review recipe/configuration, startup/control and physical acceptance,
then run focused serial CPU and affected native controls, correctness Clippy, formatting and
document checks. Record exact executed backend, adapter, platform and remaining limits before
local Conventional Commits. This advances native game delivery without completing foreign
platforms, server stripping, processed exact reconstruction, UI/Dialogue or full Foundation.

### Desktop Implementation And Initial Controls

The game-owned cooker now selects `--runtime headless|desktop` independently of its compiled
Cargo features. `cooked_recipe()` excludes the native input plugin in either feature closure;
`cooked_desktop_recipe()` retains the existing compiled keyboard bindings and the same Runtime
Image codec. The desktop selection uses `player_desktop`, the actual `desktop` project profile,
`cooked-player,desktop`, and content profile `two-level-desktop-shipping`. Incompatible window
settings and an authored startup Input selection are refused before cooking. The original
content/code fence, generated expectation, captured build and directory publication remain
the owners of package agreement.

The new desktop entry retains the original initially paused DesktopRun, genuine post-Host
presentation receipts, first-tick launch command and exact Resume ticket. Optional observations
join original Runtime/App/render/window/surface/backend identities. Ordinary interactive mode
retains startup and the most recent Scene receipts in a bounded history; explicit finite
verification keeps its complete bounded trajectory. Diagnostic exhaustion stops optional
Observation records while reserving Ready/Result output. Recoverable gameplay operation errors
remain observable without closing ordinary play. Terminal output follows original Runtime and
event-loop retirement, and an original failure payload survives later cleanup.

Initial no-desktop-feature selection controls pass **2/2**, run
`6bc1ac89-302c-446d-91db-fa653f8070f5`, with Rust1.95.0 release/locked/offline and jobs1. This is
recipe/selection evidence only. Desktop feature compilation, generated branch execution and
the full physical source-free Save/fresh-Restore workflow still require the following native
qualification; no new desktop success or performance claim is inferred from this result.

The desktop-enabled entry/selection controls pass **17/17**, run
`891bde44-f97e-4adc-9fc1-5a68d2ec8884`. These include actual clean-checkout refusal for both
ungenerated Players, explicit feature-independent headless selection, and count/byte observation
exhaustion with usable Ready/Result output. The initial release build took11m41s in this local
cache state; this is not an edit-to-result or runtime cost budget. The generated desktop branch
remains outside that compile scope until the production cooker builds it.

Independent review identified and corrected three ordinary-session defects before native
qualification: observer-enabled Scene history could end play at the seventeenth activation;
diagnostic limits could end continued movement; recoverable K/L refusal could close the game.
The revised physical trajectory includes a missing-save Load followed by normal movement and
20 additional E travels after fresh Restore, retaining unchanged stored checkpoint bytes.
The cleanup review also found that an empty process tree plus `force_killed=false` did not prove
absence of earlier native termination. The original process owner now records all termination
attempts and skips automatic residual-tree termination when containment is already empty. This
desktop acceptance requires `termination_requested=false` as well as its game/native-close facts.

The shared process/output/log/wake controls pass **34/34**, with903 unrelated tests filtered,
run `17d41301-9f3c-4d78-b0dc-62f21357ecd9`. The new actual contained-process cases distinguish a
naturally exited parent/descendant tree, a successful direct exit leaving a live descendant that
requires native termination, and explicit termination/retry without force-kill escalation. This
qualifies the receipt fact on Windows x64/Rust1.95.0; the retained Linux WNOWAIT contract is
source-reviewed here and is not new Linux execution evidence.

The desktop-enabled sample's complete ordinary all-target cohort passes **153/153**, with13
native/actual-build cases skipped, run `2ccdc432-6f9b-4a41-9d07-7d22ea9f8b52`. The first attempt
stopped at four test-helper `json!([false;4])` macro syntax errors before executing tests;
explicit four-boolean arrays preserve the same held-key assertions. The corrected run retains
checkpoint, source-change preservation, complete semantic resume and both ungenerated Player
controls. It compiles all current native wrappers, while their ignored actual workflows require
separate execution. No generated desktop or physical-play success is inferred from this cohort.

The first production desktop delivery run, `93bef06e-0143-47e4-b846-fffef4bbedfd`, compiled the
real generated desktop Player and published its three files before stopping at a package-test
configuration comparison. The catalog canonicalizes the capability set order, while the test
compared the manifest's original vector order. Comparing complete configurations after the same
public `resolve()` validator preserves every effective setting and set membership without giving
declaration order semantic authority. Original plugin/Schema/codec fingerprints and later native
checks remain unchanged. The failed run does not establish physical play or source-free execution.

The second run, `2bb3668a-b47c-4938-8bf6-49403134538a`, passed package checks and source/cache
removal, then started the sealed Player through a real native presentation and Ready. It stopped
when Windows denied the background test thread's direct `SetForegroundWindow` call before input.
A subsequent temporary foreground input-queue join still failed actual activation. That attempt
was removed; the helper instead follows the checked-in Winit Windows implementation's matched
Alt transition before requesting foreground. Physical Alt must start Up and return to Up, and
the target retains actual matching foreground PID/thread/HWND/title checks before every game key.
Failure/normal paths release injected holds and restore the originally observed foreground.
No game-command substitution, production focus-policy change or desktop configuration change
is used. Failed attempts remain failed acceptance; their cleanup receipts retain any actual
termination intervention rather than treating absence of force-kill escalation as graceful exit.

Subsequent native iterations preserve all physical assertions. The ordinary retry after an
expected missing-save Load clears the recoverable error through gameplay before pickup assertions;
the original error is not erased by the observer. A libtest worker prefix required an explicit
newline before the machine report. The full hardware Save/fresh-Restore/20-travel trajectory and
software first-present control then completed, but the final CWD check found the driver-created
`NVIDIA Corporation` directory. With normal user profile variables deliberately absent, the
driver had fallen back to the working directory. The Player command now explicitly receives
test-owned local and roaming application-data directories for native driver caches; its PATH
remains invalid, source/cache absent, and unrelated CWD must still stay empty. The immutable
three-file installation is checked separately. This does not add a game filesystem fallback.

The sample build script now watches the existing `src/bin/player` directory and additionally
watches `generated.rs` only while it exists. Tracking a missing file alone caused Cargo to rerun
the script and rebuild sample binaries on every invocation. Both clean-checkout refusal and
real cooker-generated selection remain the acceptance for these cfg branches. The local change
does not establish a generalized build-speed or edit-latency budget.

### Desktop Player Native Qualification

Final ordinary sample qualification passes **155/155**, with13 actual-build/native cases skipped,
run `10766283-8c10-4877-97ba-8022ccb548ef`. This reruns the full affected cohort after the original
launch-result observation was added, including exact resume and both ungenerated-entry refusal
paths. The independently locked public process consumer passes **3/3**, run
`5d1d5ad5-25c3-4b14-af4f-c08127105ff6`: delayed protocol bytes survive diagnostic overflow,
exact handle-based product execution retains its sealed environment/EOF stdin, and original
exit/containment/readers/output cleanup now also requires no native termination request.

Final startup qualification also distinguishes ingress admission from application. The original
gameplay consumer retains its first launch result as a read-only diagnostic in the existing
Session; `player_launch_result(&World)` does not create mutation or resume authority. The desktop
entry joins this actual result at the first positive fixed tick, while Ready still requires the
original Resume ticket and initialized running state. It does not infer launch failure from a
generic snapshot error: an ordinary Save can be refused in the same first-tick batch. CPU controls
cover no-launch action refusal and preserving the first applied result after a later rejected
launch. The final native trajectory adds invalid initial save-path rejection and separately runs
the entry's finite Travel/Save/Restore modes, retaining all ordinary physical assertions below.

The complete production desktop delivery passes **1/1**, with the private worker excluded,
run `d1213e73-40d4-4f8e-bf10-3dba1b32a75a`, completing on November16 local time. Rust1.95.0,
release/locked/offline, jobs1 and the original root target are retained. The74.688-second actual
test includes its inner Cargo work; it is not a runtime or edit-latency measurement.

The actual cooker generates the desktop expectation and builds `player_desktop` with exactly
`cooked-player,desktop`. Package configuration is compared after full public validation, including
the desktop Window960×540 and WorldCanvas320×180, no authored startup Input, plugin/Schema/codec
fingerprints and original Scene/Image Products. The three-file package is relocated; only the
checked copied source/import-cache trees are removed. All execution uses unrelated CWD and invalid
PATH with no source or Rust tools; explicit test-owned local/roaming/common data directories keep
native driver caches out of both CWD and installation. The NVIDIA cache requires the common-data
projection on this machine. CWD remains empty and package bytes remain unchanged at completion.

Two ordinary Players execute on **NVIDIA GeForce RTX4090 / DX12 / discrete-gpu**. Physical scan-code
Down/Up events are accepted only while the exact process/thread/HWND/title is foreground. Missing
Load remains recoverable, Retry clears the error through gameplay, D moves and collects each near
pickup, release preserves idle position, and M allocates markers1/2/3 across Meadow0/Cavern0/
Cavern1. Q/E/R retain independently reconstructed progress and K publishes the actual checkpoint:
score30, three distinct collected facts, three exact recorded marker positions and next ID4.
The first original native window closes normally. A fresh process begins at Meadow score0; L
restores Cavern0, M allocates4, Q/E recover the other instance/level markers, then20 extra E travels
prove bounded history does not cap ordinary play. The stored checkpoint remains byte-for-byte
unchanged throughout continued play. Current Scene and historical receipts remain live six-entity
Runtime representations with original ProjectLocation identity; both complete1024-byte sRGB CPU
Images match their independent source golden hashes, and resident Images follow the actual level.

A third finite `--verify-first-present` process selects **Microsoft Basic Render Driver / DX12 /
cpu** with explicit fallback. These three processes observe112/148/6 genuine presented
frames respectively; those counts are fixture facts, not a frame-rate budget. Every original
Runtime and event pump retires, its HWND is destroyed, and the shared native owner reports exit0,
empty containment, stopped readers, drained output, no termination request and no force-kill.
Matched injected holds are released and the original foreground is restored. Optional observations
retain matching Runtime/App/render/window/surface/device identities and distinguish bounded history
sampling from complete finite history.

The same final run executes three additional hardware-backed finite Players: `--verify-travel`,
`--verify-save` and a fresh `--verify-restore` all complete the actual scripted progress at score30,
both Images, expected current-level markers and initialized/verified running state before normal
retirement. A seventh actual Player uses an invalid absolute initial save path: its original
launch is admitted but rejected by gameplay at tick1, `launch_accepted=false` and `ready=false`,
and exit1 is expected after complete native cleanup. All seven process receipts require empty
containment, stopped readers, drained output, original window destruction and no termination
request or force-kill. The earlier three-process native result
`74487dc3-1a02-49e5-a23c-fa69e23fd785` is superseded by this full final source/entry scope.

This is actual source-free native presentation and keyboard-gameplay evidence. It does not
establish frontbuffer pixels, physical scanout, GPU duration, numeric performance budgets, Vulkan,
Linux/macOS, authored Input products, processed exact-resume, server stripping, Editor/Dialogue
integration or complete R7/R10/R12/Foundation. The existing source/offscreen pixel proofs keep
their separate scope. Independent recipe, entry and physical/cleanup reviews are closed after
the corrections above. Final static and headless closure follows below.

The original actual headless delivery also passes **1/1**, run
`c6448aa2-c4d4-4493-8458-043d18f4fd4f`, through the same **desktop-enabled cooker** and its
default headless runtime selection. Its original compiled Player, three-file relocation,
source/cache removal, six sealed processes, Travel/Save/fresh Restore, Deferred corruption
preservation, catalog refusal and cleanup assertions are retained. The obsolete test-only
prohibition on a desktop-enabled cooker is removed because placement is now explicit; package
selection and fingerprint checks remain. This277.267-second test includes the newly compiled
inner feature closure and establishes no cross-cache compilation performance comparison.

The final headless delivery is repeated after the launch-result observation change and passes
**1/1**, run `2bfe241b-723f-4b68-b9d1-fee961c7e53c`, retaining all six original isolated Players.
Root library correctness Clippy with `runtime-2d,serde,project-tooling` and sample all-target
correctness Clippy with `cook,cooked-player,desktop` pass under Rust1.95.0/release/locked/offline.
Those lints run in the clean checkout's ungenerated cfg branch; actual generated branches are
compiled and executed by the production delivery tests. Existing non-correctness warnings remain.
Root/sample/independent-consumer `cargo fmt --check`, manifest parsing, local documentation links
and ordinary whitespace checks pass. No dependency/lock versions change. Local implementation
and qualification commits retain paused UI/Dialogue work separately.

A final full-installation digest check reuses the same physical trajectory and hashes every
executable/pack/manifest byte before and after all seven Players. Its first run,
`f12cc30b-111a-4af7-8473-0325f78110a5`, completed the worker and all Player assertions but refused
the outer cleanup receipt: Windows still reported a nonempty ancestor Job at the worker's exit
instant, so the supervisor attempted termination. The new receipt correctly exposed that
intervention instead of reporting graceful cleanup. The original owner at `b92fd2e` allows a fixed250ms
natural containment-drain observation after direct-child exit before terminating residuals.
Polling does not renew that deadline; genuine remaining descendants still enter the original
bounded terminate/escalation path. This race correction is qualified separately below; the
earlier successful seven-Player run remains evidence for its original source scope.

The corrected root process cohort passes **35/35**, with903 unrelated cases filtered, run
`b42372f2-dfbe-4acf-997d-6e924b175a8b`. Actual long-lived residuals are explicitly observed before
the fixed deadline, remain unterminated just before it, then require termination at the deadline;
the allowance does not renew. A separate short-lived descendant completes naturally with no
termination. That short-lived case does not synchronize first observation against the child's
exit and is not alone proof that the waiting branch executes. Independent reliability review
finds no P1/P2 in the selected correction. Final native/public-consumer closure follows below.

The final strengthened seven-Player desktop acceptance passes **1/1**, run
`7e93d11f-69e0-42ff-a9f0-14b9111c51e4`, after `b92fd2e`. It retains every physical, scripted,
startup-refusal and cleanup assertion above, adds full before/after digests for the executable,
pack and manifest, and preserves the empty unrelated CWD. All seven original native receipts and
the outer worker complete without termination intervention. The hardware/software adapter scope,
112/148/6 ordinary/finite presented-frame observations and unchanged stored checkpoint remain.
The297.665-second actual test includes inner recompilation after the process-owner correction;
it is not a runtime performance sample. This supersedes the earlier desktop runs for the final
source and complete installation-immutability scope.

The independently locked process consumer is repeated against `b92fd2e` and passes **3/3**, run
`8bd9c181-8a30-4c1f-b964-07cfa076f969`, retaining exact launch, sealed environment, complete protocol
output and no-intervention cleanup assertions. Its feature closure is compiled separately from
the root and game; the recorded3m51s build is a local cache fact, not runtime cost evidence.

The final headless production delivery also passes **1/1** after `b92fd2e`, run
`8c169ec6-adcb-4f4d-9573-d8d481ad84d2`. All six original isolated Players, generated headless
code/content agreement, corruption preservation, catalog refusal and cleanup remain intact under
the desktop-enabled cooker's default headless selection. Its278.535-second execution includes
the separate inner feature-closure build and does not establish a performance change.

Final root library and sample all-target correctness Clippy, followed by root/sample/independent
format checks, pass against `b92fd2e` after these actual delivery runs. Selected document links,
anchors, manifests/locks and whitespace checks pass. All12 scoped research, implementation and
review agents are closed. No dependency/lock versions, UI defaults or desktop display settings
change. Runtime delivery advances while foreign execution, numeric cost budgets, processed exact
reconstruction, server stripping and the remaining Foundation requirements stay open.

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

# ADR 0117: 2D Foundation Ownership and Authoring Contracts

**Status**: Accepted
**Date**: 2026-10-15
**Amended**: 2026-10-23 for explicit phase spatial cuts, common asset mutation admission and
borrowed/scoped current-Scene access, 2026-11-01 for captured View lifetimes, 2026-11-14 for
admitted material/Shader generations, and 2026-11-21 for stock window overlay-only frames
**Owner**: game composition, managed execution, content, domain plugins, rendering, and tooling
**Implementation Plan**: [2D foundation contracts](../../plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md)

## Context

The source studies and correctness audit show author-facing execution choreography, duplicate
schema registration, load-attempt state hiding published content, primitive-kind depth ordering,
and persistence restrictions that cannot accommodate arbitrary native side effects honestly.
The user authorized a pre-1.0 refactor before adding more engine domains. The aim is a coherent
general-purpose 2D authoring path, not implementing every Proposed ADR or promising no future refactor.

## Decision

This ADR refines only the clauses listed below. Unrelated guarantees in the referenced ADRs remain
binding. Acceptance authorizes the changed contracts; it is not implementation or verification evidence.

The shared stock-construction selection in
[ADR0094](0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#shared-stock-program-construction)
retains ordered 2D Views and captured WorldCanvas mapping while making world-raster effects,
optional Canvas resolution and final full-resolution UI explicit wiring choices. Effects name
their actual pixel space; target-sized effects do not silently inherit fixed-world coordinates.
The default policy uses the same lowering as external policies, with original capture, UI
completion, producer and native retirement ownership unchanged.

### One game definition and one execution owner

App configures systems, resources, plugin contributions, and persistent schema. The resulting
definition is frozen once per executable composition and consumed by managed execution. Normal
headless, desktop, and embedded use share startup, fault, stop, and physical-close ownership.
Candidate staging and publication remain private; raw App execution is removed as a competing
production lifecycle after consumers migrate. This refines ADRs 0003/0084 without removing retryable
task retirement, sticky faults, or generation isolation.

Driver execution preserves a normally returned output independently of a subsequent authority fault.
The concrete Host reconciles acquired owners and committed bookkeeping before applying fault policy;
it must not retry an already-consumed native completion. Pre-invocation rejection returns move-only
input through the typed owned port. This refines scoped execution under ADR 0076, not transaction
rollback or recovery from arbitrary user panics.

One typed plugin declaration contributes systems, durable component IDs, migrations, and cleanup
responsibility. Provider/binding receipts remain internal composition facts. Durable owner/type/field
IDs, owner omission semantics, tombstones, and migration requirements remain unchanged (0046/0081/0098).

Strict Check/Export uses an explicit mode on the compiled project's existing headless executable,
before gameplay starts. The generic CLI can discover/build that executable before schema-dependent
strict admission. The project process checks using its own frozen definition and owns the admitted
directory capabilities, closure, build agreement, preparation, and publication. Cross-process summaries
do not transfer authority. Generic Editor recovery stays data-only and does not load project Rust.
This refines the generic CLI flow under ADRs 0110/0114, retaining source-race and product-agreement checks.

### Persistent data and native effects have different transaction semantics

Persistent document validation, migration, reference resolution, and revision publication are atomic.
Domain systems reconcile accepted configuration into transient native state and own its update and
cleanup. A plain persistent component may also be runtime data; separate representations are needed
only where native ownership/effects demand them.

A rejected document keeps its prior revision. Failed runtime reconciliation reports the accepted
revision as degraded/unavailable and supports explicit retry or reconstruction. It does not promise
rollback of arbitrary native effects. Startup candidates remain unpublished on failed startup.
Strict old-or-new native activation requires a concrete domain's prepare/activate protocol.

This refines the universal reading of persistent-binding/retirement guarantees in ADRs
0006/0027/0081/0100/0101. Existing narrow lifecycle-free transactions retain their stronger guarantees;
their guards remain until affected consumers move to the new owning seam. RequiredComponents and
hooks are not simply enabled inside an existing rollback-protected mutation. The first proof uses
the real Image pipeline and a hook-owning transient fixture, not a universal hydrator or physics SDK.

The finite external consumer composes TaskPlugin-owned asynchronous image import, image preparation,
and a hook/RequiredComponents-owning native object that retains the exact prepared image snapshot.
It proves accepted-revision retry, no late-completion resurrection after unload, and physical
Stop/CloseIncomplete/RetryClose ownership without a common coordination interface. Ordinary
ImagePlugin import budgets end at publication; immutable resident pixels can remain owned after
that work budget is released. Editor `commit_retained` accounting remains a separate policy.
This evidence does not qualify production physics/audio, arbitrary native rollback or every
project-host successor composition.

### Identity and resource state remain independently meaningful

The [authored marker consumer](../../plans/2026-10-20-persistent-two-level-game-plan.md#authored-marker-instantiation-successor)
requires explicit instantiation of an already resident Immediate Scene dependency. Hosted
inspection and instantiation use the original current content generation, exact component
definitions and full expected current receipt. The declared travel roots govern travel. Source
and processed candidates borrow or reuse their retained content. Callers retain each resulting
activation receipt and retire it before its containing Scene resource owner ends. The current
[textured marker consumer](../../plans/2026-10-20-persistent-two-level-game-plan.md#textured-prefab-runtime-successor)
uses an occurrence-local entity reference and a required Image declared by the marker Scene.
Ordinary Sprite binding resolves that Image from the containing Scene's retained resource closure.

The existing lifecycle-free product replacement transaction accepts explicit runtime-only entity
tokens and additional complete Scene activation receipts. Its identity owner prepares one aggregate
retirement/tombstone sequence. The Scene owner preflights the complete membership, hierarchy,
persistent apply and lifecycle union before mutation, then ends every original occurrence in the
same exclusive publication tail. Duplicate, stale, foreign and overlapping selections refuse.
The retirement limit uses each activation's original membership. Occurrence admission budgets
the activations retained after commit, and final hierarchy publication uses the live placements
and the same admitted retirement union.
Native retirement claims apply only to the original primary activation. Runtime repair uses the
original receipt and legal retirement. The owner rejects stale identity left by raw ECS despawn.

Logical asset UUID, content locator, derivation key, and process-local handle identify different facts.
The initial supported profile has one principal durable product and one entity-bearing document per
source; importer-private auxiliaries do not imply a public product graph. Authored entity IDs remain
document-local and opaque. Prefab provenance is structured; live lookup requires instance selection.
Expanded Scene materialization carries one immutable document/provenance candidate. Source coverage
and projection association are admitted before allocation or replacement retirement, and source
fields publish inside that prepared commit. One private current-Scene owner retains the admitted
source and its matching instance receipt. Startup installs that pair; protected restart and direct
data-only replacement advance it together. A read-only view does not expose a clonable candidate
or grant a competing authority. Hosted content replacement additionally requires concrete resource
residency preparation and retirement; Scene data transaction success alone is insufficient.

Scheduled `CurrentScene` reads and `CurrentScene::get(&World)` borrow that same private owner.
The exclusive view carries no independent source lease; borrowed source/receipt access cannot
coexist with a later mutable World commit. `ProjectScenes::scope` offers the same finite request,
inspection, cancellation, retry and acknowledgement operations to exclusive callers, using the
same predicates as the system parameter. It neither removes resources nor implicitly flushes,
advances, commits or acknowledges. Prepared inspection produces owned game data before an explicit
exclusive commit; cancelling requests retain their slot until physical return and terminal ack.

For one hosted current Scene, CPU residency uses one staged asset catalogue shared with component
decoding. Before Scene retirement, prepare exact Image slot/state replacements, owned TileSet
source changes, merged project catalogue ownership and aggregate retained-byte admission. The
exclusive commit then publishes these already validated changes with the new source/instance pair.
Retire only still-matching old owned publications and catalogue records; preserve foreign values,
foreign records and historical handle identities. Shared handles acquire the destination's admitted
Image allocation, rather than retaining the predecessor's decoded generation indefinitely. This is
a finite current-level receipt, not a general asset reference-counting service or native rollback.

Hosted Scene preparation uses the original candidate, frozen RuntimePlan and shared content-loader
budget. Travel admits startup or declared delivery roots. Existing I/O task workers prepare content
without the World; the owner exposes a read-only target before an explicit protected commit. One
request/result slot remains occupied until cancellation physically returns, and terminal results
require exact-ID acknowledgement. Runtime close revokes publication interest and existing Tasks
retain physical worker custody. A later selected closure is not another executable startup
agreement: its records decode against the already admitted frozen native schema authority.

Additive activation uses the same request slot for companion Scenes selected from the primary
Scene's admitted Deferred dependencies. The source request binds the captured target Product and
its canonical path. The processed request uses a validated `Reference` edge of the retained
primary closure. Each committed addition has its own occurrence and exact unload receipt in the
same World. Additions coexist within the Host's original content and Scene limits.

The Host publishes one identical Image, Material, TileSet or source catalog record for all live Scenes that use it.
Unload retires the exact occurrence and releases publications whose last live Scene leaves.
Before retiring any entity, the Host admits the resources required by each nonempty publication
batch. A TileSet retirement needs its Image and Asset event stores even when every Image is shared
and no Material retires. A refused unload leaves the original occurrence available for retry.
A failed reload of the same last-good Image or Material still permits retirement. After lower-level occurrence
retirement, the Host can finish only the outstanding resource release. Prepared primary travel
includes all live Host additions in its existing retirement and resource publication transaction.
Managed travel revalidates that group before native work and at commit. The retained-source
Restart still requires closed additions. Retained render inputs keep their captured allocations.
Material use retains the complete Shader/declaration/defaults and resolved texture selections.
Sharing requires the original live slot and matching definition; edited Material or WGSL content
does not silently replace another Scene's publication. Binding and prepared drawing lifetimes
remain with their original captured generation. TileSet sharing uses the original declaration
claim and exact Image capture through `TileSetAssets`; it does not republish the live slot or
version. A changed declaration refuses a new region, and the existing atlas rebuild barrier
remains. The last live user ends the owned declaration through that same owner. General native
service lifetimes still require additive contracts under ADR 0089.
[Concurrent regions workflow](../../plans/2026-10-20-persistent-two-level-game-plan.md#concurrent-regions-and-coordinated-travel-successor)
[Material region workflow](../../plans/2026-10-20-persistent-two-level-game-plan.md#material-backed-concurrent-regions-successor)
[TileSet region workflow](../../plans/2026-10-20-persistent-two-level-game-plan.md#tileset-backed-concurrent-regions-successor)

Source component values may use component-local immutable storage, independent of raw documents,
resolvers and snapshots. Public source record/document cloning preserves independent values;
borrowed access does not grant a detachable storage handle. Only private compilation may share
payload storage through drafts into move-only compiled values. Missing defaults may retain closed
selected insertions over that unchanged backing; reads never consult receiving definitions or
retain an independent owning handle. Actual migration, properties and changed reference lowering
isolate owning data before mutation. Already isolated migration output may accept defaults in
place. Requesting mutable access must not eagerly copy a value that the operation will only read.

Source, compiled and independently prepared owners each retain their full logical payload
allowance, including raw backing and selected frozen-insertion overhead, even when they share the
same allocation. Independent cooked output admits and copies complete effective data. Existing
retained leases, generation claims and per-occurrence materialized limits remain authoritative. Shared
payloads do not deduplicate budgets, own a source document or introduce another lifecycle owner.
Public source clones remain independent allocations, so they cannot extend original storage
after its original retained allowance is refunded. The source wire representation is unchanged.

Hosted source changes enter this same Scene owner as semantic principal/metadata observations.
The successful closure and latest executed automatic failure's requested paths determine dependency
interest, including a missing input. Relevant changes coalesce into one successor intent behind
the original request/result slot; incomplete observation requests a full source rescan. A frozen
Ready value is never re-read or silently replaced by a later notification, and game state still
publishes through the explicit Scene commit/overlay. Stop and cancelled/stale preparation revoke
interest without releasing physical worker custody early. Processed bases have no source route.
This Scene-level route does not invent an asset UUID or publish through a competing generic
asset reloader. Composition must select the actual publication owner; mixed asset/Scene routing
requires explicit arbitration for shared real targets. Actual qualification and remaining watcher
scope belong to the [Material/source successor plan](../../plans/2026-11-14-durable-material-and-shader-plan.md#scene-owned-source-updates).

Optional native Scene observation binds its explicit absolute project path to the original source
loader's directory identity during actual Host admission. It establishes coverage before initial
reconciliation. Lost observation replaces the sticky queue and its watcher before requesting a
full rescan; metadata/catalogue structural changes also require full revalidation. The source
adapter selects this Scene route exclusively and conflicts with the stock generic watcher.
Processed admission refuses it before acquiring native resources. Native watching and retirement
remain with the existing watcher library and Runtime close owner: every acquired original worker
is retained through setup failure, facade/receipt Drop and replacement until actual join. A
read-only retirement observer grants no factory, publication or close authority. Platform-specific
worker join, OS cleanup errors and Scene reconciliation have distinct evidence and failure states.

Managed Scene travel captures one owned attempt from each registered native domain before any
retirement begins. Each domain returns the exact runtime component types whose lifecycle effects it
owns together with the matching attempt prepared against the immutable target plus current World.
Preparation may fail without changing the current Scene or native owners. The Scene owner validates
the complete persistent and content candidate before advancing any attempt, then retains that candidate and the participants'
attempts in the request slot. The interface is intentionally limited to retirement and
reconciliation; it is not a shared event bus, general transaction framework, or rollback promise.
Callbacks run synchronously on the runtime owner thread and must be nonblocking; asynchronous
pending work is outside this protocol. `Send + Sync` permits Bevy Resource storage, not background
execution. Because TaskUpdate begins on a later frame, the owner revalidates mutable admission
facts immediately before the first native call. Failure at that point discards the unpublished
candidate and rejects the request with the old Scene intact. Queued cancellation also discards the
candidate before any native call. Both release the simulation gate on the next frame once disposal
finishes. If intervening lifecycle registration prevents safe owner access or candidate disposal,
the request reports a degraded result and retains its candidate, attempts, and cleanup intent.
Explicit retry advances only that retained operation; Stop remains available. An unsafe cleanup
never destroys the request's custody or reports a completed cancellation.

Retirement and reconciliation report completion independently per participant. A failed phase keeps
its attempt and partial native state in custody; explicit retry advances only unfinished phases.
Once retirement has started, stale admission checks cannot discard that custody. The App holds
simulation stages and virtual time for the whole frame whenever managed travel is queued or active;
owner maintenance, task progress, cleanup, and rendering remain available. Retry does not imply
restoring retired native state. During Stop, domain plugin shutdown owns native handle cleanup, and
the request retains unfinished attempts until the World is destroyed. Unclaimed lifecycle hooks
remain ineligible for Scene replacement admission.

Decoded project snapshots, retained Scene sources and escaped immutable Images share an acyclic
generation lease. Source-only closure and delivery receipts retain source facts independently.
The selected decoded-generation ceiling follows the last real decoded owner, including prepared
render packets. The source recipe may set the ceiling and the default is two. Hosts retain separate
startup admission facts for freshness comparisons and release
their decoded startup snapshots once ownership transfers to live sources and resource values.
Saved dynamic identity and an explicit authored-instance binding do not establish a universal save format.

Source rename preserves identity. Copy-as-new changes IDs and internal copied references; conflicting
mounted UUIDs fail instead of silently remapping immutable packs. Runtime export is a bake/convert
operation, not a lossless authoring round trip. This narrows the admitted work under
0037/0038/0058/0101/0103 without accepting Proposed 0083/0087/0088 wholesale.

Source Scene replacement can relocate a primary Image or Material's weak path binding in its
existing transaction. The admitted source catalogue establishes one location for the stable ID.
The World must retain the expected prior binding, and its destination must be available. The new
catalogue, path binding and typed publication share the same commit. A prior Scene's retirement
may leave a weak binding, so an exact relocation can also preserve that handle on re-entry.

Published usable value/revision is independent from the latest load attempt and diagnostic.
Preparation consumes the published revision even if a later attempt failed. Removal, identity
invalidation, or successful replacement invalidates the corresponding derivation. Async completions
still validate their owner/revision at publication. This clarifies ADRs 0033/0037/0115.

Managed Asset mutation admits the actual typed value/state pairing before capturing one owned
baseline. Success, failure and removal validate that same baseline before effects; resource
replacement or empty-slot ABA cannot be legitimized by capturing a new unrelated pair. Image
retains desired metadata independently of in-flight work and last-good pixels, with publication,
slot and metadata-observation binding. Removal revocation precedes request admission. ADR 0037
owns relative metadata/removal recovery and runtime-owned watcher rescan custody.

File-backed TileSets publish their validated atlas together with the exact immutable Image content
that passed extent admission. A later valid Image publication may advance a Sprite independently;
an incompatible TileSet successor retains its coherent eligible last-good atlas and pixels. An
Image publication epoch distinguishes replacement from removal or identity revocation. The current
managed Image publication must also match the exact storage slot: raw mutable storage access cannot
authorize retained derived content. New Tilemap extraction also requires a usable managed TileSet
publication in its exact storage slot. A revoked TileSet can retain its CPU value and an authorized
Image without authorizing new draws. Already captured draws keep their original content. Resolved
TileSet fields are private. Changing its Image/atlas through a code-first builder discards the
admitted content snapshot.

Material rendering freezes its effective binding and complete texture Image snapshots during
the original Extract. Authored Material textures use managed Image publication admission;
direct code-first bindings use the existing typed Image snapshot without a catalog requirement.
One immutable prepared Material is shared across Sprite, Tilemap cells, Views and batches. Queue
uses these exact snapshots instead of resolving original handles against a later Image cache.
Later replacement or retirement does not redirect an already captured draw. The next Extract
still admits current authored publication facts and refuses an unmanaged slot substitution.

The concrete TileSet asset owner keeps one latest Source or source-independent Product declaration
separately from its published Image dependency. ADR 0033 owns their shared admission and publication
contract. A Product descriptor names complete TileSet/Image identities and actual atlas/material
values without source records, paths or import hashes. Source adapters still require real database
and Image provenance; processed adapters prove complete bindings and actual Image content. Both
use the same reconciliation, claim, atlas barrier, mutation baseline and retained-pixel owner.
Failed or missing desired dependencies do not erase published dependency revocation.
Current metadata admission and source/catalog changes drive recovery, including a dependency path
that initially has no metadata. Reconciliation does not scan the filesystem or introduce a generic
import executor; Host capabilities own discovery and existing Image work owns decoding/publication.
Runtime snapshot publication, managed Editor refresh and TaskUpdate use the same concrete
`TileSetContent` SystemParam for source requests and reconciliation. Its resource join is private;
the independently testable `TileSetAssets` state machine retains publication and retention ownership.
Host code supplies admitted decoded sources, rejection or revocation observations and dependency demand,
without transferring filesystem authority to the domain or introducing an erased importer executor.
Unchanged dependency observations do not repeat content failures. An unchanged candidate previously
refused by the retention limit becomes eligible when its exact Image allocation fits the current
allowance. Entry and retained-pixel limits reject a successor before publication, and retained
charges follow actual immutable holders through prepared packets.
After the first successful publication, an entry retains its admitted atlas layout even if its
Image or source is later automatically revoked. A live successor with a different layout is rejected with an explicit
rebuild-required diagnostic; the latest desired source remains available for correction. This
prevents changed local indices or UV interpretation without a migration. Explicit TileSet removal
and fresh admission, including strict project reopen, may establish another layout. Image retargets
with the same layout and appearance-only changes remain eligible for live reconciliation.

Persisted cells with an admitted TileSet must belong to its atlas before strict Check, startup or
Export publishes the content closure. The current identifier is an atlas-local `TileIndex`, not a
durable tile definition ID. Empty maps and explicitly untextured maps remain valid. Rendering keeps
defensive checks for code-first runtime mutation. Tile authoring tools and durable tile-ID migration
require a separate workflow and decision.

Prepared rendering carries exact snapshots, so multiple Image revisions sharing a logical handle
may coexist in a frame. Batching and backend reuse distinguish those snapshots; bounded cache and
submitted-work retention account for coexistence instead of overwriting one logical-key entry.

### Structural mutation uses one baseline

Admitted deferred work is materialized before resolving structural topology. Preparation and commit
share a semantic baseline with no intervening external flush. Every admitted hierarchy participant,
including a childless root, participates in lifetime invalidation. Raw ECS escapes invalidate
completion rather than act as an equivalent second writer. Ordered structural transactions,
hierarchy/lifetime separation, and completed transform projection in ADRs 0097/0100/0101 remain binding.

Transform completion publishes bounded private immutable cuts rather than Global ECS components or
caller-joined completion tokens. Historical simulation reads retain their original stamp during
mutation; Fixed Resolve and Finalize/Capture read the completed tick cut, and Extract uses a separate
presentation cut. Each carries World/runtime identity, full entity generations, phase/time and
captured structural order. Local edits or entity retirement after reading a cut belong to the next
publication. Non-spatial direct parents terminate inheritance; effective parent frames come from the
same cut. Missing/failed publication rejects without a live-local fallback. ADR 0100 owns these
read roles, publication checks and the explicit deferred boundary.

### 2D order, views, and presentation

Sprites and tilemaps share per-view layer/sort-group order, optional group-scoped Y sorting, and a
stable tie order. Primitive kind does not dominate user depth. Only adjacent compatible items batch.
UI has explicit overlay placement and internal painter order. This supersedes ADR 0105's fixed
Tilemap-before-Sprite ordering while preserving private execution and resource-complete transfer.

A view owns an invertible camera/projection mapping, viewport, visibility, order, and clear/load
policy. A target owns physical configuration; presentation maps it to a window/panel. A bounded
ordered view list executes serially, including shared targets and explicit offscreen sequencing.
This supersedes the single-view cardinality clauses in 0018/0094/0111/0112, retaining exclusive target
ownership. Optional pixel-canvas policies do not quantize simulation. ADR0094 separately owns the
finite typed resource program; no backend-neutral RHI is added here.

Window camera extraction intersects each authored physical viewport with the current target raster.
With a fixed WorldCanvas, that raster is the Canvas extent while the native window is renderable.
Extraction omits the View for a zero-sized window or an empty intersection. It keeps the original
camera configuration and derives projection, clear order and presented picking from the surviving
rectangles. Without Window geometry, an explicit rectangle allows extraction and the actual target
descriptor checks it at packet admission. Caller-built packets must supply valid in-bounds
rectangles.

The default stock window UI contribution can present over a full-resolution clear when every
world View is omitted. Window geometry continues to drive UI layout, painting and input. A
presented overlay-only frame carries no WorldCanvas or world-picking authority. When a View
returns after clipping, it retains its authored settings and original lifetime identity.

Each captured View carries a non-reused process-local lifetime identity, separate from its camera
Entity, target, raster extent and ordering. Camera removal/readdition starts a new lifetime even
when the Entity survives. A temporarily unavailable target, zero extent or filtered extraction
does not retire the camera's View lifetime. Automatic extraction and manually supplied Views use
the same identity type; manual producers retain their identity with their actual View owner rather
than minting one per frame. Distinct code-owned subviews may share a source camera without sharing
identity. Camera picking uses the first presented subview of that camera containing the point.
Picking by `RenderViewId` selects an exact subview and checks the same presented bounds.
Frame source plus View identity binds historical render state. Duplicate identities in
one captured packet reject rather than aliasing two Views. These are runtime facts and are not
serialized into camera authoring data. Persistent camera edits keep their normal codec/lifecycle
contracts; this refinement does not introduce RequiredComponents or native effects into document
transactions.

Per-view queueing may omit a quad only when its actual projected instance is conservatively
separated from the clip rectangle. Floating-point uncertainty, non-finite coefficients and overflow
retain the quad; material/content authorization and diagnostics still run before visibility filtering.
Extraction remains a complete world-space observation. Queue visibility does not erase an empty
view's clear/load policy, change painter order or establish cross-frame cache authority. GPU draw
admission accounts for the geometry actually submitted rather than invisible authored population.

The first ordinary pixel-canvas consumer selects an optional fixed world raster extent. World
views raster into that extent, then the completed canvas is nearest-sampled into the actual window
using `DisplayScale::IntegerFit`; native runtime UI is drawn afterward at actual window resolution.
Whole physical scaling is used when the canvas fits, with centered bars; undersized windows retain
the existing fractional full-image downscale. WindowResolution, platform-logical input, camera
world coordinates and fractional simulation remain independent of the internal raster extent.
The project profile lowers this setting into the same Render configuration used by Rust consumers.

Canvas raster, window/image composition, native UI and final submission are one closed transaction.
The same private preparation and encoding serves runtime windows and the embedded Game View;
canvas composition does not turn an image submission into a window presentation. World pointer
conversion composes the successfully presented display mapping and exact extracted camera, checks
the current destination/configuration, and excludes bars, nonfinite points and maximum edges.
Resize, scale, suspension, retirement and device/runtime replacement revoke obsolete presentation
authority. A fixed canvas allocation is retained across destination-only resize and retired by its
backend owner on configuration/extent or device/runtime replacement. This is a finite first-party
path, not a public image-target scheduler, render graph or raw backend escape.

Material declarations join stable semantic parameters/resources, actual binding ABI and Shader
interface as one compatible generation. R9b replaces the original startup-only Palette extension
through two typed layouts and tracked Shader edits; invalid or incompatible candidates retain
usable same-epoch last-good. Captures pin original artifacts, values and resource versions,
including when first realized after newer publication. Data selects admitted fields/references
without Device/Queue authority. ADR0033 owns asset publication; ADR0094 owns exact native
realization and original submission custody. The
[complete R9b consumer](../../plans/2026-11-14-durable-material-and-shader-plan.md) must qualify
Save/Reopen/Check/Export/run and native interleaving before those workflows are reported complete.
Superseded static material interfaces are removed rather than retained as a second authority.

Persistent authoring separates the reusable `Material2dAsset` Product and authored `Material2d`
component from exact render bindings. The original Scene transaction reserves typed handles,
validates the candidate's own loaded products and commits material/Image residency with its
successful Scene publication. It cannot consult the incumbent's old domain values to validate
a successor. The carrier's native schema owns durable reference traversal; concrete parameter
and texture tooling projections derive from the same admitted declaration. Missing or revoked
managed products omit affected draws with diagnostics; an unrelated stock/code-first draw
remains independently usable. Public owned source/Shader snapshots keep the original content
reservation alive after Scene retirement without introducing a second publication owner.

The first-party carrier participates in the existing persistent Runtime2d defaults and native
Editor authoring allowlist. It adds no per-material native component definition. Material
families and source/processed import codecs remain compiled recipe choices, and the Editor's
authoring lane does not execute arbitrary Runtime or Schema plugins from that recipe.

Processed Material decode supplies the typed CPU collection. A recipe can select the native
Material publication facet, which contributes to its codec fingerprint. The resolved plan
must also install the Material plugin for the prepared closure to publish a native value.
Deep validation and Scene preparation both require that publication for a native carrier.
Native Material defaults require their own Required Image edges and Image publication facets.
CPU-only consumers can acquire the same Material and typed Image dependencies in a Server plan.

### Semantic input owns held-action lifetimes

Physical button state and per-binding `ActionOutcomes` remain distinct from gameplay-facing
`SemanticActions`. Semantic state aggregates active physical bindings and endpoints per
action/context, and emits one terminal edge after the final contribution releases. Context exit
ends a delivered semantic action once; restoring that context while inputs remain held requires
release and a fresh press. Reconfiguring the mapping expires its previous held actions. Deferred
presses retain their mapping/context lifetime and cannot revive actions after a lifetime change.
One staging owner covers normalized and synthetic input and preserves observed lifetime exits even
when a prior configuration snapshot is restored. Configuration values do not themselves authorize
reviving delivered or deferred actions.
The gameplay command bridge consumes aggregate semantic edges before fixed-tick admission; zero
or multiple fixed ticks do not duplicate those edges. This refines ADR 0041 without replacing the
bounded physical timeline, independent frame cursors, UI press/release ownership or exact-step
observation contract.

### Document transactions and toolkit-neutral presentation

Non-geometric authoring edits use explicit base-revision transactions, one atomic commit and undo entry.
They are not limited by one published-frame batch. Another actor's stale transaction is rejected without
implicit rebase. Picking/dragging remains bound to the actual displayed geometry. Selection and diagnostic
acknowledgment do not consume a document mutation slot. This refines affected exact-frame action clauses
in 0078/0111/0112 without discarding displayed-frame authority.

Content, target, device, and presented-frame identities remain separate. One private presentation owner
joins them and returns an opaque token. Toolkit texture registration belongs in the native adapter,
refining 0040/0107/0111/0112. ADR 0118 owns the current Dear ImGui frontend selection and its native
qualification. ADR 0107 permits independent toolkits and selective sharing; this foundation summary
neither selects another toolkit nor requires eventual convergence.

### Validation and effect ownership

Retain bounded ingress checks, definition collision/dependency checks, revision checks at async commit,
save-race checks, and physical cleanup checks. Remove repeated same-owner proofs only after mutable
replacement escapes have been closed. The temporary reporter swap-and-restore counterexample remains
covered; equal declaration hashes never prove arbitrary factory behavior.

Strict publication preflights the actual capability instance before avoidable file effects. Failure
after creating temporary state retains verified cleanup ownership; unverified path deletion is forbidden.
SPL requests and observations belong to one private effect owner, publish only changed observations,
allow reachable close retry, and move slow publication to cancellable owned work. ADR 0116's exact
generation/handle/digest/nonce/first-present guarantees remain unchanged. Cargo/native code is trusted,
not sandboxed. This refines interface ownership, not platform guarantees.

## Consequences and Limits

Migrate starter, examples, Reference Game, and independent consumers through each corrected seam, then
remove obsolete paths. Preserve explicit durable fixture conversion and original input on failure.
Existing SPL-U0–U2 evidence is retained; unfinished SPL work is carried into foundation U12.

Use behavioral regressions and real compile/privacy consumers. Do not extend source-parsing tests into
a Rust compiler approximation or reintroduce ADR 0099's rejected proof/approval framework. No universal
package kernel, save system, script VM, native plugin hot-unload, physics/audio backend, or second UI
is required for foundation completion. Windows/Linux/macOS share the design; native release claims
require evidence for that platform. Web remains deferred.

## Evidence

- [Correctness audit](../../verification/2026-10-15-foundation-correctness-audit.md)
- [Runtime study](../../research/2026-10-15-foundation-design/runtime-concepts.md)
- [Content study](../../research/2026-10-15-foundation-design/content-concepts.md)
- [Rendering study](../../research/2026-10-15-foundation-design/render-concepts.md)
- [Resolved plan review](../../research/2026-10-15-foundation-design/plan-review.md)

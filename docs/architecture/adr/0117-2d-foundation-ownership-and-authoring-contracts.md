# ADR 0117: 2D Foundation Ownership and Authoring Contracts

**Status**: Accepted
**Date**: 2026-10-15
**Amended**: 2026-10-23 for explicit phase spatial cuts and common asset mutation admission
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

### One game definition and one execution owner

App configures systems, resources, plugin contributions, and persistent schema. The resulting
definition is frozen once per executable composition and consumed by managed execution. Normal
headless, desktop, and embedded use share startup, fault, stop, and physical-close ownership.
Candidate staging and publication remain private; raw App execution is removed as a competing
production lifecycle after consumers migrate. This refines ADRs 0003/0084 without removing retryable
task retirement, sticky faults, or generation isolation.

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

For one hosted current Scene, CPU residency uses one staged asset catalogue shared with component
decoding. Before Scene retirement, prepare exact Image slot/state replacements, owned TileSet
source changes, merged project catalogue ownership and aggregate retained-byte admission. The
exclusive commit then publishes these already validated changes with the new source/instance pair.
Retire only still-matching old owned publications and catalogue records; preserve foreign values,
foreign records and historical handle identities. Shared handles acquire the destination's admitted
Image allocation, rather than retaining the predecessor's decoded generation indefinitely. This is
a finite current-level receipt, not a general asset reference-counting service or native rollback.

Hosted Scene preparation uses the original candidate, frozen RuntimePlan and shared content-loader
budget. Admit only startup or declared delivery roots. Existing I/O task workers prepare content
without the World; the owner exposes a read-only target before an explicit protected commit. One
request/result slot remains occupied until cancellation physically returns, and terminal results
require exact-ID acknowledgement. Runtime close revokes publication interest and existing Tasks
retain physical worker custody. A later selected closure is not another executable startup
agreement: its records decode against the already admitted frozen native schema authority.

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
The existing decoded-generation ceiling follows the last real decoded owner, including prepared
render packets. Hosts retain separate startup admission facts for freshness comparisons and release
their decoded startup snapshots once ownership transfers to live sources and resource values.
Saved dynamic identity and an explicit authored-instance binding do not establish a universal save format.

Source rename preserves identity. Copy-as-new changes IDs and internal copied references; conflicting
mounted UUIDs fail instead of silently remapping immutable packs. Runtime export is a bake/convert
operation, not a lossless authoring round trip. This narrows the admitted work under
0037/0038/0058/0101/0103 without accepting Proposed 0083/0087/0088 wholesale.

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
authorize retained derived content. Resolved TileSet fields are private; changing its Image/atlas
through a code-first builder discards the admitted content snapshot.

The concrete TileSet asset owner keeps one latest desired source separately from its published Image
dependency. Failed or missing desired dependencies do not erase published dependency revocation.
Current metadata admission and source/catalog changes drive recovery, including a dependency path
that initially has no metadata. Reconciliation does not scan the filesystem or introduce a generic
import executor; Host capabilities own discovery and existing Image work owns decoding/publication.
Runtime snapshot publication, managed Editor refresh and TaskUpdate use the same concrete
`TileSetContent` SystemParam for source requests and reconciliation. Its resource join is private;
the independently testable `TileSetAssets` state machine retains publication and retention ownership.
Host code supplies admitted decoded sources, rejection or revocation observations and dependency demand,
without transferring filesystem authority to the domain or introducing an erased importer executor.
Unchanged observations do not repeat failures. Entry and retained-pixel limits reject a successor
before publication, and retained charges follow actual immutable holders through prepared packets.
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
ownership. Optional pixel-canvas policies do not quantize simulation. No public render graph or RHI is added.

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

Material contributions are trusted Rust startup declarations with parameters, textures, alpha semantics,
and supported renderer capabilities, proven by one palette example. Shader code changes require restart
in this scope. Scripts/data select admitted definitions and values without Device/Queue authority.
Unused duplicate material descriptors are removed; public intermediate renderer orchestration is narrowed.

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
refining 0040/0107/0111/0112. Existing egui remains an adapter; no replacement toolkit is selected.

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

# Assets, Builds, Change Propagation, And Residency

Date: 2026-11-02. Source audit, not a benchmark or implementation proposal.

## Scope And Evidence

Nara is pinned to `a2640a10973b6d1724176a506237bc458aa5f3bc`; reference checkouts are Bevy
`ae6152152d6fb8308ad6038314b48ef295e80fbc`, Fyrox
`b5b151b603bba0549ac03db676568726f17e6807`, and Godot
`95c0fa5fe8a019adc29d074c16b64469b04571f7`. All inspection was local and read-only. No Cargo,
network, reference-engine execution, or new performance measurements were used. Code compilation,
iteration, CI, observability, and scene/reflection/prefab design belong to the other audit owners.

The authority is STRATEGY and Accepted ADRs 0007, 0033, 0037, 0049, 0068, 0103, 0110, and 0122.
The [CP3][cp3], [CP4][cp4], and [CP5][cp5] plans record completed, bounded qualifications;
the October 30 findings are not current deficiencies. The [cooked-delivery plan][delivery]
also records a finite Windows headless Player/base-pack outcome. Proposed ADRs 0087/0088 do not
expand those claims into a general build scheduler, patch system, or streaming platform.

**CTO position:** Nara now has credible finite content production and acquisition. The next
architectural priority is connecting ordinary resource changes to their actual consumers with
predictable extension cost. More validation states or a universal asset service would not, by
themselves, establish that workflow.

## Comparison From First Principles

| Question | Nara at this revision | Bevy | Fyrox | Godot |
|---|---|---|---|---|
| What identifies content? | Source UUID plus primary/auxiliary Product; byte digest, recipe, runtime ID and version remain separate. | Asset paths/labels and typed runtime handles; processing hashes are separate. | Registry UUID/path association; shared resource object carries load state. | ResourceUID/path resolution; loaded cache uses local paths. |
| What constitutes build reuse? | Explicit definitions, normalized settings, consumed target facts, replayed raw/presence or processed observations, verified complete output group. | Source/meta hash plus processing-dependency hashes; optional background processor. | Inspected texture loader reads source/import settings; export copies or invokes loader conversion. No equivalent closed-recipe CAS appears in that path. | Timestamp-based scan filtering, then import settings/version and source/output MD5 checks. |
| What propagates change? | Source reload reverse graph, build observations, and Runtime demand inventory are distinct mechanisms; automatic connections need consumer evidence. | Processor maintains reverse dependencies and requeues dependents; runtime loading/reloading maintains separate asset state. | Shared resource replacement and loaded/reloaded events; derived consumers still require their own response. | Reimport bookkeeping and explicit resource cache replacement; replacement can update an existing object. |
| What loads by default? | Selected source or processed workflow; Immediate closure now, Deferred destination on request. Direct acquisition is synchronous; later Scene preparation uses an I/O task. | Default unprocessed mode; asynchronous asset request, usually immediate nested dependency requests. | Request returns a shared pending resource; task-pool loader completes it. | `load` waits for completion; threaded request/status/get is explicit. External resource dependencies are loaded recursively. |
| What retains and releases? | Concrete content owners and shared decoded/pixel captures; Copy identity is not ownership. | Strong handles retain; drop events remove assets during tracking. | Manager retains unused resources for a default 60 seconds, resetting while externally shared. | References retain resources; cache stores raw pointers and unregisters on destruction. |
| How deep is streaming? | Demand-driven whole-member reads and finite closure preparation; no encoded cache or automatic mip/chunk eviction in this path. | Asynchronous whole-asset infrastructure does not itself prescribe world streaming. | Whole-resource loading plus specialized streaming audio. | Pack range I/O and threaded resources do not themselves provide a world-streaming residency policy. |

## Nara: Import To Retirement

**Identity and input.** [Product identity, `product_identity.rs:53`][n-product] excludes path,
content, type and residency from equality. `ImportRecipe` records source-import provenance
(`crates/nara_asset/src/import_recipe.rs:361`); `AssetVersion` is runtime publication state,
not content addressing (`crates/nara_asset/src/state.rs:19`).
[Build facts, `model.rs:72`][n-model] separately describe implementation identity, settings,
target, raw presence/digests, processed group/member digests and output mappings. Build-input
observations are not Runtime reference edges.

**Import and cook.** [Execution, `execute.rs:477`][n-execute] loads a historical seed candidate,
replays inputs and revalidates before returning reuse. Changed inputs trigger execution; invalid
stored content is not a hit. [Publication, `store.rs:249`][n-store] rejects differing output
for an existing closed recipe, writes blobs and manifest, then replaces the index as the success
visibility boundary. Post-index failure preserves an uncertain publication receipt. This is
stronger than merely finding a cache filename, without claiming hermetic execution or universal
crash durability. Image cooking consumes a real maximum texture dimension and Reject/FitNearest
policy (`crates/nara_image/src/cook.rs:19`); it is not yet a GPU-compression target matrix.

**Package.** [Assembly, `cooked_content.rs:646`][n-package] copies exact captured group bytes,
builds a catalog with target/configuration/codec agreement, and deep-validates every delivered
member. Required and Reference edges both impose delivery obligations. Neither a Deferred
destination nor an unused extra member can bypass package validation. This concerns content
assembly; executable construction and delivery orchestration are outside this chapter.

**Acquire, reload, retire.** [Backing, `backing.rs:55`][n-backing] either borrows captured bytes
or reads and hashes one member from the original retained file. Decoded values retain their base
and charge (`src/runtime_content.rs:731`). [Traversal, `traversal.rs:71`][n-traversal] follows
Required edges in ordinary preparation and Reference edges only when requested by the traversal
policy; exact payload/declaration agreement precedes traversal
(`src/runtime_content/dependencies.rs:40`). Source reload traverses registered reverse edges
(`crates/nara_asset/src/reload/resolution.rs:178`), separately from immutable processed bases.
Image retirement removes the receiving value only after baseline checks
(`src/project_host/runtime/content_residency/images.rs:243`); escaped pixels can remain alive.

## What The Reference Implementations Actually Prove

**Bevy.** [Defaults, `crates/bevy_asset/src/lib.rs:333`][b-default], select unprocessed mode.
[Processing, `crates/bevy_asset/src/processor/mod.rs:1106`][b-process] compares source/meta
and dependency hashes; [completion, the same file at line 1614][b-propagate] replaces reverse
edges and queues dependents. This is an executing propagation mechanism, not just a dependency
manifest. It explicitly permits an old valid asset during initial processing, relying on later
reload. Direct output writes use transaction logging for recovery rather than Nara's immutable
complete-group publication. [Loading, `server/info.rs:265`][b-reuse] coalesces path/type requests
but explicitly ignores later settings transformations for an existing entry: simpler shared
loading has semantic costs. [Nested loading, `loader.rs:715`][b-nested] normally starts dependency
loads immediately. [Handle tracking, `assets.rs:570`][b-retire] performs deferred removal after
strong-handle drops. None of this proves a memory-budgeted streaming policy.

**Fyrox.** [Manager requests, `fyrox-resource/src/manager.rs:1539`][f-load] reuse an existing resource
or create a pending object and dispatch a loader. [Texture loading, `fyrox-texture/src/loader.rs:50`][f-texture]
combines import settings and actual loading; [asset export, `fyrox-build-tools/src/export/asset.rs:30`][f-export]
walks files and calls target conversion or copies them. This is a practical alternative to a
universal artifact graph, not evidence of deterministic complete-group caching. Conversion errors
are logged by this helper before it returns success, so its contract is also weaker than Nara's
group publication. [Manager update, `manager.rs:1333`][f-retire] uses reference counts and TTL
(`fyrox-resource/src/entry.rs:30` sets 60 seconds), not byte pressure. Reload keeps a usable old
value on failure (`manager.rs:1638`). [Streaming audio, `fyrox-sound/src/buffer/streaming.rs:62`][f-stream]
has a specialized decoder and single-user restriction; it must not be confused with generic
resource loading.

**Godot.** [Import scanning, `editor/file_system/editor_file_system.cpp:562`][g-import] deliberately
trusts matching modification times before deeper checks; the same file at line 673 checks
importer version/settings and stored source/output digests. [Texture importing,
`editor/import/resource_importer_texture.cpp:916`][g-texture] emits actual platform compression
variants. [Resource loading, `core/io/resource_loader.cpp:725`][g-load] distinguishes blocking
calls, shared in-flight tasks and cache reuse. [Binary dependencies,
`core/io/resource_format_binary.cpp:623`][g-deps] initiate external loads, rather than treating
every reference as a deferred address. [Replacement, `resource_loader.cpp:559`][g-replace] may
copy into the existing resource, unlike immutable generation retention. [Pack reading,
`core/io/file_access_pack.cpp:472`][g-pack] bounds reads to a member extent without Nara's hash
of every ordinary member read. [Resource destruction, `core/io/resource.cpp:790`][g-retire], removes
the raw-pointer cache entry. These are specific local trust/performance choices, not evidence
that validation is unnecessary.

## Four CTO Judgments

### 1. Freeze Edge Meaning And Give Change Propagation A Production Owner

**Problem/evidence.** The source reload graph can traverse dependents, but its population is not
an integrated consequence of the inspected import/build path. At this revision, searches of
`src/`, `crates/`, and `tests/` find `add_source_dependency` and `record_recipe` callers only in
tests; [the production APIs, `state.rs:857`][n-graph], expose manual mutation. CP5 correctly
detects a changed auxiliary file on the next explicit build, which is a different guarantee.
This does not imply that all direct Image reload or Host revalidation is broken, or that an
external application cannot populate the graph itself.

**Counterproposal.** Preserve three meanings: observed input invalidates a build; Immediate versus
Deferred determines current load demand; published dependency relationships determine which live
consumers need refresh. Give successful dependency publication/replacement one concrete owner,
including absent optional files and removed edges. Keep failed-attempt retry observations
separate from last-good published edges. Bevy also records a failed load's dependency for retry
(`processor/mod.rs:1675`). Borrow its reverse-edge maintenance without importing its
eventual-freshness policy or constructing one universal graph.

**Cost.** Moderate integration and cancellation/replacement work; less scope than a general DAG
scheduler. **Minimum acceptance:** change/create/delete an auxiliary input of a real custom
resource, refresh the dependent without a manual graph call, recover a failed first load when
its input appears, preserve last-good on failure, replace obsolete edges after success, and
prove unrelated resources do no decode work.

### 2. Freeze An Incremental Extension Contract, Not One Mandatory Pipeline

**Problem/evidence.** A small source-only resource needs `AssetDomain` and `SourceAssetImporter`;
the independent [text consumer, `src/lib.rs:32`][n-extension], already demonstrates that modest
entry point. Durable build and processed loading add separate obligations:
[`SourceAssetBuild`, `build.rs:21`][n-build-trait] and
[`RuntimeAssetCodec`, `runtime_codec.rs:114`][n-codec]. Compound identity, actual references,
retained-cost promises and Host codec selection add legitimate work, but should not become
prerequisites for every extension. Existing Host selection is already explicit and small:
`add_asset_domain::<D>()` for source and `add_runtime_asset_codec::<D, C>()` for processed use
(`docs/guides/project-asset-domains.md:5`). Native publication remains a concrete domain
responsibility.

**Counterproposal.** Keep these optional facets and document one normal extension progression:
source-only, then durable build, then packaged CPU use, then native preparation if required.
Package recurring Host wiring around a demonstrated consumer; do not add automatic importer
discovery or a generic residency executor merely to reduce the visible number of traits.
Bevy's loader registration is easier initially but does not supply Nara's authored continuity
and complete-group guarantees. Godot likewise separates importer and runtime loader contracts.

**Cost.** Small guidance/adapter work unless another domain proves duplicated execution. The
existing compound consumer proves feasibility; do not reopen CP5 to establish it again.
**Minimum acceptance for the next extension:** a new non-Image resource completes that progression entirely through
public APIs, without engine-private edits; source-only use requires neither serializable build
settings nor native residency hooks. Count required callbacks and Host registrations rather
than treating lines of fixture code as an extension-cost benchmark.

### 3. Retain Boundary Validation; Measure Repeated Proof In The Ordinary Path

**Problem/evidence.** ADR 0049 deliberately treats file-backed inputs as untrusted. That does
not make trusted Rust extensions a sandbox: `SourceAssetBuild` explicitly relies on trusted
allocation and I/O promises. Current safety work also handles mundane corruption, interrupted
publication and concurrent editing. However, [Image cook, `images.rs:243`][n-cook], decodes upstream
before lookup and decodes cached output again at line 302. Verified group loading and input
revalidation add real work even on hits. These are static costs, not measured bottlenecks.

**Counterproposal.** Separate admission of mutable external bytes from reuse of already admitted,
privately owned immutable facts. Consider retained typed validation facts only if measurements
justify them; bind them to exact bytes, codec, settings and target. A file handle alone is not
immutable content. Godot's timestamp fast path is a useful comparison with weaker guarantees,
not a drop-in replacement for Nara's current contract.

**Cost.** Measurement first; moderate lifetime complexity if validation facts are retained.
**Minimum acceptance:** compare cold build, unchanged build, one auxiliary edit and warm cook;
report bytes read/hashed, decode count, allocations and elapsed distribution. Any optimization
must still reject corrupted members, changed settings/implementation/target and stale selection,
and retain same-recipe nondeterminism detection. Do not remove protections under a global
"trusted game" switch.

### 4. Freeze Load And Release Semantics Before Choosing Streaming Policy

**Problem/evidence.** Nara has selective loading, but [preparation, `prepared.rs:54`][n-prepared],
sizes result storage from the entire catalog; traversal also allocates catalog-sized bookkeeping.
One closure deduplicates members, while independent acquisitions decode independently. The
[Scene worker, `preparation.rs:83`][n-worker], executes the finite synchronous preparation inside
an I/O task; cancellation does not preempt that decoder traversal. These distinctions matter
more than calling the engine simply "async" or "missing streaming."

**Counterproposal.** Keep explicit owners and immediate/deferred semantics. Decide whether a
named repeated-travel consumer needs shared in-flight acquisition, decoded retention or bounded
prefetch before adding an LRU. Bevy's strong handles, Fyrox's TTL and Godot's reference cache are
alternative policies, not universal solutions. A weak identity must never silently become a
residency lease.

**Cost.** Catalog-local scratch improvements are smaller than shared-cache cancellation and
generation ownership. **Minimum acceptance:** scale catalog size while fixing demanded content;
run A-B-A travel with a shared atlas, overlapping generations, held cancellation and escaped
pixels. Measure cancellation tail and physical allocation release separately from store removal.
Tests already exercise these lifetime distinctions
(`tests/fixtures/module-consumer/tests/runtime_image_delivery.rs:1543`); they do not establish
target-hardware latency or RSS.

## Decide Now; Defer With Evidence

**Decide now:** edge meanings and successful edge-replacement ownership; optional extension
facets; complete Product versus recipe/output equality; target facts actually consumed by cooking;
whether reuse promises immutable historical content or current selection; identity versus
retention; and cancellation versus physical completion. Keep existing complete-group publication
and current-demand semantics as the baseline.

**Keep protections with real consumers:** PNG expansion bounds, old/new image overlap, finite
dependency traversal, temporary pack buffers, held-worker results and escaped pixel/group owners.
Exact/one-short qualification establishes logical accounting, not hardware calibration. Defaults
such as 1 GiB retained/512 MiB work (`src/runtime_content.rs:121`) need representative workload
measurement before being presented as production capacity planning.

**Defer:** a universal scheduler/provider, remote CAS, patch precedence, automatic eviction,
subresource streaming and additional target codecs until a game requires them. Prioritize
propagation correctness and ordinary extension completion, then warm-cache proof cost and
catalog-size/repeated-travel scaling. Existing qualification remains evidence for its recorded
consumer/platform scope; all performance conclusions here remain unmeasured hypotheses.

[cp3]: ../../plans/2026-10-30-reference-validity-and-load-demand-plan.md
[cp4]: ../../plans/2026-10-31-retained-runtime-pack-plan.md
[cp5]: ../../plans/2026-10-31-local-artifact-build-plan.md
[delivery]: ../../plans/2026-10-31-cooked-player-delivery-plan.md
[n-product]: ../../../crates/nara_asset/src/product_identity.rs#L53
[n-model]: ../../../src/asset_build/model.rs#L72
[n-execute]: ../../../src/asset_build/execute.rs#L477
[n-store]: ../../../src/asset_build/store.rs#L249
[n-package]: ../../../src/project_workflow/cooked_content.rs#L646
[n-backing]: ../../../src/runtime_content/backing.rs#L55
[n-traversal]: ../../../src/runtime_content/traversal.rs#L71
[n-graph]: ../../../crates/nara_asset/src/state.rs#L857
[n-extension]: ../../../tests/fixtures/module-consumer/asset-domain-consumer/src/lib.rs#L32
[n-build-trait]: ../../../crates/nara_asset/src/build.rs#L21
[n-codec]: ../../../crates/nara_asset/src/runtime_codec.rs#L114
[n-cook]: ../../../src/asset_build/images.rs#L243
[n-prepared]: ../../../src/runtime_content/prepared.rs#L54
[n-worker]: ../../../src/project_host/runtime/scene_requests/preparation.rs#L83
[b-default]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/lib.rs#L333
[b-process]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/processor/mod.rs#L1106
[b-propagate]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/processor/mod.rs#L1614
[b-reuse]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/server/info.rs#L265
[b-nested]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/loader.rs#L715
[b-retire]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/assets.rs#L570
[f-load]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-resource/src/manager.rs#L1539
[f-texture]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-texture/src/loader.rs#L50
[f-export]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-build-tools/src/export/asset.rs#L30
[f-retire]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-resource/src/manager.rs#L1333
[f-stream]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-sound/src/buffer/streaming.rs#L62
[g-import]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/editor/file_system/editor_file_system.cpp#L562
[g-texture]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/editor/import/resource_importer_texture.cpp#L916
[g-load]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/core/io/resource_loader.cpp#L725
[g-deps]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/core/io/resource_format_binary.cpp#L623
[g-replace]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/core/io/resource_loader.cpp#L559
[g-pack]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/core/io/file_access_pack.cpp#L472
[g-retire]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/core/io/resource.cpp#L790

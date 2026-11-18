# November 18 External Review Reconciliation

Receiving report, 2026-11-18. Three bounded review lanes and the integration owner compare both
supplied reviews with current primary sources. Source confirmation below is not a fresh execution
of an external reproduction. The concurrent TileSet unit has its own recorded runtime evidence.

## Provenance and authority

Both supplied reviews identify Nara baseline `4111cb9d44fefea1cc5ae22a1204d6aeef24e891`.
Archive paths identify local inputs, not repository dependencies or implementation authority.

| Supplied input | Original archive | Sole Markdown member |
| --- | --- | --- |
| Engine architecture review | `D:\Frankorz\Downloads\nara-engine-architecture-review-2026-11-18.zip`; 23,853 bytes; SHA-256 `befd672c233f20a37a3ebfa75d5c93a2daad0640ad4cbdf014df70b07c683599` | `2026-11-18-engine-architecture-review.md`; 67,289 bytes; SHA-256 `fa0dc80a53f023c5bd31d788ea80d1ceaabbb3f8d269249f7f9201e71c8acd0f` |
| Public API and author usability review | `D:\Frankorz\Downloads\nara-public-api-and-author-usability-review-2026-11-18.zip`; 8,957 bytes; SHA-256 `52a52ddafd6bb7046ee3bc9d0e5eb03e8cb8392aaa2b2938f96ed7e02795d18b` | `2026-11-18-public-api-and-author-usability-review.md`; 23,016 bytes; SHA-256 `7046ead6cf300a59834a88115ad2bcc968b1526339dc4025383fa271e9adfed0` |

The receiving copies are
`C:\Users\Frankorz\AppData\Local\Temp\nara-engine-architecture-review-20261118-input.md` and
`C:\Users\Frankorz\AppData\Local\Temp\nara-public-api-and-author-usability-review-20261118-input.md`.
Archive sizes, hashes, sole-member identities and receiving-copy hashes were checked locally and
match the supplied provenance. The original reviews are not imported wholesale.

The receiving checkout was `a318d1b7c1e1162bf02674bc2135dffdaf5060eb`, with intentional concurrent
TileSet/runtime delivery and other work in progress. Selected App, Transform, Winit and existing
consumer sources cited below have no differences from the review baseline to that HEAD or in the
working tree at inspection. This equality applies to those sources, not the entire external report.

[Strategy](../../STRATEGY.md), [document authority](../architecture/README.md#document-responsibilities)
and Accepted ADRs govern decisions. The existing
[November 17 priority](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#L166-L185)
already prioritizes complete runtime functionality, keeps Editor/UI/Dialogue paused and defers small
performance experiments. These two timing corrections fit that priority without a new framework or
benchmark program.

## Timing findings

| Finding | Current status | Owning correction | Consumer verification |
| --- | --- | --- | --- |
| F1: App panics after removal of `FixedTime` | **Implemented at `f51618a`; scoped verification passes below.** Six focused regressions first reproduce App's panic. | `nara_app`: required post-schedule resource accesses are fallible at their consumption points, including exact-step finalization, using the existing time-frame error and managed first-fault path. | A managed consumer removes `FixedTime` through deferred Commands in PreUpdate with zero and one due tick, and during an exact fixed step. Each drive returns the missing-resource fault, retains it on a later drive, and permits managed Stop with once-only cleanup. |
| F8: elapsed Resume uses a paused interval but Transform interpolates as running | **Implemented at `50f451c`; scoped verification passes below.** The original `2.5` versus `3` comparison is independently reproduced before the fix. | App publishes the effective pause mode of its settled transaction before Extract; Transform uses that frame fact for interpolation and history reset. Resumed control settings, remainder and explicit `drive` semantics are preserved. | The presented-transform and actual camera/packet consumers cover exact Step immediately followed by Resume plus `drive_elapsed(5 ms)`. Immediate and maintained paths both present the current pose; another 10 ms consumes the original remainder correctly. |

## Remaining architecture findings

| Finding | Receiving disposition and owning action |
| --- | --- |
| F2: source Check and Cook disagree on the selected Schema | **Scoped correction implemented and qualified below.** Cook consumes original manifest-selected source delivery, compiles with its admitted bindings and carries actual member observations through build/publication. Alternate-path, invalid/divergent selection, later replacement and actual relocated source-free package controls pass. [Shared Cook source selection](../../src/project_tooling/project_cook/package.rs), [assembly](../../src/project_tooling/project_cook/content.rs), [existing agreement](../../src/project_host/runtime_agreement.rs). |
| F3: independent desktop UI can resolve incompatible Winit sources | **Confirmed source-selection gap; defer with paused desktop UI work.** The root selects vendored Winit while AccessKit's dependency uses registry Winit. Root patches are application-owned; the generated project does not emit one. The sprite-only shipping game does not exercise this combination. Qualify local and pinned-Git template selections separately; retain the native DPI repair until its own retirement criteria pass. [Starter manifest](../../src/project_workflow/starter/product_contract.rs), [AccessKit adapter](../../crates/nara_winit/src/accessibility/platform.rs), [vendor contract](../../vendor/winit/NARA-PATCH.md). |
| F4: Material texture generation can change between Extract and Queue | **Scoped correction implemented and qualified below.** Original Extract freezes complete Material/Image captures, shared across cells/Views/batches. Queue no longer chooses later additional-texture cache entries. Real inter-stage replacement, current authored refusal, publication recovery, post-World old-pixel replay and panic cleanup pass through the original public GPU Host. [Extract](../../crates/nara_sprite_render/src/extract.rs), [Queue](../../crates/nara_sprite_render/src/queue.rs), [Image snapshot](../../crates/nara_image/src/content.rs). |
| F5: processed Material texture membership omits Image kind | **Scoped correction implemented and qualified below.** Original deep validation joins decoded Material slots to selected Image codec roles. Builder and independently encoded/reopened wrong-kind bases reject before gameplay; valid and missing-Image controls pass. [Deep validation](../../src/runtime_content/deep.rs), [Material texture facts](../../crates/nara_material/src/asset/runtime.rs), [independent consumer](../../tests/fixtures/module-consumer/material-consumer/tests/processed_workflow.rs). |
| F6: explicit PNG Reimport omits a current PNG check at adoption | **Confirmed source gap; queued for Editor correctness work.** Completion compares the worker observation and checks current metadata, then publishes. The hydration path already verifies current PNG and metadata. Reuse that bounded observation at adoption, keeping the original capability and last-good value. A watcher event alone is insufficient. [Explicit adoption](../../src/project_host/runtime/editor/image_import/transaction.rs), [hydration control](../../src/project_host/runtime/editor/image_import/hydration.rs). |
| F7: dirty Reopen lacks explicit discard intent | **Confirmed interactive gap; queued while UI is paused.** Both frontends submit the intentional discard operation directly. Keep that exact programmatic operation, but require a document/revision-bound discard-or-cancel decision for dirty interactive use. Preserve failed-read behavior and history on Cancel. [Reference action](../../editor/src/frontend/imgui/authoring.rs), [drawn action](../../editor/src/frontend/drawn/shell.rs), [Accepted discard contract](../architecture/adr/0047-editor-workspace-and-scene-document-state.md). |
| F9: ordered task results accept different completion owners | **Implemented and scoped qualification passes below.** A nonempty collection now admits its first completion owner and returns mismatched handles intact; an empty stream can rebind. Public mixed-scope regressions first reproduce the defect, then pass with the full task suite and original source-build/Host consumers. The original trace does not prove callback loss or physical worker leakage. [Result collection](../../crates/nara_tasks/src/runtime.rs), [public controls](../../crates/nara_tasks/tests/ordered_results.rs), [scope ownership](../../crates/nara_tasks/src/runtime/scope.rs). |
| F10: drawn/reference keyboard translation differs | **Confirmed source discrepancy; defer with UI.** Drawn activation handles Enter but omits focused-button Space; printable text with Ctrl+Alt is filtered. Share only the small text/command decision, preserving each Host's actual focus/IME lifetime. Require actual keyboard/IME evidence when resumed. [Drawn input](../../editor/src/frontend/drawn/input.rs), [reference controls](../../editor/src/platform/imgui/authoring_workflow_tests/owned_keyboard.rs). |
| F11: replay publication exposes a partial final file | **Implemented and scoped qualification passes below.** The CLI writes/synchronizes an owned sibling temporary before no-replace publication. It retains cleanup custody, preserves competing targets and distinguishes prepublication failure from published uncertainty and limited durability. Real record/play and publication controls pass. [Replay writer](../../samples/two-level-game/src/bin/replay.rs), [publication](../../samples/two-level-game/src/bin/replay/publication.rs), [checkpoint writer](../../samples/two-level-game/src/save/io.rs), [filesystem owner](../../crates/nara_fs/src/capability.rs). |

The baseline's Image-only additive restriction is historical. Material sharing is committed at
`780ce6b`/`640024a`; the current TileSet successor supports shared source/processed declarations and
real Tilemap floors. Source inventory counts progress from 26 to 29 to 31, and cooked members
from 13 to 14 to 15. That growth is not itself a defect. Material sharing and TileSet atlas capture
do not resolve F4's separate Material-extension handle lookup or F5's durable type requirement.
See [the qualified TileSet unit](../plans/2026-10-20-persistent-two-level-game-plan.md#tileset-region-implementation-and-qualification).

## Author usability findings

| Workflow | Receiving disposition |
| --- | --- |
| Preserve a new game's identity | **Implemented and scoped qualification passes below.** The walkthrough preserves generated manifests, universe, stable-metadata policy, startup sidecar and Cook/Player/build-script entries. It adds the collection contribution and an imported Prefab's own sidecar. Two same-named real external games retain distinct identities, finish/reset source gameplay and run relocated cooked baselines after source removal. [Walkthrough](../guides/first-2d-game.md), [consumer](../../tests/project_author_workflow.rs), [starter](../../src/project_workflow/starter.rs). |
| First system and subcommand help | **Implemented and scoped qualification passes below.** The generated README teaches an actual fixed-step plugin without adding a game API; its compiled consumer observes gameplay while Check stays silent. Scoped help runs before filesystem/build work, includes syntax/examples and preserves malformed argument rejection. Actual parser/help subprocess tests pass. [CLI parser](../../src/bin/nara/args.rs), [starter guide](../../starters/desktop-2d/v1/README.md), [help controls](../../tests/project_workflow_help.rs). |
| Recipe composition diagnostics | **Confirmed loss at the tool boundary; open.** The generic factory discards the original error into fieldless `ToolError::Recipe`; the sample also uses `expect`. Preserve bounded known `ProductRecipeError` code/plugin facts through fallible recipe entry, retaining a generic fallback for opaque errors. [Factory](../../src/project_tooling/mod.rs), [recipe errors](../../src/product.rs), [sample recipe](../../samples/collect-game/src/lib.rs). |
| Compiler and missing-sidecar locations | **Compiler-span loss confirmed; missing-sidecar diagnostic remains a scoped follow-up.** The Cargo model/parser retains code/message but no primary location. Preserve a bounded primary location and let the project owner display a valid relative path; do not expose arbitrary rendered compiler output. Reproduce the missing-sidecar case before choosing its exact projection. [Cargo messages](../../src/project_workflow/cargo_facts/messages.rs), [bootstrap output](../../src/project_tooling/bootstrap.rs), [tool failure projection](../../src/project_tooling/protocol/failure.rs). |
| First persistent component | **Confirmed example/admission mismatch; defer interactive qualification.** The opening Vec2 example lacks a complete creation value, while the scalar Collectible can supply one. Teach a creatable scalar first and demonstrate Add/Edit/Undo/Save/Reopen before the vector case. Presence-only records are an explicit ADR0090 restriction, requiring a deliberate contract amendment rather than being called an implementation regression. [Component guide](../guides/persistent-components.md), [data adapter](../../crates/nara_scene/src/authoring_decode.rs), [ADR0090](../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md). |
| Current component inventory | **Confirmed duplicated sample wiring; local follow-up.** Collect Game lists the same current types in catalog and native registration. Use existing `persistent_components!`; keep predecessor catalogs/migrations explicit and require an unchanged current fingerprint. [Sample plugin](../../samples/collect-game/gameplay-plugin/src/lib.rs), [inventory macro](../../crates/nara_reflect/src/inventory.rs). |
| Completed command-draft binding | **Concrete repeated caller choreography; local API follow-up.** Three games construct a binding from a cloned type then replace its draft. A constructor accepting the complete draft can reuse existing `ActionCommandMap::bind` validation. No new command registry is needed. [Binding](../../crates/nara_gameplay/src/action.rs), [game caller](../../samples/two-level-game/src/gameplay/input.rs). |
| Live movement versus replay admission | **Confirmed game-owned inconsistency; open.** The live MOVE branch accepts extra payload fields while `SemanticCommand::from_envelope` requires exactly two. Share the typed decoder and require equal valid behavior and equal rejection through both public paths. [Live consumer](../../samples/two-level-game/src/gameplay/travel.rs), [recording decoder](../../samples/two-level-game/src/command.rs). |
| Play and Restart feedback | **Concrete paused-UI follow-up.** Preserve explicit Save and accepted-generation behavior. At the controls, identify the dirty document and explain that Restart replays the accepted generation. Qualify using two camera values through real controls when UI work resumes. [Game panel](../../editor/src/frontend/imgui/project.rs), [Play controls](../../editor/src/frontend/imgui/mod.rs). |

## Source agreement and processed Material correction

F5 lands at `0464be5`, and F2 at `2f4e4be`. The independent workspace lock prerequisite lands at
`aa8d1ab` without changing any previously locked package version.

The F2 cooker now uses public manifest ingestion, recipe resolution and the original
`ProjectContentLoader::load_delivery` before assembling content or writing generated Player
source. Scene compilation consumes those admitted source bindings. The completed processed plan
must have the same Schema fingerprint. The delivery receipt's actual members join importer raw
observations in the original `ContentFence`; the manifest-selected bundle is no longer a fixed
inventory path. Build and directory publication retain their existing source revalidation.

F5 uses `typed::<Material2dDomain>()` while the original CPU acquisition is still live. Each
texture slot requires its target member's selected Image codec role. This adds no decoder call,
secondary reference graph, retained success-path collection or per-frame audit. A wrong target
returns `MaterialTextureKindMismatch` with the Material, slot and target Product. Host fault
lowering preserves a separate `material-texture-kind-mismatch` reason.

The independently locked Material consumer passes 4/4 in
`91fb2f2c-458e-42a3-b43c-884d6ef82a51`. Its wrong-kind case first requires builder rejection, then
directly encodes a Material targeting a present Scene and updates the actual catalog digest and
declared dependency. After source and encoded directories are removed, captured-base admission
and Material acquisition succeed, while `validate_deep` rejects the actual texture kind before
gameplay. Valid and missing-Image controls retain their original behavior. The independent lock
needed the existing Two-Level physics/animation closure; no previously locked package version
was removed or upgraded, and unrelated Windows dependency-edge churn was removed.

The first cooker check exposed an exhaustive Host error match missing the new variant. Adding
the concrete reason fixes that compilation failure, and the selected `cook,desktop,audio-cpal`
cooker feature-closure check passes. Independent bounded source/cooker and Material reviews
report no P1/P2 in their reviewed
production paths. These static reviews alone do not establish functional or package qualification.

Release Cook/source tests pass 31/31 in `a3db0fe5-f855-4876-9fec-8b8ae01a2c7e`. A selected
alternate bundle works with the default absent, and the full original source-free Session,
Marker and concurrent-region path runs after source/cache removal. Missing, corrupt and
semantically divergent selected bundles agree with public source delivery and preserve absent
or existing bootstrap bytes. Original fences reject changed selected bytes and a new file
identity carrying identical bytes. Importer and complete delivery observations agree on shared
paths. The earlier source loader now owns Material/TileSet/Input-sidecar rejections; their
original precise failure, repair and retained prepared-content controls are preserved.

An additional focused profile control passes 1/1 in
`be516c23-a84f-4c28-97df-02d14cdabd84`. It executes actual `content::prepare` for both Desktop
and AudioDesktop, checks the completed content Schema against source recipe resolution and binds
the returned observations into the original fence. It creates no generated bootstrap, Window or
audio device. This closes the shared-entry profile coverage gap without repeating native desktop
execution. Correctness Clippy and sample formatting also pass after this test addition.

The real nested-Cargo package trajectory passes 1/1 in
`0656cfc8-e6e4-4ab3-a630-3e75ed466319`, with five unrelated cases excluded. Its actual cooker
first rejects a corrupt selected bundle before changing bootstrap absence/bytes or publishing a
directory. After repair, the default bundle remains absent; the selected alternate location cooks
and publishes. The original relocated source-free Player runs with authoring source removed and
Cargo/rustc unavailable. Original content-corruption, gameplay, checkpoint/fresh Restore and
native containment/readers/output cleanup assertions remain intact. The 426.580-second case
includes serial release compilation and is not a runtime performance measurement.

The first package attempt, `b96c31b5-71ba-423f-8c97-6168b5fab228`, fails in the newly added test
because it assumes the copied project already has generated bootstrap source. The original
project has none. The corrected control preserves both absent and existing source states; the
final run above passes. This is a fixture correction, not a relaxed delivery assertion.

The affected `runtime_content` and `CookedContentBuilder` owner cohort passes 81/81, with 876
unrelated tests excluded, in `19f860e9-49ea-4b1b-83d8-b71691d799c5`. Original once-decoded deep
validation, decoder/output lifetime, exact and one-short budgets, retained-pack corruption,
codec roles and complete builder closure controls remain intact. This unit qualifies Windows
Rust 1.95.0 CPU/source delivery and native headless package execution, not new desktop GPU,
foreign-platform execution, compiler-floor or performance guarantees.

Correctness Clippy passes for the selected sample library, cooker, Cook/source/package tests and
independent Material integration. Existing unrelated warnings remain; this is not a warning-free
workspace claim. Root/sample and affected independent-package formatting pass, as do document
link/fragment, TOML/lock and whitespace checks. The paused UI/Dialogue files remain byte-for-byte
unchanged. Generated-source isolation and the shared external-game cooked entry were still open
at that checkpoint; the qualified successors below now implement those workflows.

## Extracted Material Texture Correction

The renderer replacement lands at `163dffd`, followed by public native/CI controls at `10ee433`.

F4 reuses `PreparedMaterial2d` as one immutable shared binding/image value created during
Sprite/Tilemap Extract. Managed authored and direct code-first paths retain their existing
Image admission rules. Queue consumes the captured snapshots, removing the later handle/cache
resolution. It does not acquire another World/resource owner or repeat full validation per View.
The ordinary stock Sprite Image path retains its existing preparation owner.

The [owning Material plan](../plans/2026-11-14-durable-material-and-shader-plan.md#material-texture-capture-implementation-and-qualification)
records pinned Bevy/Godot evidence, exact tests and limits. Mature engines support centralized
binding preparation and invalidation; their retained texture references do not establish immutable
historical pixels. Nara's existing content snapshot supplies that requirement. Owning CPU tests
57/57 and existing public workflows 9/9 pass. Final hardware DX12 4/4 includes the new
authored/direct replay and post-submit panic cleanup plus the original live source-update path;
the explicit software DX12 control passes 3/3. Both use the same original Host for B followed by
A after source-World retirement, verify exact full images and release the original pixel owner.

Independent production review reports no P1/P2. Testing review's P2 cleanup-evidence gap is fixed
with one typed sentinel after a genuine submitted/read-back frame, and bounded rereview closes it.
Correctness Clippy and affected formatting pass. CI is configured locally; no remote execution,
foreign-platform, scanout or numerical performance qualification follows from this unit.

## Request-Owned Player Bootstrap

At `ebc50da`, the shared `generated.rs` write is removed. The original native Cargo request owns
one bounded generated Rust input, supplies its reserved environment value and captures its logical
name/digest separately from ordinary source files. Trusted build scripts track that environment dependency
and write only Cargo's `OUT_DIR`. Package binding requires the actual generated fact and offers
no physical-source fallback. Three Two-Level Player entry points and independent renamed-engine
consumers use the same mechanism; stale output alone does not activate an ordinary Player.

The [owning delivery plan](../plans/2026-10-31-cooked-player-delivery-plan.md#generated-input-implementation-and-qualification)
records the canonical v1 manifest/API correction, reserved Cargo-configuration constraint,
budget and review corrections, and full execution evidence. Owning tests pass 17/17 and the
independent real A/B/A plus Palette delivery passes 2/2. A/B requests exist before either native
build and reuse one workspace/target; original captures run after relocation/source removal,
both wrong packs refuse, and the ordinary no-input build refuses with old Cargo output present.
The strengthened inherited-environment control also passes. All three actual Two-Level packages
pass source-free execution: headless gameplay, physical desktop Save/fresh Restore and WASAPI
callback/PCM plus physical input/close. Earlier foreground interference and script interruption
remain recorded alongside their later passing reruns rather than being treated as a green aggregate.
This is request isolation through the existing target lease and trusted build-script contract,
not another workspace copier, configuration resolver, process authority or hermetic-build claim.

## Delivery order and larger refactors

TileSet delivery and the managed F1/F8 App/Transform corrections have landed. The F2/F5 admission
corrections above precede expanding ordinary cooking. F4 now preserves exact admitted pixels
through its original renderer owner. F9, F11, shared movement decoding and
bounded diagnostics are independent corrections. Desktop UI source selection, Editor import,
discard intent, keyboard parity and installed-tool activation remain queued with the paused UI work.

The cooker owns its generated source per request. The former shared-source write was a real
mutable build input, not evidence of a successfully published wrong package. The shared entry
keeps generated facts and ordinary code observations separate. Cargo continues to own dependency
resolution and incremental build decisions.

The [shared compiled Cook unit](../plans/2026-10-31-cooked-player-delivery-plan.md#shared-cook-implementation-and-qualification)
now implements that extraction for the actual generated starter and Two-Level. Implementation
commits are `c3698d3` and `c2d09ca`. The original
loader lends sequential strict snapshots, and the common consumer uses their compiled Scenes,
Prefab provenance, resolver and stock assets without another database or Scene compiler.
Game-owned build facets preserve optional/build-only inputs and compound groups. Exact sample
counts remain qualification facts. Owning/public 86 and affected sample 38 pass. The real
independent generated game authors three Scenes, three Images and Rust behavior, cooks through
the public entry and completes two travels after relocation/source/cache removal. All three
source-free Two-Level packages also pass, including physical desktop and WASAPI checks. Source
Export stays distinct. This closes the scoped shared-entry follow-up while leaving foreign
execution, atomic/hermetic source capture and the other review findings open.
[Delivery facts](../../src/project_content/delivery.rs), [builder](../../src/project_workflow/cooked_content.rs),
[package binding](../../src/project_tooling/player_package.rs).

After the shared Cook closure, prioritize F9's completion-owner admission and F11's complete replay
file publication. The active correction unit below keeps both changes at their existing owners.
Author onboarding and bounded diagnostics follow their existing receiving actions; paused Editor
work does not become a prerequisite.

Source PNG feature separation, cache recovery, HUD batching and pipeline working-set changes are
deferred unless a named workload makes them material. These reviews do not justify a framework
rewrite, a second World/scheduler/resource owner or a micro-optimization campaign.

## Runtime Result And Replay Publication Corrections

Status: implemented and scoped execution qualified on Windows Rust 1.95.0, November 19, 2026,
after the shared Cook qualification at `8fd5b61`. Full Foundation remains active.
Implementation: `e67bdca` (ordered completion owners) and `0f979aa` (replay publication).

For F9, admit one completion owner to each nonempty `OrderedTaskResults`. Return a foreign handle
intact at insertion, preserving task execution/cancellation and output custody. Release the
binding only when its last pending handle leaves. Retained ready snapshots keep their original
owner independently. Use the existing first pending handle as the owner instead of introducing
another retained token or scanning the entire collection on every cutoff check. Prove sibling
scopes sharing workers, independent facilities, equal/different order keys, partial prefix
removal, later rebind and same-scope spawners across task kinds through the public API. Preserve
existing ordering, cancellation, physical close and source-build integration tests.

For F11, write and synchronize an owned sibling temporary through the existing filesystem
capability, then publish complete replay bytes with no replacement. Retain original failed
temporary cleanup custody for retry; report unresolved cleanup and keep its owner through CLI
exit instead of claiming successful deletion. A destination that appears concurrently must survive
unchanged. Report prepublication failure separately from an already published outcome with
uncertain or limited durability; never delete a published destination as rollback. Prove actual
record/play, exact bytes, existing/raced destination refusal, failed write/sync cleanup and
postpublication outcome semantics without a replay store or generic publication framework.

Verification stays focused on task owners/consumers, replay/file publication and the original
sample gameplay/save/replay paths. No per-frame project audit, new scheduler, unrelated UI
qualification or microbenchmark campaign is added.

### Task Completion Owner Implementation And Qualification

Three new crate-external public tests first reproduce F9 in
`0817c3de-d48a-4c0b-9102-62474b4c984f`: 0/3 pass before the correction. They exercise sibling
scopes sharing a facility, independent facilities with both colliding and distinct order keys,
and partial drain followed by empty-stream rebind while old snapshots remain alive.

`OrderedTaskResults::push` now compares the original completion tokens before insertion and
returns a mismatched handle unchanged. The first pending handle supplies that identity; all
insertion paths preserve the invariant, so cutoff validation checks that first handle instead
of scanning for mixed owners. No additional token field, allocation, lock, task cancellation or
terminal consumption is introduced. A copied spawner from the same scope and different task
kinds remain valid. The existing `PoolMismatch` variant also covers a distinct scope owner;
its display text now says completion owner.

The full all-feature task library/public cohort passes 113/113 in
`f43bc231-e1c1-4c16-89ca-8dfac98b81c8`, including cancellation, cutoff ordering, physical return,
shared-scope close, disposal and worker-start failure controls. Original source-build consumers
pass 6/6, with 973 unrelated or unselected cases excluded, in
`e5e07da6-22f6-4186-8bee-761e047d5d5a`. That feature closure does not enable the Host scope test
module; its actual `tooling`-enabled execution is recorded separately after completion. A bounded
independent correctness review finds no remaining P1/P2. These are scoped correctness results,
not numerical performance or foreign-platform execution evidence.

The `tooling`-enabled Host scope cohort then passes 13/13, with 1646 unrelated cases excluded,
in `ad61694f-d2fd-4e23-99a7-1d636e9b5fee`. It retains original body/late-result completion,
restart admission, required report checks, startup failure and final worker-join behavior.
The extra feature compilation is distinct from those thirteen executed tests. No physical
Editor window, UI feature or native display qualification is part of this correction.

### Replay Publication Implementation And Qualification

The CLI serializes its finite recording before creating an owned sibling temporary. It writes,
flushes and synchronizes that candidate through the existing `TemporaryFile` owner, retains a
live original file observation across publication, and delegates the name change to
`replace_temp(..., ExpectedTarget::Missing)`. The existing filesystem receipt supplies atomicity,
conflict protection, identity, digest and synchronization facts. No engine filesystem API or
general replay store is added. Failed temporary operations retain their original cleanup owner;
the CLI attempts cleanup and keeps unresolved custody through terminal output and process exit.
An already published destination never becomes cleanup authority for a failed later observation.

Record results add `publication.status` and separate data, metadata, name and parent-directory
sync facts. A verified complete publication can report limited or unsupported parent sync.
Postpublication verification or parent-sync failure reports `published_uncertain`, preserving
the target and refusing to imply absence. Windows parent sync remains unsupported. Linux's
name-bound source operation does not establish hostile-writer isolation, and no platform's
successful sync call establishes tested crash recovery.

Independent review catches a new result-encoding hazard before qualification: native non-Unicode
output paths can fail JSON serialization after publication. The CLI now prepares that JSON value
before reading the program, starting gameplay or creating a candidate. A public CLI regression
uses an unpaired Windows UTF-16 surrogate or Unix non-UTF-8 byte and requires structured refusal
with no final or temporary output. A follow-up source review corrects its fixture oracle: the
child runner retains stdout/stderr capture files, so the test checks for absent publication
temporaries rather than counting every fixture entry. Both findings are closed by bounded
independent rereview. The CLI and public replay fixtures also copy the actual Meadow/Cavern
sidecars added by shared Cook. Actual replay/publication results follow their execution.

The release replay CLI/publication cohort passes 19/19 in
`72ef1fd4-18b4-4dab-a512-d9622b083596` (4.571 seconds after the first 14m27s feature build).
Seven actual CLI cases preserve original gameplay record/fresh-process replay, corruption and
input/path refusal controls, and verify exact decode/re-encode bytes plus the new publication
facts. Twelve direct production-publication cases cover partial writes, file-sync boundaries,
competing final names, original temporary identity and cleanup retry, foreign files at old
temporary names, postpublication replacement/content change, parent-sync failure/unsupported
facts, and stdout write/flush failures with retained cleanup. The native non-Unicode case runs
on Windows in this qualification; Unix source coverage is not Linux execution evidence.

The affected release save/replay/checkpoint cohort passes 31/31, with 133 unrelated cases
excluded, in `5d467f15-eeda-4f16-83c3-1549116a044b`. It retains original fresh-project complete
game replay, intermediate-result and source-drift refusal, canonical formats, command/capture
order, failed destination repair, normal Runtime cleanup and consumer-panic retirement. Save
controls preserve exact old targets, competing owners, migration and uncertain-publication
readback. Formatting between cohorts triggers an additional dependency build; these build
durations are verification costs, not runtime-speed evidence.

All-target/all-feature task correctness Clippy and the selected release replay/library/test
targets pass with `-D clippy::correctness`. Root and sample format checks, added document
links/fragments and whitespace checks pass. Advisory warnings remain, including existing error
sizes and test-only Windows permission-reset/style suggestions; no warning-free claim is made.
Both implementations receive independent bounded review, and the concrete path-encoding and
test-oracle findings are corrected before the passing replay run. All three scoped agents are
closed. The fifteen paused UI/Dialogue files keep their original SHA-256 values and remain
outside these commits; dependency manifests and root/sample locks are unchanged.

These results qualify the concrete Windows task and replay workflows. Linux/other native
execution, power-loss recovery, hostile-writer exclusion and hard latency are not established.
No display/GPU acceptance, repeated Cook package trajectory or numerical benchmark is required
for these owner-local corrections. Remaining receiving actions, full R4/R12/Foundation and
paused Editor work stay open. Next prioritize the independent author's identity-preserving
first-game workflow and subcommand discovery already identified above, keeping the generated
Cook/Player/build-script entries and explicit codec selection usable while adding gameplay.

## Independent Author Workflow Successor

Status: implemented and qualified for the scoped Windows Rust 1.95.0 workflows below on
November 19, 2026, after `2fcfc97`. Full Foundation remains active.
Implementation: `752fbd8` (scoped CLI help) and `af7886c` (documented independent author consumer).

Preserve a newly created project's `source_universe`, `stable_metadata` policy and exact startup
sidecar while adding the collection game's source. Keep its generated manifest, build script,
Cook and Player rather than replacing the whole project with the repository sample. Import the
Player Prefab with a new project-owned sidecar; copied Image metadata remains the authored asset's
identity. Add gameplay to the existing recipe and keep the explicitly selected stock Image codec.
Local Cargo members retain the same engine coordinate, and the project-owned Cook request names
their actual compiler inputs instead of adding a dependency resolver.

The generated README first teaches one complete observable fixed-step plugin through the current
API, before persistent schema and Editor workflows. Actual Check must not run it; the generated
headless target must print its first-tick marker. Subcommand help must succeed before filesystem,
compiler or gameplay work, with scoped syntax and a usable example.

Create two independent external projects with the same slug. Capture creation identities, make
the documented first-system edit, add the actual collection plugin/content, refresh/check through
the compiled tool and finish/reset both games through the ordinary headless entry. Prove distinct
source universes, startup/Prefab identities and unchanged creation-owned bytes after authoring.
Keep generated Cook/Player usable: actually Cook, relocate both installations, remove source/cache
access and execute their original compiled Players. Their default headless profile has no scripted
collection input; distinguish this baseline execution from the source headless win/reset proof.
Cross-pack selection must refuse before gameplay. No Editor UI, package resolver, new game API or
timing benchmark is needed for this author workflow.

### Author Workflow Implementation And Qualification

The CLI adds terminal `-h`/`--help` at finite subcommand boundaries, including
`repair startup-scene --help`, before filesystem or build work. Supplied duplicate, unknown,
missing-value and conflicting options still reject; help does not reinterpret an option's
missing value or accept trailing arguments. Native paths remain lossless, while textual
arguments keep Unicode requirements. Every scoped help text has syntax and an applicable
example. The parser and actual subprocess cohort pass 14/14 in
`140c5af2-2cf8-4fcd-8889-c9482f939a10`, including inaccessible/missing paths and existing fake
projects with no project/output writes. Independent correctness review finds no remaining P1/P2.

The first-system example is compiled from the generated README's actual Rust block. The guide's
recipe and Cook-input blocks also supply the external consumer's actual edits. The guide no
longer copies the sample manifest or entire `src/` tree. It retains creation-owned identity,
startup metadata, Image codec, build script, Cook/Player and engine dependency selection, adding
the ordinary collection contribution and a project-owned Prefab sidecar. That metadata uses the
same fields as the shown PowerShell data; this qualification uses the public Rust metadata owner,
not execution of every tutorial shell command.

The first fixture run fails before project compilation because its code-block extractor assumed
LF while the Windows checkout uses CRLF. Normalizing only the fixture's read text fixes that
test issue. The next run compiles and executes the first system, then catches a fixture TOML
insertion error for an initially absent `workspace.members`; inserting through the existing table
fixes it. A later complete run reaches both source win/reset and relocated Players but its final
test oracle incorrectly requires a digest error for two unequal catalog lengths. The actual
Player correctly rejects the pack header first. These failed runs remain distinct from the final
qualified result and do not identify new Runtime defects.

Testing review adds byte-for-byte Scene/Prefab/Image and metadata preservation after each
win/reset and after Cook, plus normal failure exits and exact refusal diagnostics. A separate
one-byte, equal-length valid-JSON catalog mutation retains the original header/payload and
requires the compiled catalog-digest mismatch before gameplay. Original source, Cook and failed
process cleanup custody stays in the existing consumer helper. Corresponding project/cache
removal and a sealed Player environment are not full filesystem isolation: the shared target,
engine checkout and system toolchain remain present. The complete consumer's final execution
and limits follow its actual result.

The strengthened complete external-game trajectory passes 1/1 in
`1f747021-03ab-4bfb-8e91-86c5712a5554` (299.217 seconds, including actual nested builds).
Both projects are created outside the checkout through the real public owner with the same
`collect-game` slug. Their original manifest bytes, source universes, stable-metadata policy and
startup sidecars survive all edits; imported Prefab identities are distinct. The generated
first-system block actually compiles, Check omits its marker and headless gameplay prints it.

Each game adds the ordinary collection plugin while retaining the generated codec, build script,
Cook and Player. It refreshes/checks through its own compiled tool, rejects a missing Prefab
sidecar, recovers after repair and actually finishes and resets the complete source game. Scene,
Prefab, Image and metadata bytes remain unchanged after both plays and Cook. Both project-owned
cookers build/publish and relocate their three-file installation; corresponding project sources,
Cook caches and captured tools are removed before sealed Players run from an empty unrelated
CWD. Both execute their compiled first-step plugin and complete 4096 fixed ticks. This is a
headless baseline, not cooked win/reset or pixel/interactive desktop evidence.

Real cross-project pack substitution refuses with normal exit 1 and a known expectation/header
diagnostic. A separate equal-length valid-JSON catalog-byte mutation preserves the header and
all member payloads and receives the precise catalog-digest mismatch before gameplay. Restoring
the original pack allows the same Player to complete. Every passing native child supplies its
original empty-containment, stopped-reader and drained-output receipt without termination
intervention. Failed fixture runs are retained separately; they do not grant later process
authority or count as qualified positive runs.

Both bounded independent reviews have no remaining P1/P2 after the actual source-preservation
and rejection-oracle fixes. CI's existing serial generated-game step now includes the author
consumer; the YAML configuration is local, with no remote CI execution or publication claim.
Actual Git dependency publication, foreign-platform execution, hermetic/clean-machine delivery,
arbitrary local dependency discovery and resumed Editor UI remain separate requirements.

The final ordinary parser/help/creation/CLI cohort passes 29/29, with four nested-build or ignored
cases excluded, in `5fea892b-98f6-4858-94cd-4b49e22b3c27`. This includes original immutable-Git
coordinate admission, local relation/collision/tamper defenses and prepared source identities;
it does not execute a remote Git dependency. Focused correctness Clippy for the CLI and affected
public consumers and root formatting pass. The help fixture's temporary cleanup additionally
checks its resolved original parent before recursive removal. Final fixture checks and document
evidence keep the existing publication and process owners; no general test framework is added.

After that cleanup guard, the five actual help subprocess tests pass again in
`a6384086-fe01-497f-9399-e6810f40e849`; the CLI/help correctness Clippy and root format check
also pass. Added document links/fragments, TOML/locks, workflow YAML and whitespace checks pass.
Existing advisory warnings remain; no warning-free workspace or new compiler-floor claim is
made. All three scoped implementation/review agents are closed. The fifteen paused UI/Dialogue
files retain their original SHA-256 values and are outside the commits; dependency versions and
locks are unchanged. This closes the two receiving author-workflow items, not all author
ergonomics or full R1/R7/R12/Foundation. Next preserve useful recipe-composition and compiler
location diagnostics at the current tool boundary, and keep the already identified command
decoder/binding corrections as bounded independent Runtime follow-ups.

## Calendar dependencies and simulation clocks

A bounded manifest/lock/source search finds no direct `chrono` or `jiff` use in Nara's core,
sample or Reference Game. The paused GPUI consumer lock retains Chrono 0.4.45 through pinned
Open-GPUI and its scheduler. The peer framework exposes `DateTime<Utc>` in its calendar-clock
interface and uses local timestamps for logs/accessibility diagnostics. Its monotonic clock is
already `web_time::Instant`; Nara's simulation uses its integer fixed clock and `Duration`.

On November 18, the official crate metadata lists Jiff 1.0.0, published November 15, with MSRV
1.85 and no yanked flag. The initial metadata/README-only check misses Chrono's maintenance
decision. The user supplies issue #1768, titled "Soft-deprecating chrono and chrono-tz", which
states the intent to wind down maintenance and recommends Jiff. The RustSec source advisory
`RUSTSEC-2026-0543`, dated September 14, marks all Chrono versions `unmaintained` with no patched
version and recommends Jiff or time. This is an informational maintenance advisory, not evidence
of an exploitable defect in this checkout. Absence of a README notice does not contradict it.

Select a framework-owned migration to Jiff 1.0 for the remaining calendar timestamps and public
`Clock::utc_now` contract. Update the scheduler, test clock, framework adapters, log formatting
and accessibility timestamps together, then qualify native/wasm feature closure and timestamp
round-trips. Preserve their monotonic `Instant` and `Duration` use; Jiff does not repair F1/F8.
The paused Nara consumer adopts the resulting pinned framework revision after its own integration
qualification. No dependency or paused GPUI lock changes are made in this runtime unit.
[Jiff 1.0.0 metadata](https://crates.io/api/v1/crates/jiff/1.0.0),
[Chrono metadata](https://crates.io/api/v1/crates/chrono),
[maintenance decision](https://github.com/chronotope/chrono/issues/1768),
[RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0543.html),
[advisory source](https://github.com/RustSec/advisory-db/blob/main/crates/chrono/RUSTSEC-2026-0543.md).

## Timing Source Traces At The Receiving Baseline

These traces and line references describe the receiving `a318d1b` sources before the corrections.
Their proposed controls are now exercised by the implementation results below; they are retained
to explain the failure and why another schedule-wide audit was not selected.

### F1: resource consumption after user schedules

Frame entry checks required resources through
[`TimeFramePlan::from_world`, lines 1039–1059](../../crates/nara_app/src/lib.rs#L1039-L1059).
That preflight precedes all Core schedules. The schedule invocation returns after running against
the mutable World ([lines 1981–1993](../../crates/nara_app/src/lib.rs#L1981-L1993)); its authority
validation does not itself require the clocks
([runtime authority, lines 145–177](../../crates/nara_app/src/runtime.rs#L145-L177)).
After PreUpdate and its deferred removals, the fixed loop writes `FixedTime` with infallible
`resource_mut` at [line 2049](../../crates/nara_app/src/lib.rs#L2049) when a tick is due, and at
[line 2053](../../crates/nara_app/src/lib.rs#L2053) even with no due tick. Exact Step runs FrameIntent
and FixedUpdate before another infallible write at
[lines 2123–2127](../../crates/nara_app/src/lib.rs#L2123-L2127). A system can therefore return normally
after removal and leave App's own subsequent access to panic.

The receiving fix should use the existing
[`TimeFrameError::MissingResource`, lines 537–541](../../crates/nara_app/src/lib.rs#L537-L541), which
already maps to a sticky managed fault through
[`finish_app_drive`/`enter_fault`, lines 2830–2850](../../crates/nara_app/src/runtime.rs#L2830-L2850).
The runtime capture wrapper simply invokes the operation at
[lines 2901–2906](../../crates/nara_app/src/runtime.rs#L2901-L2906); this App panic does not become
that typed error. Inspect the other required accesses actually reached after schedules in the same
[frame transaction, lines 2054–2084](../../crates/nara_app/src/lib.rs#L2054-L2084) and
[exact-step tail, lines 2141–2144](../../crates/nara_app/src/lib.rs#L2141-L2144).
This is an owning access correction, not a requirement to validate every clock after every schedule
or to recover arbitrary user panics.

Existing source tests cover
[missing clocks before frame planning, lines 3336–3362](../../crates/nara_app/src/lib.rs#L3336-L3362)
and [removal during Last, lines 526–544](../../crates/nara_app/tests/runtime_time_continuation.rs#L526-L544).
Neither covers removal between preflight and an App clock access. Preserve the latter's successful
frame when `FixedTime` is removed after its final use; a blanket end-of-schedule clock check would
change that behavior. The proposed zero/one/exact-step consumer checks above exercise the actual
failure points and the managed fault/close contract, rather than a helper implementation.

### F8: transaction mode and presentation mode

Accepted [ADR 0039, lines 80–85](../architecture/adr/0039-main-loop-time-pause-and-runtime-state.md#L80-L85)
requires an elapsed interval that began paused to settle as paused even when Resume is applied.
Exact Step preserves the remainder and does not perform ordinary extraction
([lines 66–73](../architecture/adr/0039-main-loop-time-pause-and-runtime-state.md#L66-L73),
[105–118](../architecture/adr/0039-main-loop-time-pause-and-runtime-state.md#L105-L118)).
[ADR 0100, lines 111–119](../architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md#L111-L119)
requires paused presentation to reset history to the captured current value.

The driver captures `paused_interval` before applying Resume
([runtime line 2578](../../crates/nara_app/src/runtime.rs#L2578)). Resume changes control state and
World settings to running
([lines 2626–2635](../../crates/nara_app/src/runtime.rs#L2626-L2635),
[2763–2771](../../crates/nara_app/src/runtime.rs#L2763-L2771)), but the same drive selects the paused
App transaction for that elapsed interval
([2696–2703](../../crates/nara_app/src/runtime.rs#L2696-L2703),
[2784–2787](../../crates/nara_app/src/runtime.rs#L2784-L2787)). App forces a *local copy* of settings
to paused at [lines 1039–1048](../../crates/nara_app/src/lib.rs#L1039-L1048).
The published [`RuntimeFrameStatus`, lines 1101–1114](../../crates/nara_app/src/lib.rs#L1101-L1114)
contains clock quantities but no effective interval mode.

Transform instead reads the already resumed `RuntimeTimeSettings.paused()` at
[presentation lines 199–205](../../crates/nara_transform/src/presentation.rs#L199-L205).
That value controls history reset and interpolation at
[lines 227–261](../../crates/nara_transform/src/presentation.rs#L227-L261).
After exact Step, the fixed pair can still be previous `x = 2`, current `x = 3`; retained alpha
`0.5` then selects `2.5`. A paused maintenance Extract resets the pair to `3/3`, explaining the
ordering difference in the external account. This is a source-derived explanation, not a new run.
The native runner uses this affected elapsed path at
[`nara_winit` line 1334](../../crates/nara_winit/src/lib.rs#L1334).

Publish the transaction's effective mode through App's existing frame facts and consume it at
Transform's presentation boundary. Zero fixed steps or zero virtual delta alone cannot identify
a paused interval: an ordinary running frame can also have either value. The existing
[Transform consumer, lines 761–765](../../crates/nara_transform/src/snapshot_tests.rs#L761-L765)
already specifies interpolation during a running zero-tick frame. Its later
[Step/Resume sequence, lines 786–819](../../crates/nara_transform/src/snapshot_tests.rs#L786-L819)
inserts paused maintenance before Resume. The
[moving-camera capture consumer, lines 159–193](../../crates/nara_render/src/runtime/tests/captured_time.rs#L159-L193)
also inserts maintenance and stops before Resume, so these do not cover the reported ordering.

For a 20 ms timestep and 50 ms initial drive, the proposed immediate/maintenance comparison should
observe first-settlement tick `3`, zero fixed steps, zero virtual delta, remainder `10 ms`, alpha
`0.5`, current/presented `x = 3`, and no spatial interpolation. A subsequent elapsed `10 ms` should
advance to tick `4` with zero remainder. Keep the existing
[explicit-versus-elapsed Resume control, lines 217–246](../../crates/nara_app/tests/runtime_time_continuation.rs#L217-L246).
These are headless consumer expectations; they do not require UI work or establish native pixel
qualification.

## Evidence limits and handoff

### Managed Time Correction Unit

After TileSet delivery commits `759daae`, `c32d2ad` and `b14076f`, implementation starts on F1/F8.
Keep all original clocks in the World and the existing managed driver/fault/close route. Replace
App's infallible post-schedule reads/writes only where it actually consumes a required resource.
Do not add a schedule-wide clock audit or change the existing successful Last-stage removal case.
Completed user work before a missing-resource fault is not rolled back.

The F8 successor adds an effective simulation-pause fact to the existing App frame status, captured
from the same settings used to settle the interval and published before Extract. Transform reads
that fact for presentation/history reset; public control settings still describe the requested
next mode. An exact fixed step reports active simulation without becoming an ordinary rendered
frame. Preserve integer clocks, remainder, paused maintenance, explicit-versus-elapsed drive and
running zero-step interpolation. Use focused public Runtime and presented-transform regressions;
no calendar dependency migration or unrelated native/UI matrix belongs to this correction.

The six F1 regressions first fail in `09d555b5-50ef-47a7-91c2-f923170f4690`: the original App
access panics, so the tests cannot reach managed Stop. They cover `FixedTime` removed during
PreUpdate with zero/one tick or exact FixedUpdate, `RenderTime` removed before finalization,
and status/exit requests removed before their final consumption. The private mutable access
helper initially needs the existing Bevy `Resource<Mutability = Mutable>` bound; the corrected
owner passes App library plus public time continuation 87/87 in
`e6d3edfb-78bb-4f5f-a296-9469ad4de571`. All six now return the exact typed missing-resource error,
retain the first fault, prevent successor schedules and stop with once-only plugin cleanup.
The original successful Last-stage removal control is preserved. Independent F1 review reports
no P1/P2; all-target correctness Clippy passes before the separate F8 change.

The first F8 test build incorrectly asks the plain spatial view for an interpolation flag. The
corrected fixture reads the existing public presented-projection consumer's captured flag. Its
actual red run `7270d556-e4f7-4f07-819c-3d482add43b9` reports the same current `x = 3`, tick `3`,
zero steps/delta, `10 ms` remainder and alpha `0.5`, but immediate Resume presents `2.5` while the
maintenance control presents `3.0`. After adding `RuntimeFrameStatus::simulation_paused` and
using it in Transform, the App/Transform library and public time continuation selection passes
128/128 in `fc80a925-4d23-4aa3-8ed3-cbd5d3672236`. The control state is Running while that first
elapsed interval reports simulation paused. Exact Step reports active simulation despite paused
control settings; the next `10 ms` consumes the original remainder and reaches tick `4`.

The actual render capture selection passes 7/7, with 63 unrelated cases excluded, in
`d4d5d8cc-7112-48f1-b0e4-2f90e36db4ba`. It checks the real extracted camera View's center at
`x = 3`, the captured requested settings versus actual zero virtual delta, subsequent tick
advancement, and retention of the original packet after Runtime close. Existing scaled,
zero/multistep, failed extraction and delayed target-adaptation controls remain intact. These
are CPU runtime/projection facts; no GPU, OS timing, scanout or new cross-platform execution
claim follows. The earlier TileSet native delivery keeps its own source revision and scope.

The new public status field requires complete `RuntimeFrameStatus` literals to provide
`simulation_paused`. Ordinary readers and default construction retain their existing interfaces.
This is a pre-1.0 contract correction, with no compatibility alias or second pause owner.

F8 lands at `50f451c`. Independent bounded review reports no P1/P2 for both fixes. Final
App/Transform/Render all-target/all-feature correctness Clippy, the root Runtime/tooling library
check and root formatting check pass on Windows with Rust 1.95.0. Existing advisory lints remain.
Document links/headings and whitespace pass, and all fifteen paused Editor/Dialogue files retain
their original bytes. All twelve scoped implementation/research/review agents in this work unit
are closed. No remote publication, dependency upgrade or calendar-clock migration is performed.

The three review lanes ran no Cargo commands, tests, formatting or native/UI operations. Their
test-source reads establish intended coverage, not fresh passes. The integration owner separately
executes the TileSet workflow and records its run IDs in the owning plan; those passes do not
reproduce the external review's defects. Public dependency metadata, README, maintenance issue
and RustSec advisory were read remotely. GitHub's issue API returned a rate-limit 403; the public
issue page and original advisory source supplied the facts without authenticated access.
The archives carry review prose without the standalone probe sources or execution logs, so their
historical execution claims retain that limit. The timing corrections have the fresh results
above; all remaining findings still require their own scoped implementation and consumer proof.

# Post-Refactor Ownership And Ergonomics Review

Date: 2026-10-22, Asia/Shanghai.
Initial reviewed Nara source: shared dirty working tree based on
`3b7b8617462e42a2dade9cff4a5cdb54bf8975d4`, branch
`test/compiled-asset-domain-export`. Final integration is committed through `0633602`; executed
acceptance is recorded separately. This remains a bounded source review.

## Assessment

The refactor moves ordinary workflow policy toward owners that can complete it. Runtime close now
distinguishes physical completion from final failure; managed Scene travel retains domain progress;
source-local reference editing uses the document transaction; independent dialogue documents use
Host persistence; ordinary product entrypoints centralize bootstrap and cleanup. These are useful
increases in interface depth, rather than merely smaller source files.

Keep these boundaries. Their remaining complexity mostly represents different facts: persistent
validity, mutable World admission, native readiness, source authority, exact asynchronous completion,
and physical custody. Collapsing them into a generic plugin, transaction, registry, or result bus
would move coordination back to consumers. Six bounded opportunities remain below. None is a newly
confirmed runtime correctness defect; API friction, unsupported capabilities, and source-confirmed
work requiring measurement are identified separately.

The correction lanes below retain the review-time acceptance snapshot. Subsequent executed
cohorts and integration disposition belong to the
[final integration evidence](../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md).
This source review does not itself certify runtime acceptance.

## Scope And Evidence

The review follows [Strategy](../../STRATEGY.md#L14), the
[current U1-U9 plan](../plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md#L16),
the [origin source review](2026-10-22-engine-first-principles-source-review.md#L22), and the
[public extension research](../research/2026-10-22-bevy-public-extension-ergonomics.md#L7).
Accepted ADRs 0084, 0090, 0117 and 0118 supply the relevant contracts. Public entrypoints,
implementation owners, independent consumers and selected test bodies were read together.

Only this report is written by this review. No Cargo, formatting, commit, application execution,
web access, or inspection of `vendor/` was performed. Other participants continue changing the
shared tree, so the source links describe the inspected working state and can move before commit.
This is not an exhaustive review of unrelated subsystems or another independent correctness sweep.

### Review-Time Acceptance Snapshot

The coordinating thread supplied these actual results during review:

| Scope | Actual result supplied | Qualification limit |
| --- | --- | --- |
| External dialogue/source-import consumers | 16/16 passed; run `3b6f9a54-3de4-4480-be96-b01d99c2b4c2` | The selected external cohort, not every extension or product workflow |
| Hidden native U6/U7 workflows | 2/2 passed; run `bb7db5ad-f00e-4460-b2c1-2875add55d00` | Windows 11, DX12, Rust 1.99, dirty tree; no additional platform, visible-window/OS-input, scanout, or timing claim |
| Current App/Winit and Scene/tooling cohorts | Running; result unknown at this review's cutoff | No pass or whole-refactor closure claim |

The evolving evidence owner is the
[recovery and review-corrections verification record](../knowledge/engineering/verification/2026-10/2026-10-22T114555Z-workflow-refactor-recovery-and-review-corrections-43dfe5955a684108a8a65dab7aee5ed3.md).
The two results above are explicit review-time updates, not inferred from older pending paragraphs
in that record. Earlier cohorts and catalogue measurements retain their original source/feature
scope; they do not qualify subsequent repairs automatically.

The following were correction lanes at the review cutoff, not new findings from this report:

| Lane | Correction already owned by the coordinating thread | Disposition here |
| --- | --- | --- |
| U4 | Diagnostic privacy for unknown kind/version text, RON classification, and legacy-placement guidance | Source corrections and regressions are visible; current acceptance remains pending |
| U7 | Scene/Prefab versus project-relative extension source namespaces and actual-path exclusion in both Open directions | Included in the corrected external source/test surface; the reported 16-test cohort is bounded evidence, not all lifecycle acceptance |
| U3 | Winit final-failure interpretation and current pending-participant evidence independent of historical errors | Source corrections and regressions are visible; App/Winit result remains unknown |
| U5 | Additional-retirement claim scope, discard when candidate entities are missing, and cleanup diagnostic retention | Active correction/review lane; final owner/public-consumer evidence remains pending |

For example, U4 now explicitly distinguishes syntactically valid identifiers from publishable
content in [format lowering](../../src/project_content/format_diagnostic.rs#L131). U7 has both
[workspace namespace lookup](../../crates/nara_tooling/src/workspace.rs#L663) and
[Host ordinary-Open exclusion](../../src/project_host/runtime/editor/document_open.rs#L191).
U3 exposes [current pending participants](../../crates/nara_app/src/runtime.rs#L609), and U5
contains [additional-retirement and discard regressions](../../crates/nara_scene/src/spawn/publication.rs#L303).
Their presence establishes intended coverage, not execution success.

## Fixed Mature-Engine Comparison

The local reference checkouts were read-only and clean when inspected. Their HEADs match the
origin review's pins. No claim about the latest upstream release is made, and the earlier
research's online supplement is not reverified or used as current-version evidence here.

| Baseline | Relevant source mechanism | Applicable lesson and boundary to retain |
| --- | --- | --- |
| Bevy `85503941e488152711a4695bbe248e9ed9f453fc` | [Plugin registration and lifecycle](../../repo-ref/bevy/crates/bevy_app/src/plugin.rs#L57); `cleanup` is documented before schedule execution | Prefer one coherent typed contribution at ordinary callsites. Bevy construction cleanup is not evidence that Nara can remove runtime physical-close custody. |
| Same Bevy pin | [AssetLoader associated settings](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L32) and [context-owned auxiliary reads](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L608) | Reuse domain settings and dependency observation. Mutable asynchronous asset handles do not replace Nara's admitted Scene generation or source-carrying delivery receipt. |
| Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558` | [Editor undo access](../../repo-ref/godot/editor/plugins/editor_plugin.h#L117), [unsaved/save/apply hooks](../../repo-ref/godot/editor/plugins/editor_plugin.h#L213), [import contributions](../../repo-ref/godot/editor/plugins/editor_plugin.h#L240), and [inspector contributions](../../repo-ref/godot/editor/plugins/editor_plugin.h#L258) | Judge extension completeness across editing, history, persistence and visibility. These declarations do not prove equivalent atomicity, cancellation or receipt guarantees, and do not require adopting Godot's object model. |

Nara's [associated source settings](../../src/project_content/domains.rs#L60),
[separate typed Editor facet](../../src/project_host/runtime/editor/source_asset_import.rs#L33),
and [consuming complete composition](../../editor/src/platform/composition.rs#L59) already apply
the useful Bevy lesson. The facet's typed `import_action` derives the source kind and settings
encoding. `install_panels` rejects selected Host contributions instead of silently dropping them;
the advanced trusted build also carries its selection. Do not reintroduce forwarding domains or
another global package registry to make these constructors look identical.

## U1-U9 Responsibility Review

| Unit | Current responsibility depth and evidence | Why the remaining boundary belongs there |
| --- | --- | --- |
| U1: UI routing | The [admitted adapter transaction](../../crates/nara_ui/src/interaction/transaction.rs#L228) recognizes only specific ordinary nonmodal misses after resolving presentation authority. Consumed capture/reconciliation work still follows the commit path. | The adapter knows both the shown UI route and gameplay finalization. A blanket conversion of rejection to pass-through would lose modal, disabled, stale-route or capture distinctions. |
| U2: rendering | [GPU compatibility](../../crates/nara_sprite_render/src/types.rs#L201) excludes instance tint. [Complete-target preflight](../../crates/nara_render_wgpu/src/backend.rs#L1635) checks combined draw demand through [residency admission](../../crates/nara_render_wgpu/src/texture.rs#L225). | Adjacent batching preserves painter order. Exact immutable image demand and old live occupancy are different facts: impossible demand rejects, temporary retained occupancy can defer. A general resource scheduler is unnecessary. |
| U3: runtime close | [Progress and final evidence](../../crates/nara_app/src/runtime.rs#L557) distinguish `CompleteWithError`, pending obligations and terminal live errors. The [ledger](../../crates/nara_app/src/runtime/close.rs#L153) and [owner custody](../../crates/nara_app/src/runtime/owner.rs#L530) are cohesive responsibilities. | `Stopped` proves physical terminality, not successful teardown. Historical retryable errors must not become final failure after recovery; live terminal obligations must not be dropped as complete. Host/Winit corrections still need current acceptance. |
| U4: diagnostics | One [safe format lowering owner](../../src/project_content/format_diagnostic.rs#L10) retains reason, location, versions and budget facts for production entrypoints. | The decoder knows the error, while the product boundary decides what may be public. Retaining typed reasons is useful; forwarding arbitrary parser/document strings would break the privacy contract. Current repairs remain pending. |
| U5: managed travel | [Captured domain attempts](../../src/project_host/runtime/scene_requests/domains.rs#L58) and [request driving](../../src/project_host/runtime/scene_requests.rs#L442) own retirement, retained candidate, publication and reconciliation progress. The [prepared Scene token](../../crates/nara_scene/src/spawn/publication.rs#L9) returns itself on failed publication. | Complete persistent preparation precedes irreversible native work. Failed native phases remain owned; retries skip completed participants. Rechecking a mutable baseline after the next-frame cut and after domain removals is justified. Additional-retirement/discard corrections remain pending. |
| U6: local EntityRef | [Field admission](../../crates/nara_scene/src/authoring_decode.rs#L2526) and the [document-projected picker](../../editor/src/frontend/imgui/field_editor/entity_reference.rs#L6) use ordinary revisions/history. The [external workflow](../../tests/fixtures/module-consumer/tests/entity_reference_editing.rs#L119) follows edit, Undo/Redo, Save, strict Check, Reopen and headless consumption. | The document owns target integrity and preserved unknown records. The frontend edits a draft, not World identity. Cross-Prefab/projection semantics remain an explicit capability boundary, not a reason to reject supported local references. Native evidence is limited to the reported U6/U7 run. |
| U7: independent documents | The [typed domain owner](../../crates/nara_tooling/src/workspace/extension.rs#L10) owns content/commands/history/codecs; [Host lanes](../../src/project_host/runtime/editor/extension_documents.rs#L90) own capabilities and publication. The [dialogue consumer](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/dialogue_documents.rs#L164) exercises the shared lifecycle. | Domain history semantics, Workspace action revision/dirty projection, Host persistence and toolkit drafts have distinct lifetimes. Type erasure is constrained to the target document and Rust owner; it does not authorize arbitrary service invocation. |
| U8: ordinary launch | [DesktopProductLaunch](../../src/project_host/product_launch/ordinary.rs#L70), [HeadlessProductRun](../../src/project_host/product_launch/headless.rs#L67) and [root opening](../../src/project_host/product_launch/root.rs#L21) own inherited bootstrap, location, identity, profile and bounded cleanup driving. | Ordinary games supply product configuration rather than cleanup choreography. Process termination after an expired live obligation belongs to the process entrypoint; advanced embedded Hosts retain their manual owner policy. Result interpretation remains O1 below. |
| U9: attributed work | [Task admission](../../crates/nara_tasks/src/runtime.rs#L2049) appends without rebuilding an unpressured non-coalescing queue. [Delivery](../../src/project_content/delivery.rs#L193) carries one lazy complete catalogue across roots. | Capacity/coalescing still require their specific work. Complete stable-ID discovery proves uniqueness; on-use digest checks prove freshness. O2/O3 separate remaining caller disposal and per-root copying from the removed repeated-discovery mechanism. |

The [ECS facade](../../crates/nara_ecs/src/lib.rs#L1) still honestly exposes Bevy ECS. Its narrow
lifecycle-free transaction support addresses publication effects, not a second ECS. Private module
extraction is worthwhile where close, custody, publication or diagnostics form a coherent owner;
file size alone does not justify more public interfaces.

## Ordinary Complete Workflows

| User goal | Current end-to-end path | Remaining limit or acceptance dependency |
| --- | --- | --- |
| Build gameplay and run it from development, supervised Play or exported source | One recipe, ordinary desktop launch, verified source/bootstrap, runtime, presentation and physical close; [starter entrypoint](../../starters/desktop-2d/v1/src/bin/desktop.rs#L17) consumes the Host | Samples still inspect the product outcome separately from outer `Result`. No new portable export or first-presentation qualification is supplied by this review. |
| Wire two objects in a local Scene | Select a target from the document, Apply, Undo/Redo, Save, Reopen, strict Check and runtime decode through the same public field lane | The inspected external tests describe the full path; the supplied native run qualifies its bounded widget trajectory. Cross-source targets and large-picker responsiveness remain separate. |
| Add an independent dialogue editor | Trusted registration/Open, typed panel action, owner history, dirty state, Save/Reopen, Save/Discard/Cancel, panel retirement and session close | The 16-test external cohort and 2-test native cohort are actual bounded results. Registration is session-local; automatic document discovery/reinstallation on project replacement is not promised by this slice. |
| Add a compiled asset parser and optional Editor importer | One domain settings type, bounded dependency reads, explicit recipe/facet selection, typed import request, metadata publication, Check/Play/source delivery | The external source-import result is bounded evidence. Native resources, a universal importer and dynamic project-code loading do not follow from CPU preparation. |
| Travel with independent native domains | Prepare complete target, capture two participants, revalidate, retire, publish, reconcile, retry unfinished work or Stop | [Two-domain fixture](../../tests/fixtures/module-consumer/tests/support/domain_reconciliation/hosted.rs#L239) is consumer evidence to execute, not a hardware SDK claim. U5 corrections and final acceptance remain open; no multi-frame pending native protocol is implemented. |

This is the right direction for Strategy's public production coverage: the game supplies gameplay
and product intent; the engine owns its lifecycle. A complete named consumer matters more than the
number of trait implementations or registrations available.

## Which Checks To Keep

[ADR 0117's validation rule](../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md#L259)
allows removing repeated same-owner proofs only after mutable replacement escapes are closed.
Use the fact established by a check, not the similarity of its function name, to classify it.

| Check boundary | Fact it establishes | Review disposition |
| --- | --- | --- |
| Portable bytes, grammar, budgets, domain settings and duplicate IDs | External input is interpretable, bounded and unambiguous | Keep at ingress; admitted immutable values should carry the result internally. |
| Runtime generation, capability identity and source namespace | The operation belongs to this owner and actual source | Keep at each ownership transfer. Workspace namespace and Host actual-path checks answer different questions. |
| Scene baseline immediately before native retirement | World/lifecycle/identity facts still match the retained next-frame candidate | Keep; earlier preparation cannot establish facts after intervening frames. |
| Scene publication after domain retirement | Claimed state is actually gone and remaining publication/retirement is safe | Keep; a participant returning success is not proof that it removed every claimed component. Claims must not authorize unrelated retirements. |
| Async document publication and saved-checkpoint receipt | This exact document/source/incarnation/revision is still current and these bytes were published | Keep. Dispatch acceptance, task terminality, disk publication and a clean document are different outcomes. |
| Close polling, pending projection and final-error interpretation | Physical responsibility ended; current stalled owners and historical errors remain accurately observable | Keep, with one shared final-failure query. Logical cancellation and an error code cannot certify disposal. |
| Repeated `validate_inputs` inside one unchanged delivery call | Repeats lineage/root/schema agreement, including composition fingerprint computation | Confirmed repeated mechanism; measure and consider one private admitted operation scope under O3. Do not remove mutable source checks with it. |

## Six Remaining Opportunities

### O1. Make Ordinary Product Success Harder To Misread

Classification: confirmed API semantic friction; no observed wrong-exit defect. Priority: next
small ergonomics pass after current correctness acceptance.

[Desktop `run`](../../src/project_host/product_launch/ordinary.rs#L76) wraps the terminal report
in `Ok`; [headless `run`](../../src/project_host/product_launch/headless.rs#L68) does the same.
A runtime/startup/teardown failure represented by `DesktopRunOutcome::Failed` or
`HeadlessRunOutcome::Failed` therefore remains an outer success. The outer error instead describes
preparation/source-opening failure. The [starter](../../starters/desktop-2d/v1/src/bin/desktop.rs#L32)
handles both layers correctly, but ordinary callers must know that `run()?` does not establish
product success. The headless starter repeats the outcome distinction as well.

Document the two meanings directly on both `run` methods and evaluate one ordinary success/exit
conversion against these existing consumers. Keep the report and actionable teardown evidence
available; do not flatten physical completion, startup failure and cleanup failure into one string.
Acceptance is a small consumer that cannot accidentally treat a failed product report as a
successful process after only handling the outer error. A generic result framework is unnecessary.

### O2. Measure Capture Disposal On The Submitting Thread

Classification: source-confirmed latency mechanism requiring measurement; not a demonstrated
deadlock, frame-budget violation, or replacement-executor requirement.

The [spawn contract](../../crates/nara_tasks/src/runtime.rs#L1447) now explicitly says rejection,
coalescing and stale cleanup may dispose captures on the submitter. [Admission](../../crates/nara_tasks/src/runtime.rs#L2133)
correctly releases the queue mutex before `safe_drop_pending`, so an expensive destructor no longer
holds that mutex. It still runs before submission returns. Lock locality and caller occupancy are
different properties; the U9 append correction does not promise a bounded submitter duration.

Measure rejected, coalesced and pressure-reclaimed captures from real Editor preparation workloads,
including destructor time separately from queue work. Change disposal custody only if this is
material, and reuse the existing Tasks physical-retirement machinery. Preserve bounded retention,
shutdown and cancellation semantics. Do not claim an improvement in frame P99 from removing a
queue scan, or add another executor to hide an unmeasured destructor.

### O3. Attribute Per-Root Copies And Repeated Input Proofs After Catalogue Reuse

Classification: confirmed repeated mechanism; performance significance and best correction remain
unmeasured for these residual phases.

The old root-count-times-directory-discovery mechanism has been addressed: delivery carries one
operation-owned complete catalogue. [Capture](../../src/project_content/catalogue.rs#L70) copies
the catalogue/database once into that owner; [each later LoadContext](../../src/project_content.rs#L1071)
reserves and clones both again. Independently,
[delivery ingress](../../src/project_content/delivery.rs#L112), its
[startup-consuming helper](../../src/project_content/delivery.rs#L128), and
[each root load](../../src/project_content.rs#L924) call `validate_inputs`, which recomputes the
[schema composition fingerprint](../../src/project_content.rs#L985).

The [existing measurement](../benchmarks/2026-10-22-stable-id-delivery-catalogue.md) supports reduced
discovery counts, with explicit timing/feature limitations. It does not isolate clone allocations,
fingerprint cost or their relative contribution. Budget high-water marks are not measured RSS.
Measure those two phases before choosing between the current clear copies and a private borrowed
immutable catalogue plus per-root mutable state. One private admitted delivery scope could carry
unchanged input facts if they are immutable for its entire lifetime. Preserve budget charging,
complete duplicate discovery and [referenced metadata digest comparison](../../src/project_content/catalogue.rs#L403).
No persistent cache, generation registry or weaker final source admission is justified here.

### O4. Qualify The Open Picker At Real Scene Sizes

Classification: supported workflow with an unmeasured scaling mechanism; not a current assignment
correctness defect. Priority: when representative authoring Scenes create pressure.

The [closed picker](../../editor/src/frontend/imgui/field_editor/entity_reference.rs#L11) now
finds the selected row and builds one preview label, rather than building every choice label. It
still scans for that row and may walk preceding rows for ancestors. When opened, it calls
[choices](../../editor/src/frontend/imgui/field_editor/entity_reference.rs#L54), allocates the
full labelled vector and visits all choices on each paint. Ancestor-path bytes make the cost depend
on depth and label size as well as entity count. This is not evidence of a cache bug or measured
native input lag.

Measure closed versus open paint/allocations and the actual target-selection task with realistic
hierarchy depth and duplicate names. If material, keep filtering/clipping or incremental labels
inside the frontend projection lifetime. Preserve stable source identity, readable ancestry,
captured revision, cancel behavior and owner validation. A second authoritative entity catalogue
or generic graph editor would not improve the owning contract.

### O5. Let A Concrete Thread-Affine Service Define Pollable Close Needs

Classification: explicit capability boundary; no demonstrated unsupported production consumer or
correctness defect. Priority: triggered by a real native service.

[RuntimeCloseParticipant](../../crates/nara_app/src/runtime.rs#L918) remains `Send`, while callbacks
execute on the App owner thread. [RuntimeCloseContext](../../crates/nara_app/src/runtime.rs#L881)
offers immutable World access and protected mutable access to ordinary `Resource` values, but no
dedicated mutable NonSend resource operation. Owner-thread runtime custody does not itself prove
that a participant can retain or poll an arbitrary thread-affine object. Immediate plugin shutdown
may be sufficient for a particular service; no concrete consumer requiring more was established
by this review.

Keep the current contract until a real native owner demonstrates teardown that must span polls.
Then prove construction-thread polling, late cleanup, physical completion and Stop with that
consumer before choosing a narrow owner-thread participant/access seam. Do not relax Send bounds,
introduce an unsafe wrapper, make all plugin traits async, or add a generic native-service registry
on speculation. U5's synchronous nonblocking attempts do not establish multi-frame native travel.

### O6. Measure Extension First Open And Save Capture Before Broadening Async Work

Classification: documented first-slice capability/responsiveness boundary; not a newly confirmed
UI stall. Priority: when a real extension codec exceeds the small dialogue workload.

[Initial extension Open](../../src/project_host/runtime/editor/extension_documents.rs#L149) reads
one existing source, capped at 16 MiB, and decodes synchronously before publishing the document.
[Save checkpoint capture](../../crates/nara_tooling/src/workspace.rs#L2573) invokes the domain's
`encode` on the owner thread; later file publication is background work. A byte limit and a pure
codec contract do not bound elapsed time. The small dialogue consumer and its passing bounded
native trajectory do not establish responsiveness for larger documents.

The [guide](../guides/editor-documents.md#L34) states the synchronous first-Open limit and
session-local registration honestly. Measure first Open and encoding separately from I/O/publication
before changing the interface. If needed, extend existing document-open/persistence preparation
with an owned immutable capture while preserving typed domain history and exact publication.
Keep panel drafts disposable and filesystem authority in the Host. Do not create a second extension
worker protocol or an unrestricted command bus. Automatic session reinstallation is a separate
capability question for a real replacement workflow, not an implicit promise of panel registration.

## Disposition And Analysis Limits

The coordinating thread completed the U3/U4/U5 correction lanes and applied the final U7
file-alias repair before closure. Extension, Scene and Prefab sources now compare actual current
file identities through retained publication parents, preserving exact path reservations and
following atomic Save and parent-directory rename. Independent review found no remaining blocker
in that repair or ordinary launch. Product `run` documentation explicitly separates preparation
success from the report outcome and states the cooperative cleanup-deadline limitation.

The [final integration evidence](../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md)
records 689 affected library tests, the 188-test Editor cohort and scoped owner/public consumers.
Its final rows add 27 external extension/import tests, 42 Host/persistence/generated-starter
tests, 15 creation/export-source tests and two affected Windows/DX12 native trajectories.
These results qualify their named workflows; the source review does not expand them into
whole-engine, operating-system input, other-platform or release qualification.

U1-U9 are closed within the selected plan. Select further architectural work from O1-O6 through
an ordinary consumer or attributed cost. Keep complete ingress, mutable ownership-transfer and
physical-custody checks; remove repeated proof only when its actual cost and unchanged facts are
established. No new protocol surface is required for completeness.

This review supplies source-grounded boundary reasoning and six bounded opportunities. It supplies
no independently executed regression, benchmark, GPU run, Linux/macOS qualification, native
scanout evidence, new export acceptance, or whole-engine performance claim. The integration
results above are supplied by the coordinating thread. The six opportunities are not newly
confirmed correctness defects.

# Foundation Review After U1-U15

Date: 2026-10-19. Source baseline: `aab8ddd`. This is an architecture/source review and repair
roadmap, not an Accepted ADR or implementation completion claim. U1-U15 remain completed within
their recorded scope. A review of those corrections does not establish that every engine workflow
or public input is correct.

The user withdrew the unrelated Pele URL. It is excluded from this review and current design
direction. Comparative sources are the local Bevy checkout
`85503941e488152711a4695bbe248e9ed9f453fc` and Godot checkout
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. References describe those pinned sources, not their latest
upstream state.

**Subsequent implementation:** S1 corrected N01, N02, N08 and N09 through `bfbd2b8`.
The [S1 verification record](../knowledge/engineering/verification/2026-10/2026-10-19T143606Z-authoring-s1-safety-and-field-override-recovery-0aca8f3bff914ffeaf0254f40a5cb118.md)
contains owner, public-control, GPU and selected Windows native results and their limits.
The original findings and review-time evidence below remain historical. N02 additionally required
removing data-only Set-only gates and implementing exact inverse presence; correcting the button
alone did not complete the workflow. S2 subsequently corrects N03 with exact result correlation,
retained field drafts and the default public field helper; its
[verification record](../knowledge/engineering/verification/2026-10/2026-10-19T153834Z-authoring-s2-exact-results-and-retained-field-editing-2b09e865afc64b7990e2adc737fa2aa7.md)
separates ordinary/public/native evidence from remaining capability limits. At that S2 milestone,
N04-N07 and triggered G follow-ups remained open; the dated disposition below records later work.

Six independent research lanes covered execution/tasks, components/plugins, content/persistence,
Editor workflows, iteration/delivery and render/platform ownership. A seventh lane challenged the
combined conclusions. Researchers were read-only; the integration owner ran the bounded Prefab
canaries below. No new runtime architecture or production fix was implemented in this review.

## Current Disposition — 20 October 2026

The findings, source locations, canaries and recommended sequence below describe the original
review baseline. They are retained as history, not as a list of present defects. S1-S3 and the
separate N05 App follow-up are implemented and qualified within the evidence linked from the
[authoring design](../architecture/authoring-interface-ergonomics-design.md). S3 closes the selected
finite-constraint and component-domain diagnostic work; arbitrary cross-field/native invariants
remain an explicit validation boundary. N06 and N07 remain capability triggers, not confirmed bugs.
The selected concrete corrections for N01-N05, N08/N09 and I01-I04 now have qualified evidence
through S1-S3, the N05 App follow-up, U10, `3b70e48`, owned Save/Reopen/startup preparation and
the Scene/Prefab/Image custody repairs. This closes those bounded corrections, not arbitrary
native validation, future capabilities or every possible engine defect.

The selected I/G follow-ups now have distinct dispositions:

| Follow-up | Current disposition | Next dependency |
| --- | --- | --- |
| I01 / G1 standalone cancellation | Corrected at `3b70e48`: pending handles become logically cancelled before retained capture disposal. Task API semantics are explicit. The [v1 investigation](../benchmarks/2026-10-20-editor-responsiveness.md) owns the regression and limits. | Measure rejected/coalesced capture disposal and queue pressure before promising bounded caller latency or changing the queue. Logical cancellation still does not prove physical retirement. |
| I03 / G2/G9 Save and Reopen | [Owned Save](../benchmarks/2026-10-20-owned-editor-save.md) and [Owned Reopen](../benchmarks/2026-10-20-owned-editor-reopen.md) are implemented and qualified within their linked scopes. Native codecs, migrations and document publication retain owner-thread admission. | The adjacent full Host lifetime custody correction is also qualified below; slow storage, complex codecs and total CPU remain separate measurements. |
| Post-terminal failed-Save cleanup | Full Scene/Prefab Host lifetime custody is implemented and qualified at `fe3ad96`. The [custody plan](../plans/2026-10-20-editor-host-lifetime-custody-plan.md) and [verification record](../knowledge/engineering/verification/2026-10/2026-10-20T030524Z-editor-host-lifetime-cleanup-custody-qualification-12c6b025d6c74ed1bf6900c8b0413877.md) own acceptance and limits. | No further Host framework is required by this repair. |
| I02 / Project Open startup closure | The v1 workload triggered investigation: at 4,096 empty entities, startup-closure P50 was 9.920 / 10.048 / 9.911 ms and P95 11.194 / 10.312 / 12.028 ms across three processes. Owned startup preparation is implemented and qualified at `eb1ee98`; three optimized v4 processes pass in the [startup report](../benchmarks/2026-10-20-owned-editor-startup-closure.md). | Owner/Editor/native, independent module/reference and workspace qualification pass. Session/runtime construction and publication stay on the owner. The 8 ms trigger is not a product SLA. |
| Subsequent sweep / Image metadata publication cleanup | The confirmed lost-cleanup-authority defect is corrected and qualified at `bae0440`: the concrete `FilePublicationHost` registers with finite runtime custody before mutation, and the session latch exists before the Host is borrowed. The [Image verification record](../knowledge/engineering/verification/2026-10/2026-10-20T035244Z-editor-image-metadata-publication-cleanup-custody-qualification-c097c716d6f3480692d800285ec53544.md) records owner/Editor/native and workspace results. | [Image custody acceptance](../plans/2026-10-20-editor-image-publication-custody-plan.md) retains its own scope. No new feature or timing claim follows. |
| Other G1/G2/G3 latency concerns | Browser Open, source-receipt/compiled-Prefab refresh, caller capture disposal, GPU completion holds and aggregate Runtime workers have no qualifying measurement here. | Measure their actual consumer before choosing queue indexing, callback deduplication, another executor or another job system. |
| N06/N07 and G4-G8 | Code freshness/state restore, multiple same-kind documents, dependency roles, live rename, residency/save, reusable domain fields and Tile authoring are workflow-dependent capability work. | Let a concrete game or tool establish the required contract; these are not hidden closure conditions for the completed corrections. |

The lifetime follow-up gives each admitted file Host one retained owner across idle periods,
terminal failed Save, failed/stale Reopen, lane replacement, content-runtime replacement and
session abandonment. The runtime close participant retains cleanup authority until physical
retirement, and admission closes on the first close attempt, including while prior work is still
pending. It replaces operation-scoped Save custody rather than fabricating idle Save requests.
Final qualification passes 125 selected root/public tests, 189 Editor/application tests and three
selected actual Windows/DX12 native workflows, plus the locked all-features/all-targets workspace
check. Counts overlap; this is not cross-platform release certification. The linked record owns
compound failed-cleanup, runtime-replacement and held-read admission evidence. Existing v1/v2/v3
measurements remain frozen; no new latency result follows.

The subsequent [startup-closure slice](../plans/2026-10-20-owned-editor-startup-closure-plan.md)
moves bounded source reads, decode, dependency traversal and pure schema classification into an
owned linear attempt. The Open Host reuses small task pools and five physical capture permits;
logical cancellation cannot admit unlimited unretired work. Adapter admission, native registry/
Plugin construction, session/runtime creation and Workspace publication remain on the owner.
Only the live attempt can admit its completed result. Cancel/Exit revoke publication without
claiming to preempt OS reads; unchanged StartupClosure polls do not request presentation.

All three optimized v4 processes pass. At 4,096 empty entities, maximum startup owner-advance P95
is 0.066 / 0.081 / 0.077 ms. Startup completion is still observed at roughly 11 ms, full Open at
20-22 ms and session publication remains roughly a 6 ms owner call. These are different spans,
not faster storage or lower total CPU. Prepared-receipt disposal is measured after reaching
SessionPublication without creating a session; it is not untaken task-payload destruction or a
generic caller-destructor guarantee. The report retains all process rows and frozen v1/v2/v3 data.

The [startup qualification record](../knowledge/engineering/verification/2026-10/2026-10-20T032446Z-owned-startup-closure-publication-and-responsiveness-qualification-f187b61104d4443187cea8b9bc392460.md)
records final qualification at `eb1ee98`: 131 selected engine/public tests, 195 Editor/application/
panel tests and four selected actual Windows/DX12 native workflows pass. Independent Scene and
panel consumers pass 3 and 2 tests; all 4 reference consumers pass, including the 386-second full
Play/Restart/Stop/Exit workflow. The runtime-ui-off test, locked all-features/all-targets workspace
check (70 seconds), formatting and diff checks pass. Three v4 processes retain 864 rows each and
162 summary groups, independently reviewed. Counts overlap and do not establish a native SLA or
cross-platform release support.

The original N/I corrections retain their linked repair evidence. A subsequent same-class sweep
identified a separate Image metadata cleanup defect: at that reviewed baseline,
`EditorImageImportHost.publication_cleanup` was the sole authority after terminal failure
(transaction lines 232 and 603-606). Explicit close could stop the runtime while cleanup remained
pending, then session Drop lost the remaining retry authority. That historical finding is now
corrected by the [Image custody slice](../plans/2026-10-20-editor-image-publication-custody-plan.md).

The concrete metadata `FilePublicationHost` registers with finite HostCustody before writing,
and the transaction establishes its session latch before borrowing that Host. Failed cleanup
therefore survives terminal results, close, Drop and unwind independently of the Image projection.
Runtime close remains incomplete until cleanup retires. Success retires mutable publication
authority and retains only the exact read source and binding; pending completion still opens
metadata afresh and validates identity/digest. Source conflict and last-good policies are unchanged.

Three new compound tests pass: missing/existing metadata failure followed by direct Drop; completed
task shutdown with file close still incomplete followed by Drop; and catch/unwind after actual
publication through the real Host mutex. They assert nonempty temporary files, unchanged PNG/
metadata/Scene bytes, exact directory entries and one retained runtime until independent atomic
fault release removes the temporary file and returns quarantine to baseline. The panic case proves
retry through a poisoned Host mutex. Independent integration review found no current reachable
blocking defect. The [Image qualification record](../knowledge/engineering/verification/2026-10/2026-10-20T035244Z-editor-image-metadata-publication-cleanup-custody-qualification-c097c716d6f3480692d800285ec53544.md)
records 116 root tests passing with 81 excluded, 186 Editor tests with 19 native tests excluded,
and two selected actual Windows/DX12 native tests passing. Final source is `bae0440`; the locked
all-features/all-targets workspace check passes in 56.28 seconds, with formatting and diff checks
passing. No additional runtime-ui-off qualification ran in this Image slice.
These are separate overlapping cohorts, not a combined total or broader release certification.

Image import runs only in ordinary Authoring; trusted compiled content-runtime replacement has
no actual consumer intersection with this workflow. Finite custody transfers registered variants
by construction, without claiming a native test of an unreachable intersection. Schema CLI failures
retain their explicit manual-cleanup contract. Earlier lower-filesystem unwind remains best effort;
this slice covers cleanup transferred into the publication Host, not every possible OS failure.
All v1-v4 measurements stay frozen, with no latency-improvement claim. The remaining registered
G9 costs, including Browser admission, receipt/content refresh, capture disposal, GPU holds and
aggregate workers, need concrete consumer measurements; they are not additional confirmed defects
closed by this slice. Future capabilities and cross-platform release proof retain separate scope.

## Assessment

Keep the existing ECS substrate, managed Runtime ownership, immutable content/render candidates,
explicit schema lineage and source-built composition. No evidence here justifies replacing those
foundations. Remaining weaknesses cluster around admission of public raw data, the complete
submit/accept/reject lifecycle of authoring actions, and the meaning of data/code/document identity
as workflows grow.

At the review baseline, five behaviors deserved correction before adding more authoring tools. Other rows identify
contracts to settle when their concrete consumers enter; they are not all current defects.

| ID | Finding | Classification | Recommended order |
| --- | --- | --- | --- |
| N01 | Custom/in-memory Prefab sources can panic before structural diagnostics | Confirmed public-interface correctness defect; two executed failures | First |
| N02 | Inspector Remove confuses authored presence with effective default | Current frontend behavior defect; source-confirmed | First |
| N03 | Drafts retire before owner acceptance; custom panels lack correlated action outcomes | Current draft-loss behavior plus an extension interface gap | First, then real external authoring consumer |
| N04 | Data-only schema coverage cannot express common native domain constraints | Deliberate validation limit with future tooling cost | Before expanding reusable component authoring |
| N05 | Codec/plugin diagnostic bridges discard extension error identity | Extension interface gap, with privacy intentionally preserved | Alongside N04 or the next domain validator |
| N06 | Code-input observations, authored content and executable generations have different freshness guarantees | Existing explicit limit; predesign for automatic code rebuild | Before automatic code-currentness or restore claims |
| N07 | Multiple panels/tabs do not provide multiple same-kind Host document owners | Existing safe capability limit | Before cross-document tools |
| N08 | Editor target allocation lacks the expected OOM error channel used by Image uploads | Current allocation error-path gap; source-confirmed, no OOM injection | First |
| N09 | A permanently invalid static fragment shader is retried every frame | Current repeated-failure work; source-confirmed, no percentile measurement | Alongside current render corrections |

## N01. Validate Resolved Prefab Topology Before Expansion

Public [PrefabDocument fields and constructors](../../crates/nara_scene/src/prefab.rs):34 permit
programmatic raw documents; [SceneEntityRecord children](../../crates/nara_scene/src/document.rs):113
are equally public. The [resolver contract](../../crates/nara_scene/src/prefab.rs):197 returns a
bare document, and [in-memory insert](../../crates/nara_scene/src/prefab.rs):218 does not admit it.

The [public expansion entry](../../crates/nara_scene/src/prefab.rs):494 starts expansion directly.
Source preflight counts materialization work; empty overrides clone the source without running
patch validation. Expansion indexes [roots](../../crates/nara_scene/src/prefab.rs):832 and
[children](../../crates/nara_scene/src/prefab.rs):906 before the final expanded-document validator.
A dangling source root or child therefore panics instead of producing `PrefabExpansionReport`
with no candidate and an error diagnostic. The public spawn path also consumes expansion.

This was executed with Rust 1.95.0, Windows, repository lockfile, shared target and
CARGO_BUILD_JOBS=1:

```text
cargo +1.95.0 nextest run -p nara_scene --lib --features serde --locked -E 'test(foundation2_probe_)' --test-threads=1 --no-fail-fast
```

Run `ce1d7583-9969-48cf-ab54-e2594d1942a8` failed both expected-diagnostic tests: the missing-root
case panicked at line 832, missing-child at line 906, each with `no entry found for key`.
The two temporary tests were removed by exact patch after capturing the result; the production
and test tree has no retained Rust change. No passing repair is claimed.

The root case is small enough to reproduce in an ordinary consumer:

```rust
let source = AssetRef::path("bad.prefab.ron").unwrap();
let prefab = PrefabDocument::new([SceneEntityId::new("missing").unwrap()], []);
let resolver = InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), prefab);
// Expand a valid Scene containing an anchor that references `source`.
```

**Owning correction:** validate the resolved source's placement graph before the first lookup
that assumes graph completeness. Reuse the existing structural validator and diagnostics. Avoid
a new family of public validated-wrapper types or repeating native codec validation at every edge.
Prove both expand and spawn fail without panic, World mutation or identity retirement; retain
source-cycle, duplicate-ID, override and materialization-budget coverage.

**Limits:** file-backed candidates already validate before resolver insertion; this is not evidence
that ordinary malformed files bypass Host admission. Duplicate IDs and hierarchy cycles belong
to the same owning regression set, not separate findings.

Godot [PackedScene](../../repo-ref/godot/scene/resources/packed_scene.cpp):239 checks graph positions
while instantiating. Bevy's [resolved Scene](../../repo-ref/bevy/crates/bevy_scene/src/resolved_scene.rs):28
uses explicit failure results. The transferable principle is ownership of input completeness and
failure reporting, not their scene/object representation or transaction guarantees.

## N02. Separate Stored Overrides From Effective Defaults

[Inspector projection](../../crates/nara_tooling/src/inspector.rs):1067 exposes both
`SceneInspectorFieldState::Present/Defaulted` and an effective `value`. The
[native Remove button](../../editor/src/frontend/imgui/authoring.rs):651 instead checks
`field.value.is_some() && !field.required`.

This disables removal of an explicitly authored required field that has a default, although the
[patch owner permits that reset](../../crates/nara_scene/src/patch.rs):2156. It also enables Remove
for a missing optional field with a materialized default, then the raw `remove_path` fails.
The owning patch/history behavior already distinguishes omission from an explicit stored value.

**Correction:** consume the existing authored presence state. Allow removal when an authored value
exists and `!required || default_value.is_some()`. Use a restore-default label when appropriate.
Prove required/default, optional/default and optional/no-default through controls, Undo/Redo and
Save/Reopen, asserting raw omission as well as the effective displayed value. This source-confirmed
finding has not yet received a new native reproduction run.

Godot separates [whether a property can revert](../../repo-ref/godot/core/object/object.cpp):652
from its revert value. Nara already has the necessary state; no new default-value subsystem is needed.

## N03. Retain Drafts Until The Correct Owner Outcome

The [scalar Apply path](../../editor/src/frontend/imgui/authoring.rs):738 removes its draft when
wire parsing succeeds, before the document owner accepts the action. Parsing uses the wire kind,
not the complete shape/numeric domain. A derived `u8` field accepts text `256` as U64 at this stage;
the owner rejects it, keeps the document/revision intact, and reports feedback, but the next paint
shows the old value because the user's input is gone. Recursive Apply similarly clears its node
at [local conversion](../../editor/src/frontend/imgui/authoring/recursive_controls.rs):42.

The extension interface has the same incomplete lifecycle: [ImguiEditorPanelContext](../../editor/src/frontend/imgui/workspace.rs):38
offers `model()` and `emit(action) -> ()`. Precise
[EditorActionBatchReport outcomes](../../crates/nara_tooling/src/frame.rs):743 exist, but
[frontend report observation](../../editor/src/frontend/imgui/workspace.rs):188 does not deliver
each custom panel's own transaction result. Global revision, single-slot field feedback and value
comparison cannot establish success when two panels submit from the same revision.

**Correction:** track submitted actions by originating panel incarnation and action position,
route existing outcomes or batch rejection back to that instance, and retire drafts only after
the corresponding success. Preserve failed input for correction. Explicitly terminate filtered
actions during closing, and prevent a retired instance's result from reaching its replacement.
Use the existing action/history owner; do not add a generic event bus or another transaction system.

Proof needs a real external panel that commits data, with numeric-domain rejection, stale revision,
two-panel conflict, batch rejection and panel retirement. Navigation's later-frame selection proof
from U13 remains valid but does not prove this different transaction-result requirement.
Early shape validation can improve feedback; it cannot replace authoritative acceptance.
This finding is source-confirmed; no new UI interaction test ran in this review. It deepens G9.

## N04. Make Validation Coverage A Deliberate Component Contract

[Native preparation](../../crates/nara_reflect/src/registry.rs):1632 applies schema checks and the
native codec. [Data-only admission](../../crates/nara_scene/src/authoring_decode.rs):1661 uses
`validate_component_value`, whose documented scope is field coverage, defaults, wire kinds and
numeric width/domain. Common semantic conditions remain native-only.

For example, [UiText decode](../../crates/nara_ui/src/codec.rs):257 requires positive logical size,
while its exported schema uses F64. It also rejects nonempty semantic text when the enable flag
is false. An external cooldown/health component has similar range or cross-field conditions.
The generic Editor can preserve a structurally valid edit which later fails native Check.

ADR 0118 explicitly says successful data authoring does not assert Runtime readiness. This is
therefore an honest capability gap, not a broken sandbox or an existing data-safety guarantee.
Before many game components depend on separate handwritten validators, choose one small real
consumer for bounded declarative constraints, include those facts in catalog identity/lineage,
and share their interpretation across data-only edits and native preflight. Constraints that
cannot be expressed remain owner-specific, with their validation coverage stated clearly.
Do not introduce an arbitrary expression language or execute project Rust in ordinary Editor Open.
Keep this separate from G7's reusable field-adapter question.

## N05. Preserve Safe Extension Error Identity Across Bridges

[ComponentCodecError](../../crates/nara_reflect/src/codec.rs):20 represents field/detail failures
with arbitrary strings. The [Scene diagnostic bridge](../../crates/nara_scene/src/diagnostics.rs):293
correctly redacts them, but custom domain reasons collapse into generic kinds. Data-only admission
also drops the underlying value error. Meanwhile
[plugin component registration](../../crates/nara_app/src/plugin.rs):395 stringifies structured
registry failures.

Before domain-specific Check/Inspector repair hints grow, provide a bounded stable custom code,
safe static summary and validated stable root-field ID through the owning codec/plugin error
bridges. Reuse `nara_diagnostic` vocabulary; preserve private raw error details and redaction of
dynamic map keys, payloads and absolute paths. A consumer should distinguish `cooldown.negative`
from `health.maximum-below-current` without parsing Display text. Existing structured registry
callers and `PluginPrepareFailure` already retain more information, so this is a bridge gap rather
than a failure of the entire diagnostics design.

## N06. Name Code, Content, Build Attempt And Running Bytes Separately

[Export input revision](../../src/project_workflow/export/build.rs):19 observes five generated
entry files; [Play input revision](../../editor/src/supervised_play/build/source.rs):13 observes
Cargo manifest/lock. The saved-source receipt covers authored startup content. Executable digest
and immutable generation identify the bytes actually run.

Ordinary local gameplay crates are already supported by the public guide. Sequential build and
run works. ADR 0116 explicitly does not claim a complete Rust-module/build-script/environment/
proc-macro closure, and Restart intentionally launches the same generation. A Rust-only save
can leave authored-content identity unchanged. These facts are not a current export correctness
defect or a reason to prohibit ordinary Cargo modules.

Before automatic rebuild promises that the game matches current code, choose cooperative finite
input observation or immutable-source build semantics. Keep content revision, observed editable
code, attempt and executable generation distinct. Cover module/local-crate changes, a second save
during compilation and an old attempt completing late. Do not implement Rust name resolution or
replace Cargo. Before state-preserving rebuild, separately choose reset-to-authored, checkpoint
restore or another explicit game experience under G6; Last Good is executable retention, not save.

Current edit-to-result samples do not supply current P50/P95 populations for that decision. Use
a small public game and decision-local data/body/structural edit workloads; record preparation,
Cargo, generation publication, READY and changed gameplay result separately, with cache/profile/
toolchain and missing values. ADRs 0068/0099 keep this outside a general benchmark service.

## N07. Document Ownership Must Grow Before Cross-Document Tools

[Host open](../../src/project_host/runtime/editor/document_open.rs):80 retains one Scene lane and
one Prefab lane; opening another source of the same kind replaces the existing document through
CloseDocument. Multiple tab facts and panel instances do not add independent persistence owners.
Fixed Source panels explicitly require activation before inspecting an inactive source.

Before two-Scene comparison, cross-source reference refactoring or pinned Inspectors for different
documents, key the necessary Host lanes and immutable projections by document ID. Let the actual
consumer determine cross-document transaction needs; do not infer Save All, a recovery journal or
atomic project-wide publication from per-document save receipts. G5 still separately governs live
source relocation. This is a safe current limitation and a G9 extension trigger, not data loss.

Source-built panels also do not yet imply inline Inspector, menu or source-handler contributions.
The first concrete domain tool should select the narrow needed seam and continue emitting admitted
actions. Godot's [Editor plugin entry points](../../repo-ref/godot/editor/plugins/editor_plugin.cpp):515
illustrate separate responsibilities; they do not require a dynamic ABI or full plugin framework.

## N08. Keep Editor Target Allocation Failure Inside Reservation Results

[Viewport reservation](../../crates/nara_render_wgpu/src/editor_execution.rs):1242 checks the live
device's maximum texture dimension, then directly creates the texture and view. It has no local
OOM error scope. The native [resize path](../../editor/src/platform/imgui.rs):282 reserves a new
target before retiring the old one, so valid ordinary operations can temporarily retain both.
Valid dimensions and finite view count do not guarantee allocation under external memory pressure.

The [device setup](../../crates/nara_render_wgpu/src/backend.rs):1215 installs device-lost handling,
not an allocation-error result for this operation. Expected allocation failure can escape the
`Result` through WGPU's uncaptured-error handling, rather than returning a typed reservation failure.
Independent review checked the actual locked dependency, `wgpu 30.0.1` with `wgpu-core 30.0.0`:
`wgpu/src/backend/wgpu_core.rs:657-694` routes uncaptured errors to its default panic handler, and
`:1630-1645` routes texture creation errors into that sink. The local `repo-ref/wgpu` checkout is
not the version authority for that behavior.
The already-corrected [Image allocation path](../../crates/nara_render_wgpu/src/texture.rs):833
captures expected allocation failure. This is a missing error channel, not evidence of unlimited
view creation, a measured VRAM limit or an injected OOM failure in this review.

**Correction:** extend the existing target reservation owner with expected allocation-error
capture and a typed deferred/rejected outcome. Preserve the old target, its registrations and
presentation authority if the candidate fails. Inject a controlled allocation failure and prove
no target publication/charge leak, then successful smaller allocation and normal retirement.
Aggregate target-memory policy remains a separate measured question under OQ-016/OQ-024.
Godot's [resource allocation checks](../../repo-ref/godot/servers/rendering/rendering_device.cpp):1766
support rejecting failed resources before publication; no render graph follows from this repair.

## N09. Remember Permanent Pipeline Rejection

Static fragment source is already a public capability: the
[palette material example](../../examples/palette_material.rs):14 provides included WGSL through
`Material2dDefinitions`. Its [definition constructor](../../crates/nara_material/src/lib.rs):111
checks shape/identity/nonempty source, not WGSL syntax.

[Pipeline acquisition](../../crates/nara_render_wgpu/src/backend.rs):1649 caches successful pipelines
only. [Shader/pipeline creation](../../crates/nara_render_wgpu/src/quad.rs):226 synchronously waits
for validation and maps invalid source to content rejection. The ordinary render path skips the
rejected frame; the next frame sees the same immutable source/key and repeats the failed work.
No source or dependency can become ready spontaneously in this closed static-source path.

**Correction:** retain terminal pipeline rejection under the existing key and device namespace.
Keep retryable allocation/device outcomes distinct. Restart/device replacement can invalidate the
corresponding cache state. Prove the same bad source is attempted once across multiple frames,
valid material still presents, and a new namespace revalidates. The observed defect is repeated
failure work, not a measured P95/P99 hitch. No shader graph, public cache API or OQ-002 expansion
is required. Bevy's [pipeline states](../../repo-ref/bevy/crates/bevy_render/src/render_resource/pipeline_cache.rs):46
and [retry classification](../../repo-ref/bevy/crates/bevy_render/src/render_resource/pipeline_cache.rs):911
illustrate the distinction between missing dependencies and permanent shader failure.

Both render findings received a separate adversarial source/contract check. N08 is an expected
error-channel repair; N09 is deterministic repeated work. Neither requires a performance threshold
to describe its control flow, and neither supplies a measured driver failure or frame-time result.

## Existing Follow-Ups: Sharpen Rather Than Duplicate

| Existing trigger | New clarification | Recommended proof before mechanism |
| --- | --- | --- |
| G1/G9 task cancellation | Standalone abandoned-owner pending cancellation currently progresses with capture disposal; a blocking first capture can delay the second pending handle's terminal state. Managed close already cancels the batch first. Drop's terminality guarantee needs an explicit choice. | Preserve the second handle, hold the first capture's Drop, assert the chosen logical terminality behavior, then release all gates and prove physical retirement. Source inference only; not executed here. |
| G3 worker budget/frame jobs | Multiple Runtime owners each install default pools; background ownership is not scoped frame parallelism. | Measure aggregate worker count and CPU/P99 under the real Editor/game combination; evaluate Bevy schedule/query parallelism before another job kernel or process-global task service. |
| G2/G9 responsiveness | Byte/work-count limits do not bound synchronous Open/Save or render preparation latency. | Select representative source/Scene/storage workloads and phase budgets; offload only transportable preparation with stale-result and physical-retirement proof. |
| G2/G9 submitted-resource retirement | Image completion holds are currently registered per batch/auxiliary image; repeated use can add callbacks without adding texture bytes. This is not a leak or a violation of the separate telemetry callback bound. | Measure high-batch repeated-image submissions and delayed GPU completion first; if necessary deduplicate holds under the existing submission owner, preserving exact physical lifetime. |
| G7 component fields | Aggregate creation templates and reusable domain-field adapters remain separate questions. | One actual component needs creation, editing, defaults, migration and strict Check; do not add an adapter registry just to cover hypothetical types. |
| Platform qualification | Windows/DX12 and the Linux FIFO cohort do not prove the complete external author workflow on Linux/macOS. | Run the same public Create/plugin/Save/Play-error-repair/Stop/Export/arbitrary-CWD workflow on the claimed native platform. No new release framework is needed. |

## Module Locality

Current raw line counts include tests and describe review scope, not an automatic split threshold:

| File | Lines | Useful responsibility seam |
| --- | ---: | --- |
| `crates/nara_tasks/src/runtime.rs` | 3,479 | task state/result, queue/executor, abandoned-owner retirement, close adapter |
| `crates/nara_app/src/runtime.rs` | 3,886 | managed drive/control, fault bridge, close ledger, retirement |
| `crates/nara_app/src/lib.rs` | 3,976 | composition/build/admission; inline tests begin near line 2,035 |
| `crates/nara_tooling/src/workspace.rs` | 4,672 | document/source facts, slots, persistence authority, projections; tests begin near 2,920 |
| `crates/nara_reflect/src/registry.rs` | 2,969 | registration/owner merge, schema admission, value/reference validation |
| `crates/nara_scene/src/patch.rs` | 3,055 | wire/cost, admission, apply/inverse, placement |
| `src/project_content.rs` | 2,905 | source acquisition, exact closure receipts, domain reconciliation |

The 7,338-line Runtime, 5,725-line Image, 5,401-line Scene and 4,054-line UI adapter test files
also deserve contract-local extraction when substantially changed. A first `#[cfg(test)]` can
guard one import or helper, so it is not a reliable production/test line counter.
Split privately as each owner changes; preserve exports and observable ordering. Avoid splitting
by arbitrary line limits, introducing new crates merely to move code, or growing a generic test
framework. N01 can be corrected in its existing owner without a whole Scene rewrite.

## Review-Time Recommended Repair And Design Sequence

1. Repair N01 source topology admission and N08 target allocation failure, then N02 default/reset
   semantics and N09 permanent pipeline rejection. Use focused red/green
   owner and native/public-consumer tests with no unrelated architecture expansion.
2. Complete N03 as one submit/outcome/draft lifecycle, using a real external editing panel. This
   should precede additional domain tools; add early shape feedback without duplicating authority.
3. Use N04/N05 together with one domain consumer to decide finite constraint/diagnostic facts.
   Record schema lineage changes explicitly; retain unsupported native validation as a declared limit.
4. Choose current measurement workloads for G1/G2/G3/G9. Make the cancellation terminality choice
   before promising standalone pool Drop behavior. No blanket async conversion or new job system.
5. Enter N06/N07 and resource/platform extensions when their named workflow starts. Decide the
   user experience first, then commit the smallest complete contract and refactor its owner.

This order prevents avoidable interface debt while retaining the valid ownership foundation.
It does not declare the findings fixed, authorize implementation of every future capability, or
reopen the completed U1-U15 goal.

The subsequent [authoring interface ergonomics design](../architecture/authoring-interface-ergonomics-design.md)
compares three interface directions and records the implemented default field editor, exact
dispatch correlation, component inventory and bounded constraints. Its linked S1-S3 and N05
verification records, followed by the Save/Reopen reports and current custody plan above, own
completion evidence. The original review-time recommendations do not reopen those corrections.

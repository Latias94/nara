# Captured WGPU Render Programs

Use a finite program for native Rust texture/buffer dependencies beyond stock drawing, such as
two-pass blur, preserved Scene color, compute statistics and a histogram. `WgpuRenderHost` owns
device acquisition, submission and retirement. The
[independent consumer](../../tests/fixtures/render-program-consumer/README.md) implements the
whole workload through public APIs without a World, Runtime, window or private executor.

This guide covers the bounded prerequisite admitted by
[ADR0094](../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs).
The [typed policy replacement](../knowledge/engineering/verification/2026-10/2026-10-31T192756Z-original-typed-render-policy-capture-and-complete-consumer-qualification-df7f9377f51142b78060b53617c800a1.md)
also carries ordinary Runtime/Editor consumers and an independent alternate policy through this
Host. The [captured-history successor](../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md)
adds exact View/Feature history candidates and independent temporal consumers. Complete
cross-stream composition and custom-drawn Editor qualification remain in the
[render unit](../plans/2026-10-31-render-composition-and-host-execution-plan.md).

## Definitions, Instances And Captures

Implement `WgpuRenderFeature` in the native adapter package. Its immutable definition pairs one
`Capture` type with its exact adapter. `State` belongs to one instance and device epoch,
`Prepared` owns one operation's native work, and `Report` returns typed evidence after successful
group admission. Capture exact CPU inputs and typed resource ports; do not capture a live World,
mutable content resolver or one-shot UI completion permit.

Install a `WgpuFeatureDefinition<F>` with `host.create_feature_instance(&producer, &definition)`.
Reuse that instance for later captures; use two instances for independently mutable state.
Dropping an instance closes admission irreversibly. Submitted uses retain its original definition
through cleanup. A replacement of the same Rust type cannot receive old work or reports.

Declare `WgpuDeviceRequirements` before `begin_frame` or other device acquisition. Requirements
aggregate requested WGPU features and the strongest limits, including correct buffer-alignment
direction. Adapter support alone does not enable a capability. Late requirements must fit the
original enabled request; `ReconstructionRequired` refuses the contribution without invalidating
other producers. A healthy device is never silently replaced.

## Policy Capture And Selection

Implement `WgpuRenderPolicy` when a renderer owns capture meaning and complete graph wiring.
Its immutable definition freezes device requirements. Its `Capture` contains replayable CPU data
and the original logical declarations. Call `WgpuFeatureDefinition::declare` once for each
independently mutable effect incarnation and retain or clone that declaration into captures.
Each preparation calls `bind(capture, context)` to create immutable `Bindings` holding actual
typed Feature owners. `WgpuPolicyBindingContext::feature` resolves the original declaration within
the closed device request. Cloning a declaration preserves its incarnation; another `declare`
creates an independent one. Only successful complete publication changes cached membership. `build`
receives the exact capture, neutral packet and actual destination format. It has no World, Device,
Queue or presentation authority. Return `WgpuPolicyProgram` with its published final texture and
explicit observation counters. The Host checks the final texture's actual extent, format,
transfer and alpha contract before execution.

For normal ECS capture, install `WgpuCapturePlugin`, select one exact installation with
`install_wgpu_policy`, and run the domain collector in `WgpuPolicyCaptureSet::Capture`. The
collector reads ordinary ECS parameters and calls `WgpuPolicyFrameWriter::capture` with an
`Arc<Capture>`. Check `can_capture` before consuming move-only domain work. Backpressure leaves
the already accepted input intact. A second installation cannot silently replace the selected
route. `WgpuWindowPresentationPlugin` selects the ordinary native-window consumer; capture alone
does not require a window or create a second native execution owner.

The default product explicitly composes `WgpuStockCapturePlugin`, `WgpuStockPolicyPlugin` and its selected
`WgpuStockSpritePlugin`/`WgpuStockUiPlugin` contributors. Compiling a domain does not install it.
Independent image owners may retain their installation and call `installation.capture` directly.
Call `host.prepare_policy` before device acquisition to admit nondefault requirements. The same
accepted `WgpuRenderInput` can adapt an equal-sized physical window target to an image with
`with_image_target`; original source, frame, logical Views and WorldCanvas facts remain pinned.

Closing or dropping an installation rejects new captures. Accepted inputs retain their original
definition and execution lease, so Host construction never looks up the currently installed policy
to reinterpret them. Submitted work retains the original capture and CPU instances through actual
completion. Producer close still refuses execution. A retained CPU input can build new instances
on a replacement Host; it contains no old native state. Host caches are finite and reclaim entries
after the installation, accepted inputs and submitted uses release their leases.

UI paint packets remain replayable data while backend presentation permissions stay move-only in
their concrete sidecar. An external policy's collector can check `can_capture_with_ui`, take one
`UiBackendSubmissionReader` frame, split it with `WgpuUiSubmission::capture`, and pass its typed
capture plus the original sidecar to `capture_with_ui`. Acceptance moves both into the same slot;
failure returns the original sidecar for retry or explicit settlement. A collector must not drop
that returned responsibility to impersonate successful capture. `cancelled` aborts an unbound
attempt and defers an already bound one. Content rejection skips that policy frame and allows
the next Runtime frame to repair it; an owner or topology violation still faults the Runtime.

A trusted backend can take untouched attempts with `into_prepared_submissions` before native
binding. A bound sidecar refuses the transfer and returns itself. The `nara_ui_render::advanced`
batch binder checks all retry generations before moving any attempt into its bound state; a
failure retains the complete original batch. Binding alone does not establish presentation.
The backend must present the exact packet before returning its one-shot `presented` outcome.

`WgpuUiProgramBinding` binds the exact UI operation key, original Feature
instance and typed capture Arc. That operation must write the selected final texture. The Host
checks its actual original typed report before output publication; drawing UI into an unrelated
texture cannot complete a permit for the displayed image. Equal packet contents cannot substitute another operation. An
image submission with Runtime UI uses `submit_for_display` and retains its designated display
completion obligation. Authoring Canvas has no Runtime UI permission. A passive mirror never
settles another occurrence's permission.

These APIs do not establish a persistent renderer recipe, runtime route replacement or native
code unloading. Complete current qualification belongs to the render unit and its evidence.

## Composing With Stock Drawing

The shared stock-construction replacement is under qualification in the
[current unit](../architecture/render-program-production-design.md#next-production-unit-public-stock-composition).
Use `WgpuStockStages` inside an ordinary `WgpuRenderPolicy` to reuse default Sprite, Canvas and UI
lowering. Include its `device_requirements`, resolve its original `bind` result through the policy
context, and call `begin` with that binding, the immutable `WgpuStockCapture`, the current build
context and finite `GraphLimits`.

The returned `WgpuStockProgramBuilder` owns one ordinary graph. `world(operation_key)` draws
ordered Views into `WgpuWorldColor`; caller Features use `graph()` to create and process distinct
versions. If the packet has a WorldCanvas, `resolve_canvas(operation_key, &world_version)` applies
its exact nearest mapping and returns `WgpuSceneColor` at target extent. Otherwise world color
already has target extent. A generic effect can accept either marker; wiring chooses the actual
pixel space and radius units. No identity copy is required solely to relabel the no-Canvas path.

Call `finish(ui_operation_key, &selected_scene, additional_outputs)` after the desired Scene
effects. It validates the actual target shape/color interpretation and draws captured UI at full
target resolution into a separate final version, with its exact original UI binding. Additional
outputs can retain untouched Scene versions for before/after composition. The final version joins
the output group once even when already listed; repeated caller entries still reject. Operation
keys, producer access and all normal graph and Host budgets remain enforced.

Actual stock UI drawing produces `WgpuCompositeColor`. Without UI, the final texture retains the
selected Scene marker. `WgpuPublishedFrame::final_texture<S>()` and
`WgpuImageOutput::texture<S>()` both require that original semantic type; neither relabels an
equal-shaped texture. Reducing a publication to its image retains the selected resource owner,
interpretation, epoch and shared allocation guard, without retaining the complete program,
reports, scratch or CPU capture. Import that exact lease with the normal `import_texture` path
for further composition or declared readback.

For normal stock collection with a custom policy, explicitly install `WgpuStockCapturePlugin`
and the selected domain contributors. Compose in `WgpuStockCaptureSet::Compose` using
`WgpuStockFrameReader::take(&installation, &writer)`. A foreign/closed route, busy connection or
unavailable slot leaves the original draft untouched. `into_parts` returns the immutable snapshot
and optional original UI sidecar when UI is compiled. Submit with `capture` or `capture_with_ui`,
retaining any returned sidecar for retry or settlement. Do not drain the UI backend a second time.

A `WgpuStockCaptureAdapter` owns one exact installation plus a conversion from complete stock
inputs to its typed capture. It lets Scene/Canvas construction select the same renderer without
downcasting its captures for picking, taking UI responsibility or querying a World at native
execution. Product selection uses an explicit compiled recipe; it is not a durable pipeline
asset, a hot route replacement or permission to load project Rust into a data-only Editor.

## Typed Versions And Dependencies

Create `WgpuRenderProgramBuilder` with an original source generation, capture/frame index and
`GraphLimits`. Reserve `WgpuProgramTexture<Semantic>` and `WgpuProgramBuffer<Semantic>` with small
code-owned markers for Scene color, final composite, raw counts and normalized frequencies.
Equal native descriptors do not make these ports interchangeable. Descriptors use WGPU directly.

Reserve versions, then declare operations in any order. Each local version has one writer.
Reads and preservation create dependencies; unique operation keys only break ready ties.
Declare every access even when a capture already contains the port. Foreign-candidate ports,
missing/duplicate writers, cycles, feedback and incompatible access reject before native work.

Writers select whole-texture clear, whole-buffer zero or an exact predecessor copy into distinct
storage. Copy initialization may change the destination semantic marker while preserving physical
shape and texture transfer/alpha. It cannot overwrite or relabel the predecessor. Shared attachment
passes load initialized contents and Store the result. Scissor does not narrow initialization.
Initial shared scope is whole single-sample, single-mip 2D baseline uncompressed color textures
and whole four-byte-aligned unmapped buffers. Unsupported scopes, formats, usages and MSAA refuse.

`finish(&[port.output(), ...])` compiles one complete group. Retrieve exact results with
`outputs.texture(&port)`, `outputs.buffer(&port)` and `outputs.report(key, &original_instance)`.
Output order does not retarget resource identity. Typed reports describe encoded work; they grant
no presentation or UI-completion authority.

## Temporal Candidates And Exact Retry

Temporal captures retain a source-bound `WgpuHistoryLineage` and their original per-View topology.
That topology owns a `WgpuHistoryView`, created once from the captured `RenderViewId`, and clones
it into accepted captures. Keep it across resize, reorder and temporary extraction absence; release
it when that View's temporal lifetime ends. Slot reclamation follows both this owner and the effect
declaration, so sharing an effect cannot retain every destroyed View. These are runtime facts, not
persistent camera fields or a global View registry.

Return the original lineage from `WgpuRenderPolicy::history_lineage`. During `bind`, use
`history_texture` or `history_buffer` with the original View owner, effect declaration, code-owned
slot and exact descriptor. The returned CPU token contains no native resource. All declarations
finish before the Host pins the complete basis, including empty/reset revisions. During `build`,
`previous_texture`/`previous_buffer` borrow those exact predecessors. A missing predecessor has an
explicit initial, reset or device-recreated reason. Import the predecessor normally; initialize
every new version explicitly. `slot.bind(&new_port)` associates that slot with its new output, and
`WgpuPolicyProgram::with_history` supplies the complete binding set. Output order is immaterial.

For retained retries, call `host.prepare(&producer, &input)` and then
`host.submit_candidate(&producer, candidate)`. Preparation owns the original program, actual
Feature owners, target contract, complete history basis and budget guards. It does not retain a
surface or an unbound native submission reservation. Failure returns that same move-only candidate;
temporary pressure can be retried without rebinding or calling policy build again. Stale/reset
bases and consumed captures reject. The one-shot `submit` convenience discards a refused candidate;
another call performs a new preparation and is not the exact-retry protocol.

`WgpuPublishedFrame` owns the complete successful group. `history_texture` and `history_buffer`
read that publication's original slots, and `final_texture` exposes its selected typed final
image for ordinary imports. Retaining an older published frame never redirects it to latest state.
Publication advances all slots and the lineage watermark together. Reset, regroup, target/source
handoff and effect replacement do not erase successful-capture deduplication. Reset with explicit
`WgpuHistoryReset` requests before preparing a descriptor-incompatible successor.

A successful publication cannot be accumulated again after later surface/display failure. UI,
READY, picking and physical completion remain separate outcomes. Retained display retries use
`submit_candidate_for_display`, which returns both candidate and original UI sidecar on refusal.
Do not settle a sidecar and later recreate its responsibility from cached input. Old binding or
consumed-candidate destructor panics after publication are recorded through the Host's
`cleanup_fault_count` and the complete group's `cleanup_panicked` observation, preserving the
successful result and native target state. The latter also observes late cleanup of that group's
original submitted resources, so it can change after native completion. Prefix clones retain their
own fault count after Host retirement. Cleanup faults do not mean that a completion callback was
abandoned; only genuine abandonment uses the separate recovery protocol.

A new Host/epoch reports `DeviceRecreated` for empty temporal slots; the effect declares its new
initialization. For example, a zero seed yields `0.25 * current` for a quarter-weight accumulator.
That is a new temporal lineage realization, not reconstruction of its unavailable old accumulated
pixels. Exact temporal replay/checkpoints remain a separate capability.

## Native Work And Budgets

`preflight` visits every original capture before local native allocations, state creation or
uploads. Its context exposes enabled limits/features without a Device, Queue or mutable state.
Reject invalid captures here. Refusal preserves untouched instance caches; preparation failure
retires only the native instances that attempt actually touched.

Stock Sprite/UI lowering registers the same immutable batch slices in a private collector.
Complete main/material Image demand and known automatic custody/engine records admit before
native work. The shared stock cost calculation also serves existing Runtime/Editor consumers
during policy migration. Arbitrary external extra retention remains fallible dynamic work.

Stock execution for ordinary Runtime windows, Scene, authoring Canvas, embedded Game and
independent image consumers uses that same native program transaction. Ordered world Views share
one Sprite writer. A fixed WorldCanvas uses declared world/physical-resolution textures and the
original nearest composite;
UI then writes a distinct final version at physical resolution. Its automatic mapping custody and
engine metadata join preflight even without Sprite/UI support. Current Canvas destination scope
is RGBA8/BGRA8 in linear or sRGB formats, preserving its declared alpha interpretation.

The Host may bind already-reserved target storage as a private local output. It still needs its
normal writer and initialization; it grants no public raw initialized import. A pure preflight
refusal preserves that reservation. Later outputs allocate after preflight and image receipts
share the actual program storage guard. The same submission operation accepts each original
`WgpuRenderInput`; Sprite/UI combinations do not select separate image facade methods.

For an ordinary Runtime window, the acquired surface is the Host's final consumer of the exact
physical-resolution program image. It joins the same encoder and native submission. Output
admission, actual surface present, original UI completion and physical GPU completion remain
separate observations. World-picking permission belongs to its producer and target lifetime.
Presenting in another window cannot replace it; a successful successor in the same physical window
revokes the displaced frame. Producer Drop revokes permission before returning. The managed Runtime owns capture identity,
while the Host clock only ages its private caches.

Both `create_state` and `prepare` receive `WgpuFeaturePrepareContext`. Use `device()` for pipelines,
`resources()` for declared versions and scoped uploads for private inputs. Reserve native payload
bytes with `reserve_bytes` before allocation. Keep the cloneable `WgpuFeatureAllocation` with the
actual allocation in state, prepared work or reports; clones share one charge. The submission
retains that guard and upload destination before native writes begin.

Uploads and additional `retain` calls return a `Result`; propagate admission failure before
continuing. Original submission custody is flat and bounded, and releases in reverse dependency
order. Automatic graph entries are checked before program allocation. Adapter-requested extra
entries still obey the Host's hard limit, which refuses rather than suggesting a deferred retry.

Queue uploads precede the encoded command buffer. Upload a private input, then copy/use it during
encoding; uploading a shared destination would precede its declared initialization. The consumer's
Upload Feature demonstrates this with a private staging texture.

`encode` receives declared resources and an encoder, with no Queue/acquire/submit/present entry.
`context.split()` allows simultaneous resource and mutable encoder borrowing. Resource lookups
check candidate, declaration and semantic identity. Exact native Rust is trusted: handle clones
do not authorize undeclared writes, independent submissions or destruction of immutable imports.

`required_metadata_bytes(limits)` covers graph/scratch, native execution tables, resource records
and the bounded state index. Each Feature operation additionally admits exact inline capture/state/
prepared/report wrappers and automatic retention records before capture-wrapper allocation.
User-owned indirect heaps, adapter-requested extra retention, allocator rounding and native
pipeline/driver overhead are outside this modeled envelope. It is not RSS or measured residency.
The builder clamps the effective metadata allowance to a hard 64 MiB before allocation. Pending
policy candidates reserve that full envelope plus a bounded 2 MiB of capture/binding/history storage;
the Host shares a separate pending-candidate metadata/count budget. Caller-owned indirect heaps
and independent programs created outside the candidate are not part of this modeled allowance.

All known local resource guards are acquired before creating any native local object; refusal
releases the guards immediately. Graph allocations, retained outputs and adapter-private payloads
share the Host logical-byte budget. Every guard follows the original allocation's last actual use.

`with_resource_byte_limit` restricts that shared logical payload budget; `retained_resource_bytes`
observes its current responsibility. Stock Image, glyph, fallback, instance and material allocations
use the same private preparation owner as program resources. Cache hits share the original guard;
returned native bindings/views retain their use through cache removal and physical completion.
The former per-Image completion callbacks are removed. Shared cache age uses the Host maintenance
frame, independently of producer capture/frame identities.

Toolkit-owned resources retain their separate accounting. The mutable WorldCanvas cache and old
quad/window executor are private test oracles and are absent from production compilation. This
budget covers the represented native payload owners; it is not a promise that every device
allocation is admitted by one complete GPU-memory budget.

## Admission, Outputs And Close

`submit_program` validates producer/instance, prepares and encodes under original custody, inspects
native errors, then revalidates epoch/admission. Failure publishes none of the group and leaves the
successful capture watermark unchanged. Even failed preparation may leave uploads; the Host retains
their original prefix. Budget pressure can defer the same immutable capture. A descriptor beyond
enabled device hard limits is a content refusal; unrelated output release cannot make it fit.

Import only exact initialized Host outputs with `import_texture`/`import_buffer`. Raw handles and
caller initialization flags are not imports. Execution rechecks original Host, epoch, shape and
interpretation. Old outputs remain readable after their producer closes while that epoch is valid.

`submitted_prefix` tracks physical custody independently of publication and later producer work.
After settlement, outputs retain only selected versions/reports; scratch and original CPU inputs
can release. Retain reconstruction input separately. An output cannot recreate retired CPU facts
or transfer an old native instance/import to another Host.

`retire_producer` returns its frozen prefix while other producers may continue. Final
`poll_retirement` stops the Host and drives the original queue drain. Publication and telemetry do
not establish physical completion. The [image-output guide](render-image-outputs.md) covers the
separate display-occurrence and interaction authorities.

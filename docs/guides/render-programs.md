# Captured WGPU Render Programs

Use a finite program for native Rust texture/buffer dependencies beyond stock drawing, such as
two-pass blur, preserved Scene color, compute statistics and a histogram. `WgpuRenderHost` owns
device acquisition, submission and retirement. The
[independent consumer](../../tests/fixtures/render-program-consumer/README.md) implements the
whole workload through public APIs without a World, Runtime, window or private executor.

This guide covers the bounded prerequisite admitted by
[ADR0094](../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs).
Typed policy capture installation, alternate policy, history and complete custom-drawn Editor
composition remain work in the [render unit](../plans/2026-10-31-render-composition-and-host-execution-plan.md).

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
share the actual program storage guard. Typed policy capture and general policy installation
remain the following migration work; the existing stock facade names are not a new policy API.

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

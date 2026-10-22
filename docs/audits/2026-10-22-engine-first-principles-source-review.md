# Engine Source Review From First Principles

Date: 2026-10-22, Asia/Shanghai.
Reviewed baseline: `32b797c3e502c60804b87cd361a6e5da551d137c`.
Scope: Rust-first desktop 2D production, runtime ownership, native-domain composition,
content, authoring, rendering, and public extension ergonomics.

## Assessment

Nara has a coherent ownership foundation, but ordinary game production and independent extension
composition remain substantially less complete than its failure and publication protocols.
The largest architectural concern is the knowledge required to combine individually correct
modules. A game or extension author still has to coordinate some engine-owned lifecycle protocols,
and several conservative refusals also intercept normal production operations.

The review found concrete implementation problems alongside these design limits. These categories
must remain separate: an intentional limitation can be a high product priority without being a
contract violation. A source-confirmed cost is not an observed frame-time regression.

Keep `bevy_ecs`, managed execution, explicit persistent identity, document ownership outside the
World, and immutable resource-complete render packets. Focus refactoring on the owning boundaries
described below. A replacement ECS would not resolve these findings.

## Method And Limits

The primary reviewer traced public consumers and module interfaces. Three independent Codex
reviewers investigated runtime/ECS/tasks, content/Scene/persistence, and rendering/UI/input.
Candidate findings were challenged against Accepted ADRs, existing tests, and counterexamples.
The review included current implementations, the starter, independent module consumers, and the
collect and two-level games; it is not an exhaustive proof of every line or platform.

The working tree initially had no tracked changes. The unrelated untracked `vendor/` directory
was not inspected or changed. Temporary regression probes were used to test selected findings;
their results are recorded below. No production repair or new architecture decision is adopted
by this report.

The mature-engine comparison uses fixed local sources: Bevy
`85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. These are comparison baselines, not claims about the
latest upstream release. Existing audits were background evidence, not proof that a current path
was correct.

## Implementation Findings

### R1. Nonmodal UI Misses Consume Gameplay Clicks

Priority: P1. Classification: confirmed correctness defect. Confidence: high; executed probe.

A 200 x 100 nonmodal UI surface with a 100 x 50 button consumes a left press at `(150, 75)`.
The frame reports `blocks_unhandled_input() == false`, but an ordinary `Left -> fire` gameplay
binding receives no `Started` outcome.

The complete path is [pointer.rs:30](../../crates/nara_ui_kernel/src/interaction/pointer.rs#L30)
returning `NoPointerTarget`, then
[transaction.rs:230](../../crates/nara_ui/src/interaction/transaction.rs#L230) treating every
kernel `Rejected` outcome as a route error. The error arm at
[transaction.rs:36](../../crates/nara_ui/src/interaction/transaction.rs#L36) finalizes the event
as `RejectedClosed`.
[timeline.rs:364](../../crates/nara_input/src/timeline.rs#L364) treats that disposition as UI
consumption, withholding it from gameplay.

[UiFrame::blocks_unhandled_input](../../crates/nara_ui_kernel/src/frame.rs#L654) explicitly
identifies an active modal root as the reason to stop unhandled input. Existing letterbox tests
exercise points outside the content rectangle, not an ordinary miss inside it. The unfocused
Enter test checks absence of a UI action without checking delivery to gameplay.

This affects normal HUD composition, where a UI surface covers the game window but its controls
occupy only a small area. Related right-button and unfocused-key cases follow similar source
paths but were not independently executed in this review.

Correct direction: distinguish ordinary nonmodal misses from stale presentation, disabled
controls, modal blocking, and authority failures at the adapter boundary. Preserve captured
press/release ownership. Converting every rejection into pass-through would introduce a different
input bug.

### R2. Per-Instance Tint Splits Identical GPU State

Priority: P2. Classification: unnecessary batching and allocation work. Confidence: high;
executed CPU probe.

[SpriteMaterialKey](../../crates/nara_sprite_render/src/types.rs#L202) includes tint, and
[build_sprite_batches](../../crates/nara_sprite_render/src/queue.rs#L147) requires that entire
key to match. Adjacent red and blue sprites therefore form different batches even when texture,
shader, sampler, alpha mode, view, layer, and sort group are identical.

Tint already travels in `SpriteInstance.color`. The actual
[WGPU adapter](../../crates/nara_render_wgpu/src/sprite.rs#L22) uses only texture, sampler, and
alpha mode for GPU material state, while its
[instance conversion](../../crates/nara_render_wgpu/src/sprite.rs#L44) carries each color
independently. [quad.rs:335](../../crates/nara_render_wgpu/src/quad.rs#L335) prepares a buffer
per batch and [quad.rs:427](../../crates/nara_render_wgpu/src/quad.rs#L427) draws each batch.

For N adjacent sprites whose only incompatible key field is tint, the source produces N batches,
buffers, and draws where one ordered instance batch is sufficient. This is relevant to ordinary
colored sprites, flashing enemies, fades, and particles. It is a work-count finding, not a measured
FPS claim. `from_color` selects Blend independently of color, so alpha-mode changes do not justify
this split.

Correct direction: keep per-instance attributes out of the GPU compatibility key. Preserve the
existing painter order and merge only adjacent compatible items. Do not sort transparent objects
by color or texture to hide the problem.

### R3. A Setup Failure Is Misreported As A Shutdown Failure

Priority: P2. Classification: confirmed diagnostic correctness defect. Confidence: high;
executed probe.

[shutdown_plugins](../../crates/nara_app/src/lib.rs#L1790) returns a shutdown error when the
failure report contains any primary setup error, even if its shutdown-failure list is empty.
[Runtime retirement](../../crates/nara_app/src/runtime.rs#L3092) records that result as
`RuntimeCloseCause::PluginShutdown`.

A plugin whose build fails and whose default shutdown succeeds can therefore retire completely
while `close_evidence().plugin_shutdown_failed()` returns true. The Host then uses this evidence
in [completion_failure_report](../../src/project_host/runtime.rs#L365) and emits
[project.run.cleanup-plugin-shutdown](../../src/project_host/runtime.rs#L1728).

The user is told that cleanup failed after the original setup error, although there was no
shutdown failure. This undermines repair feedback and the trustworthiness of close evidence.
It is not evidence that resources were released too early or leaked.

Correct direction: retain primary setup failure, actual shutdown failure, and physical retirement
as independent facts. Add a setup-failure/successful-shutdown regression alongside genuine
shutdown-failure cases.

### R4. An Impossible Texture Working Set Is Reported As Temporary Deferral

Priority: P2. Classification: source-confirmed progress/status defect. Confidence: medium-high;
no GPU-specific execution in this review.

The [texture cache](../../crates/nara_render_wgpu/src/texture.rs#L25) admits at most 1,024 resident
Image versions and 256 MiB. Every residency refusal becomes
[ImageResidencyDeferred](../../crates/nara_render_wgpu/src/texture.rs#L344), and the
[error conversion](../../crates/nara_render_wgpu/src/texture.rs#L762) classifies it as retryable.
The [runtime handler](../../crates/nara_render_wgpu/src/lib.rs#L633) records a Deferred skip and
leaves the backend ready.

Consider a stable visible sequence of 1,025 distinct logical 1 x 1 images. Preparing a whole target
must succeed before submission
([backend.rs:1082](../../crates/nara_render_wgpu/src/backend.rs#L1082),
[backend.rs:1629](../../crates/nara_render_wgpu/src/backend.rs#L1629)). The first 1,024 cache hits
refresh their use on every attempted frame
([texture.rs:267](../../crates/nara_render_wgpu/src/texture.rs#L267)); they never become idle
enough to evict. The last image remains deferred indefinitely, so no frame is presented.

The project file/CPU budgets admit this case, and the instance-byte preflight does not reject
1,025 quads. Different logical AssetIds remain distinct cache identities even with identical
pixels. Existing held-texture tests demonstrate recovery after old owners release; they do not
cover a current working set that can never fit.

There is a live diagnostic containing residency limits. The defect is classifying an impossible
current demand as a condition that waiting can resolve. A startup can remain without first
presentation; an already running product can keep its old picture.

Correct direction: preflight the distinct required image demand for the target/frame. Reject a
working set that exceeds the hard limit with an explicit content result; retain Deferred for
temporary occupancy by previous in-flight owners. Select the Host response explicitly. A general
resource scheduler is not required.

### R5. Useful Format Error Categories Disappear At Production Entrypoints

Priority: P2. Classification: developer-feedback defect. Confidence: high from source;
not fault-injected in this review.

[SceneFormatError](../../crates/nara_scene/src/format.rs#L285) distinguishes byte limits,
header, payload, persistent contract, embedded version, and decoded budgets. The
[Scene mapping](../../src/project_content.rs#L2332) and
[Prefab mapping](../../src/project_content.rs#L2353) preserve only the special legacy-placement
case; everything else becomes a generic format-invalid error with a path. The
[Editor admission mapping](../../src/project_host/authoring/document.rs#L375) similarly flattens
format errors, and its `Error` implementation provides no source chain.

Malformed JSON, an unsupported format version, and a document beyond its budget demand different
repairs. The ordinary Check/Play/Open path hides this distinction despite the decoder retaining it.

Correct direction: preserve safe structured cause categories, observed/expected versions,
budget kinds and maxima, and parser location where available. Do not publish arbitrary parser
messages or document content. The diagnostic privacy boundary should classify information rather
than erase every useful distinction.

## Architecture And Workflow Findings

### D1. Cleanup Error Outcome And Physical Responsibility Are Conflated

Design priority: high. Classification: existing conservative policy requiring an owning-contract
change, not a newly discovered memory-corruption defect.

The public [terminal close error](../../crates/nara_app/src/runtime.rs#L641) permanently disables
further polling of that participant
([runtime.rs:1002](../../crates/nara_app/src/runtime.rs#L1002)), while the
[completion condition](../../crates/nara_app/src/runtime.rs#L1052) accepts only `complete`.
A terminal participant consequently cannot finish through RetryClose.

An unfinished dropped owner enters thread-local quarantine
([runtime.rs:3267](../../crates/nara_app/src/runtime.rs#L3267)). Nonempty quarantine at thread
exit triggers [process::abort](../../crates/nara_app/src/runtime.rs#L3355). Dropping even an
otherwise simple App during panic unwinding also takes the quarantine route. A caller that
catches a panic needs to know the owner-thread drain protocol before its thread exits.

Retaining an active worker, native lease, or thread-affine object is correct. The design problem
is that a close operation cannot directly report both physical completion and terminal error
evidence. A failure outcome and unfinished physical responsibility are different dimensions.
The final process policy is also implemented at TLS destruction, far from the operation that
could explain the responsible participant and phase.

Correct direction: model physical completion separately from error evidence; quarantine only
actual retained responsibilities; make Host exit policy and diagnostics explicit. Truly stuck
native work may still require process termination. This does not promise recovery from arbitrary
plugin panics or permit unsafe destruction of live resources.

### D2. Hook-Owning Native Domains Need Manual Scene-Travel Coordination

Design priority: high. Classification: known native-domain integration gap.

The [hosted commit](../../src/project_host/runtime/scene_requests/api.rs#L307) ultimately reaches
[Scene retirement](../../crates/nara_scene/src/spawn.rs#L795),
[hierarchy admission](../../crates/nara_hierarchy/src/writer.rs#L226), and
[ECS lifecycle validation](../../crates/nara_ecs/src/__private.rs#L171). Retirement checks every
component on the retiring entity, including runtime-only components, for relevant hooks and
observers. A normal cleanup-on-remove physics/audio component therefore makes direct Scene
replacement reject even when all persistent configuration is pure.

This is intentional and covered by
[runtime_component_despawn_observer_rejects_scene_replacement_atomically](../../crates/nara_scene/src/tests.rs#L2716).
The [external reconciliation fixture](../../tests/fixtures/module-consumer/README.md#L51)
works by explicitly
[retiring native state first](../../tests/fixtures/module-consumer/tests/support/domain_reconciliation.rs#L396),
including removal of its owned runtime components. It proves one manually coordinated domain,
not automatic composition of multiple independent domain plugins into ProjectScenes travel.
The product overlay callback is not a before-retirement lifecycle participant.

An adjacent limit is the `Send` bound on
[RuntimeCloseParticipant](../../crates/nara_app/src/runtime.rs#L896), despite live World ownership
being thread-affine. Its context lacks mutable NonSend access. Immediate plugin shutdown can
handle some such services, but pollable thread-affine cleanup is not established.

Correct direction: use a real native domain to define how accepted data, native preparation,
retirement, publication, reconciliation, and degraded retry compose. Then challenge it with a
second independent domain. Keep persistent transaction guarantees; do not delete lifecycle guards
or claim arbitrary native side effects can be rolled back.

### D3. The Generic Inspector Cannot Edit Object References In A Complete Local Scene

Design priority: high for an integrated editor. Classification: explicitly deferred capability.

[authoring_field_change_is_preservation_safe](../../crates/nara_scene/src/authoring_decode.rs#L2509)
unconditionally excludes `EntityRef` fields. The
[frontend draft](../../editor/src/frontend/imgui/authoring/value_draft.rs#L181) also rejects that
shape. Even a complete source-local Scene cannot connect `Switch.target -> Door` through the
ordinary Inspector/field-edit action. A reference-bearing record or container has related limits.

There is a native counterexample: the trusted compiled Editor path dispatches to
[`workspace.apply_command`](../../src/project_host/runtime/editor.rs#L2417), and native
[`ApplyScenePatch` field assignment](../../crates/nara_scene/src/patch.rs#L2034) checks Edit
capability and value kind without imposing this EntityRef exclusion. This finding concerns the
generic/data-only Inspector and field-edit lane, not every Rust or compiled-panel editing path.

ADR 0090 records this boundary, so it is not an implementation violation. However, object wiring
is central to everyday game production, not an advanced editor luxury. Current structural
authoring and reliable Save/Reopen do not close that workflow.

Correct direction: first prove one complete local target-assignment workflow with reference
validation, Undo/Redo, Save/Reopen, and strict Check/Play. Define source/instance/projection
semantics before extending it across Prefabs. A generic graph editor is unnecessary for this proof.

### D4. Panel And Import Extensions Do Not Provide Extensible Document Ownership

Design priority: next. Classification: strategic extension boundary, not a broken current API.

The [panel context](../../editor/src/frontend/imgui/workspace.rs#L65) submits only the closed
[EditorAction](../../crates/nara_tooling/src/frame.rs#L586) set.
[EditorDocumentKind](../../crates/nara_tooling/src/workspace.rs#L48), the private
[document slots](../../crates/nara_tooling/src/workspace.rs#L248), and
[panel source kinds](../../editor/src/workspace/panels.rs#L98) support Scene and Prefab.
Revision/history operations also embed Scene-specific types.

A third-party dialogue or animation editor can draw a panel and import a CPU product. It cannot
register an independently owned document that participates in the Host's dirty state, undo,
Save/Reopen, replacement, and close prompts. Encoding it as a Scene is possible but imposes entity
document semantics. A captured controller doing its own I/O bypasses the integrated lifecycle.
Panel drafts are deliberately retired on hide/close and cannot substitute for durable documents.

Correct direction: take one real third-party document through the complete lifecycle, then expose
the minimum document-owner contribution required. Preserve exact revision and asynchronous
completion semantics. An unrestricted `Any` command bus or toolkit-neutral widget framework would
not establish that ownership.

### D5. Ordinary Product Entrypoints Still Expose Too Much Host Protocol

Design priority: next. Classification: public-interface depth and maintenance debt.

[DesktopRun](../../src/project_host/runtime/action.rs#L386) already hides most App/schema/Scene
startup, and [root resolution](../../src/project_host/product_launch/root.rs#L23) centralizes
location rules. The remaining protocol is nevertheless copied into the
[starter entrypoint](../../starters/desktop-2d/v1/src/bin/desktop.rs#L22),
[collect entrypoint](../../samples/collect-game/src/bin/desktop.rs#L22), and
[two-level entrypoint](../../samples/two-level-game/src/bin/desktop.rs#L32): inherited bootstrap,
source verification, Cargo identity, profile selection, and incomplete-close driving.

The [starter library](../../starters/desktop-2d/v1/src/lib.rs#L31) also exposes native directory
handle flags and trust selection. The games copy that logic; cleanup deadlines have already
diverged from five to ten seconds. Generated code reduces typing but does not give these policies
one owner when the engine protocol changes.

Correct direction: deepen the root product Host entrypoint around an explicit development root,
product identity, recipe, and game-owned options. Keep manual DesktopRun driving for advanced
Hosts. Keep environment/path authorization out of low-level `nara_fs`; its handle-based capability
boundary is useful.

## Source-Confirmed Costs That Need Workload Evidence

### M1. Ordinary Task Admission Rebuilds The Entire Pending Queue

[TaskPoolExecutor::admit](../../crates/nara_tasks/src/runtime.rs#L2037) allocates and reconstructs
the whole pending queue while holding its mutex for every admission, including the ordinary
Reject policy. With one occupied worker and N queued submissions, that contributes quadratic
total scanning/movement and serializes worker access with submission. This is independent of
coalescing.

Default pending capacities are 512/512/256, not the maximum allowed 1,048,576 per kind. No current
frame-P99 failure was measured. Make ordinary append inexpensive by moving cancelled-node
reclamation to a suitable incremental or capacity-pressure path; preserve cancellation and
physical-custody tests. No replacement executor is needed to address this mechanism.

### M2. Identity Catalogue Work Is Repeated Across Scene Roots

[load_inner](../../src/project_content.rs#L899) creates a new LoadContext. A stable-ID reference
causes [ensure_reference_catalogue](../../src/project_content/catalogue.rs#L16) to scan project
metadata. [Delivery](../../src/project_content/delivery.rs#L188) independently loads each declared
root, repeating that catalogue work; later Scene preparations also create a fresh context.

The source establishes repeated root-count x project-directory work for roots that use stable-ID
references, not a measured loading latency. A root using only paths or no such references need
not trigger the full catalogue scan. The current complete scan is justified by the need to prove
identity uniqueness:
unparseable metadata may conceal a target or conflict. Silently skipping bad metadata would
weaken that proof.

Measure representative cold/warm Check, delivery, and travel first. If material, move a complete,
generation-bound immutable identity index to its proper owner and reuse that snapshot. Preserve
conflict diagnostics and exact source revalidation; a cache must not replace authorization.

## What To Retain And What To Generalize

- `nara_ecs` honestly exposes Bevy ECS. Bevy's typed systems and resources remain a useful substrate;
  changing storage would not fix lifecycle composition, editor ownership, or batching.
- Managed execution correctly centralizes publication, sticky faults, and physical retirement.
  Keep that owner while revising the error/exit policy in D1.
- Persistent documents and unavailable-record preservation stay outside the live World. This
  protects authoring data and permits recovery without executing arbitrary game Rust.
- Compiled source-domain selection, typed settings, and exact source-carrying export receipts are
  real extension and delivery mechanisms. They should remain narrow until a richer consumer needs
  more.
- Resource-complete snapshots and presentation-bound pointer mapping correctly model what was
  actually shown. Keep the shared Sprite/Tile painter order and adjacent-only batching.

The fixed [Bevy Plugin](../../repo-ref/bevy/crates/bevy_app/src/plugin.rs#L57) comparison is useful
for ordinary typed registration with little caller choreography. Its `cleanup` hook is construction
cleanup, not Nara's runtime shutdown protocol; copying the name would not solve D1/D2.
The fixed [Godot EditorPlugin](../../repo-ref/godot/editor/plugins/editor_plugin.h#L117)
demonstrates integration across undo, import, inspector, and custom tools. The relevant lesson is
completing the extension author's workflow, rather than adopting its object model wholesale.

Two secondary interface questions remain below the main findings. Configured plugin factories
can capture behavior outside their canonical configuration; exact duplicate resolution cannot
compare those captures. This is a documented caller contract, not a demonstrated strict-protocol
bypass. Ordinary gameplay examples also repeatedly encode/decode Rust movement values through
string-keyed generic command payloads. A typed facade over canonical ingress could reduce repeated
validation, while retaining ordering/replay metadata. Neither question justifies a universal
extension kernel or another registry without a complete consumer.

## Recommended Sequence

1. Correct R1-R3 and qualify their actual consumers. Add finite working-set refusal coverage for
   R4 and preserve actionable structured format causes from R5.
2. Resolve D1's physical-close/result contract before spreading its current quarantine pattern
   into more plugins. Then prove D2 with a real native domain and independent composition.
3. Complete D3's object-wiring workflow. Deepen the ordinary product entrypoint from D5.
4. Select one independent document editor for D4. Let that consumer determine the extension
   contract rather than adding multiple speculative registration surfaces.
5. Measure M1/M2 and changing game/editor workloads. Attribute the cost before introducing broad
   caches, custom scheduling, parallel ECS policy, or storage changes.

These recommendations prioritize work; they do not turn every deferred feature into an immediate
implementation commitment. Stronger extension capability means independent modules can complete
their lifecycle with supported public APIs, not merely that a new trait can be implemented.

## Executed Verification

All Cargo commands in this review used the existing target directory and `CARGO_BUILD_JOBS=1`,
and were started sequentially. The probes assert expected correct behavior; failing here is
evidence of the reviewed defects, not a claim that the production suite passes.

| Probe | Command and result |
|---|---|
| Nonmodal empty-space click | `cargo nextest run -p nara_ui --lib --locked -E 'test(review_probe_nonmodal_empty_space_reaches_gameplay)' --status-level fail --final-status-level fail`: one test failed; observed `[]`, expected `[Started]`; 85 filtered tests. Run `d9e8dc70-0b13-4077-a342-4fdd8499f0c9`. |
| Setup failure with successful shutdown | `cargo nextest run -p nara_app --test review_cleanup_diagnostic_probe --locked --status-level fail --final-status-level fail`: one test failed because successful shutdown was reported as failed; physical retirement was Retired. Run `36ae27b6-8636-401f-867a-77f21dd52638`. |
| Per-instance tint batching | `cargo nextest run -p nara_sprite_render --lib --locked -E 'test(review_probe_instance_tint_does_not_split_batches)' --status-level fail --final-status-level fail`: one test failed; observed two batches, expected one; original source order assertions passed; 32 filtered tests. Run `28092cae-8186-4dbf-b0aa-7a5d242a5f54`. |

Temporary probes were removed after recording these results; the delivered change is this report
only. The UI probe closed its runtime before asserting the captured result. No abort was needed to
reproduce R1/R3. R4 was not exercised on a GPU; M1/M2 have no new timing measurements. Existing
tests cited elsewhere were inspected, not rerun as a complete acceptance suite. No new Linux,
macOS, native-input, scanout, release, or whole-engine performance qualification is claimed.

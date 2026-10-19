# Expanded Foundation Audit

Status: twelve research lanes and independent challenge complete; recommendations not implemented.

The [foundation correction plan](../plans/2026-10-19-1335-refactor-foundation-correction-plan.md)
maps these B findings, the earlier A findings and module seams to implementation units or explicit
entry triggers. It does not change the evidence or completion claims of this audit.

## Scope And Evidence

This extends the [Bevy/Godot architecture comparison](2026-10-19-nara-bevy-godot-architecture-review.md)
and its A01-A09 findings. The [research lane map](../plans/2026-10-19-foundation-audit-expansion.md)
defines twelve separate subjects. Child researchers are read-only; the root adjudicates findings
and owns the report. At most three child agents run concurrently because of the session limit.

Source baseline: Nara `d88a96f`, whose runtime code remains `835c5e4`; Bevy
`85503941e488152711a4695bbe248e9ed9f453fc`; Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. Reference-engine paths below include `repo-ref/`.
Existing untracked files were preserved. No runtime behavior or Accepted ADR is changed by this
review. Proposed acceptance cases are future verification, not tests run in this audit.

## Verdict And Coverage

Retain the current ECS, component/plugin split, frozen definitions and managed ownership. The new
results are fourteen distinct items: three P1 safety/recovery recommendations and eleven P2
correctness, extension or triggered-capability items. This is not fourteen newly reproduced bugs.
P2 performance claims below identify source-level work or missing capability without asserting a
measured frame regression. Earlier A01-A09 remain open recommendations in the linked comparison.

| Lane | Subject | New result |
| --- | --- | --- |
| J01 | Background tasks, cancellation, overload and retirement | B05-B06; frame-job distinction feeds B07 |
| J02 | ECS components and persistent/native transactions | No new independent defect; preserve lifecycle guards |
| J03 | Reflection, schemas, defaults, migrations and generic consumption | B02-B03 |
| J04 | Plugin plans, groups, recipes and configuration | B04 |
| J05 | Managed clocks, pause/step, faults and close | B07 capability boundary; no new time/close defect |
| J06 | Editor documents, undo, persistence and close | B01 |
| J07 | Input timeline, semantic actions and command integration | B08; platform vocabulary trigger below |
| J08 | Runtime UI, text, fonts and presented semantics | B10-B11 |
| J09 | Filesystem capabilities, source admission and Export | B09 |
| J10 | Asset storage, Image reload and watcher recovery | B12 |
| J11 | Rendering, views, device admission and backend resources | B13-B14 |
| J12 | Dependency graph, features and independent product consumers | No new independent defect |

The root also inspected identity/hierarchy/transform and diagnostics joins. An independent
challenge rechecked high-impact counterexamples and found no material withdrawal necessary.

## New Findings

### B01 — P1: Dirty-Close Decisions Can Be Replayed Against The Next Document

Classification: current action-interface defect; high confidence from complete static control flow.

`EditorWorkspaceIntentAction::Resolve` contains only a decision, without the document or prompt
identity (`crates/nara_tooling/src/frame.rs:475`). Workspace-intent actions do not require a current
frame (`frame.rs:544`). A batch dispatches every action in sequence
(`src/project_host/runtime/editor/actions.rs:61`, `669`). The decision applies to the current
`pending.decision_document()` (`src/project_host/runtime/editor.rs:2362`). For Exit, Discard
immediately advances the cursor and prepares the next dirty document (`2381`, `2818`).

**Counterexample:** A and B are dirty. The frame shows A's exit prompt. A batch contains two copies
of A's Discard action. The first accepts A and makes B current; the second accepts B without any
observation of B's prompt. The product wrapper retains all Resolve actions
(`editor/src/app.rs:1687`, `1718`), so it does not remove this ambiguity. Delayed replay across
frames has the same missing target association. This can authorize discarding B's edits when the
subsequent exit/project replacement completes.

Pending intents already block document mutation (`editor.rs:2276`). This finding does not claim
that revisions can change while the prompt is open. The defect is the changing decision target,
not an absence of the document mutation lock.

**Refactor:** the close owner issues an opaque prompt token binding intent instance, document and
prompt generation. Resolve carries that token; accepting a decision consumes it. Cancellation,
a new intent, advancing to another document and returning from a failed Save rotate the token.
Keep the current semantic action path; another command bus is unnecessary.

**Acceptance:** duplicate A Discard accepts only once and leaves B pending; old actions reject after
Cancel/new Exit and Save failure; ordinary sequential Save/Discard/Cancel and replacement continue
to work. Rejecting a stale action changes neither document nor disk nor current prompt.

**Precedent:** Godot explicitly associates edit history with object/scene context
(`repo-ref/godot/editor/editor_undo_redo_manager.cpp:107`, `385`) and keeps close-tab context
(`editor/editor_node.cpp:6934`, `4327`). This supports retaining a target; it does not prove that
Godot GUI callbacks provide Nara's semantic-batch replay protection.

### B02 — P2: A Derived Vec2 Makes An External Component Unavailable To Generic Editing

Classification: current authoring limitation with a growing schema migration cost; high confidence.

`Vec2::value_shape()` returns `Opaque(Map)` (`crates/nara_reflect/src/authoring.rs:184`).
Generic authoring requires every field shape to be supported
(`crates/nara_scene/src/authoring_decode.rs:783`) and rejects any Opaque node (`800`). Without a
component-specific adapter, the entire record becomes KnownUnbound (`1387`).

An external derived `SpawnConfig { position: Vec2, count: u8 }` can use native encode/decode but
cannot edit even its scalar sibling through the generic Editor. Built-in Transform2d is not proof
of this external workflow: its schema splits axes into five scalar fields
(`crates/nara_transform/src/lib.rs:277`). Opaque rejection is a correct protection because the
catalog lacks member semantics; widening generic admission would be the wrong fix.

**Refactor before many components adopt it:** make supported basic math types declare closed
member shapes and numeric domains under one stable root field. Reconcile the existing Accepted
contract that deliberately keeps Vec2 opaque. Changing this shared helper changes consuming
component schemas: each owner must carry its version/predecessor/migration obligation. A global
helper edit cannot silently migrate all game schemas.

**Acceptance:** an independent component with Vec2, a scalar and `Vec<Vec2>` remains data-editable
when project Rust is unavailable, supports Undo/Save/Reopen, rejects malformed members and passes
strict Check after explicit predecessor migration.

**Precedent:** Bevy exposes Vec2's x/y reflection
(`repo-ref/bevy/crates/bevy_reflect/src/impls/glam.rs:250`); Godot has a Vector2 property editor
(`repo-ref/godot/editor/inspector/editor_properties.cpp:4184`). This is separate from A09's
third-party field-codec interface.

### B03 — P2: Inspector Widgets Do Not Consume Recursive Shape Or Explicit Null

Classification: existing consumer capability gap; high confidence.

The tooling view carries complete `shape` (`crates/nara_tooling/src/inspector.rs:334`), but the
native generic Inspector branches only on outer wire kind
(`editor/src/frontend/imgui/authoring.rs:571`). Map and List show read-only labels (`602`).
The document owner already admits complete field writes for closed recursive shapes without
EntityRef (`crates/nara_scene/src/authoring_decode.rs:2509`).

`Option<u8>` and `Option<bool>` present the inner numeric/bool kind. Existing widgets can produce
Some, but provide no Null/Some control to restore None. A derived Option root is a required field;
deleting the field is not equivalent to explicit Null. Lists, maps and closed records similarly
have admitted transaction semantics that the frontend does not expose. EntityRef mutation remains
deliberately rejected by the owner and is excluded from this finding.

**Refactor:** dispatch controls by validated recursive shape. Start with Optional Null/Some,
closed records and bounded scalar containers, reusing current root-field revision/undo commands.
Keep widget drafts local and data validation in the owner; do not add project-code callbacks.

**Acceptance:** native None -> Some -> None, record/list edits, Undo/Redo and Save/Reopen;
invalid leaves and stale revisions reject without changing unavailable records.

**Precedent:** Godot constructs Dictionary/Array editors from property metadata
(`repo-ref/godot/editor/inspector/editor_properties.cpp:4359`, `4369`). Nara should preserve its
stronger document transaction ownership while completing that consumer path.

### B04 — P2: Product Recipes Cannot Configure Profile-Installed Defaults

Classification: ordinary public product composition gap; high confidence, with an existing consumer.

Runtime UI and its renderer already accept an immutable custom FontFace
(`crates/nara_ui/src/lib.rs:54`, `62`;
`crates/nara_ui_render/src/integration.rs:959`, `969`). Direct App composition configures both
groups as documented in `docs/guides/rust-product-recipes.md:183`.

File-backed Headless/Desktop/Editor product entry points instead accept ProductRecipe
(`src/project_host/runtime/action.rs:337`, `1072`). The Host appends the recipe alongside its
profile defaults (`src/project_host/composition.rs:253`), which already install UI and presentation
groups (`src/product/profile.rs:128`, `158`). Recipe configure replaces only entries inside that
recipe (`src/product.rs:345`); defaults are absent there. Appending the same plugin collides with
the required profile slot (`crates/nara_app/src/plugin/resolve.rs:900`).

Advanced `ProjectRuntimePlugins::configure` exists (`src/project_host/composition.rs:292`), so the
engine can configure these modules. The ordinary product interface cannot carry that configuration
through its supported Host path. Repeating this gap for future default modules would spread custom
Host construction and private initialization knowledge into games.

**Refactor:** use the real custom-font workflow to add typed, checked configuration of known
profile defaults to ordinary composition. Keep one plan resolver for identity/dependency/schema
checks and fingerprints. Do not use silent last-writer override, arbitrary Host-policy callbacks
or a provider registry.

**Acceptance:** one product configuration selects the same FontFace for UI compilation and
presentation through Headless, Desktop, Check, Editor Play and Export; missing paired
configuration reports a clear error and no advanced Host interface is required.

**Precedent:** Bevy configures existing default plugins with `set/try_set`
(`repo-ref/bevy/crates/bevy_app/src/plugin_group.rs:362`, `374`); Godot routes project fonts into
normal theme startup (`repo-ref/godot/scene/theme/theme_db.cpp:58`, `79`).

### B05 — P2: Task Rejection And Coalescing Run Capture Destructors On The Submitter

Classification: public extension latency/retirement risk; no demonstrated built-in product stall.

Task admission drops rejected, coalesced and already-cancelled jobs synchronously after releasing
the queue lock (`crates/nara_tasks/src/runtime.rs:1975`, `2049`, `2052`).
`safe_drop_pending` at `2610` catches panic, but runs user-owned capture destructors on the caller.
A blocking or expensive Drop can therefore stop an Editor/frame-thread submitter even though the
task body never ran. TaskId exhaustion also drops the new closure at `1407`.

The contract promises no waiting for queue capacity, not universal nonblocking destruction.
Current Image captures mainly file capabilities and budget leases; this audit does not establish
an existing Image hang. The destructor runs outside the queue Mutex, so no queue-lock deadlock is
claimed. Unclaimed result destruction is ordinary generic Rust ownership and needs separately
documented responsibility.

**Refactor before latency-sensitive external task owners:** decide whether rejected work returns
ownership, or executor-owned disposal uses existing bounded retirement lanes. Account discarded
captures in capacity and truthful close evidence; an unbounded global deferred-drop queue would
replace the risk with memory growth. Clarify running versus worker-available statistics when a
worker has logically completed but is still dropping its final captured/result owner.

**Acceptance:** controlled blocking Drop on rejection/coalescing/cancel cleanup does not block a
promised nonblocking submit path; disposal saturation is explicit; releasing the gate allows the
same close owner to finish. Preserve panic isolation and cancellation semantics.

**Precedent:** Nara's own close path already uses retained ownership and finite disposal lanes
(`runtime.rs:2272`, `2640`, `2740`). Bevy task spawning
(`repo-ref/bevy/crates/bevy_tasks/src/task_pool.rs:566`) does not establish a general guarantee
that arbitrary Rust destructors cannot block.

### B06 — P2: Ordinary Task Admission Rebuilds The Entire Pending Queue

Classification: confirmed algorithmic expansion risk; current latency impact is unmeasured.

Every `admit` allocates a new VecDeque while holding the queue Mutex, moves all existing jobs to
filter terminal entries and replaces the queue (`crates/nara_tasks/src/runtime.rs:1981`). This
happens even without cancellation and under the ordinary Reject policy. Coalescing then scans
again. Filling n pending jobs while workers are occupied requires O(n squared) total existing-job
visits/moves; workers dequeue under the same lock (`1803`).

The default finite queue is modest, but the allowed per-class pending maximum is 1,048,576
(`runtime.rs:28`). A count bound does not establish an acceptable batch-import admission tail.

**Refactor after focused measurement:** keep ordinary enqueue local; budget cancellation
maintenance or clean when capacity requires it. Add a coalescing index only if real workloads
justify it. Reuse the current queue/state machine rather than introducing another scheduler.

**Acceptance:** measure submission P95/P99, allocations and lock duration at 512, 4,096 and the
actual batch target with workers held busy; retain FIFO, pending-only coalescing and cancellation
capacity release.

**Precedent:** Godot's `_post_tasks` appends ordinary/low-priority queued work without rebuilding
the entire queue (`repo-ref/godot/core/object/worker_thread_pool.cpp:224`, `252`, `259`). Its
different fallback policy is not a recommendation to run rejected work on Nara's caller thread.

### B07 — P2: Background Pools Are Not An Integrated Frame Job System

Classification: capability boundary, triggered by measured CPU work; not a current execution bug.

`TaskPools::spawn` accepts owned `'static` work (`crates/nara_tasks/src/runtime.rs:1396`). It cannot
borrow frame data and join a bounded scope before returning. Root ran
`cargo metadata --locked --format-version 1 --all-features`; the actual resolved `bevy_ecs 0.20.0`
features are `async_executor, backtrace, bevy_reflect, default, serialize, std`, without
`multi_threaded`. Its default schedule executor is therefore single-threaded. Three background
pool categories do not establish parallel ECS simulation.

**Trigger:** a real animation, simulation or extraction workload misses its frame budget. Forcing
that work through background copies would spread duplicate data, integration generations and
join/latency assumptions into game modules.

**Direction:** first evaluate Bevy's existing multithreaded schedule/query facilities, with an
explicit aggregate thread budget and main-thread affinity. Keep background imports in their
existing cancellable pools. Only investigate a distinct scoped frame-job interface if measured
work still requires it, as OQ-026 already specifies. Do not create another general job graph now.

**Acceptance:** independent systems can overlap, conflicting writes/order stay serialized,
NonSend work stays on its owner thread, faults reach the correct Runtime, and no borrowed work
survives its frame barrier or stop boundary.

**Precedent:** Bevy separates executors and scoped tasks
(`repo-ref/bevy/crates/bevy_ecs/src/schedule/executor/mod.rs:44`;
`repo-ref/bevy/crates/bevy_tasks/src/task_pool.rs:317`). Godot explicitly joins same-stage process
groups (`repo-ref/godot/scene/main/scene_tree.cpp:1329`). No performance benchmark was run here.

### B08 — P2: Context Changes Do Not Complete Delivered Action Lifetimes

Classification: sustained-action lifecycle gap; high confidence in behavior, no stock-product
failure reproduction. Per-binding outcomes remain a supported low-level contract.

`ActionMap::disable_context` only changes a set (`crates/nara_input/src/lib.rs:866`). If gameplay
already consumed Started, a later physical Release during the disabled interval is discarded by
`InputTimeline::stage_actions` (`timeline.rs:1466`) and filtered again by action resolution
(`lib.rs:1173`). Re-enabling does not synthesize a release/cancellation. The API has only Started
and Released (`902`), with no record of which semantic actions reached a consumer.

A movement consumer that sets a bool from these outcomes can therefore remain active after
opening and closing a menu. The reference game uses that consumption shape
(`reference-game/src/resources.rs:102`; `systems.rs:555`), although its stock path does not prove
this menu scenario. Existing disabled-context tests start with the context disabled and do not
exercise the exit of an already delivered action.

The same future state owner must handle multiple bindings: W and ArrowUp intentionally emit
independent edges, but the gameplay bridge matches action/context/phase
(`crates/nara_gameplay/src/lib.rs:164`). Directly treating those as a sustained aggregate bool
stops movement when one binding releases while another remains held. This is not a violation of
the explicitly per-binding outcome API; it is missing higher-level action state.

**Refactor before context switching/rebinding/held-action consumers expand:** define a semantic
action owner that tracks binding/endpoint contributions and a deterministic context-exit policy.
Preserve the existing physical timeline and per-binding observations. Decide how deferred edges
relate to mapping revisions rather than remapping them implicitly at arbitrary delivery times.
Do not force Editor shortcuts and Runtime UI through one gameplay ActionMap.

**Acceptance:** exactly one exit after a delivered Started is disabled; no resurrection on
re-enable; two contributing bindings remain active until both end; pause and mapping replacement
use the declared revision policy; quick edges, zero-tick retention and command order remain intact.

**Precedent:** Godot tracks event/device contributions and computes aggregate action state
(`repo-ref/godot/core/input/input.cpp:1088`, `1100`, `1113`, `1974`). That supports an explicit state
owner rather than changing Nara's low-level event meaning silently.

### B09 — P2: Unix Leaf Open Can Block Before Rejecting A Non-Regular File

Classification: current platform-specific robustness defect; high static confidence, no Linux
execution reproduction in this Windows review.

`DirectoryCapability::open_file` opens the leaf before checking its facts
(`crates/nara_fs/src/capability.rs:241`, `265`). Unix uses RDONLY/NOFOLLOW/CLOEXEC without NONBLOCK
(`platform/unix.rs:206`, `214`). Both strict openat2 and ordinary openat receive those flags.
`validate_file_facts` rejects non-regular objects only after open returns (`capability.rs:759`).

A selected manifest, Scene or Image path can be a FIFO with no writer, including after external
replacement of a regular file. A blocking read-only open then cannot reach type rejection, bounded
reading or decoding. Path-resolution constraints do not require a regular leaf. Strict Check and
content admission use this interface (`src/project_host/check.rs:188`;
`src/project_content.rs:1265`, `1291`).

**Refactor at nara_fs:** use an appropriate nonblocking leaf-open/type-validation sequence on the
same opened object. Keep handle-bound checks; do not replace them with a racy stat(path)/open(path)
proof or add individual timeout patches to every consumer.

**Acceptance:** Linux native child-process tests open FIFOs without writers through open_file and
open_child_file in supported trust modes and return NotRegularFile within a parent-enforced
deadline. Preserve ordinary-file, symlink and identity behavior. The process deadline prevents a
regression from hanging an entire nextest worker.

**Precedent:** Godot prechecks file kinds before fopen
(`repo-ref/godot/drivers/unix/file_access_unix.cpp:90`), and another inspection path uses
O_NONBLOCK (`drivers/unix/dir_access_unix.cpp:700`). Borrow the nonblocking inspection principle,
not the name-race window of its separate stat/open sequence.

### B10 — P2: UI Text Measurement And Paint Use Different Boxes

Classification: current correctness defect with an existing legal consumer; high static confidence.

The UI compiler passes padding to Taffy (`crates/nara_ui_kernel/src/compiler/root.rs:580`) and
measures text under layout constraints (`119`). It then reshapes wrapped text using the entire
node width (`245`) and publishes the whole node rect as the text rect (`272`). The renderer
consumes that rect (`crates/nara_ui_render/src/planner/lowering.rs:247`) and uses its outer origin
for glyph placement (`299`). Left/top padding is therefore not applied to a node's own text;
final shaping can also use a different wrap width from measurement.

`crates/nara_ui_kernel/examples/layout_gallery.rs:80` already combines padding and text on one
node. Parent padding applied to child nodes is a different path and does not fix this case.

**Refactor at the UI compiler:** publish one explicit text content rect, using the same content
constraints for measurement, final shaping, origin and text clipping. Keep the node's outer box
for background, hit testing and control semantics; the renderer should not infer padding again.

**Acceptance:** a 100-wide auto-height text node with 20 units of horizontal padding shapes at
width 60 and paints at the content origin. Layout and paint agree after padding changes. Existing
sidebar and controlled Button cases retain correct outer hit/background geometry.

**Precedent:** Bevy subtracts padding/border and uses the content box for text
(`repo-ref/bevy/crates/bevy_ui/src/widget/text.rs:215`, `416`); Godot offsets Button text by style
margins (`repo-ref/godot/scene/gui/button.cpp:452`, `459`).

### B11 — P2, Triggered: Historical Glyphs Can Exhaust A Small Current UI Workload

Classification: bounded-cache recovery risk; not unbounded memory or a violation of capacity refusal.

For the same font revision, `GlyphAtlas::stage` clones the historical atlas and only inserts keys
(`crates/nara_ui_render/src/atlas.rs:75`, `98`). Reaching historical glyph capacity rejects a new
key (`101`) even when the current frame references very few glyphs. Keys include exact font-size
bits (`crates/nara_text/src/engine.rs:168`), so changing sizes can consume entries with unchanged
text. With capacity two, sequential A/B/C single-glyph frames demonstrate the distinction between
history and current working set; no execution measurement was performed here.

Font revision changes or explicit `UiPresentationPlanner::retire_target`
(`planner/mod.rs:164`) reset the atlas. Ordinary surface successors retain font generation
(`crates/nara_ui/src/runtime/surface.rs:164`), and integration retires only removed/changed routes
(`crates/nara_ui_render/src/integration.rs:539`). Failure is safe: ContentRejected aborts the
candidate (`639`), restoring last-presented planning (`planner/mod.rs:300`). New-key frames can
remain stuck on the old displayed UI; old-key frames may still succeed. Reset/reconstruction is
possible, so this is not absolute irrecoverability.

**Refactor when long-lived content/size churn triggers it:** add small-capacity pressure evidence
and rejection metrics, then consider rebuilding a private atlas from the current working set with
a fresh incarnation. Old packets retain immutable snapshots. Do not fake font revisions or retire
the whole UI session to conceal capacity exhaustion; keep CPU entry policy and GPU residency
accounting distinct. A universal cache framework is unnecessary.

**Acceptance:** C can recover after historical A/B holders retire, while an old A packet remains
valid and cannot alias the replacement atlas. A genuinely over-capacity single frame still rejects
boundedly and retains last-good.

**Precedent:** Bevy offers atlas cleanup (`repo-ref/bevy/crates/bevy_text/src/font_atlas_set.rs:52`)
and Godot exposes font/glyph cache clearing (`repo-ref/godot/scene/resources/font.cpp:2477`, `2693`).
These are evidence of recovery controls, not proof of a bounded LRU equivalent to Nara's needs.

### B12 — P1, Degraded I/O: Superseded Image Work Still Blocks Current Publication

Classification: concrete reload recovery and retained-bookkeeping defect; high static confidence,
with no new barrier-based execution reproduction in this audit.

Each AssetId owns an ordered stream plus an attempts map
(`crates/nara_image/src/reload.rs:144`). A newer request advances generation
(`crates/nara_asset/src/reload/resolution.rs:255`) and appends another handle (`reload.rs:611`).
Coalescing only replaces work still pending in the pool; already running older work is not
cancelled. Poll consumes only a ready prefix (`704`;
`crates/nara_tasks/src/runtime.rs:1107`), and apply checks generation after that (`reload.rs:802`).

**Counterexample:** IO worker one is blocked on old generation A. Worker two finishes valid newer
B. A has no remaining publication authority, yet B cannot reach apply until A finishes. Repeated
later failures or coalesced cancellations also retain handles, terminal records and attempts behind
A. Queue capacity bounds live pending work, not these completed historical maps. Successful pixel
results still hold Image reservations; this finding does not claim unbounded decoded pixels.

Different assets have separate streams, and publication/version/slot guards prevent late A from
overwriting B. The gap is liveness and retained terminal bookkeeping inside one concrete owner.
ADR 0080 deliberately permits eligible ordered predecessors; the existing reverse-completion test
(`crates/nara_image/src/tests.rs:3416`) uses the same generation. Neither should be removed.

**Refactor:** distinguish latest desired generation from physically outstanding older work.
Logically cancel/retire superseded generations so their handles no longer block current eligible
publication; retire stale terminals independently with bounded bookkeeping. TaskPools keeps actual
worker/capture close ownership. Preserve within-generation order, Poll cutoff and final publication
checks; no generic importer executor is needed.

**Acceptance:** hold A with a barrier on one of two workers; successful B publishes on the next
eligible Poll before releasing A; late A cannot write back. While A stays blocked, submit many
failed/coalesced successors and demonstrate bounded owner records and correct reservation charges.
Retain the same-generation ordered-prefix regression.

**Precedent:** Bevy tracks per-asset task ownership and independently removes finished tasks
(`repo-ref/bevy/crates/bevy_asset/src/server/mod.rs:445`, `2057`). This is task-lifetime precedent,
not proof of equivalent publication authority or dependency atomicity.

### B13 — P1: Legal CPU Image Extents Can Escape Typed GPU Admission

Classification: current backend robustness defect; high source confidence, no new GPU reproduction.

Default Image import permits dimensions up to 16,384 (`crates/nara_image/src/limits.rs:10`).
The backend requests `DeviceDescriptor::default()`
(`crates/nara_render_wgpu/src/backend.rs:1161`). The actual lock resolves `wgpu 30.0.1` and
`wgpu-types 30.0.0`; its registry source `Limits::default/defaults` gives a default 2D extent limit
of 8,192. Requested device limits control validation even if the adapter supports a larger texture.

`ensure_image` checks snapshot kind/length and upload/residency bytes
(`crates/nara_render_wgpu/src/texture.rs:242`, `288`), then calls `create_texture_resource` at
`786` without comparing extent to live device limits. A legal 16,384-by-1 RGBA Image is only
65,536 bytes, so byte admission cannot protect the device call. The ordinary primary/auxiliary
Image path does not use the viewport limit check in `editor_execution.rs:1241`.

No uncaptured-error handler is installed in this crate. Its pipeline error scope
(`crates/nara_render_wgpu/src/quad.rs:223`) does not surround this texture creation. Device
validation can therefore escape WgpuTextureError/ContentRejected and use WGPU's default panic
handler (`wgpu 30.0.1/src/backend/wgpu_core.rs:692`). This does not establish that every host must
terminate its process; the defect is predictable content failing outside the typed admission path.

**Refactor at the device resource owner:** validate each concrete texture class against the live
device capability before creating it, and use an appropriate error scope for expected creation
failures. Re-admit after device epoch/capability changes. Keep CPU import policy independent of a
particular GPU and preserve snapshot/lease ownership.

**Acceptance:** on a default-limit device, 8,192-by-1 succeeds while 8,193-by-1 and 16,384-by-1 yield
typed rejection without panic; a subsequent legal frame succeeds. Cover Sprite primary and material
auxiliary Image paths, with no false publication or leaked budget charge.

**Precedent:** Godot checks actual driver texture-type limits before allocation
(`repo-ref/godot/servers/rendering/rendering_device.cpp:1655`, `1675`). This is separate CPU-data
and device-resource admission, not a reason to copy its RenderingDevice abstraction.

### B14 — P2: A Per-Frame Instance Budget Is Reset For Each View

Classification: current multi-view budget scope mismatch; high static confidence, not a measured
allocation or OOM claim.

The backend names its 64 MiB limit per frame (`crates/nara_render_wgpu/src/quad.rs:14`), but
`admit_quad_instance_buffers` checks only the supplied batches (`426`, `436`).
`prepare_target_draw` invokes `prepare_submitter_draw` separately for each view and the UI pass
(`backend.rs:1420`, `1435`, `1480`). Earlier allocations remain held in PreparedTargetDraw
(`crates/nara_render_wgpu/src/lib.rs:375`, `397`); the final byte sum is telemetry (`417`).

Each instance is 80 bytes. Two views with 524,288 instances each independently admit 40 MiB and
together hold 80 MiB. Sprite extraction has no matching aggregate instance limit. Sixteen views
are a finite target limit, so this is not unbounded growth, but the advertised per-frame budget
does not describe the actual admission scope. Editor targets can also share one Host transaction.
ADRs 0054/0096 retain per-frame resource admission rather than redefining it per view.

**Refactor:** specify the Runtime frame/Editor Host transaction scope, then let that backend owner
accumulate reservations across all views/passes/targets in the scope. Reject before the excess
allocation. Arena/ring-buffer choices remain separate measured optimizations.

**Acceptance:** each view is individually below the limit but their sum exceeds it; reject the
successor allocation structurally, report the total and recover on the next frame. Include mixed
Sprite/UI passes and two Editor targets.

**Precedent:** Bevy resets an owner-level sprite instance buffer outside its view loop and appends
view data (`repo-ref/bevy/crates/bevy_sprite_render/src/render/mod.rs:584`, `591`, `663`, `704`).
This supports aggregate ownership, not a claim that Bevy enforces the same 64 MiB hard limit.

## Reviewed Without New Independent Findings

- **J02 component transactions:** persistent candidates bind to a frozen registry; lifecycle-free
  insertion/despawn guards hold exclusive World access; Scene prepares components, identity,
  hierarchy and retirement inside one owner. The native consumer in
  `tests/fixtures/module-consumer/tests/support/domain_reconciliation.rs:76` proves runtime-only
  hooks and RequiredComponents. Their deliberate restriction in strong persistent transactions
  is not a global ECS limitation. Artificial mutation between an internal prepared token and
  commit violates the documented baseline contract; no current Scene caller was found doing it.
- **J03 lineage/defaults:** owner-local catalog lineage, tombstones, bounded effective defaults,
  explicit Null and lossless unavailable records retain their intended meaning. Manual codecs
  already support external custom scalar components. Limited predecessor scope and native codec
  invariants are explicit contracts, not newly discovered defects.
- **J04 plugin lifecycle:** pure closure, factory preparation, fallible build/finish, poisoning and
  reverse shutdown retain ownership. Equal configuration fingerprints do not prove arbitrary
  trusted factory behavior; no such proof should be inferred or added as a framework.
- **J05 time/runtime:** atomic time planning checks overflow before clock publication; sticky
  faults stop the next stage/catch-up tick; paused frames retain integration/extraction/cleanup;
  exact step intentionally retains debt/remainder/interpolation. CloseIncomplete retains the same
  owner for retry. No new timing or physical-retirement defect was confirmed.
- **J06 documents:** bounded inverse history, content-state dirty tracking, source-bound save
  checkpoints, base-revision Reopen and unavailable manifests passed source review. B01 concerns
  decision target identity, not missing general document transaction safety.
- **J07 commands:** tick/source/sequence, Capture-to-Ack and bounded poison state preserve command
  ownership. No new zero-tick command loss or multi-tick duplicate consumption was found.
- **J08 presentation:** exact presented receipts, isolated layout/geometry generations, inert modal
  fallback, focus continuity and resource-complete packets remain sound. Current Button-only
  runtime behavior is an explicit finite contract, not a universal widget extension promise.
- **J09 publication:** bounded reads hash the same returned bytes; Export validates exact closure
  membership/source and staged digest; namespace publication does not claim immutable contents or
  universal crash durability. Handle-bound cleanup/orphan ownership should remain.
- **J10 watcher/publication:** sticky RescanRequired and backend rebuild are explicit contracts.
  Existing Image/TileSet snapshots, last-good state, slot authority and revocation guards remain
  intact; no old-pixel overwrite was found in the B12 scenario.
- **J11 rendering:** exact Image versions, actual-holder inventory, submission retention and
  source/device/target identity separation remain useful. Shared encoding paths do not need a
  render graph or a second render World merely because another engine uses them.
- **J12 modularity:** optional presentation/backend dependencies and independent locked module
  consumers demonstrate real modularity. The collect-game external gameplay plugin contributes
  schema through the ordinary recipe. Root profile size or crate count alone does not prove a
  dependency defect; more crates would not establish better extension quality.
- **Root integration check:** hierarchy completion and its measured baseline remain intact;
  removing owner-derived globals through an advanced ECS escape is detected by render extraction
  rather than silently turning into local-transform fallback. Diagnostics have bounded, classified
  ownership; missing producer bridges already have explicit consumer-triggered scope.

## Triggered Boundaries, Not Additional Bugs

| Subject | Evidence and next trigger |
| --- | --- |
| Native input vocabulary | `crates/nara_input/src/lib.rs:35`, `769` has finite keys/bindings; `nara_winit/src/lib.rs:1181` drops unsupported keys. Timeline Wheel exists, but managed driver/Winit ingress is incomplete. First ScrollView/rebinding/chord consumer must complete native ingress and separate physical/logical/text meaning before durable bindings spread. |
| More runtime widgets | `crates/nara_ui/src/runtime/candidate.rs:20` accepts the exact Button behavior contract. A real slider/checkbox should prove controlled behavior/state/action ownership rather than adding arbitrary World callbacks. |
| Multiple native domains | The hook-owning fixture proves one reconciliation path, not arbitrary cross-domain unload. A second real domain must prove update, partial failure, retire and RetryClose order before shared coordination is extracted. |
| Complete build reproducibility | Export tracks a finite generated-entry build-input set, not arbitrary Rust modules/build scripts. ADRs 0110/0116 explicitly limit this scope. A future reproducible-build claim needs an input/environment snapshot, not a home-grown Rust source parser. |
| Failed Export staging | Exact cleanup deliberately retains an orphan instead of recursively deleting unproven contents. Unattended repeated export needs an explicit bounded recovery workflow before claiming automatic cleanup. |
| Bevy upgrades | Version-coupled lifecycle/observer/resource metadata checks are centralized. Upgrade qualification must exercise their behavior, not only compile. This is no evidence for replacing the ECS. |

## Recommended Order

1. **Protect current actions and recovery:** B01 prompt identity, B12 superseded Image retirement
   and B13 device resource admission. Retain the earlier A01-A03 Scene/schedule contract work.
2. **Complete component and product authoring before more consumers:** B02 explicit math shapes,
   B03 recursive/Optional Inspector and B04 checked defaults configuration. Coordinate schema
   changes with owning versions and keep generic Editor recovery data-only.
3. **Correct current geometry/platform and scope semantics:** B10 text content boxes, B09 Unix
   leaf opening for Linux qualification and B14 owner-level multi-view budgets. B08 belongs before
   context switching/rebinding introduces repeated game-local reset logic.
4. **Measure and deepen targeted runtime work:** B05 disposal ownership, B06 admission cost and
   B07 ordinary ECS parallelism when frame pressure requires it; B11 atlas recovery under historical
   churn. Preserve the prior real-map and full-document measurement gates.

These are dependency groups, not one giant rewrite. Each owning-contract change should include one
real public consumer, its failure/recovery path and removal of the obsolete workaround. Missing
features do not justify speculative generic executors, native rollback, package kernels or cache
frameworks.

## Verification And Artifacts

- Static source review and independent challenge only; no new nextest suite, timing benchmark,
  Linux FIFO execution, native GPU readback or platform qualification was run.
- One serial `cargo metadata --locked --format-version 1 --all-features` confirmed the actual
  Bevy ECS feature cohort. Cargo downloaded a dependency into its cache; project runtime files
  and the lockfile were not changed.
- Document checks passed: 66 qualified source locations exist within file bounds, linked documents
  resolve, B01-B14 are complete, and the HTML has 22 unique IDs with 10 valid local/section links.
  Seven JavaScript assertions passed for all/P1/P2 filtering, search, empty results and counts.
  These checks are document validation, not browser visual or runtime acceptance.
- The expanded standalone HTML stays outside the repository:
  `D:\Frankorz\Downloads\nara-foundation-audit-expanded-2026-10-19.html`.
- Existing untracked `myeasylog.log` and `vendor/` are unrelated and remain untouched.

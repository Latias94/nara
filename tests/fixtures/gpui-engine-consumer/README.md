# Nara-Owned GPUI Rendering Consumer

This independently locked workspace selects maintained Open-GPUI Git revision
`68d52b009dde3c784b01268208cb1d0ed690e99c`. This is a local, unpublished development
checkpoint on `refactor/editor-framework-evolution`, including the current API,
controls and reviewed shared Dockspace adapter. All framework
packages and the optional Jellyflow adapter must resolve the same Git source. The
[integration study](../../../docs/research/2026-11-11-open-gpui-engine-integration-layers.md)
records the qualification scope. The engine and ordinary renderer fixtures do not acquire
this framework dependency. This is a public integration consumer, not the selected full Editor
frontend.

The canonical manifest and lock now select that one immutable source. Registry
patches for GPUI/components prevent the optional Jellyflow manifest from introducing
an older registry GPUI; the EGL correction is pinned separately. No temporary path
patch is needed for this source selection. The older published `f75c10b` native
receipts retain their historical scope. This new source has no remote publication
claim: another machine needs the exact local Git objects (checkout or transferred
bundle), or a separately authorized publication, before fetching it.

The optional Jellyflow packages resolve through an adjacent maintained `jellyflow` checkout.
Cargo requires those path manifests even when `jellyflow-trial` is disabled. The handoff records
the qualified library revision; this sibling requirement belongs only to this independent fixture.

## Optional Jellyflow Consumer

Enable `jellyflow-trial` to inspect two independent
native graph Views over one Jellyflow `GraphRuntime` and history. Custom retained nodes contain
real graph ports, native controls and drag handles. `--jellyflow` opens the interactive consumer;
`--smoke-jellyflow` runs the finite native trajectory and closes.

Drag the node's `Drag title` header to move it. The node body contains independent native
controls; `Local label` is a button. Middle-drag on empty canvas pans, and the wheel zooms.
Both Views share graph edits and Undo while retaining separate navigation and local controls.

```text
cargo +1.95.0 build --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view --features jellyflow-trial,ui-work-observation
```

After selecting the backend as below, run `target/release/examples/gpui_view.exe --jellyflow`.
The ignored test
`engine_owned_gpui_hosts_real_jellyflow_nodes_and_independent_views` checks accepted native
geometry and actual pixels, a local control, captured node drag with one history entry, Undo,
independent pan/zoom and normal retirement. Set `NARA_GPUI_VIEW_EXE` to the absolute prebuilt
release executable when running that test from a debug nextest build.

The node/control clicks, captured node drag and middle-button pan use injected Winit
cursor/button events through the same `Viewer::input` entry as the interactive window,
including physical-to-logical coordinates, pressed-button tracking and explicit focus
loss/regain. The hidden Scene-isolation click and final wheel zoom retain direct platform
input. The actual scale is reported in the input marker. This does not prove physical mouse
delivery from the desktop.

This window is a graph integration consumer. The existing independent source Editor is
`nara-editor-drawn`; see the [Editor panel guide](../../../docs/guides/editor-panels.md) for
its build command, current workspace/Inspector features and remaining frontend work.

This consumer deliberately selects Jellyflow as the graph authority. It does not mirror or
replace Nara's authored ValueGraph, Session admission, saved-content identity or persistence.
The [engine handoff](../../../docs/plans/2026-11-12-nara-ui-integration-handoff.md) records
the external-domain seam and complete Dialogue Graph successor. A Shader Graph, general docking,
game UI choice and numeric scale budgets retain their own acceptance. Executed results and
known library defects belong in the linked studies and handoff.

## Dialogue Graph Domain

The public `dialogue` module is a separate `consumer.dialogue-graph` extension document.
Line, Choice and End nodes, stable choice ports, atomic edits, one domain history and the
versioned byte codec use Nara's original Session/file workflow. A local conversation preview
follows one explicit transition at a time; cycles and incomplete authoring states are legal.
The existing list-of-lines Dialogue and finite ValueGraph formats remain separate.

With `jellyflow-trial`, `dialogue::jellyflow::DialogueFacts` captures an immutable core graph
and checked ID/incarnation mappings. It creates no Jellyflow runtime or second history.
The original extension command result carries a typed `DialogueCommandReceipt`, bound to that
exact command and accepted Workspace revision. Its `outcome()` supplies the domain effect and
actual allocated IDs; `view()` retains the exact post-command immutable source. `CreateFromWire`,
including creation while reconnecting either endpoint, ordinary reconnection and mixed node/link
removal each validate and commit one candidate with one Undo boundary. A successful semantic
Noop shares the original source allocation and preserves history.
`external_facts()` retains that exact graph allocation and supplies complete live-incarnation
observations to Jellyflow's `ExternalGraphDocument`. Two ordinary `ViewSession`s now consume
that Source through the public Session/file tests, with independent local state and geometry.
Equal-value Noop publication, coalesced remove/Undo and explicit reset after Reopen preserve
the original Nara document/history and prevent retired native identities from returning.
Mixed feedback refused before admission returns its original move-only packet; layouts can
be resubmitted using the same original token after moving its unsupported commands out.
`DialogueCanvasRequest` lowers typed Jellyflow interactions through their captured domain
basis into an original Session command. `publish` pairs its exact accepted outcome and retained
source with a sealed `DialogueCanvasPublication`; `complete` adopts the actual canonical
identities from that receipt. Invalid evidence retains the original authorities for repair.
An original A receipt can settle after B without rolling either View backward or reexecuting
the command. The external native journal prerequisite belongs to Jellyflow. The two-View
Dialogue controls below use Session snapshots independently; external graph integration and native qualification remain open
in the [consumer plan](../../../docs/plans/2026-11-12-dialogue-graph-integration-plan.md).
The `--jellyflow` mode above continues to demonstrate the standalone runtime.

The following external-source command requires the missing Jellyflow prerequisite documented
below. For the independent portable workflow, use the command in the next section instead:

```text
cargo +1.95.0 nextest run --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --features jellyflow-trial,ui-work-observation --lib --test graph_document --test dialogue_document --test-threads=1 --no-fail-fast --no-tests fail
```

The plan records actual results and limits. Source lifetimes are separate from saved content
identity; adapters must retain the original document/revision and preserve unchanged choice IDs
and outgoing targets when constructing complete choice replacements.

### Two-View Dialogue Workflow (Not Yet Qualified)

Enable `dialogue-workflow` for `--dialogue` and `--smoke-dialogue`. This feature selects GPUI
controls over the original immutable Session snapshots and does not enable `jellyflow-trial`.
Each View has independent node selection, node/property drafts, scrolling, popup state and
conversation preview. The editors queue exact actions through the existing `Viewer::drive`
and original `EditorProjectSession`. No second writable graph, history or runtime is created.
The page explicitly marks the graph canvas and drag as unavailable. Positions remain editable
through fields; Line links use a next-target selector whose selection applies immediately.
Choice text, labels, targets and order use one atomic Apply. Node creation, deletion, entry,
Cancel/Apply, shared Undo/Redo and original Save/Reopen remain available through controls.
Dirty drafts retain their original basis until explicit Review or Cancel. Review performs a
three-way field merge: unchanged local fields adopt accepted peer values, while overlapping
changes display Current and Draft and require an explicit choice before Apply. Choice labels,
targets, removal and ordering merge by stable choice identity. Keeping a locally edited choice
after its accepted removal requests a fresh allocation rather than resurrecting the retired ID.
Reopen retires old drafts and node actions until a live node is explicitly selected. Each View
has a bounded, independent preview scroll region. Closing either or both Views leaves the
document open and retires that View's callbacks and queued work.

The portable `dialogue::workflow` module has public Session/file regressions in
`tests/dialogue_document/workflow.rs`. The Windows smoke target uses accepted Window geometry,
the original Winit input converter, injected text/IME, same-Scene GPU readback and original
Host retirement. Optional `NARA_GPUI_DIALOGUE_CAPTURE_DIR` writes the readback as a PPM file.
These are test implementations, not passing test receipts.

The initial API audit inspected local Open-GPUI
`f2407cdae92e9412a44b3b3979c595139d369902`; subsequent verification uses the
coordinating Open-GPUI branch and the exact source boundary recorded below.
The controls target its GPUI `Element` blanket `IntoElement` implementation. The four
consumer wrappers no longer implement `IntoElement` explicitly. ValueGraph path painting now
uses `Window::paint_path` clipping instead of the removed public `Path.bounds` field. The
accepted-frame transaction retains reusable `Fn` callbacks rather than consuming captured facts.
The manifest now records Open-GPUI `68d52b009dde3c784b01268208cb1d0ed690e99c`
for all five direct packages and both registry overrides. Cargo metadata resolves
all 23 GPUI packages to that same exact Git identity; Nara's own Taffy 0.10.0 and
GPUI's 0.15.1 remain independent dependencies. The coordinating verification fetched
the immutable source from the adjacent Git checkout using a per-process URL
rewrite; no global or repository Git configuration changed. The committed URL
remains the canonical GitHub source, and no push occurred. After providing those
exact source objects on the runner, run serially:

```text
cargo +1.95.0 nextest run --locked --release --target-dir target --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --test dialogue_document -E 'test(workflow::)' --test-threads=1 --no-tests fail
cargo +1.95.0 build --locked --release --target-dir target --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view --features dialogue-workflow,ui-work-observation
cargo +1.95.0 nextest run --locked --release --target-dir target --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --features dialogue-workflow,ui-work-observation --test gpui_view -E 'test(engine_owned_gpui_dialogue_edits_two_views_saves_reopens_and_previews)' --run-ignored ignored-only --test-threads=1 --no-tests fail
```

The first command compiles portable Session/file tests; their real Host Open currently needs
the Windows/Linux filesystem guarantees described below. The latter commands need Windows
to compile and exercise the native page. Set `CARGO_BUILD_JOBS=1`, the prebuilt absolute
`NARA_GPUI_VIEW_EXE`, `NARA_WGPU_FORCE_FALLBACK=0` and run separately with `WGPU_BACKEND=dx12`
and `vulkan`. On macOS, `gpui_view` compiles only its unsupported-platform entry, and its native
test binary has no tests. The smoke trajectory checks actual control input, multiline/CJK,
independent drafts and preview cursors, popup Escape/outside closure, original persistence,
position edits, atomic choice edits, node creation/entry/deletion and shared history. Preview
assertions consume the page's accepted Line/Choice/End publication. Retained Text/Apply callbacks
from accepted frames are replayed after Cancel, Reopen and View close. The surviving View is
edited and cancelled after its peer closes. The added deletion-recovery trajectory selects
another live property target without Undo, then replays the deleted target's old Text/Apply
callbacks and checks that they stay retired. The ordinary keyboard trajectory uses the Winit
converter for character input, Enter, Backspace and all four arrows, checks the original
UTF-16 caret and commits one history entry. Clipboard interoperability has no fixture receipt.
These are unexecuted native regressions. Graph gestures,
physical input, OS IME and performance remain unqualified.

### Review Follow-Up Verification

The existing six-reviewer run completed against the frozen consumer snapshot
`1e2dc6be47bfbe5b34d7e8b5271de779fd3fc3d7`. Its independent merge, validator and report
record four confirmed findings: cross-View overwrite during Review, retired node actions,
unreachable long preview content, and missing resource lookup. The original receipt remains
under `/tmp/compound-engineering-502/ce-code-review/20261113-165020-dff6809b/review.json`.
It describes the frozen predecessor, not a successful review of the following edits.

All four have source corrections and new regressions. Pure workflow tests exercise
field and stable-choice merges, explicit overlapping resolution, accepted removal versus
local editing, conflicting order and source/incarnation gates. A public Session regression
joins peer preservation, explicit resolution, original history and Save. The native trajectory
now requires the surviving View to contain the injected text in Editing state, then return to
the original text and Clean state after Cancel. It queues accepted Apply and ordinary Undo
before closing each View, drains the original queue, and checks unchanged document, revision
and history. The native driver and smoke helpers were separated without changing the original
phase order; phase 99 checks the last closed View's drained work. A separate resource prelude
uses accepted controls to query and select a full Chinese/long project path in one View,
independently find a Scene in the other View, receive its original asynchronous Open terminal result,
reject a stale index generation, and retain both local queries and selections after collapse.

An independent follow-up found that removing a conflicting choice after Review could make
Use Current silently clear the conflict without restoring the choice. Choice-label and
choice-target resolution now restore the complete current member when it was locally removed.
If the current member is also absent, resolution returns false and retains the conflict;
Apply keeps its original refusal. Pure regressions cover both fields through Review, Remove,
Use Current and domain Apply, plus failed field resolution after accepted removal. The public
Session regression also checks the restored stable ID, label, target and original Undo/Redo.

On 2026-11-13, the final source passes **93/93** portable library cases, no skips, nextest
`1073d26e-99ec-4cea-8aab-349548b46eb9`. This includes the resource-owner query test and both
new choice-resolution regressions. The earlier 91-case run
`6c75d87e-0f8d-401d-82ef-0d9cb060682c` predates the last correction. Final verification used
Rust 1.95.0, `--locked --lib --test-threads=1`, the unoptimized test profile, one Cargo build
job and the existing root `target`; its log is `/tmp/nara-editor-dialogue-final-library-tests.log`.
Task-scoped formatting passed without changing other Rust files; the initial 19-file pass
and final four-file pass are recorded in `/tmp/nara-editor-dialogue-followup-fmt.log` and
`/tmp/nara-editor-dialogue-final-fmt.log`.

The updated `dialogue_document` Session test target compiles with `--no-run`, recorded in
`/tmp/nara-editor-dialogue-final-session-compile.log`. The final actual Windows example,
including resource UI, native driver and smoke sources, passes the locked Rust 1.95.0 Windows
SDK check at the unchanged canonical GPUI pin. Its log is
`/tmp/nara-editor-dialogue-final-windows-msrv.log`; the command is:

```text
CARGO_BUILD_JOBS=1 cargo +1.95.0 xwin check --cross-compiler clang-cl --locked --target x86_64-pc-windows-msvc --target-dir target --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view --features dialogue-workflow,ui-work-observation
```

These receipts cover the final corrections; the 83-case library and Windows SDK receipts
below retain their predecessor scope. The historical macOS Session result remains **0/12**
at `project.editor.prefabs-open-failed`; the updated Session cases were compiled, not run.
Windows DX12/Vulkan native execution remains pending.

The coordinator independently repeats the final library cohort on actual Rust 1.95.0,
**93/93**, `734e2acd-0b1b-43d0-ada7-745f76cc9eb4`, and on the updated development
Rust 1.100.0, **93/93**, `358ed07d-bd75-471e-b2ce-2e1034af87fb`. The latter uses
task-local nextest 0.9.151. The final Windows SDK check is repeated successfully in
22.75 seconds, and the current Session target again compiles without execution. Logs
are `/tmp/nara-editor-dialogue-final-consumer-library.log`,
`/tmp/nara-editor-dialogue-final-rust1100-library.log` and the Windows/Session paths
above. Final independent source recheck closes the Review/Remove/Current finding;
the four-file workflow/Session scope fingerprint is
`1e338b40e9d16198204e984180119c7a3361130dc0905a221dde52602498c082`.
This bounded recheck does not rewrite the original six-reviewer report or grant
unexecuted native qualification.

### Project Resource Lookup

Each View has a collapsible Project resources / documents entry. It shares the original
Session Project Browser's immutable source index and reuses `EditorProjectBrowserState` for
its own query, expansion and selection. `EditorProjectBrowserView::shared_snapshot` is the
only new original-owner seam: it shares the existing Arc without copying entries or file
authority. Query work advances at most 256 entries per render; the page shows at most 48
matching rows and asks for a narrower query when more match. The selected project path
remains complete, including Chinese text and long names, when the finder is collapsed.

The original TrustedCompiled lane deliberately has no generic source index. Dialogue mode
therefore opens its one Session through the existing `EditorProjectOpenHost` authoring route;
other native modes retain their compiled route and original source benchmark. The Dialogue
fixture seeds its read-only resource before authoring Open, so the original scan observes it.
Its background Scene starts empty and still uses the same sprite construction and RenderHost.
There is no second index, filesystem enumeration, writable document owner or resource property
added to the Dialogue schema.

Assets remain inspect-only. Open document is enabled only for indexed Scene/Prefab entries
and queues the original `EditorAction::ProjectBrowser` intent. A captured index generation
is checked before dispatch, and the original Session retains exact file admission, pending
Open, activation, terminal result and acknowledgement. Dialogue itself continues using its
existing extension-document Open/Save/Reopen owner. This bounded finder does not add a new
Dialogue-source Browser capability. Pure tests cover shared index identity, independent
Chinese/long-path query and selection, refresh preservation and removal retirement and pass
in the final 93-case library run. The compiled native prelude checks the actual finder and
original asynchronous Open terminal result; native execution remains pending.

### Windows SDK Compile Receipt

On 2026-11-13, Rust 1.100 `cargo xwin check --cross-compiler clang-cl --locked`
against `x86_64-pc-windows-msvc`, the actual `gpui_view` example and
`dialogue-workflow,ui-work-observation` passes through the real Windows SDK.
The temporary current-source manifest retains the original fixture source files
and reuses the Nara `target`. The first run exposed an incorrect root import of
`ListboxOption` and an untyped keyboard callback. The corrected run, including the
deleted-target recovery and ordinary-keyboard smoke source, passes in
`/tmp/nara-editor-dialogue-workflow-windows-fixed-check.log`.
This compiles the Windows page that the Mac unsupported-platform entry excludes;
it does not execute Session/file, DX12, Vulkan or the native trajectory.
The same Windows SDK command also passes with the fixture's declared Rust 1.95.0
compiler, recorded in `/tmp/nara-editor-dialogue-workflow-windows-msrv.log`.

The final canonical manifest/lock then passes the same **locked Rust 1.95 Windows
SDK check** at `68d52b009dde3c784b01268208cb1d0ed690e99c`, with no path overrides,
in `/tmp/nara-editor-dialogue-canonical-windows-msrv.log`. Its locked library
cohort passes **83/83**, no skips, nextest
`d608554f-2dd5-4f6d-838e-7938c008673f`. Cargo metadata confirms 23 GPUI packages
from one exact Git source, the selected WGPU Git revision and the pinned EGL
correction. This replaces temporary-source compilation evidence for the selected
development source; the failed Mac Session/file run and missing Windows native
execution retain their distinct limits.

### Initial Canonical Dependency Resolution Failure

The coordinating lane attempted the portable canonical `--locked` run on 2026-11-13.
`/tmp/nara-editor-dialogue-workflow-tests.log` records failure before compilation: Nara's
`nara_ui_kernel` requires `taffy =0.10.0`, while the adjacent `jellyflow-open-gpui 0.4.0`
requires registry `open-gpui =0.2.0`, which requires `taffy =0.10.1`. The nextest metadata
invocation used `--all-features --filter-platform=aarch64-apple-darwin --locked`; optional
dependencies still participate in resolution even when the selected test does not use them.
No workflow test result follows from that failed resolution. The coordinating lane then
built a temporary consumer with both registry and Git GPUI sources patched to the current
local workspace. That temporary source selection does not publish a new Git revision or
qualify the canonical old pin. The final selected Git source and registry overrides above
subsequently close this resolution conflict without changing sibling Jellyflow sources.

### macOS Session Host Blocker

`/tmp/nara-editor-dialogue-workflow-local-tests.log` records that the temporary local-patched
test consumer compiled. Its macOS `dialogue_document` run executed 12 tests: 0 passed,
12 failed, 0 skipped. All failures occur during real project Open with
`project.editor.prefabs-open-failed`, before the intended editing assertions. This includes
the five new `workflow` tests and seven existing Session/file regressions.

`nara_fs::DirectoryCapability::open_directory_exact` calls `preflight_exact_spelling` before
opening descendants. `crates/nara_fs/src/platform/mod.rs` returns `FsError::Unsupported`
outside Windows/Linux. Independently, `platform/unix.rs` returns false from
`local_identity_semantics_proven` outside Linux. These existing platform guarantees were
not bypassed or changed. Session/file execution remains pending on a supported Windows/Linux
runner. A successful macOS test-binary build does not compile the `cfg(windows)` native page
and does not qualify its controls or GPU trajectory.

The same temporary current-source consumer separately passes **83/83** portable
library tests, no skips, nextest `b82a860b-2faf-461c-aec3-b931e2d7d2ac` on Rust
1.100. Those pure Dialogue/ValueGraph cases do not open the Session/file Host and
do not change the twelve failed Session cases above.

### External Graph Source Blocker

The local Jellyflow checkout observed at `83ccc07` lacks `ExternalGraphDocument` and
`with_external_graph_handler`. A subsequent source audit checked all twelve local refs and
all advertised remote refs, including both pull-request heads. The remote publishes
`main=8a7c2df55acba7f00f5465d9c97ffd8387b791f0`; fetching that exact commit succeeds. Its
`OpenGpuiSceneHost::new_measured_surface(ViewSession, OpenGpuiNodeRegistry)` exists, but
`ExternalGraphDocument`, `ExternalGraphRequest` and `with_external_graph_handler` are absent.
Fetching both documented prerequisite commits `0a9453a6df5483d30ad05195bd62fdbdab38a97e` and
`3a430b906d8b902bee6a682c436dcdede1de7cd5` by full SHA fails with `upload-pack: not our ref`.
No checkout, reset, source edit or ref update was performed in Jellyflow. The guessed native
handler/completion call was removed. A bundle, source archive or reachable ref containing the
actual prerequisite is required before its exact native graph binding can be implemented.
Selecting the published source would also break the pre-existing optional external-source
consumers under `jellyflow-trial`. This missing contract does not enter `dialogue-workflow`.

## Typed Graph Consumer

The same executable accepts `--graph` for two independent panels over one source-built
`ValueGraph` extension document; `--smoke-graph` runs its finite native trajectory and closes.
The graph owns Scalar/Vector constants, scalar Add and one Output, at most 64 nodes/128 links,
64 history snapshots and a 64 KiB JSON source. An incomplete graph displays no evaluated value.
Typed ports refuse mismatches, occupied inputs and cycles through the domain command owner.

Use Add controls to create nodes, choose an output port then an input, and select a Scalar node
to edit its value. Clicking a connected input without a pending output disconnects it. Focus a
node header and use arrows to move it or Delete to remove it. Tab/Enter/Space operate controls;
Escape cancels a pending connection. Zoom/pan and scalar drafts belong to each panel. A dirty
draft survives external source changes and needs explicit Reload. Save/Reopen use the existing
Editor file Host; reopening issues a fresh source incarnation and clears document history.

Observed control geometry and its document/revision publish only from the original submitted
Window frame. Automation waits for that displayed basis, rather than treating a domain result
or increased submission count as proof that the source is already displayed. Pending later
refresh work may coexist with the correct submitted frame. Input remains subject to the
original rendered handlers and exact captured-basis validation.

Drag a node header with the primary button to preview its position; matching release sends one
Move to the document owner. Escape, focus loss, pointer cancellation or a source change drops
the preview without history. Middle drag pans the local viewport, including outside its bounds;
ordinary wheel pans, and Ctrl/platform-wheel zooms around the pointer. Lines and pixel deltas
retain distinct units. Wheel during a middle drag rebases its pointer baseline so release does
not jump; wheel during a node drag cancels its uncommitted move. A wrong-button release cannot
finish either gesture. Zoom is bounded to 0.5–2 and pan to ±15000 logical pixels per axis.

`--smoke-graph-gestures` runs the separate continuous-input trajectory over the same original
Host. It is distinct from the ordinary create/link/edit/history graph workflow. Qualification
waits for submitted source, view transform and node preview rather than relying on source
revision alone. The graph consumes the existing GPUI capture and targeted wheel interfaces;
its gesture math is a finite consumer module, not another input system or generic UI runtime.

```text
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --lib --test graph_document --test-threads=1
cargo +1.95.0 build --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view --features ui-work-observation
```

After selecting the native backend and shared target as below, launch
`target/release/examples/gpui_view.exe --graph`. The ordinary `gpui_view` native integration
test includes a separate graph child alongside the existing complete Scene child. Work sampling
remains `--measure-ui-work`; the graph is not a new numeric cost baseline. Curved paths use
the public GPUI canvas and the same original Nara WGPU encoder/submission owner. This finite
consumer does not qualify a complete Shader Graph, asynchronous graph compiler, two independent
engine Views, native multi-window docking or game UI execution.

## Independent Graph Preview Views

The same executable accepts `--views` for two actual engine image Views over a finite graph
evaluation. `--smoke-views` runs relocation, independent retirement and late-completion
qualification; `--smoke-views-close-pending` closes normally while one original evaluation
Task is still held. Each View has a distinct Render producer, source generation, View ID,
320-by-180 target and frame stream. The original Nara Host owns both outputs and the one native
window submission. These are finite numeric color previews, not Scene object picking or a
shader compiler.

Click A to increment the first scalar or B to increment the second. Edit graph changes the
first scalar directly through the same document owner. Compile preview explicitly reevaluates
the current observation. Hold next compile and Release held compile exercise asynchronous
ordering without blocking a worker thread. Relocate A keeps its pixels but admits a fresh
display occurrence; Close A retires only its producer. Pulse B produces a new B frame through
its independent stream. Save/Reopen and dirty close use the original public Editor Host.

Captured work executes on the installed GPUI BackgroundExecutor. At most four original Task
handles remain in flight; newer compile demand waits for their physical return and then starts
automatically. Latest request and exact document/revision admission are separate from execution.
An old result after an edit, Reopen or close cannot replace last-good value or diagnostics.
Last-good images retain their original source label while pending or closed, and no stale
source grants domain input. Normal application shutdown releases held jobs and reclaims the
original Task results before closing the logical surface or GPUI application.

View input joins its immutable placement/output occurrence with the exact Host composite and
original accepted Window publication. A retained image in a live device epoch stays valid for
passive sampling after producer close; that validity does not restore the old input route.
This consumer uses one existing HostedSurface, so it changes no one-Surface-Slot-per-target
contract. Native multi-window docking, physical input, device reconstruction, numeric budgets
and the complete Editor/game choice keep separate acceptance requirements.

```text
cargo +1.95.0 build --locked --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --test gpui_view --run-ignored only --test-threads=1 --no-fail-fast
```

Set `WGPU_BACKEND=dx12` or `vulkan` and `NARA_WGPU_FORCE_FALLBACK=0`. Reuse the engine workspace's
target directory and serialize Cargo. Launch the prebuilt `gpui_view` executable without
`--smoke` to inspect the Scene View, controls and draft field interactively; `--smoke` runs the
bounded native workflow and closes. The child-process test never starts Cargo.

After building from the Nara root with its shared target directory, inspect the Windows example:

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
.\target\debug\examples\gpui_view.exe
```

The Nara adapter implements `HostedSurfaceRenderer` and records the GPUI Scene directly into
the original Host's acquired window attachment. It creates no additional UI window, device,
surface or submitter. The real `EditorProjectSession` produces the Scene preview; its immutable
output is imported without a pixel copy. Actual Host composite receipts and GPUI accepted-frame
publication jointly admit picking. UI recordings remain local until the exact window attempt
prefix completes, including refusal and panic paths.

Smoke assertions cover glyphs/buttons and actual Scene pixels, overlay input exclusion, Editor
selection, source translation and Undo, 4096-byte draft presentation, IME commit/cancellation,
last-submitted input while a successor waits, stale picking, paint refusal/panic recovery,
resize/ABA/zero extent, fractional logical-root scale and closing with a pending candidate while
another engine producer remains live. Qualification-only readback renders the same frozen Scene
to an offscreen target in the original window submission; it is not a swapchain readback, scanout
proof or steady-state performance path. Physical IME, physical DPI migration, full frontend/game
adoption, device loss and numeric costs retain separate acceptance requirements.

The same composition also paints actual Source and Cooked PNG content. A public recipe plugin
observes sealed stock captures during two managed headless runs. The Source imports a real sRGB
PNG; the Processed run consumes an admitted two-member Scene/Image catalog and explicit producer
expectation after the temporary source/cooked backing has been destroyed. Both Runtime owners
retire before those original inputs render through the same native Host. Extra public prepared
Image observations are dropped before returning the captures; the consumer uses no private Sprite
batch representation. Rendering the escaped inputs after retirement is the replay evidence.

The adapter obtains transfer/alpha from each exact retained output's `descriptor()`. It selects
`WgpuSceneColorSpace::LinearSrgb` explicitly and records directly to the Nara sRGB attachment;
the framework's default numeric-sRGB composition remains unchanged. The smoke verifies all four
alpha bands, a translucent UI overlay, clipping and fractional-root scaling for both content
modes. [ADR0092](../../../docs/architecture/adr/0092-sdr-color-space-alpha-and-output-encoding.md)
owns the linear working-space and associated-output contract. These SDR controls do not select
HDR display output, gamut mapping or a tone mapper.

The manifests share one fixed official WGPU Git revision containing the qualified surface-drop
and wasm-timeout corrections. Registry `30.0.1` does not contain those later fixes. An application
combining the reference Dear renderer must also patch its registry WGPU edge at its own root,
as described in the [render-output guide](../../../docs/guides/render-image-outputs.md).
This consumer enables stock UI alongside the Editor's render-only path to exercise additive
Cargo feature unification without requiring the Runtime UI product in the framework.

Source presence and compilation do not establish native acceptance. Exact runs, revisions,
failures and limits belong to the integration study and foundation plan.

The current native smoke also mounts a source-owner Hierarchy and two independently retained
Inspector projections. They consume one observation from the selection-independent entity query, while each draft,
IME/preedit state and conflict lifetime remains panel-local. A selected X edit is prepared and
committed through the original Editor basis/history owner; a stale second draft refuses until
Reload, and the original Undo restores the source and both clean projections. Scene preview
updates use a separate cached View, so the native output records isolated Hierarchy reuse rather
than counting a parent refresh as local invalidation. The caller-owned render counters are
diagnostic only; they do not qualify layout, upload, GPU or latency budgets. The exact native
qualification and public CPU fixture results remain in the foundation plan. Further Undo may
remove the Sprite itself: the panels show an absent property and keep the Viewer alive for Redo.

The Hierarchy now uses GPUI's original variable-height `ListState`. Its temporary source contains
10,000 real empty declarations plus the constructed Sprite. The Sprite is pinned first as a
fixture display policy; other declarations retain source order. Public entity-query pages are
consumed completely, while the list builds only its visible/overdraw rows. The local Filter field
applies committed text explicitly, and Details changes row extents through original list
remeasurement. Surviving row keys retain their scroll offset across filtering; an absent anchor
falls back to the first row with zero offset. This is a finite consumer, not a generic tree editor.

The full native smoke scrolls near the end, filters and clears the source, changes row heights,
selects an exact virtual row and restores the Sprite before the original recovery/close path.
Injected Winit PixelDelta/LineDelta events exercise the ordinary wheel adapter; at logical scale
1.25, 6.25 physical delta pixels become five logical pixels. Actual monitor DPI migration and
physical wheel/trackpad behavior remain unqualified. `NARA_GPUI_COLLECTION` separates query pages/
records, filtering/reconciliation and row callbacks at its named checkpoint. Those counts omit
allocation, equality/string comparisons, framework layout and GPU work; they are not a total
O(visible) or latency claim. Source snapshots, optional filter indices and ListState retain
O(logical rows) metadata; old frame/resource leases can outlive a replacement.

## Native UI Work Measurements

The optional `ui-work-observation` feature enables the existing GPUI profiler and the native
`--measure-ui-work` mode. It uses the same original Nara Host, renderer and public source-owner
workflow. Each of the following five cases runs eight warmups followed by 64 recorded samples:

| Case | Work observed |
| --- | --- |
| `equal_observation` | Query and publish an equal source observation without a preview refresh. |
| `static_panels_live_preview` | Refresh the Scene preview while unchanged panels remain retained. |
| `local_property_edit` | Alternate the Sprite X property through the original source/history owner and publish its projections. |
| `list_height_change` | Toggle Details through the original variable-height list remeasurement path. |
| `property_edit_broad_refresh` | Perform the same property edit with an explicit broad UI refresh for comparison. |

From the Nara root on Windows, prebuild the release example and run only the ignored measurement
test with the matching feature. Use `vulkan` instead of `dx12` for an actual Vulkan adapter; keep
`NARA_WGPU_FORCE_FALLBACK=0`. Run Cargo commands serially and reuse the shared target directory:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path $PWD.Path 'target'
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
$env:NARA_GPUI_VIEW_EXE = Join-Path $env:CARGO_TARGET_DIR 'release/examples/gpui_view.exe'
cargo +1.95.0 build --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view --features ui-work-observation
cargo +1.95.0 nextest run --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --test gpui_view --features ui-work-observation --run-ignored only -E 'test(measure_hosted_gpui_work)' --test-threads=1
```

For a separate allocation observation, keep the same environment and prebuild again with
`ui-work-allocations`, which includes `ui-work-observation` and installs the existing
`open_gpui_bench_metrics::CountingAllocator` in the example. Run this build/test pair separately
from the CPU-only observation because allocation instrumentation changes its cost:

```powershell
cargo +1.95.0 build --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view --features ui-work-allocations
cargo +1.95.0 nextest run --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --test gpui_view --features ui-work-allocations --run-ignored only -E 'test(measure_hosted_gpui_work)' --test-threads=1
```

The test launches only the prebuilt example with `--measure-ui-work`; the child never starts
Cargo. It allows at most 330 seconds, kills and reaps an overdue child, checks each output log
against a 256 KiB limit, and retains the logs under the temporary `nara-gpui-view-*` directory.
It requires the eight-warmup/64-sample configuration, a `NARA_GPUI_WORK_COUNTS` record with
`samples=64` and a `NARA_GPUI_WORK_CPU` record with `phase=query_snapshot samples=64` for every
named case, and the exact final marker:

```text
NARA_GPUI_WORK_OK cases=5 retained_native_frames=0 diagnostics=0 original_host=true
```

The real example checks draw/record/submission counts, unchanged-panel reuse and final native
frame retirement. The test checks these completed records without timing thresholds. The
measurement mode performs no diagnostic pixel readback; the existing `--smoke` workflow retains
its separate visual and recovery assertions.

`NARA_GPUI_WORK_CPU` reports instrumented CPU-side elapsed P50, P95 and maximum observations.
The existing `mutation`, `query_snapshot`, `publish`, `preview` and `settle` phases sit inside
`owner_sample`. GPUI phases and renderer recording are nested within that work, so their totals
and percentiles are nonadditive. In particular, `gpui_request_layout` includes View description
construction and layout requests; `gpui_prepaint` includes layout computation and prepaint.
`recorder_cpu_nested_in_settle` measures CPU recording within the existing settling work. These
observations do not measure GPU execution, display scanout or end-to-end input latency.
The mutation interval also includes the original `drive_editor_frame(Duration::ZERO)` after
dispatch; it does not isolate preparation, commit or Editor model advancement. The Scene preview
case refreshes static source output and does not advance a Game simulation.
The attribution successor additionally records `source_selection`, `source_prepare`,
`source_commit` and `source_drive` inside the two property-edit cases. These are nested scopes
around the same basis/selection validation, original prepared edit, action dispatch and original
Editor drive. They preserve its checks, preview/model work and ordering. Their CPU observations
and optional allocation records are subdivisions of `mutation`; do not add them to the parent
or add their percentiles together.
The same optional mode forwards Nara's `editor-work-observation` feature and installs a standard
thread-scoped `tracing-subscriber` Registry layer filtered to `nara::editor::work` and
`nara::scene::work`. Twenty-five original owner spans distinguish preview compilation, preview
drive/Runtime/materialization, Editor model publication, Scene expansion/structural closure,
values/provenance output, Authoring completion, Native restriction/flat admission, unbound
preparation, binding/revalidation and final validation/publication. Further materialization
subdivisions observe value admission/decoding, predecessor admission, retirement hierarchy,
identity preparation/adoption/registration, final hierarchy and identity-epoch publication.
The engine installs no subscriber and the feature is disabled by default.
These elapsed spans are inclusive/nested, with subscriber overhead; their per-sample call
checks do not establish worker-thread work or exclusive CPU time. The probe requires one reached
span of each kind per original property edit, except two predecessor/identity-preparation
calls and four retirement-hierarchy calls. Repeated intervals are summed within each sample;
their reported percentiles describe that sum, not individual invocation latency.
Allocation requests remain observed at the four outer source steps, without invented per-span
allocation attribution.
These call expectations apply to this flat source without Prefab instances; a structural
closure invocation can perform multiple internal layouts, and other sources can require retries.

Allocation records count calls and requested bytes on the calling thread for the named owner
phases. The optional `NARA_GPUI_WORK_HEAP` snapshot reports process allocator-requested live
bytes, including trace/sample storage; it is not RSS, total native heap usage or GPU memory.
Retained native frame counts do not establish a total memory bound. Profiler and optional
allocator overhead remain part of the instrumented run.
The frame occupancy maximum covers sample-end checkpoints, not transient peak retention.

The source has 10,001 logical rows: 10,000 empty declarations and the Sprite. Each
`query_snapshot` still consumes the complete public entity query and constructs its snapshot in
O(N) work, including equal observations. Visible-row construction and retained-panel reuse do
not make the complete observation O(visible), and these finite samples do not establish a
general latency or total heap budget.

# Dear ImGui frontend parity progress

Status: **U7 candidate committed at `26be74f`; production selection and U8 removal in progress.**

The current source migration is tracked in [U8 progress](u8-migration-progress.md).
The candidate evidence below remains tied to the recorded versions and commits; it does
not certify the still-integrating removal or its external acceptance.

This record supplements the historical [U6 adoption evidence](adoption-evidence.md).
It is not a replacement acceptance contract. The [active plan](../../plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md)
and [ADR 0118](../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
continue to govern the complete migration and removal of the old frontend.

## Implemented migration surface

The candidate frontend now paints registered semantic panel instances through source-built
companions. Built-in hierarchy, Browser, Inspector, Authoring, Game and diagnostics use the
same registration. Each instance owns its widget drafts. Project management and document
controls emit existing application actions; opening ordinary data does not execute project code.

Scene and UI Canvas presenters join real Nara image submissions to actual window presentation
receipts. Scene mapping changes renew displayed picking authority without reaccepting stale
content. A UI Canvas rerender of unchanged source retains its compiled geometry generation.
Neither panel contributors nor tooling receive GPU or mutable Session access.

Two additional native regressions are being qualified. Supervised Play reuses the existing
semantic canary and guarded reference-game copy while painting the actual Dear ImGui Host.
Its first run (`1de2db18-5784-463a-b437-a125d45e9529`) selected collect-game, which lacks the
canary's hard-coded `player-weapon` probe entity; that fixture mismatch is not a frontend defect.
The corrected reference-game run passes below; no product owner rule changed for this fixture error.

The separate Canvas workflow drives actual widgets through test-only Dear IO after Winit's native
poll and before `begin_frame`. It uses actual displayed image/overlay rectangles and retains the
presented-only picking gate. This route neither injects OS events nor establishes system IME/input
routing. Its Windows/DX12 run below demonstrates hidden-window API presentation, not monitor
scanout. The fixture keeps docking but disables secondary viewport creation so the synthetic
workflow cannot activate a new secondary native window; multi-window qualification remains separate.

## Verified checkpoints

All Cargo work is serialized with Rust 1.95.0, one build job and the shared repository target.

| Scope | Evidence | Limit |
|---|---|---|
| Editor library | `dfa1d0b5-05c0-440c-8cff-bf4cb69e4b69`: 115 tests pass | Before the subsequent popup and continuous-field regressions |
| Integrated Editor library | `d81ababc-51a8-4060-b680-c9fbea661482`: 121 tests pass | Includes cancellation and docking; not full native parity |
| Expanded Editor library | `8c565959-d3a4-40ef-b17a-56736b35b3ee`: 129 tests pass | Includes actual Scene/Prefab/UI widgets, Manager/Recovery/dirty-close flow, result feedback, layout restoration and finite navigation repaint; native parity remains separate |
| Shared Runtime owners | `00454f39-acd8-49ec-8369-be64830bf2cc`: 204 tests pass, one native test skipped | Root library with all features; separate native scope remains required |
| Application, generic composition/recovery and workspace | `b04ce1c8-7a0a-478a-a42c-125553537655`: 24 tests across five binaries pass | Existing semantic owners and ordinary Scene/Prefab preservation |
| Independent ImGui panel companion | `3b497501-48eb-424e-9091-09f23d56654c`: compiled external workspace consumer observes selection and dispatches an existing action without changing history | Expanded close/reopen evidence follows below |
| Independent panel lifetime | `b6030d98-5f1a-488b-a6d7-289e4b2ed513`: same instance closes, paints closed, reopens and dispatches through its companion | Dirty document, nonempty history and selection remain owner-controlled; headless real Context |
| Docking and cancellation | `f41f3fe0-19c0-4ca6-b63a-61c8142269d1`: six real owner/Context regressions pass | Independent central leaves, trusted contribution placement, INI restoration/reset, Escape, local draft retirement and Inspector gesture ownership |
| Canvas release endpoint | Red `3ee147fe-426f-43ac-9348-b513ff36c518`; green in the 115-test run | Freezes the pointer on release while a replacement preview awaits presentation |
| First popup display | Red `34dc1ba7-713d-4d75-9fae-8296d996d15f`; green `b44e2ed4-9745-47c0-a6c6-c9df098408f0` | Real Context mouse input, hidden sizing frame, actual popup draw list and finite return to idle |
| Scene and ordinary Prefab widget workflow | `0d75f580-5111-495a-89a8-13b3004123f9`: actual frontend mouse/keyboard controls create an entity, add a custom scalar component, edit to 42, Undo/Redo, Save and Reopen for both document kinds | Real Dear Context and parsed saved source preserve unavailable records; this is not native GPU/window acceptance. The separate status test in this run failed during fixture cleanup |
| Workspace result dismissal | `4e1244ef-2430-4f8e-a342-d8b53ac1fbe1`: a real mouse click acknowledges a cancelled close, permitting another close request while retaining dirty source | The earlier test-cleanup error is fixed by cancelling its second pending CloseDocument before requesting Exit; no application-owner rule was relaxed |
| Specialized UI Prefab widgets | First pass `133cb8b9-9dd8-401e-8fe7-42563372490e`, repeated in the 129-test run | Trusted real Context scalar-drag Preview/Escape cancellation, visibility Undo/Redo, image choice, node creation/deletion Undo/Redo, Save and Reopen; parsed typed source agrees. No native Canvas resize claim follows |
| Manager and project close widgets | `a7dba96b-8990-485b-8d6a-525a33b5fc6f` passes this workflow; repeated in the 129-test run | Actual path/profile entry, initial-open Cancel, corrupt-bundle Recovery, repaired Retry, replacement Cancel, dirty Close Cancel/Discard and unchanged disk source; the separate navigation test was red in this run |
| Layout and Play feedback regressions | Red `133cb8b9-9dd8-401e-8fe7-42563372490e`; green in the 129-test run | Restore recovers saved semantic instances/visibility with INI; real owner rejects dirty external Play and actual rendered text explains the rejection |
| Dirty-close progress projection | Red `1004dc8c-dc26-475f-8907-864f8b5a40ee`; green in the 129-test run | Real AwaitingDecision workflow plus a frozen public Saving projection proves Cancel is no longer offered after decisions close. The generic fixture completes file persistence within one drive, so this is not a claim of observing an intermediate native Saving frame |
| Keyboard navigation redraw | Red `a7dba96b-8990-485b-8d6a-525a33b5fc6f`; green in the 129-test run | First Tab after mouse focus is characterized as restoring the navigation cursor; the second Tab applies focus on finite followup frames without requiring key release, then becomes idle. Earlier failures assuming the first Tab immediately changes the item are fixture errors, not product-defect evidence |
| Updated native multi-view lifecycle | Windows/DX12 SHA256 `4c6459484dbb7a33df00dcf753453fca0e66d87e7a27683d5abe3154124804c6`; `target/u7-lifecycle-p_gfewzl/native.log`; exit 0 | Four exact frames in each primary, secondary, primary-minimized, returned, Game-closed, Scene-closed, reopened and device-recovered phase; stale image receipt rejection and ordered shutdown pass. First present is 17ms after GPU initialization. This bounded automatic trajectory sends no global keyboard/mouse input and does not certify ordinary authoring or IME |
| Pending Canvas GPU submission | Red `8021ff30-ed39-4d84-aa52-96375e72866d`; green `ebec908a-90c7-46b8-b31a-93e29e03f51e` | Explicit Windows/DX12 ignored native test uses a hidden owned window and actual Nara GPU. Four owner drives retain one exact pending image receipt; no surface is acquired or presented, and source picking remains unavailable. Normal GPU and Session cleanup precede assertions |
| Native supervised external Play | `0caeab8c-779b-4f62-a1f1-146c5a97c6b7`: Windows/DX12, 464.320s; `target/u7-imgui-supervised-play-nextest.log` | Real guarded reference-game compilation, READY/Stop, injected compiler failure, explicit Run Last Good at generation 1, repaired saved revision 2 and successor generation 2, READY/Stop and complete physical cleanup. The actual Dear ImGui Host presents three Manager frames and 27,529 workspace frames. Actions come from the existing semantic canary, not physical widget input; duration includes first child builds and is not an iteration benchmark |
| Native supervised Play through actual buttons | `8b285cde-741a-47f8-a9b4-9b83b7c2f262`: Windows/DX12, 49.924s; `target/u7-external-play-buttons-final.log` | Actual visible Play/Stop/Run Last Good widgets receive local synthetic Dear IO after native polling. Real child compilation and READY, failed build, generation-1 last-good execution, repaired revision 2/generation 2, Stop and physical cleanup pass. The primary presents 2,903 frames, including three Manager frames. Document edits and saves still use the existing semantic canary; no global OS input is injected. This cached repeat is not an iteration benchmark |
| Native Canvas widget workflow | `64e0af21-f108-4b1f-b952-fd0c662c05b4` first passes; primary-only fixture rerun `fef0d1a7-d826-4499-8690-c6e94202bd9e` passes in 3.020s; `target/u7-imgui-canvas-workflow-nextest.log` | Real Windows/DX12 hidden-window sampling grants source picking, then actual Canvas/toolbar widgets move, resize, Undo/Redo, Save and Reopen. Typed source agrees; history depths are 0/1/2/1/2; unrelated root record and startup Scene bytes remain unchanged. Input is synthetic Dear IO after native polling, with no global OS input. Graphics and normal Session cleanup precede behavior assertions |
| Native-workflow integration checkpoint | `17388fbc-19cf-4a32-b611-70b2be95a0de`: 129 ordinary library tests pass, three native tests skipped and qualified separately; workspace all-feature/all-target locked check session `18653` exits 0 in 38.78s | Includes production compilation without the test-only input seam. Existing warnings remain. No owned Cargo or native process remains after this checkpoint; U7 full review and remaining product parity are still open |
| Ordinary native Scene and Prefab controls | `ac00b25f-195a-44c9-8aa6-1033b0c4c539`: Windows/DX12 authoring trajectory passes in 5.343s | Real hidden primary GPU presentation with local synthetic IO creates an entity, adds the custom scalar, edits to 42, Undo/Redo, removes/restores the component, saves and reopens both document kinds. Typed saved data, history, dirty state, every original entity and the complete unavailable manifest agree. Fixture Rust still contains its deliberate compile error, proving ordinary Open did not require project compilation. The separate panel-instance regression in this run is intentionally red |
| Existing panel instance restoration | Red `ac00b25f-195a-44c9-8aa6-1033b0c4c539`; green `028d78a5-7beb-4b1f-b118-651f79f11b1e` and `ac0c658a-8fcf-4720-af5d-4de9faf374d8` | Real Context checkbox restores the same closed Source Inspector, preserving target and instance count and leaving another Inspector unchanged. The ordinary Workspace menu now calls the same instance controls |
| Embedded Game display parity | Three regressions red in `028d78a5-7beb-4b1f-b118-651f79f11b1e`, green in `ac0c658a-8fcf-4720-af5d-4de9faf374d8` | Actual checkbox selects 384x192 physical pixels for a 128x64 image at DPI 1.5, retains canvas resolution, and scopes nearest/image/linear commands. Rendered stale/paused text and capture-overlay geometry pass. Test marker callbacks prove command generation, not GPU execution |
| Expanded native UI workflow | `ac0c658a-8fcf-4720-af5d-4de9faf374d8`: Windows/DX12, 4.751s | Extends the Canvas trajectory with actual visible field scrolling, scalar Preview/Escape cancellation without history/source writes, bool Undo/Redo, image popup choice, create/delete Undo/Redo/restore and Save/Reopen. Every original record differs only in the exact admitted fields/child insertion; new image path and unrelated Scene bytes agree. Local synthetic Dear IO and real hidden-window GPU presentation, with complete cleanup |
| Native Manager/Recovery/replacement/close | `ac0c658a-8fcf-4720-af5d-4de9faf374d8`: Windows/DX12, 4.373s | Actual controls cover Opening Cancel, corrupt-bundle Recovery, repaired Retry, replacement Cancel/rejection, dirty Close Cancel/Discard/Save and original disk-record preservation. The prior `028d78a5` failure clicked an offscreen Retry; the corrected fixture scrolls its actual clip into view instead of changing the owner or issuing a semantic Retry |
| Post-delta workflow and compile checks | `eabc5b7c-59e8-42e2-92ea-7d81b22846ff`: five key widget/layout/navigation tests pass; workspace all-feature/all-target check `96377` and Clippy `73445` pass; affected Editor all-target Clippy `53319` passes | Existing warnings remain, so lint is not claimed clean. Final `cargo +1.95.0 fmt --all -- --check` and `git diff --check` pass. This checkpoint is uncommitted U7 and does not provide its final full code-review receipt |
| Ordinary generic Scene | Windows/DX12 binary SHA256 `d32d8898b45e0e90d66721d1c68b3880c346429bd1c6c027921c9326512324eb`; copied fixture `target/u7-native-26mlnym4` | Native creation, Name addition, Chinese clipboard edit and Save; parsed saved source preserves the complete unrelated `signal-node`, including its unavailable record |
| Ordinary Browser/UI/Import | Windows/DX12 binary SHA256 `25ac9a738195183dc320b6a545710aa1455f21f4bb514051cd6232ad769fa752`; copied fixture `target/u7-native-x_kw7emh`; exit 0 | Filter, UI Prefab Open, exact Canvas picking and overlay, Back to Scene, recipe popup, Linear Reimport, corrupt-PNG failure retaining the prior 1x1 Linear last-good image |
| Trusted Canvas movement | Binary SHA256 `59a1884290636580f43925e4c11dfef6f6dba7c8f62de8a19ff9862563491d3d7`; copied reference project `target/u7-native-9n85huzj` | Native drag moves `wave-status` from `[24,68]` to `[72.68852949142456,109.31147623062134]`; Save affects only that entity. Undo visibly restores its original placement. Subsequent controller focus/coordinate ambiguity and forced cleanup exclude this run from full Undo/Redo/Reopen or clean-shutdown acceptance |

Native images, copied fixtures and controller logs under `target/` are local observations,
not committed-source external acceptance. No result for one executable hash certifies later
source changes. The image-import run uses the ordinary data-only authority; continuous and
structural UI edits there remain read-only by the existing document owner.

## Defects found by real consumers

- The Host originally stopped repainting after a combo's hidden content-measurement frame.
  It now requests finite followup frames for active windows with pending autosizing. Inactive
  dock tabs and ordinary hidden windows do not cause continuous redraw.
- Restoring a cursor after a full Canvas input overlay must submit an item. The original UI
  Canvas path hit Dear ImGui's boundary assertion on first Open; the rebuilt native run succeeds
  after submitting `Dummy`, as the Scene path already does.
- The initial toolbar width and fixed height could detach it accidentally or clip Save controls
  on the tested DPI. Initial placement is now in the primary viewport and content determines its
  bounded height. Persistence success text no longer dumps internal digests into this toolbar.
- Independent review found Escape-after-deactivation can commit a continuous field, and cancelled
  fields can retain local drafts when switching Inspector mode. Red run
  `b73ed5b2-1c0a-4e96-89da-443d5ab2daa5` reaches both behavior assertions; corrected code passes
  `f41f3fe0-19c0-4ca6-b63a-61c8142269d1`, including a real trusted gesture and observer Inspector.
- Default windows previously overlapped. A private declarative recipe now uses each registered
  instance's semantic placement, tabs peripheral panels and gives each central view a separate
  leaf. It uses the existing Dear ImGui builder, preserves INI with `IfMissing` and replaces only
  on explicit reset. Windows native screenshot `target/u7-native-ppvp1vs4/opened.png` at SHA256
  `b6a86f8c2eb89ae5c037c4d31125a047d57b8fe28c82c470ee0307a46d1b79d0` confirms actual initial
  docking. The subsequent controller could not obtain foreground ownership, so it certifies no
  Play interaction. Global mouse injection was stopped instead of sending input to another app.
- The port omitted project-content refresh remediation and workspace-result acknowledgement.
  Both now emit their existing actions from Diagnostics. Workspace dismissal has real Context
  mouse coverage above; their final native click trajectory is still pending. No new owner
  validation or Runtime-edit product feature was added.
- Restore previously loaded only toolkit INI, leaving closed panels closed and retaining newly
  added instances. It now restores the same saved semantic layout and INI, reapplies admitted
  primary bounds and retires panel-local drafts. GPU replacement retains this saved snapshot
  independently from the current live INI.
- External Play returned before displaying its operation result, making a dirty-Scene rejection
  appear inert. Both Play placements now share the existing typed result presentation. Ordinary
  Diagnostics also retain their stable diagnostic codes.
- The unsaved-changes window offered decisions in Saving/RetiringRuntime as well as
  AwaitingDecision. Only AwaitingDecision now offers Save/Discard/Cancel; progress phases display
  progress, and Save uses the same persistence retry eligibility as the document toolbar.
- Keyboard navigation also needs finite frames to apply a scored result. The Host follows pending
  navigation work without treating a merely active navigation window as continuous activity.
- A submitted Canvas awaiting sampling was treated as missing, so empty candidate takes requested
  another render and the third owner drive submitted identical content again. The native GPU
  regression observes the duplicate transaction before the fix. Submission eligibility now
  recognizes a matching pending image; displayed-frame and picking eligibility still require
  an actual presentation receipt.
- A parity audit found that the Workspace menu offered new panels by kind without restoring an
  existing closed multiple instance. The new real-Context checkbox regression is red in
  `ac00b25f-195a-44c9-8aa6-1033b0c4c539`; the verified fix uses the existing instance's `reopen`
  operation instead of creating or retargeting it.
- The same audit identified missing embedded Game integer scaling, stale-source/paused feedback
  and the captured-input Escape hint. Their implementation and regressions pass above.
  Integer scaling uses the existing physical `DisplayMapping2d` and standard nearest/linear
  draw callbacks, without a new per-texture sampler API or GPU ownership boundary.
- Actual external Play button input exposed a shared frontend gate defect. Ordinary authoring
  projects publish `project_content = NotInstalled`, while external execution admits saved source
  through its own receipt/child READY path. Both frontends incorrectly required the embedded
  content's `is_current()` before enabling Play. Native button regression `fed0202b-b209-4328-aac2-782e25131562`
  fails; diagnostic rerun `213f2fb5-9051-450f-97ca-d8eeab8dbc3c` confirms a visible button, clean
  document, idle persistence and `can_request = true`, with `NotInstalled` as the false gate.
  The UI now requires clean documents for external Play; embedded content checks and the owner's
  saved-source admission remain unchanged. Real button/child-process verification passes in
  `8b285cde-741a-47f8-a9b4-9b83b7c2f262` above. The intermediate `54a8c305` run reached first
  READY/Stop before its fixture looked for a successor Play widget using the prior painted
  Stopping frame; the fixture now paints the new state before locating and clicking it.

These frontend defects belong to Nara's integration. The separately reproduced
`dear-imgui-winit` 0.20.0 shutdown borrow conflict is a backend defect, documented in
[U6 evidence](adoption-evidence.md#confirmed-upstream-shutdown-defect). The
[local WGPU patch](../../../vendor/dear-imgui-wgpu/NARA_PATCH.md) supplies missing presentation
observations; it does not establish an upstream rendering defect. The Vulkan implicit-layer
failure also reproduces without any Dear Context and must not be attributed to ImGui.

## Remaining parity and delivery gates

The frozen 52-file candidate received the actual nine-persona review
`20261017-123342-26bf5b2a` (base `0ee59e9`, diff SHA256
`26f842f54418f132584594a386d3ee2c30f3394a0395f10e9003f6396218541d`). Its historical
verdict is **Ready with fixes**, not approval of subsequent fixes, default selection or U8.
The review produced three P2 findings and one bounded structural-widget coverage gap.

- Scene content invalidation (#2) has a real native red-to-green trajectory: `1d0ec3da`
  retains an old Scene image after successful image publication; `2c4f4f90-667c-42a4-b35e-cb7bba615af4`
  passes after the owner issues a content stamp and the presenter compares each candidate.
  The stamp includes the existing render producer, document/revision, extent and successful
  image-publication revision. Ordinary unchanged drives reuse the submitted image; geometry
  revisions keep their original per-drive meaning. CPU run `78bc5c07-39c8-4fe8-b03c-744854eea980`
  passes three tests covering reuse, real publication, rejected stale publication, replacement
  producers and publication overflow before asset-store mutation. Native evidence observes
  prepared pixel values and actual composite receipts, not GPU readback or scanout.
- Canvas redisplay authority (#3) is revoked when the current window has no matching receipt,
  while retaining the submitted GPU image for reuse. Widget input additionally requires the
  confirmed rectangle, viewport/window and density; post-presentation admission filters edits
  if the current composite was not confirmed. Native regression `41656426` is red before the
  receipt fix. The focused seven-test Canvas/Game run in
  `target/u7-canvas-mapping-and-game-regressions.log` passes, including the unchanged native
  move/resize/save/reopen proof. Real-Context mapping/release and retained-cancellation tests
  pass in `5901f4b2-96d8-4180-9dec-497d8503e189`. Follow-up review identified that an already
  emitted Commit also needs cancellation if its release frame is deferred; that final delta
  passes the separate real-widget admission proof `e83e3857-5f05-48b3-9025-428ba2580f90`.
  Its actual Canvas widgets produce Begin, Preview and release Commit; an unconfirmed
  release restores the baseline without history/disk writes, while a confirmed release
  commits exactly once. This is a CPU admission-contract proof, not native surface deferral.
- Render errors now retain the already-painted output long enough to reject its Canvas
  actions through the same cancellation path. Only Canvas rollback may reach the owner;
  unrelated unpresented actions are discarded, and original Game/error completion continues.
  The real-widget failed-frame contract passes in `15776fe1` and `9acbfb0c`, preserving the
  selection, baseline, history and disk source. This tests the CPU error-admission contract;
  it does not inject a physical device failure on the release frame.
- The Canvas reopen-wake finding (#1) needs the actual menu's frame ordering. Three native
  idle-policy runs (`cb11ba9d`, `6fd3a6f4`, `3a50edae`) finish with frame counts `[3,1,3]` for
  initial/closed/reopened phases when visibility changes before the frame. They did not cover
  Workspace changing visibility during painting, after the Host's submission decision.
  The additive actual-menu test reproduces this distinction in `15776fe1`: six local input
  frames open the menu and click its existing checkbox, one scheduled frame then reaches
  idle without presenting the Canvas. No Escape, unrelated event or unconditional extra frame
  follows release. The Host now carries the Ready Canvas's empty-take rebuild request into
  one finite followup. Matching pending/submitted images and unavailable/rejected content do
  not request that work. The unchanged test passes in `9acbfb0c-07f1-4ec7-995b-540b8ff4fab3`:
  six input frames, three deadline frames, presented Canvas, closed menu and unchanged source.
  Its eight-test batch also preserves the prior idle controls, pending-image reuse and the
  original native Canvas move/resize/save/reopen workflow.
- The structural-widget gap now has an additive real-Context test passing in
  `fd9db735-3332-41fd-a3a1-f6ad2f411790`. Actual Move subtree and Placement controls reparent
  a complete subtree and change its sibling anchor, then verify single transactions,
  Undo/Redo, Save/Reopen and complete unrelated-record preservation. Existing proofs and
  their helpers remain intact.

The new ordinary Scene test also exposed a separate Nara density-adaptation defect. In pinned
Dear ImGui 0.20.0, a viewport framebuffer scale of zero means to use
`IO.DisplayFramebufferScale` (`imgui.cpp` draw-data initialization and font-density rules).
Nara incorrectly interpreted it as zero physical size. Scene, Canvas and Game now use the
upstream fallback rule; the native Scene test above exercises this actual single-window case.
This is not an upstream layout or rasterization defect.

The same review coordinator independently inspected the final fixes in a bounded follow-up,
without repeating the nine-persona review or changing its historical receipt.
`20261017-123342-26bf5b2a/followup/closure.json` records all three findings closed, including
the additional Commit-drop and render-error rollback paths, with no remaining confirmed
finding in that scope. It records the inspected source hashes and the exact menu red/green
distinction. This is a same-serving-family Codex review, not a cross-model review.

The earlier integrated Editor library run
`57e18faf-6846-42af-9cb5-c92b202953ae` passes **147/147**, zero skipped, in 126.012s on
Windows/DX12. It includes all ignored native workflows: ordinary Scene/Prefab controls,
idle Canvas reopening, unconfirmed redisplay, Canvas move/resize/save/reopen, Manager recovery,
Scene image import/reimport and real supervised Play buttons with child-process retirement.
Root owner run `9354e63c-632e-47e1-bd6d-9ad62829480d` passes **207 tests**, with its separate
native test skipped.

Final Editor library run `732c1985-8bdf-4cfd-b5f0-a17ffa278717` passes **149/149**, zero skipped,
in 122.267s after the menu-wake and render-error corrections. Its nine ignored native tests
all execute; supervised Play takes 68.947s with actual child compilation and retirement.
This certifies the local candidate checkpoint, not default selection or the U8 clean
committed-source external workflow.

Final `cargo fmt --all -- --check` and affected Editor all-feature/all-target Clippy pass;
the latter finishes in 38.75s and retains warnings. The preceding workspace all-feature/
all-target locked check passes in 3m08s, and the root's no-default
`presentation-2d,tooling,serde` check passes in 2m08s without `runtime-ui`.
No warning-clean or unmeasured-platform claim follows from these checks.

- Repeat the verified native Scene/Prefab control trajectory after final integration changes.
- Repeat the verified complete native UI Canvas/field/image/structure workflow after final changes.
- Repeat verified native Manager/Recovery/replacement/dirty-close controls after final changes.
- Retain the verified supervised external Play button/child-process trajectory after final changes.
  Repeat concurrent embedded Game/input on the completed frontend.
- Verify final panel close/reopen/layout behavior, text and clipboard routing, and honest IME,
  accessibility and platform capability statements.
- Run affected owner suites and compatible feature checks, perform simplification and an actual
  independent U7 review, then select the production frontend only after parity passes.
- U8 still requires clean committed-source external Editor/Play/Check/development Export,
  dependency consumers and removal of egui, qualification paths and obsolete aliases.

The read-only U8 dependency inventory identifies two migrations that must precede deletion:
`EditorGamePresentedStamp` moves from the old `platform/viewport.rs` into neutral Game input,
and the new native Play proof's guarded `SupervisedPlayCanaryHost` fixture moves into private
test support. Neither requires retaining the old production Host or canary entry points.
Existing application, authoring-loop and first-game tests keep their owner/disk/process
assertions while their widget driver changes. The root facade removes `tooling-egui` without
adding a GUI replacement feature; the independent panel consumer uses its Dear companion as
the ordinary path. Final acceptance uses a clean archive of the implementation commit, a
separate sibling game directory and the existing `first_game_workflow`/Check/Export workflow.

Only Windows native observations are recorded here. Linux/X11 and macOS remain unverified;
Wayland detached windows retain the backend's explicit unsupported capability. The U6 Vulkan
implicit-layer limitation and upstream Winit shutdown defect remain separately documented.

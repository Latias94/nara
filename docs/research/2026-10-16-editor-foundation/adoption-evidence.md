# Dear ImGui adoption evidence

Research date: 2026-10-16. Qualification decision: 2026-10-17.
Status: **U6 Windows adoption gate passed for the bounded integration slice**.

The production Editor still uses egui. U7 and U8 have not started. This record follows
ADR 0118 and the active Editor Foundation plan. The decision admits full frontend implementation;
it does not establish ordinary-editor feature parity or external committed-source acceptance.

The final fully exercised profile is Windows 11 / DX12. Vulkan interaction has additional native
evidence, with a separately isolated implicit-layer teardown limitation described below. Native
Linux/X11, macOS and full accessibility remain unqualified. No global layer settings or automatic
production filter were changed. See the
[U6 verification record](../../knowledge/engineering/verification/2026-10/2026-10-16T163933Z-editor-u6-qualifies-direct-dear-imgui-on-windows-with-explicit-backend-and-cleanup-limits-e42a0dbfec6643e380f6e9275f29b0d2.md).

## Selected integration

- Rust 1.95, WGPU 30.0.1, Winit 0.30.13 and Dear ImGui Rust 0.20.0.
- Direct Winit/WGPU backends under the Nara Host; no `dear-app`, Bevy renderer or second device.
- A bounded Inspector, hierarchy, diagnostics and real Scene/Game images use the existing
  application, revision-bound actions, document history and Runtime completion owners.
- The released WGPU report exposed only viewport ID sets. The local, opt-in
  [presentation observability patch](renderer-020-patch.md) retains exact
  sampled `TextureView` handles and backend-constructed secondary present records. Nara joins
  them synchronously to the latest successful image submission on the same device and queue.
  The delegated route continues to submit and present secondary surfaces once; Nara owns the
  primary surface transaction. The patch is not an unconditional upstream adoption claim.

The patch starts from the released registry source at upstream tag commit
`af2942c78651b280d18af621de55b5462bb1e10a`; its MIT and Apache-2.0 texts are retained.
See [renderer contract](u6-renderer-contract.md) and [text baseline](u6-text-baseline.md).

## Evidence so far

| Gate | Observed result | Remaining work |
| --- | --- | --- |
| Dependency cohort and MSRV | Direct 0.20 dependencies and private bridge compile with Rust 1.95 | Final integrated feature/consumer matrix |
| Backend sample observations | 100 vendor library/integration tests pass, including actual GPU Elements, clipped/zero commands and texture replacement | Native replacement and device recovery |
| Concurrent Scene/Game images | Windows native qualification obtains 4 exact frames in primary | Input and full authoring regression trajectory |
| Native detachment and return | Exact secondary/minimized/returned frames plus real mouse docking, detachment, redocking, secondary resize/close/reopen across native DPI 144 and 120 | Final integrated regression and independent review |
| Repeated image writes | Native run rejects the overwritten image receipt before invoking paint | Final integrated regression rerun |
| Ordered normal shutdown and recovery | Real `device.destroy()` pauses Play, renews the entire graphics namespace on the same primary window, republishes paused images, resumes and stops; normal shutdown exits successfully | Early-construction failures and physical resource accounting |
| Chinese text and IME | Physical Pinyin, visible preedit/caret candidate, committed Chinese, cancellation and Tab/ShiftTab pass in primary and detached text windows with an explicit SimHei merge; live Chinese glyph updates pass while secondary Game presents | Default font policy belongs to the complete frontend; other native platform profiles remain unqualified |
| Clipboard and keyboard navigation | Sentinel-checked paste/copy/cut and committed IME copy-back pass; native Game capture releases to Editor text and Stop in a secondary window | Complete product shortcuts and authoring parity in U7 |
| Layout persistence | Native Save, detached Restore and Reset pass through ordinary controls and neutral preferences | Physical offscreen monitor-removal restoration remains unverified; finite bounds fallback has owner tests |
| Independent panel lifetimes | Native Scene/Game close/reopen passes exact frames; real mouse/WM_CLOSE secondary close leaves Play Running and ordinary reopen produces a fresh window/image | Complete product panel contributions in U7 |
| Accessibility | Current native egui Editor does not initialize/publish an AccessKit tree; toolkit harness trees are separate | Record actual selected-frontend baseline; full accessibility remains separate |
| Performance and resources | Same native consumer/size baseline records first-present, idle CPU, redraw timing, process memory, GDI/User objects and retained WGPU handles; no observed workflow-blocking regression | These debug observations are not a GPU memory benchmark or leak soak |
| Other desktop platforms | Source routes only | Native Linux/X11 and macOS qualification; Wayland must retain docking with an honest detachment reason |

## Windows native presentation checkpoint

The coordinating agent ran the real reference-game qualification binary with one native event
loop and the engine-created GPU namespace. Execution `41542` exited successfully with:

```text
primary_frames=4
secondary_frames=4
returned_frames=4
first_present_ms=6
stale_receipt_rejected=true
renderer_then_platform_shutdown=true
native_ime=unverified
```

Command, from the Nara root in PowerShell:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_TARGET_DIR = 'F:/SourceCodes/Rust/nara/target'
$env:NARA_IMGUI_QUALIFICATION_CANARY = '1'
cargo +1.95 run --manifest-path reference-game/Cargo.toml --locked --features editor-imgui-qualification --bin editor_imgui_qualification
```

The timing starts after GPU initialization and is not a comparable cold-launch or egui benchmark.
Window relocation exercises real native surfaces and presentation; it does not establish mouse
docking, IME, monitor changes, GPU completion or display scanout. The normal shutdown result does
not establish every early-error cleanup path.

Earlier native execution exposed an attempt to take a Game candidate before its managed Runtime
was admitted. The slice now requires an active start request and target before taking candidates.
The diagnostic path also exposed a D3D12 debug-layer exception during Vulkan/device teardown after
that early error. The original error is now preserved; the failing cleanup path still needs
separate verification rather than being inferred from a successful normal close.

Vendor test run: `e5ff46d3-7456-4dd8-b9e7-ec4e82be1516`, 100 passed, 0 skipped, with
`DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`. The independent focused correctness review confirmed the sample
and pending-frame chain and identified a possible collision for a nonstandard primary WindowId.
The bridge now excludes its actual primary identity when mapping secondary surface lifetimes.
Final mapped-identity test and native rerun remain part of integration verification.

## Native lifecycle and text follow-up

Native execution `12480` exits successfully after four exact frames in each of the primary,
secondary, primary-minimized secondary, returned, Game-closed, Scene-closed, reopened and recovered
phases. Recovery deliberately destroys the engine-created device after a bounded drain. It pauses
Play, retires Dear renderer/platform state, recreates that namespace using the same primary Window,
publishes a new-backend paused Game image, resumes and finally verifies Stop clears the Game image
lifetime. Secondary rendering continues while the primary surface is suspended; no primary
acquisition or presentation is fabricated from its retained configuration.

The probe first reproduced a destroyed-device allocation panic (`73187`). Device-loss callbacks
were not being polled before new allocations and the signal was consumed by its first check.
The owner now polls nonblocking before admission and keeps loss sticky until retirement. The
regression test failed in `728b8b48-65c9-44e3-9a56-50ab8c565be6`, then both loss tests passed in
`aced93ec-73d8-4cb0-9581-55ac5491f60a`. A paused Runtime also requires one zero-delta drive after its
capture source is restored; without that, the first recovery run timed out waiting for an image.
The corrected native recovery run `33193` passed before the minimized-window trajectory was added.

Text execution `1f0a72` uses actual physical Pinyin keys and the installed Windows Chinese layout,
not `KEYEVENTF_UNICODE` or injected Winit events. Native events report successive nonempty Preedit,
empty Preedit and `Commit("中文")`; the actual field becomes `中文`. Clipboard output is checked after
replacing OS clipboard contents with a sentinel, preventing an unchanged source clipboard from
masquerading as a successful Copy. Paste, Copy and Cut retain `Nara clipboard 中文 42`; the committed
IME string copies back as `中文`. The isolated child preferences directory avoids personal layout
changes, and the controller restores the target input language and prior text clipboard afterward.

The owned-window screenshot shows the preedit at its insertion position and the native candidate
immediately beneath it. This was observed on Windows at 150% scaling with an explicitly merged
SimHei font. The first interaction exposed clipped controls because initial panel geometry did not
scale with the configured font; default positions and sizes now use the active font metric.
No secondary-window IME, cancellation, full keyboard-navigation, cross-monitor or default font
coverage claim follows from this one primary-window trajectory. Local trace artifacts remain
under ignored `target/`; they are not personal desktop captures for publication.

## Native interaction and input ownership follow-up

Text/layout execution `72269` exited successfully after mouse detachment of the text panel,
physical Windows Pinyin composition/commit, clipboard paste/copy/cut, composition cancellation,
Tab/ShiftTab focus return, and ordinary Save/Restore/Reset controls. Restore removed the actual
secondary window; Reset persisted an empty toolkit layout. This extends the primary-only result
above without claiming a default font policy or another operating system's behavior.

Execution `51827` exited successfully using physical mouse docking and detachment of the real
Game panel. Saved toolkit data contained a DockId after both docking and redocking. The owned
native windows reported DPI 144 and 120 using a per-monitor-aware observer. The secondary was
resized, closed and reopened while Play remained Running; reopening produced a fresh native
window and presented image. Secondary Game keyboard capture then released to Editor text,
Chinese glyphs were added while Game kept presenting, and the ordinary Stop control reached an
Empty state with no Game image. Earlier system-DPI-aware observations that reported both
windows at 144 were observer virtualization, not mixed-DPI evidence.

That interaction exposed two Nara defects, not an identified Dear ImGui core defect:

- Native focus loss or surface retirement conflicted with a Runtime UI route reservation held
  by a buffered Game candidate. The shared EditorProjectSession now completes only its buffered
  candidate as interrupted before applying endpoint invalidation. A Host-held candidate remains
  busy. Regression tests first failed in `4f81b988-943f-4227-9947-13c94505a044`, then passed after
  the owner fix; strengthened run `cab62395-c2ff-4e69-8f28-c18886d9dd7e` additionally verifies
  surface retirement and exactly one fixed update across an interrupted Step/retry.
- Actions apply after the current UI presentation. Stop removes the running cadence, so both
  direct actions and an asynchronously completed Play state change must request a successor
  repaint. Execution `51827` verifies the corrected native secondary Stop path.

Lifecycle execution `bd656a` subsequently passed all eight four-frame phases, stale-content
receipt rejection and renderer-before-platform shutdown after real device destruction/recovery.
An earlier rerun `2b0d83` exposed qualification accounting: session close could finish between
redraws before the canary recorded its terminal phase. The canary now records the completed
presentation trajectory when requesting exit; actual application retirement, event-loop return
and graphics shutdown must still succeed before a success record is emitted.

Integrated nextest run `14eb747b-892d-4b0c-9d9a-30a3262a4200` passed all 233 Editor, WGPU and
UI-render library tests with one test thread. This includes actual GPU tests, input-owner tests,
destroyed-device rejection and shared UI submission/completion coverage. It predates only the
subsequent early-error qualification branch and nonfunctional comment cleanup; final verification
and independent full review remain pending.

## Comparable Windows observations

Execution `39262` used the same reference-game intent, native Host entry point, debug Rust 1.95
binary, 1200 by 800 logical window (1800 by 1200 physical at 150%), isolated preferences and Scene
workload. Runs were sequential, with a three-second settling interval and five-second process
measurement interval. Redraw runs requested native repaint at approximately 60 Hz. The Dear
frontend is a bounded slice, so this compares the admitted workload, not full widget parity.
No optional CJK font merge was used in this baseline.

| Frontend / workload | First present (ms) | CPU seconds / measured seconds | Redraw p50 / p95 (ms) | Presents during observed launch |
| --- | ---: | ---: | ---: | ---: |
| egui / idle | 1344.394 | 0.953125 / 5.017 | 33.122 / 35.361 | 244 |
| egui / requested redraw | 1334.722 | 1.187500 / 5.006 | 33.121 / 35.760 | 244 |
| Dear / idle | 1214.382 | 0.015625 / 5.017 | 9.556 / 9.556 (three samples) | 3 |
| Dear / requested redraw | 1247.380 | 0.937500 / 5.004 | 24.082 / 32.433 | 155 |

First-present measurement begins at native Host metrics initialization before window/GPU setup;
it is distinct from the post-initialization six-millisecond canary metric. Redraw duration is
host callback wall time, not GPU execution time. Idle Dear's three samples do not establish a
steady-state latency distribution. Native requests may coalesce, so the present counts do not
represent identical delivered frame rates or a general performance ranking.

At the end of the idle interval, egui working set/private bytes were 276303872/477011968;
Dear's were 234418176/455471104. Requested-redraw values were 239955968/431550464 and
234442752/457928704 respectively. GDI object count was 16 for both; User objects were 36 for
egui and 37-38 for Dear. Both retained one WGPU device, one surface, three buffers and two render
pipelines. egui retained four textures/three views/five bind groups; Dear retained five/five/six.
These are user-retained WGPU handle counts and process memory, not physical GPU allocation bytes.
The short baseline and lifecycle checks do not establish leak-free behavior over arbitrary
sessions. No reproducible workflow-blocking regression appeared in this measured workload.

## Post-review qualification

The complete nine-persona review receipt is run `20261016-153024-b9f25bd9`. It reports three
confirmed findings against the frozen pre-fix snapshot: recovery continuation, direct secondary
receipt rejection coverage, and an idle Scene reopen without a successor paint. Its verdict is
**Not ready**. Later fixes require their own evidence; that receipt does not cover them.

Ordinary, non-canary execution `4de631` subsequently passed idle Scene close/reopen with a fresh
view identity, Scene resize from 869x366 to 773x325, and paused Game resize with an exact presented
image. No additional native repaint was injected after the interaction to complete the observation.
Text execution `47191` passed physical Chinese composition, clipboard, cancellation and keyboard
navigation in a DPI-120 detached text window; Restore retired the old native window automatically
and Reset left one primary window. The earlier failing Restore observation remains red evidence.
Idle observation `d17c4b` recorded zero additional process CPU seconds during 5.019 measured seconds
and only four total launch redraws. These results support bounded follow-up frames, not continuous
polling. Lifecycle execution `bfeff7` passed all eight four-frame phases before the later recovery
owner and initialization changes.

Reference nextest `36ac3cd2-aba2-44b0-9bc8-8466b290b325` passed all four input/recovery tests. The new
trajectories preserve a dirty Exit decision while a Host-owned recovery pauses the actual Runtime,
keep ordinary Pause/Resume rejected, and consume an already queued Step exactly once after its
capture source retires. The application/Host integration has subsequent changes and still needs
its final verification.

### Confirmed upstream shutdown defect

`dear-imgui-winit` 0.20.0 at the pinned tag has a concrete combined-shutdown defect. In
`src/platform/ownership.rs:739-746`, the successful `if let` body retains the temporary immutable
`runtime.borrow()` while `clear_runtime` calls `borrow_mut()` at line 449. The documented
`shutdown` operation promises to release multi-viewport state, but this path panics with
`RefCell already borrowed`. Native bridge test runs `8e5b9fab-ecdd-413a-933a-9c950a4bac27` and
`b7a88b3d-2b34-4521-8d52-fd4c0c10239d` reached that exact line. The first run also exposed a fixture
that failed to complete platform-window updates after deliberately rejecting a stale frame; that
separate fixture error was corrected through the public frame lifecycle before the second run.

Nara's normal explicit sequence already disables viewports between renderer and platform shutdown.
The bridge fixture now follows that same sequence and passes on the explicit profiles below.
This is an identified upstream Winit-backend defect, distinct from Nara's demand-redraw and Runtime
control bugs; no upstream core or Rust Context-binding defect follows from it. No Winit fork or
public fake-report constructor was added.

### Initialization teardown and Vulkan implicit-layer isolation

The new `fail-after-platform` construction probe exits with the requested error. Returning after
renderer creation reproducibly exits with code 2173 under this machine's default Vulkan layer
configuration. CDB identifies D3D12 debug-layer error 921 during Vulkan device/surface destruction.
Moving local GPU ownership ahead of Context, explicit rollback and flushing/polling did not resolve
that external failure, and must not be described as its fix.

The discriminating bare-primary test `602d2967-7412-4e5b-a18c-8e507830e143` also exits 2173 after
configuration and drain without initializing or calling any Dear Context, platform or renderer.
It records Vulkan, an RTX 4090 and NVIDIA driver 617.14. This establishes that the failure does
not require ImGui. The same early-renderer failure binary exits with its requested error on DX12.
It also exits correctly on Vulkan when only the child process disables all implicit layers, or
individually disables `VK_LAYER_GAMEPP` or `VK_LAYER_NV_present`. The experiments establish a
layer-dependent interaction; they do not identify which layer implementation is independently
defective. No system setting or production environment filter was changed.

The diagnostic filter follows the Khronos
[loader debugging procedure](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderDebugging.md#disable-layers).
It is an observation profile, not a shipped workaround. The two ignored native tests, including
real secondary Device/Queue/replay/exact-view/stale receipt rejection, both pass on unfiltered
DX12 (`f262213c-5b0d-4dc1-93e2-e5f328f222f1`) and on Vulkan with only the child's GamePP layer
excluded (`39df5edc-37ba-4c16-bf7f-94ef1ee3460e`). The bridge test presents the secondary image
without acquiring or painting the primary. Its explicit disable-viewports shutdown also avoids
the separately confirmed upstream RefCell defect above.

These platform-specific results close the direct bridge coverage finding. Final integrated
nextest `f0ac7f6a-032c-4144-91fe-d3c071dbfbd7` passes 233 library tests; the two interactive tests
are executed separately as above. Final source build `36194` and native `238b14` pass all eight
four-frame lifecycle phases on DX12, including real device destruction, recovery, paused image
publication, Resume, Stop and shutdown. Early-platform and early-renderer probes exit with their
requested errors; the post-present probe retains its original error without a cleanup crash.
Final native text `51913` passes detached DPI-120 Chinese IME, clipboard, cancellation, keyboard
navigation and physical Restore/Reset cleanup on that same backend.

Workspace all-target/all-feature locked check `62349` and affected Editor/WGPU all-target/all-feature
Clippy `70101` pass. Clippy retains warnings and is not claimed clean. Independent delta review
confirms recovery against dirty/retained owners and finite repaint scheduling; its duplicate-drain
suggestion was removed before the final source build. The original frozen review's findings have
been resolved with the source and native evidence recorded here; its historical verdict is intact.

Proceed to U7's full parity matrix and then U8's clean external workflow and old-path removal.

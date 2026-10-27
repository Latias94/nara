# Native Editor Qualification

This is the manual Windows/DX12 qualification gate for the selected Dear frontend.
Ordinary workspace tests exclude these ignored cases and do not establish native acceptance.
For a Windows/DX12 release qualification, run the three critical trajectories and the additional
groups required by the claimed capabilities. For development changes to native view/input, device
replacement, frame cadence or supervised lifecycle ownership, select the trajectories covering
the affected behavior. A focused result establishes only that tested scope.
Documentation-only changes do not require another native qualification run.

## Environment

Use Windows with an interactive desktop, Rust 1.95.0 and an actual DX12-capable adapter.
Run Cargo serially in the checkout with its existing target directory.
The three critical trajectories below use hidden native windows and owned Context input;
they submit and present through production paths without moving the user's pointer or activating a visible window.
The separate external-process trajectory uses a visible, initially inactive Editor window as specified below.
They do not establish monitor scanout, accessibility parity, or Linux/macOS qualification.

An unavailable adapter/desktop is a failed qualification prerequisite, not a passing skipped test.
Keep the previous platform claim unchanged until this gate can run.
A permanently provisioned native CI runner is not currently claimed.

## Required Trajectory

Inspect the intended diff and record the source revision first.
Then execute the existing critical tests explicitly:

```powershell
git rev-parse HEAD
$env:CARGO_BUILD_JOBS='1'
$env:WGPU_BACKEND='dx12'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive) | test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) | test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts)' --test-threads=1
```

For the critical qualification group, all three tests must execute and pass; an empty selection
is not acceptance. A narrower development selection must identify the affected behavior and
the tests actually executed, without claiming the complete group passed.
They cover same-output Game resize and authoring input, pause/resume cadence, exact Game input and Step, independent view close/reopen, detachment/minimization, device recovery, and retirement.
The ordinary non-native Editor suite separately protects cadence policy and source-diverged Restart admission.

For toolkit/input upgrades, also exercise the real Nara Project path/Profile fields through the
registered native backend and core IME composition:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_project_fields_use_upstream_ime_composition_and_reject_stale_owner_commit)' --test-threads=1
```

This proves preedit/draft isolation, one commit and rejection of a late commit after the active
field changes. It constructs Winit events directly and does not qualify OS candidate-window
placement, actual input-method event delivery, or manual cross-window typing.

## External Process Scheduling

For changes to external Play cadence, maintenance wake or child-exit presentation, also run:

```powershell
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_external_running_maintenance_stays_quiet_and_presents_child_exit)' --test-threads=1
```

This trajectory performs a real supervised Cargo build and READY handshake, then checks at least
three due process-maintenance cycles without another Editor redraw. A test-only exit trigger in
the guarded project copy ends that child with exit code zero; production Host observation must
publish and present the exit. It tests process-exit observation, not graceful Runtime teardown.

This trajectory uses a visible, initially inactive Editor window so Windows delivers its real
redraw requests; the ordinary game child can also show its desktop window.
No OS pointer or keyboard input is injected. The copied fixture, child and native graphics are
retired by their existing owners. Allow up to ten minutes for build/READY on a cold target.

## Broader Qualification

When the change affects image/Canvas workflows, project opening, docking/layout, or supervised builds, also execute the corresponding ignored native tests.
For retained Scene source selection and ordinary Prefab/Canvas navigation, execute:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_retained_scene_receipt_survives_prefab_navigation_and_bound_close) | test(native_ui_canvas_priority_survives_closing_retained_scene)' --test-threads=1
```

These hidden-window trajectories qualify the retained Scene's exact presentation receipt while
an ordinary Prefab is active, disabled inactive-Scene picking, explicit return to the bound Scene,
and additive second-Scene publication with stale first-Scene picking rejection. Closing the bound
second Scene does not fall back to the retained first Scene. The second trajectory retains ordinary
and UI Prefabs together, preserves Canvas priority through unrelated Prefab and Scene Close, and
retires it after its own source closes. These are bounded same-kind Open/navigation trajectories,
not qualification of multiple simultaneous preview Runtimes or every persistence combination.
The retained-Scene trajectory also performs four real empty-entity creation/Undo pairs. Its
same-owner/extent Scene epochs must retain the exact native target identity and texture
registration while old presentation tokens fail. It explicitly invalidates a presentation with
an incompatible source revision, then requests the unchanged accepted Host source and requires
a fresh submitted/composite token. That recovery probe simulates rejected-source observation;
the owner tests separately exercise actual materialization rejection and exact source restoration.

For complete Prefab-to-local conversion, also execute:

```powershell
cargo +1.95.0 nextest run -p nara-editor --all-features --lib --locked --run-ignored only -E 'test(native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab)' --test-threads=1
```

This hidden-window trajectory uses actual Inspector mouse and wheel input to prepare, review and
commit the same conversion for both Scene and Prefab sources. It checks fixed remaps, exact local
hierarchy, ordinary local children, another linked instance, one Undo/Redo and actual Save/Reopen.
It compares Sprite geometry, material and source texture pixels through the presentation owner and
requires actual native composite receipts. It does not read back the GPU framebuffer or qualify
monitor scanout. Host tests separately cover stale source/working-copy refusal and physical
cancellation/Close; a passing native success path does not establish those failure paths.

For finite source-tool facets, panel/Inspector result routing or parameter retirement, build the
actual C# consumer and run its explicitly selected native workflow:

```powershell
python tests/fixtures/behavior-languages/run.py --native-editor
```

The runner builds the real child artifacts, executes the window-free language/Session consumers
and the selected Application consumer, then runs
`native_source_tool_inline_property_and_panel_share_review_history_and_persistence` with DX12.
That hidden-window trajectory prepares through the health property control and accepts through
the shared Source Tool panel, verifies a single Undo/Redo, the ordinary armor control, Attach,
activation/parameter retirement, generic fallback, Save, physical Close and fresh Open. Rust
materializes the saved component data through public Scene bindings. Both native scopes must
present. The current fixture publishes and inspects a real local tool artifact, explicitly
approves its managed Host, then removes only the fixture's installed DLL before native probing.
The widgets therefore exercise captured managed images rather than the development tool path.
This qualifies that bounded approved-artifact input, not the full install/update/remove UI,
durable enablement, a language SDK, executable gameplay publications or the future custom-drawn
UI runtime. Historical evidence before this fixture replacement retains its development-path scope.

For trusted Play source association and source-local document Close, also execute:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_unrelated_prefab_close_preserves_presented_play_source_until_source_close)' --test-threads=1
```

This hidden-window DX12 trajectory opens and closes an anchored Prefab while Game continues
through exact submitted/composite receipts with the same source, Runtime generation and target.
Closing the associated Scene retains it while Stop is pending; completed native frames then
observe exact Stop and target retirement before reporting source removal. It does not prove
intra-frame ordering or the held cancellation/failed-start cleanup paths; public Host tests own
those separate trajectories.

The full native platform group can be selected with:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(platform::imgui::)' --test-threads=1
```

Supervised-build trajectories invoke real Cargo builds in their own fixtures and can take substantially longer than the three critical tests.
Do not start another Cargo invocation while they run.

## Evidence and Release Use

Record the exact tested source revision, dirty-file scope if any, Rust version, Windows/DX12 profile, nextest run ID, selected tests, result, and any environment failure in engineering-memory verification evidence.
Bind the final release claim to committed source; a passing test of another worktree or a differently resolved dependency graph is insufficient.
Keep this document as the stable procedure and put execution results in evidence records.

The first-party Dear renderer adapter has an additional adapter-required gate using the root
workspace lock and the published 0.21.0 backend:

```powershell
$env:DEAR_IMGUI_REQUIRE_WGPU_ADAPTER='1'
cargo +1.95.0 nextest run -p nara_render_wgpu --locked --features editor-imgui --lib --test-threads=1
```

This gate tests Nara's integration with the production WGPU 30 dependency resolution.
Its offscreen tests do not replace the actual native trajectories above.
For changes to native retirement, partial graphics construction or shutdown/replacement gating,
also execute the focused retirement trajectories:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(platform::imgui::retirement_tests::)' --test-threads=1
```

They cover exit before first presentation without unrelated events, a real dirty Scene edit and
Cancel, dirty Exit arriving during queued reconstruction, SDK callback drift reported after
renderer release, actual SDK attach refusal after renderer construction, and healthy Open Host
worker return before graphics/process Exit. The healthy-Host trajectory uses a staged real event
handler: it dismisses the manager's initially focused text field with Escape through the owned
Context input. After initial and CloseRequested presentation settles, at least three actual
maintenance deadline wakes must deliver no new redraw or presentation while the worker remains held. Only
after release may physical Tasks return and native destruction finish. It starts with a hidden,
inactive window; if Windows withholds presentation, it shows the same inactive window after
500 ms and records that fallback. It does not inject OS pointer or keyboard input. The other
retirement trajectories remain hidden. They observe retained
Context/backend identity and destruction before a
replacement. The adapter gate separately checks initialization-upload readback, retained
device/queue drain after destruction, failed attach custody, and shutdown/binding idempotence.
These checks do not qualify arbitrary SDK exceptions, detached outstanding epochs, foreign
sidecar recovery or OS window destruction. The complete lifecycle trajectory above owns actual
secondary presentation and device replacement.

For renderer identity/binding changes, also execute the real delegated-window bridge:

```powershell
cargo +1.95.0 nextest run -p nara-editor -p nara_render_wgpu --features nara_render_wgpu/editor-imgui --lib --locked --run-ignored only -E 'test(secondary_bridge_rejects_foreign_replay_stale_and_unsampled_images)' --test-threads=1
```

It verifies actual secondary presentation before primary acquisition, exact native registration,
foreign device/queue rejection, report replay, binding replacement and stale image-content receipts.

For the optional fixed world canvas, also run the same actual Game input/resize/Step workflow with
the canvas profile:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_fixed_world_canvas_keeps_ui_input_resize_and_step_coherent)' --test-threads=1
```

Exact world pixels and bars are separately checked through the shared target encoder in
`nara_render_wgpu::canvas::tests`; native presentation alone does not establish pixel readback.

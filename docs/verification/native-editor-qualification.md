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
For renderer identity/binding changes, also execute the real delegated-window bridge:

```powershell
cargo +1.95.0 nextest run -p nara-editor -p nara_render_wgpu --features nara_render_wgpu/editor-imgui --lib --locked --run-ignored only -E 'test(secondary_bridge_rejects_foreign_replay_stale_and_unsampled_images)' --test-threads=1
```

It verifies actual secondary presentation before primary acquisition, exact native registration,
foreign device/queue rejection, report replay, binding replacement and stale image-content receipts.

# Native Editor Qualification

This is the manual Windows/DX12 qualification gate for the selected Dear frontend.
Ordinary workspace tests exclude these ignored cases and do not establish native acceptance.
Run this gate before a release claim or a change to native view/input, device replacement, frame cadence, or supervised lifecycle ownership.

## Environment

Use Windows with an interactive desktop, Rust 1.95.0 and an actual DX12-capable adapter.
Run Cargo serially in the checkout with its existing target directory.
The tests use hidden native windows and owned Context input; they submit and present through production paths without moving the user's pointer or activating a visible window.
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

All three tests must execute and pass; an empty selection is not acceptance.
They cover same-output Game resize and authoring input, pause/resume cadence, exact Game input and Step, independent view close/reopen, detachment/minimization, device recovery, and retirement.
The ordinary non-native Editor suite separately protects cadence policy and source-diverged Restart admission.

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

The renderer patch has a separate adapter-required gate using the root workspace lock:

```powershell
$env:DEAR_IMGUI_REQUIRE_WGPU_ADAPTER='1'
cargo +1.95.0 nextest run -p dear-imgui-wgpu --locked --no-default-features --features wgpu-30,multi-viewport-winit --lib --test integration_test --test-threads=1
```

The renderer's mutually exclusive backend features are not tested with `--all-features`.
Its unit tests alone do not replace the actual native trajectories above.

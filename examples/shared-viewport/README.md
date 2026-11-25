# Nara + Open-GPUI shared viewport

This Phase 2 example opens two peer native windows through Open-GPUI Mode A. **Nara Scene View**
shows the Nara stock renderer's animated sprites over a tilemap. **Nara Inspector** exercises native
CJK composition and controls the viewport's refresh. Nara's capture uses a logical window extent
for camera extraction. Both actual desktop windows and their swapchains belong to Open-GPUI.

The app requests one WGPU device from the combined renderer requirements. It passes the same
device, queue, and device-error state to Nara and Open-GPUI. Nara submits an `Rgba8UnormSrgb`
image with premultiplied alpha, then GPUI samples that texture directly. The imported image owns
an `Arc<WgpuImageOutput>` so the texture can return to the Nara cache after the last GPU use.
Same-size updates replace the backing and request retained presentation. An extent change imports
a new image and repaints the viewport.

## Run on Windows

Use Rust 1.95 and the Phase 2 Nara and Open-GPUI checkouts as siblings. The example has its own
Cargo workspace and takes local Open-GPUI path dependencies. The commands below start at the
Nara repository root.

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
$manifest = 'examples/shared-viewport/Cargo.toml'
cargo tree --manifest-path $manifest -d -p wgpu
cargo run --manifest-path $manifest --release
```

Confirm a single `wgpu 30.0.0` package at revision `cddb2f5d80dc0009b2ef6767635140cacf90a007`.
Set `WGPU_BACKEND` to `dx12` or `vulkan` to record evidence for a specific Windows backend.
On a multi-GPU system, set `WGPU_ADAPTER_NAME` to a distinctive part of the display-capable
adapter's name. Each native window checks the selected adapter against its own surface.

## Native smoke

```powershell
cargo run --manifest-path $manifest --release -- --smoke | Tee-Object -Variable smokeOutput
if ($LASTEXITCODE -ne 0 -or $smokeOutput -notcontains 'SHARED_VIEWPORT_SMOKE_OK') {
    throw 'Shared viewport smoke failed'
}
```

Leave both windows visible and untouched during the smoke. It uses a 15-second deadline after
opening the windows. Both must submit visible, nonempty frames through their native renderers.
After 30 warmup frames, the smoke samples two seconds of animation and checks for repeated
presentation of an unchanged GPUI scene revision. It then pauses animation, resizes the viewport,
refreshes the inspector, and checks that both windows present their updated scenes. A final
refresh of the paused viewport must submit one new Nara frame at the same GPUI scene revision.
The success marker follows a shared-queue completion callback. Check the marker along with the
exit code because the Windows GPUI event loop uses zero for ordinary shutdown.

## Display and IME verification

1. On a display configured for at least 120 Hz, record the release smoke's `viewport_fps`,
   `display_refresh_hz`, `nara_frames`, `retained_submissions`, and `inspector_submissions`.
   Record the OS, adapter, driver, backend, display mode, and DPI with the output. The frame
   measurements count native renderer submissions. The OS compositor owns scanout cadence.
2. Drag each window between displays at 100%, 125%, or 150% scaling and resize it on both sides.
   Watch the viewport's physical extent and scale in the console. Check that the tilemap and
   moving sprites stay visible, and that translucent sprites blend over the two background shades.
3. Pause the viewport in the inspector, then click **Refresh once**. Check that the image updates
   once and stays still. Resize while paused and confirm that the scene fills the new bounds.
   Resume animation and close either window to exercise independent native lifetimes.
4. Click **Scene name**, switch to a native Chinese, Japanese, or Korean IME, compose text, select
   a candidate, and commit it. Check the marked-text border and committed glyphs. Repeat after
   moving the inspector across displays. Record mixed-DPI drag and IME results from these manual
   checks. The automated smoke exercises programmatic resize on its current display.

The smoke checks Nara's transient texture creation, native buffer creation and encoded texture copy
commands during its fixed-size sample. It also reports texture cache reuse. These are Nara-side
counters. To qualify the full Phase 2 GPU budget, capture steady-state frames in PIX on DX12 or
RenderDoc on Vulkan. Check both renderers for transient texture and buffer creation and full-screen
copies. Use a CPU trace to check the frame path for synchronous GPU waits. Record native Linux and
macOS results separately.

# Independent Render Composition Consumer

This standalone workspace consumes public content, Runtime and exact-wgpu Host APIs. It owns no
second GPU device or queue submitter and imports no engine-private transport or probe hooks.
The native frontend owns actual Winit windows; the Host owns acquisition, submission, presentation
and original-prefix retirement. This is a bounded composition consumer, not an Editor toolkit.

## Workflows

- `content.rs` checks real PNG source import, production Scene cooking, independently admitted
  processed Image acquisitions and retained pixels after backing removal. This ordinary test
  does not run a Runtime or establish native presentation.
- `examples/content_window.rs` runs the ordinary Source then Processed desktop actions through
  one explicitly retained physical event loop. Each normal Host publishes its later Scene after
  an actual red presentation. A read-only sealed-input observer retains the original red/green
  stock inputs and removes live Image/prepared entries before the green native presentation.
  Full Source retirement, failed startup recovery and full Processed retirement precede backing
  removal. One offscreen Host then replays all four inputs in interleaved order; a replacement
  Host repeats them. Every old/new pixel, actual producer refusal and original-input identity is
  checked separately from the normal Hosts' presentation receipts.
- `examples/streams_window.rs` registers two real windows with one Host and independently publishes
  an image composition. Deterministic30/60/144 schedules distinguish capture, image and display
  clocks; they are not measured frame rates. A pauses/retires while B and image composition keep
  advancing. Exact older A pixels, independent mirror occurrences, placement snapshots, acquired
  paint refusal and native retirement are asserted.
- `composition.rs` and its drawing support provide a separate clipped Scene/control/popup numeric
  oracle with explicit linear-premultiplied color and fractional mapping. They do not establish
  widget editing, IME/accessibility or arbitrary shader inverse support.

The public `WgpuPolicyFrameReader` borrows replayable input after Seal and before Publish. It does
not consume the original batch or take any one-shot UI completion sidecar. Native display receipts
prove exact occurrence sampling and window presentation. This consumer retains placement/clip/DPI/
role snapshots itself and rejects stale joins; receipts alone do not prove input mapping.

## Run

Use Rust1.95.0, the root target and serial Cargo commands. Native selections require an actual
hardware adapter and desktop; missing presentation, readback or retirement fails the prerequisite.

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target'
$env:WGPU_BACKEND='dx12'
$env:NARA_WGPU_FORCE_FALLBACK='0'
cargo +1.95.0 build --manifest-path tests/fixtures/render-composition-consumer/Cargo.toml --locked --examples
cargo +1.95.0 nextest run --manifest-path tests/fixtures/render-composition-consumer/Cargo.toml --locked --run-ignored all --test-threads=1 --no-fail-fast
```

The window tests launch their prebuilt examples on the real main thread, never another Cargo.
`NARA_RENDER_COMPOSITION_STREAMS_WINDOW_EXE` and `NARA_RENDER_COMPOSITION_CONTENT_WINDOW_EXE`
may select the corresponding exact executable explicitly.
The sample uses an inactive native desktop and does not inject OS keyboard or pointer input.
Window and pixel assertions supply distinct evidence; actual monitor scanout, SDK-owned secondary
window internals, real device loss and other platforms retain their own qualification boundaries.

The independent workflows have separate Windows/DX12 execution evidence under the
[render unit](../../../docs/plans/2026-10-31-render-composition-and-host-execution-plan.md).
Their bounded qualification does not complete the external stock Scene-effect seam, full render
unit, editable Nara-drawn Inspector or remaining foundation work.

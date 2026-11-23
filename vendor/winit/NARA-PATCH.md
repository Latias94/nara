# Nara Platform Patches

This directory contains the published `winit` 0.30.13 crate from crates.io. Its upstream Git
revision is `e9809ef54b18499bb4f2cac945719ecc2a61061b`, also recorded in
`.cargo_vcs_info.json`. The original Apache-2.0 license and upstream sources are retained.
Registry download markers/checksums are not part of this path dependency. `rustfmt.toml` is
restored from that same upstream revision so maintained files retain the original formatting;
its unstable formatting options use the installed nightly rustfmt, separately from the qualified
Rust 1.95.0 production compiler.

## Windows DPI Repair

The original Windows 11 mixed-DPI title-bar trajectory reproducibly leaves the window on the
old monitor while the held cursor reaches the other monitor. Native scale changes oscillate
between 1.25 and 1.5. Programmatic placement and explicit size requests outside a move loop
pass on the same machine.

Behavior changes are limited to four files:

- `src/platform_impl/windows/dark_mode.rs` exposes the existing cached Windows build observation
  to its sibling module. It adds no OS-version query or cache owner.
- `src/platform_impl/windows/event_loop.rs` uses the OS-suggested DPI rectangle on Windows 11
  (build 22000 or newer), retaining the legacy adjustment on older or unknown versions. If the
  application requests a client size through `InnerSizeWriter`, the suggested origin
  is retained and the outer extent is adjusted using the existing decoration calculation.
- `src/event.rs` and `src/platform_impl/windows/event_loop/runner.rs` retain an explicit Windows
  size-request flag through synchronous or buffered delivery, including a request numerically
  equal to the proposed size. The flag is written while holding the original size mutex and
  observed after reading that size. Other platforms retain their original constructor and
  representation. Windows adds a small shared flag allocation per DPI dispatch; no performance
  or allocation reduction is claimed.

The normal stable `cargo fmt --all` also formats local path dependencies. Its six layout-only
normalizations are retained in `linux/x11/{dnd.rs,event_processor.rs,mod.rs,util/input.rs}`,
`macos/window_delegate.rs`, and `windows/window_state.rs`, all under `src/platform_impl`.
Each was checked byte-for-byte against the same formatter's output from pristine published
source. The four Windows repair files also pass the restored upstream nightly formatting
configuration. The macOS keyboard table below is a separate behavioral change.

The Windows 11 policy follows upstream's merged
[`488c036a05d418e13bbdcdc349e2db2f6b7f58e2`](https://github.com/rust-windowing/winit/commit/488c036a05d418e13bbdcdc349e2db2f6b7f58e2).
The compatible unmerged [PR4694](https://github.com/rust-windowing/winit/pull/4694), head
`39c4009c41e5977d779cf8a365792c8153ca77c5`, documents the same live-drag feedback loop.
Preserving an explicit size override is a local correction for the separately reported
[`InnerSizeWriter` issue](https://github.com/rust-windowing/winit/issues/4721), rather than a
claim that either upstream change already resolves it. Nara does not adopt the 0.31 platform
split to obtain this repair.

## macOS ISO Key Identity

The macOS event adapter asks Carbon for the keyboard type when Cocoa reports `0x0a` or `0x32`.
It swaps those two event codes for the ISO keyboard type before choosing the Winit physical key.
The normalized table and its inverse use `0x0a` for `IntlBackslash` and `0x32` for `Backquote`.
Logical key and text translation continue to use the original Cocoa event. The Cocoa adapter in
SDL 3.2.10 uses the same two-code normalization. Nara's real ISO hardware and mixed ANSI/ISO
device delivery remain open for native verification.
[SDL Cocoa event adapter](https://github.com/libsdl-org/SDL/blob/release-3.2.10/src/video/cocoa/SDL_cocoakeyboard.m) ·
[Round twelve report](../../docs/audits/2026-11-23-prefab-authoring-player-and-input-review.md)

## Integration And Retirement

The engine workspace depends directly on this path so an ordinary Git/path consumer receives
the repaired engine adapter. The root and independently locked application workspaces also patch
crates.io to this same directory, unifying direct and transitive Winit users. Application-level
Cargo patches do not propagate into an unrelated consumer's root manifest: an application that
also imports Winit or a Winit integration from crates.io must select this source for those users
until a compatible upstream release provides the required behavior. The real game-reconstruction
fixture rebases its known engine and Winit patch paths when copying the project outside this tree,
while preserving the copied lockfile and ordinary public Cargo build path.

The original Nara native owner remains responsible for windows, event loops, normalized input
and physical retirement. This patch adds no platform interface or replacement native Host.
The ignored `nara_winit` geometry and title-bar tests retain programmatic controls, actual OS
input in both directions, default and explicit-size drag behavior, Runtime synchronization and
original native retirement. Actual results and remaining scope belong in the Nara native
qualification guide and UI integration handoff.

Retire this directory and the corresponding manifest patches when a compatible upstream source
passes the affected consumers, including explicit size overrides and both ISO key identities.
Foreign native execution, maximized/snap/keyboard moves and GPU presentation retain their
separate qualification scope.

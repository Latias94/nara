# Nara Windows DPI Patch

This directory contains the published `winit` 0.30.13 crate from crates.io. Its upstream Git
revision is `e9809ef54b18499bb4f2cac945719ecc2a61061b`, also recorded in
`.cargo_vcs_info.json`. The original Apache-2.0 license and upstream sources are retained.
Registry download markers/checksums are not part of this path dependency. `rustfmt.toml` is
restored from that same upstream revision so maintained files retain the original formatting;
its unstable formatting options use the installed nightly rustfmt, separately from the qualified
Rust 1.95.0 production compiler.

## Selected Repair

The original Windows 11 mixed-DPI title-bar trajectory reproducibly leaves the window on the
old monitor while the held cursor reaches the other monitor. Native scale changes oscillate
between 1.25 and 1.5. Programmatic placement and explicit size requests outside a move loop
pass on the same machine.

The source changes are limited to four files:

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

The Windows 11 policy follows upstream's merged
[`488c036a05d418e13bbdcdc349e2db2f6b7f58e2`](https://github.com/rust-windowing/winit/commit/488c036a05d418e13bbdcdc349e2db2f6b7f58e2).
The compatible unmerged [PR4694](https://github.com/rust-windowing/winit/pull/4694), head
`39c4009c41e5977d779cf8a365792c8153ca77c5`, documents the same live-drag feedback loop.
Preserving an explicit size override is a local correction for the separately reported
[`InnerSizeWriter` issue](https://github.com/rust-windowing/winit/issues/4721), rather than a
claim that either upstream change already resolves it. Nara does not adopt the 0.31 platform
split to obtain this repair.

## Integration And Retirement

The root and independently locked application workspaces patch crates.io to this same directory.
That selection applies to direct and transitive Winit users; application-level Cargo patches do
not propagate into an unrelated consumer's root manifest. A downstream application must select
the repaired source itself until a compatible upstream release provides the required behavior.

The original Nara native owner remains responsible for windows, event loops, normalized input
and physical retirement. This patch adds no platform interface or replacement native Host.
The ignored `nara_winit` geometry and title-bar tests retain programmatic controls, actual OS
input in both directions, default and explicit-size drag behavior, Runtime synchronization and
original native retirement. Actual results and remaining scope belong in the Nara native
qualification guide and UI integration handoff.

Retire this directory and the corresponding manifest patches when a compatible upstream source
passes those affected consumers, including explicit size overrides. Foreign native execution,
maximized/snap/keyboard moves and GPU presentation retain their separate qualification scope.

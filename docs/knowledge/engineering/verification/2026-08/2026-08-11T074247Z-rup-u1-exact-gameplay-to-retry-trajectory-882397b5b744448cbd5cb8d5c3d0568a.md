---
type: "Verification Evidence"
title: "RUP-U1 exact gameplay-to-Retry trajectory"
description: "Closes the exact Runtime UI behavior trajectory with typed input evidence, native Windows UI Automation ingress, backend-submitted successor presentation, and causal Retry replacement."
timestamp: 2026-08-11T07:42:47Z
record_id: "882397b5b744448cbd5cb8d5c3d0568a"
tags: ["ui", "runtime-ui", "reference-game", "rup-u1", "accessibility", "wgpu"]
status: "complete"
producer_id: "codex-root"
run_id: "runtime-ui-product-slice"
related_plan: "docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e88ad06"
verified_by: "codex-root"
---

# Verification

- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path reference-game/Cargo.toml --locked
  --test ui_product --test first_wave --test-threads=1 --no-tests fail` passed all 30 tests with
  run ID `947b2a1c-3155-41b1-8b38-ad3213eec9fa`; zero tests were skipped.
- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path reference-game/Cargo.toml --locked
  --features desktop-accessibility-probe --bin desktop_accessibility_probe --test-threads=1
  --no-tests fail` passed all 12 probe state-machine tests with run ID
  `6703d0e5-0e89-423f-9771-dec288029193`.
- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path reference-game/Cargo.toml --locked
  --features desktop --test desktop_flow --test desktop_render --test desktop_parity
  --test-threads=1 --no-tests fail` passed all 13 desktop tests with run ID
  `e4e8ecc2-7d67-4702-aaaa-ec8e8a7abe35`.
- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path reference-game/Cargo.toml --locked
  --test runtime_drive --test headless_snapshot --test headless_cli --test-threads=1
  --no-tests fail` passed all 11 headless/runtime tests with run ID
  `276792ee-f5b0-4b14-a57c-98d0165e8a33`.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_ui -p nara_winit --test-threads=1
  --no-tests fail` passed all 95 owning-module tests with run ID
  `7fa2eda1-fa69-4b68-8ac7-af671be1d20d`; zero tests were skipped.
- `cargo check` for the probe-specific feature passed. Scoped strict Clippy for the library,
  probe, `ui_product` and `first_wave` passed with `-D warnings`; the root dependency emitted only
  its pre-existing dead-code warnings. Direct `rustfmt --check` passed for the owned Rust files, and
  `git diff --cached --check` passed before commit. Repository-wide write-mode formatting was not
  used because unrelated concurrent Rust files remained dirty.
- One bounded Windows run of `desktop_accessibility_probe` passed through the real Winit,
  AccessKit, Windows UI Automation and WGPU path. It observed HWND `394484`, Retry RuntimeId
  `[42, 394484, 4, 0, 9, 0, 0]`, journal fence `0 < 1 <= 1`, armed frame revision `136`, action
  presentation epoch `178`, action frame revision `179`, run generation `2`, and a strictly newer
  submitted WGPU frame `134 -> 193`.

# Result

RUP-U1 is complete at commit `e88ad06`. The implementation lineage is:

- `5b09ff1` binds pointer, keyboard, gamepad and component accessibility activations to exact
  presented receipts and one typed Retry command in the focused fixture.
- `ead4083` installs the first-party Winit/AccessKit ingress without exposing platform identities to
  Runtime UI authoring.
- `97b4923` preserves endpoint detach, retirement, invalidation floors and non-reused platform node
  identities across native target lifetimes.
- `e88ad06` adds the real Windows UI Automation probe and carries the exact journal action sequence
  through the Retry command, pending replacement and applied generation.

The ordered product trajectory begins with the Scene/Prefab-authored HUD, reaches Completed, enters
Retry pending, rejects one replacement while retaining the same focused identity, accepts a second
activation, and removes the retired Retry paint, hit, focus and semantic facts before the captured
successor UI exposes run generation two. Paused delivery, concurrent activation, stale routes,
modal underlay blocking and capture cancellation retain focused component coverage.

The platform proof is causal rather than a conjunction of independent observations. The runtime
arms an exact semantic `UiKey` and journal high-water mark before UI Automation invokes the exact
RuntimeId. Only a successor accessibility action for that target can enter the command path; its
journal sequence must survive the game-owned replacement. Completion then requires a newer WGPU
submission, a positive successor semantic marker and an empty post-platform journal barrier.

The frozen UIF compile, present and dispatch inventory remains unchanged. RUP-U1 adds the named
first-party platform adapter and one hidden Host retirement query required to balance native
lifetimes; neither becomes ordinary Runtime UI authoring API.

# Evidence

- Component evidence proves exact receipt, target, source and one-command behavior for pointer,
  keyboard and gamepad, plus the platform-neutral accessibility mapper. It is not cited as native
  accessibility or backend evidence.
- The Windows probe proves that a real UI Automation client can observe and invoke the AccessKit
  Retry node and that the exact action reaches the first-party product path.
- The WGPU facts prove backend submit/present progression for the successor. They do not prove that
  a human saw or approved the pixels.
- The probe intentionally exits after its evidence barrier. Its short-lived blank/background and
  rectangular test content are not Runtime UI visual-product acceptance.

# Follow-up

RUP-U2 is the sole next active unit. It owns the ordinary source-data edit and restart, the three
16:9 targets, one non-16:9 letterbox case, fractional pointer mapping, persistent desktop visual
inspection, glyph/packet residency, presentation-free headless parity and arbitrary-CWD package
launch. It must not reuse this automation probe as pixel or authoring-roundtrip evidence.

# Citations

- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/architecture/reference-game-ui-product-slice.md`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-10T155957Z-uifm-u2-successor-foundation-conformance-baseline-19cfa496dc2346368764057367cc2163.md`

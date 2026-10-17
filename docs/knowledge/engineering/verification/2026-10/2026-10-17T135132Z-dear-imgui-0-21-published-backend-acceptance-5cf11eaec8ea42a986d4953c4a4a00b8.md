---
type: "Verification Evidence"
title: "Dear ImGui 0.21 published backend acceptance"
description: "Published registry cohort, owned renderer mapping, native IME and Windows DX12 product qualification with bounded text-intent cadence."
timestamp: 2026-10-17T13:51:32Z
record_id: "5cf11eaec8ea42a986d4953c4a4a00b8"
status: "completed"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-17-dear-imgui-021-upgrade-plan.md"
git_commit: "6a89a77257917aecaa10170ce1795d2de831a407"
verified_by: "Serial root nextest and independent correctness/adversarial Codex review"
---

# Verification

Implementation source: `6a89a77257917aecaa10170ce1795d2de831a407`, following `257dac0`.
The root owner ran Cargo serially with Rust 1.95.0, `CARGO_BUILD_JOBS=1`, existing target
directories and one nextest thread. Workers performed source research, exclusive implementation
and independent review without running Cargo. Subsequent closure changes are documentation only.

# Result

R1-R6 / U1-U2 are complete. All five Dear family packages resolve from crates.io at 0.21.0 in
the root, module-consumer and reference-game locks. Published core/Winit/WGPU packages carry
upstream VCS identity `58a44069ee923c7378ec1663873655df8694c49f`, matching the read-only reference.
The obsolete 45-file renderer fork and its workspace exclusion/test cohort are removed.
Historical provenance remains in Git and the short archived research note.

The device-scope-created adapter owns its renderer and actual texture registrations. It joins
external binding revisions to actual TextureViews and current Nara image receipts, consumes live
route reports before primary acquisition, preserves partial secondary success, and keeps native
window mapping independent of successful presentation. Delegated receipts identify native
registrations; they do not claim unobserved internal surface configurations or acquired extents.

Upstream Winit/core now owns IME preedit and its cancellation. Nara's duplicate overlay/filter is
removed. A separately proven on-demand redraw defect is fixed: delayed text-input intent gets
finite followup frames, then returns to idle. Flags propagate to native windows correctly, and
Canvas selection uses an explicit centered stroke under the new drawing contract.

# Evidence

| Gate | Observation |
|---|---|
| `cargo +1.95.0 fmt --all --check` | Pass on committed implementation source. |
| `cargo +1.95.0 check --workspace --all-features --all-targets --locked --offline` | Pass; existing unrelated dead-code warnings remain. |
| `cargo +1.95.0 nextest run -p nara-editor --lib --locked --offline --test-threads=1` | `8d5467b0-a79e-4223-9e52-b78a47da2fb7`: 132/132 pass, 14 native ignored. Includes the final bounded IME-intent followup fix. |
| `cargo +1.95.0 nextest run -p nara_render_wgpu --features editor-imgui --lib --locked --offline --test-threads=1` | `85beff8c-02fc-437b-99c7-aeec63659429`: 57/57 pass, 2 native ignored, adapter required and forced fallback. Actual WGPU recording covers revision replacement, canonical/alias views, framebuffer clipping and unregister. |
| `cargo +1.95.0 nextest run --manifest-path module-consumer/Cargo.toml --locked --offline --test-threads=1` | `455d45a5-3957-4220-b0f8-667c9d67969a`: 18/18 pass, including the independent panel consumer. This integration run preceded only the private cadence followup and test-fixture corrections; the dependency/API cohort is unchanged. |
| `cargo +1.95.0 nextest run -p nara --test ci_policy --locked --offline --test-threads=1` | `d51c050e-6835-4146-b76b-223f86cfcf91`: 2/2 pass for the new first-party adapter gate and complete workspace commands. |

The committed-source Windows/DX12 run `7bd607b9-2bd0-40b1-be92-54e7c4ddcdc2` executes **7/7**
native trajectories, 245 outside its selection. Command:

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:WGPU_BACKEND='dx12'
$env:DEAR_IMGUI_REQUIRE_WGPU_ADAPTER='1'
cargo +1.95.0 nextest run -p nara-editor -p nara_render_wgpu --features nara_render_wgpu/editor-imgui --lib --locked --offline --run-ignored only -E 'test(secondary_bridge_rejects_foreign_replay_stale_and_unsampled_images) | test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive) | test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) | test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts) | test(native_project_fields_use_upstream_ime_composition_and_reject_stale_owner_commit) | test(native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes) | test(native_canvas_widgets_pick_move_resize_and_save_reopen_real_source)' --test-threads=1 --no-fail-fast --success-output final
```

These cover real secondary samples before any primary acquisition, foreign GPU/replay/stale
content rejection, actual registered-window mapping, detach/minimize/reopen/recovery/retirement,
Game input/refocus/Step, resume cadence, Scene reimport, Canvas edits/save/reopen, and actual Nara
Project path/Profile widgets through Winit/core composition. The device-loss diagnostic in the
lifecycle case is the deliberately exercised recovery path, followed by successful retirement.

## Failures retained and resolved

- First native run `aeca2708` passed five established Editor workflows and failed two new fixtures.
  IME events were sent before the normal delayed IO intent enabled native IME; adding the proper
  ordinary frame lifecycle passed `08ba267d`. Rejected GPU admission needed the normal subsequent
  `update_platform_windows` watermark; completing that lifecycle passed `755cd0d8`.
- A separate warm-window production regression then failed in `755cd0d8`: focus existed but IO
  text intent never advanced under the existing on-demand scheduler. The minimal intent mismatch
  followup passed the complete Editor suite in `8d5467b0`; stable focus requests no continuous
  fast redraw. Independent input correctness review closed this P2.
- Independent renderer review corrected a test's nonempty in-frame scissor to a genuinely
  framebuffer-external scissor. The renderer promises command clipping, not triangle visibility.
  Adapter-required execution subsequently passed. Both source reviews have no remaining findings.

# Follow-up

This acceptance is finite Windows/DX12 evidence. IME tests inject events into owned backend
registrations, not into the desktop: OS candidate-window placement and manual cross-window typing
remain unqualified, as do Linux/macOS release support, accessibility parity, GPU completion and
scanout. Delegated internal surface configuration/acquired extent remains an upstream boundary.
The unrelated quiet-external process trajectory is not re-run by this dependency upgrade; its
historical process/cadence evidence is not relabeled as 0.21 evidence.

CI now requires an adapter for Nara's integration test against its production WGPU 30 resolution;
it does not pretend to execute tests inside a crates.io dependency. No remote publication occurred.
Pre-existing untracked `myeasylog.log`, vendor `Cargo.lock` and `imgui.ini` are preserved.

# Citations

- [Upgrade plan](../../../../plans/2026-10-17-dear-imgui-021-upgrade-plan.md).
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md).
- [Native qualification procedure](../../../../verification/native-editor-qualification.md).
- [Historical renderer patch provenance](../../../../research/2026-10-16-editor-foundation/renderer-020-patch.md).

---
type: "Work Progress"
title: "Public native window composition and complete independent consumers integrating"
description: "Bounded native paint/read-use and sealed CPU observation are implemented; actual multi-window, content and clipped drawing qualification remains in progress."
timestamp: 2026-10-31T21:40:41Z
record_id: "d3f6a6a45a4a4f5c9797bbd00deac863"
producer_id: "codex-foundation-main"
run_id: "window-composition-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "b34741c"
---

# Summary

Continue the complete active foundation Goal. Previous View/history replacement `01bc53b` and
continuity `b34741c` are locally merged; `main = refactor/foundation = b34741c`. This successor is
entirely uncommitted. Do not commit until integration, actual consumers, independent review and
affected native/static checks close. Full0121 remains Proposed. No remote publication.

The only initial untracked directory was `vendor/`; preserve it untouched. Other current source
changes belong to the authorized parent/workers. Reference checkouts remain read-only. Cargo is
strictly serial/jobs1/root target, including independent workspaces.

# Details

## Owning Replacement On Disk

Accepted ADRs0032/0040/0078/0111 now admit the finite trusted native frontend boundary and clarify
Host sample/present facts versus compositor-owned immutable mapping. The production design names
the complete two-window/image, source/processed and clipped drawing consumers.

Core parent files are `host.rs`, `presented.rs`, `frame.rs`, `lib.rs`, and new
`host/window_tests.rs`. The breaking replacement removes old Editor-specific native scope names:

- Public root exports `WgpuWindowDeviceScope`, `WgpuWindowFrame`, `WgpuWindowFrameOutcome`,
  `WgpuWindowPresentedReceipt`, `WgpuImageCompositeReceipt` and concrete GPU retirement.
- `begin_window_frame`, `record_image_sample`, receipt `display_frame_index`, and generic
  `WindowPaint` / `window_paint` replace their old names without aliases. Actual Dear adapter
  names remain `WgpuEditorImgui*`.
- `WindowPaintStorage` pre-admits complete tables and prefix metadata. Native output/display and
  extra-command counts cap at256; samples deduplicate before growth. Composite joins prepare in
  reserved storage before present, and generation commits only after successful present.
- Window/read-use callbacks run under a small private unwind boundary, including actual queued
  uploads on refusal/panic. Encoder finish and Queue submit converge on submitted/current-prefix
  custody even on unwind. User callback panic is attempt-local `NativeCallbackPanicked`; native
  finalization/submission panic is `NativeExecution` with existing owner-replacement meaning.
  Pre-present callback panic returns no present authority. Closed Host refusal precedes capacity.
- `WgpuPolicyFrameReader` borrows the sealed CPU input after Seal/before Publish. It may clone
  replayable meaning without taking the managed transport, UI completion sidecar or connection
  barrier. Do not promote `take_captured_policy_batch` as a consumer shortcut.

Popper completed the disjoint migration of17 existing adapter/Editor/Root files and is closed.
Root formatting has run; independent workspace formatting waits for its writers to freeze.

## Actual Evidence So Far

| Scope | Actual result and limit |
| --- | --- |
| Initial native window metadata/callback tests |2 ordinary passed, run `cd68cdbe-a17d-47b8-b35c-928579d031b8`. |
| Real native read-use/dedup/command bound |2 DX12 passed, run `71b0aa01-14de-418a-8d28-e166aae825f7`,2.345s; before subsequent reader/close-priority addition. |
| Root/Editor all-feature libraries |`check -p nara -p nara-editor --all-features --lib --locked` passed24.16s; before reader addition. |
| Reader/metadata/close ordinary cohort |5 passed /410 skipped, run `02b890f8-a21c-42c8-bb5f-f859947d03c4`,0.649s. Includes original UI sidecar preservation and missing-policy observation. |
| New standalone native window example |LIVE exec86235, `nara-window-composition-consumer-build.log`; no terminal result yet. |

Logs are in the system temporary directory, not tracked memory. No current complete independent,
Editor native, broad owner, minimal/portable or final Clippy qualification exists for this unit.

## Independent Consumer Integration

`tests/fixtures/render-composition-consumer` is new. Parent owns its manifest/README/lock and
integration. Manifest contains explicit root product/render dependencies, WGPU30.0.1/Winit0.30.13.

The first offline-generated lock accidentally selected138 different shared dependency resolutions.
The verified Cargo process family118744/9588 for exec85114 was deliberately stopped to avoid
unrelated dependency upgrades; that session is terminal-1, not a product failure. The new generated
lock was based on root `Cargo.lock`, then resolved offline. Full metadata encountered a missing
Linux-only cached `accesskit_atspi_common`; no-deps metadata completed and confirmed the consumer
package plus zero shared versions outside the root baseline. Exec86235 builds that aligned locked
graph and must be polled before any other Cargo. No build artifacts were deleted or moved.

- Newton `01a193d3-88c3-7e10-aed3-309deb53a92b` completed/froze and is closed. Files:
  `examples/streams_window.rs`, `tests/streams_window.rs`, `tests/support/window_gpu.rs`.
  Two actual native windows share one Host; A captures twice then pauses/retires, B captures60,
  image stream144, across720 deterministic ticks. The image imports old A1 and current B.
  Placement snapshots pin destination/clip/DPI/role; changed mapping admits a new occurrence.
  Parent fixed receipt equality and added actual acquired paint-panic/pre-present-panic phases
  in addition to returned paint refusal. No physical surface-loss claim or fake completion.
- Ampere `01a193d3-56c0-7282-90b3-501cb9c8c5be` is active again on its content files only:
  `tests/content.rs`, `support/content_fixture.rs`, `content_runtime.rs`, `content_readback.rs`.
  It changed the initial draining approach to cached `SystemState<WgpuPolicyFrameReader>`.
  Parent challenged its LocalHeadless recipe requesting DesktopWinit/RenderWgpu: inspect whether
  HeadlessRun can actually drive/cancel original native publication. It may need actual DesktopRun
  and native-only test classification. Do not regain progress by draining private transport.
- Rawls `01a193db-ce88-7781-b980-c31fbfc565fe` is active again on its7 drawing files only:
  `tests/composition.rs`, `support/drawing_{model,gpu,readback}.rs` and3 shaders. Finite8x6 Scene
  patterns,32x24 linear RGBA32Float composition, independent f64 oracle, fractional DPI/letterbox,
  clipped control/popup, alpha-zero and real second-effect budget refusal are source-ready.
  Its initial raw-texture report plus `submit_image_use(&[])` manually rebuilt custody and was
  rejected. Replace with a genuine readback Feature importing the exact typed output, declaring
  COPY_SRC and owning a reserved MAP_READ report through normal Host `submit_program`. No new
  public readback API or raw-texture bypass should be added merely to make the test pass.
- Jason `01a193d3-acf0-7901-8fe2-454f36ca7c8b` remains read-only reviewer. Its initial3 findings
  are accepted: unwind left unbound custody, growth/post-present allocation was unbounded, and
  composite receipts cannot claim immutable mapping without the compositor snapshot. Final draft
  review is pending. Dear delegated native windows retain their separate SDK-report/partial-success
  qualification, not a retroactive promise to control SDK internal allocations or presentations.

# Next Action

1. Poll exec86235 before another Cargo. Fix actual example API/compiler issues. Build then run its
   native subprocess test with explicit DX12/fallback0, using the prebuilt real-main-thread exe.
2. Finish Ampere's actual first-party input workflow and Rawls's proper typed-import readback.
   Compile frozen files serially; do not accept manual native custody or internal batch draining.
3. Collect Jason's final owner review; finish bounded owner tests/native panic evidence. Qualify
   actual content, drawing and streams before broad claims. Preserve real unavailable/minimize/
   resize observations without turning optional OS responses into fabricated success.
4. Inspect/freeze then format the standalone workspace. Run affected owner/public, Root/Editor,
   actual native trajectories, minimal/portable and correctness checks; record exact source limits.
5. Update current guides/ledger/evidence, precise local commit and ff main only after the selected
   complete unit qualifies. Continue the full foundation Goal; do not promote full0121 or close
   editable Inspector/material/motion/Schema work merely because these tracers pass.

# Citations

- `docs/plans/2026-10-31-render-composition-and-host-execution-plan.md`
- `docs/architecture/render-program-production-design.md`
- `docs/architecture/adr/0032-render-backend-integration-boundary.md`
- `docs/architecture/adr/0040-render-resource-lifetime-and-submitter-ownership.md`
- `docs/architecture/adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md`
- `docs/architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md`
- Previous qualified unit `01bc53b`; integration `b34741c`; registration predecessor
  `cfd7154ad052484e8b7411bec7980299`.

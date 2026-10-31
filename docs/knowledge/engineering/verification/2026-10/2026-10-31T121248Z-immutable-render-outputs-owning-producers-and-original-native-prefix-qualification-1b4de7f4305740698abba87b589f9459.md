---
type: "Verification Evidence"
title: "Immutable render outputs owning producers and original native prefix qualification"
description: "Verification Evidence for Immutable render outputs owning producers and original native prefix qualification."
timestamp: 2026-10-31T12:12:48Z
record_id: "1b4de7f4305740698abba87b589f9459"
producer_id: "codex-foundation-main"
run_id: "render-immutable-output-20261031"
---

# Verification

Qualify the production immutable-image/producer replacement based on `c1384a9` on
`refactor/foundation`. The implementation and this record are integrated together; prior native
desktop evidence remains scoped to its original source. Windows11, Rust1.95.0, locked WGPU30.0.1
and explicitly selected DX12 are the native qualification environment. Cargo ran serially with
`CARGO_BUILD_JOBS=1`, sharing the repository target with the independent consumer.

# Result

The production Editor/image-only owner is now `WgpuRenderHost`. Target configuration, immutable
physical pixels, exact display occurrence, reconstruction input, producer admission and native
submission responsibility have independent lifetimes. The old mutable viewport texture/last-frame
lookup and Editor-only execution type are deleted, without compatibility aliases.

`WgpuRenderProducer` is a non-Clone Host-bound admission owner. Copyable IDs are observation facts;
closed producers and their stale reservations cannot reopen. Final target activation exclusively
borrows the Host and producer through its infallible commit. A1/A2 use different storage, charged
through inactive reservations, output leases and actual submitted uses. Device loss and close
revoke interaction independently of passive pixel retention.

Every native attempt pre-reserves original custody before uploads, including failed image/paint
preparation and SDK-owned secondary submissions. The Host retains all unresolved prefixes, not
only the latest one. Callback abandonment has explicit original-device recovery, independent of
output/UI success. Final Host Drop drains original custody or follows the existing fail-stop
policy; a callback unwind cannot release incomplete dependencies. Native read-only imports use
Host-owned submission, rather than rebuilding their own callback/retention protocol.

Dear registrations retain exact output/display leases. Same-window mirrors have distinct binding
and occurrence identities. Game completion designates one occurrence; M-only actual presentation
returns D's original UI permit unchanged, and later D completes it once. Neutral Scene facts retain
output version/occurrence, while document/revision, geometry, mapping and compatible input capture
retain their existing authorities. Original CPU input can reconstruct after actual loss without
resolving current assets; releasing the input leaves it unavailable while its pixels remain.

# Evidence

| Check | Actual result and scope |
| --- | --- |
| Render/WGPU/Editor ordinary owners | **478 passed**,59 skipped; run `1d5cb1e2-3a74-4c14-adca-3701751a0e72`, build34.03s/run132.490s. Complete ordinary libraries after common Host names and owning-producer/activation migration. Subsequent changes are the browser notification seam, additional ignored native release test and documentation; final native/minimal/wasm checks cover those affected paths. |
| Root and neutral preview | **445 passed**,757 skipped; run `11f13242-750c-4da7-a19e-a7d2e330bfa4`, build4m29s/run169.531s. All-feature Root library and `editor_persistence`, selected with `fake_preview_toolkit::`, `desktop_run::`, `game_view::` and `editor::`; includes the five actual neutral toolkit workflows with output/display facts. |
| Final actual native owners and Editor | **27 passed**,474 skipped; run `ab95c246-1dd3-4188-ba71-ddc5665122a1`, build2m17s/run35.101s. WGPU Host10, original-submission8, native bridge3 and selected Editor6. The exact selection is recorded below. |
| Final independent public image consumer | **3 passed**,0 skipped; run `b165310e-4063-46c2-848a-5785cc2f3a8e`, build9.53s/run2.981s. Separate locked workspace with no default features, Root product, serde, toolkit, Winit, Runtime or hidden window. Public owned producers, actual readback of every pixel, A close/B progress, exact/one-short budgets and original input reconstruction after Host retirement. |
| Minimal native product | `cargo +1.95.0 check -p nara --no-default-features --features 'desktop-winit,render-wgpu' --lib --locked` passed,25.30s. No Scene serialization/full2D product requirement is added. |
| Browser compile control | `cargo +1.95.0 check -p nara_render_wgpu --no-default-features --target wasm32-unknown-unknown --lib --locked` passed,1.69s incremental after the correction below. Compile-only, no browser execution or async initialization claim. |
| Correctness lint | `cargo +1.95.0 clippy -p nara_render_wgpu -p nara-editor --all-features --all-targets --locked -- -D clippy::correctness` passed again on final source,23.33s. Existing non-correctness warnings remain; unrelated warning cleanup is excluded. |
| Formatting and links | Root and independent `cargo fmt --all` passed; local links in the current output/ownership/ADR/consumer documents resolve. No migration guide or legacy API alias was introduced. |

The final27 native selection explicitly executed:

- `host::tests::native_*`: retained A1/A2/resize/close pixels, A retirement while B submits,
  irreversible producer close, actual late native submit failure and same-input retry, exact
  output budgets, allocation rejection, original snapshots through loss, input release with
  retained pixels, direct Host Drop and the callback-panic fail-stop subprocess.
- `submission::tests::*`: original native bytes, dropped token handles, B progress, destroyed
  device, preparation-upload/SDK current-prefix settlement, rejected successor, real discarded
  callbacks and explicit rejected-index recovery.
- `host::imgui::tests::*` ignored native cases: exact native secondary bindings and retained
  output occurrences; multiple targets sharing submission demand; partial/foreign/replayed
  observations; paint rejection after an actual queued upload with native readback.
- Editor's `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`,
  `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`,
  `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`,
  `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`,
  `native_ui_canvas_priority_survives_closing_retained_scene`, and
  `native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes`.

Earlier successful groups overlap this final selection and are not additional coverage counts:
ordinary210; first native core14/15; corrected callback/native-bridge5; core17; first Editor5/6;
corrected Scene/Game2; independent3. Independent adversarial review identified owning-contract
defects in copyable producer admission, delayed activation, failed-paint custody, error recovery
classification, abandoned callbacks and raw native-use choreography. They were repaired in the
owning interfaces. Final independent review reported no concrete remaining P1/P2 within the
reviewed native output/producer/consumer contract; it did not run tests or review future Features.

## Corrections And Evidence Limits

The first abandonment test assumed a rejected native index would co-dispatch with a mapping
panic. That ordering is not guaranteed. The replacement separately qualifies real callback
destruction and controlled discarded notification against a real rejected native index; neither
path fabricates completion. Actual original fence/QueueEmpty evidence remains required.

Scene Reimport's temporary composition fixture omitted ordinary dockspace/menu work-area
submission. Returning to normal panels resized the target and invalidated its exact presentation.
The fixture now preserves both obligations and waits for real display confirmation. It still
requires stable current pixels, exact retained predecessor sampling, rejection of predecessor
picking, and no unnecessary ordinary repaint submission. Target checks were not relaxed.

The first wasm check exposed WGPU's unconditional Send callback requirement. The correction
passes only a thread-safe CPU notification through that callback; browser GPU resources stay with
their agent-local owner and settle locally on observation. Native callbacks retain the original
pair/resources. No unsafe Send/Sync assertion or browser execution claim follows.

Actual tests cover selected Windows/DX12 draw, submit, loss, callback and close paths, not physical
monitor scan-out, exhaustive driver OOM/Internal failures, arbitrary panic recovery or another OS.
Logical output-byte accounting is not driver residency or a measured output-pooling/performance
claim. A no-index SDK callback abandoned after submit may require original QueueEmpty for explicit
recovery; ordinary producer retirement still uses its frozen callback/index while B progresses.

# Follow-up

Keep the complete foundation Goal active. Continue ownership-design step4: production typed
graph/Feature adapters, external texture/buffer Feature, alternate composition policy and Nara-drawn
clipped composition through the same native owner. Move stock Sprite/UI across that seam and
delete displaced central branches. Proposed0121 remains Proposed. Complete history/publication
groups, R9b material/shader declarations and the editable multilingual/numeric self-drawn Editor
retain their ordered consumer gates. This record closes neither full R8/R9 nor the foundation plan.

# Citations

- [Current unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Selected ownership design](../../../../architecture/render-host-ownership-design.md)
- [Public guide](../../../../guides/render-image-outputs.md)
- [Independent consumer](../../../../../tests/fixtures/render-output-consumer/README.md)
- [Previous native Session qualification](2026-10-31T105632Z-owned-native-desktop-execution-and-post-host-presentation-qualification-4db921de03c94598b2ee20d7e82508c5.md)

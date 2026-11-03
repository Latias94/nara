---
type: "Verification Evidence"
title: "Native owned scalar Inspector and presented input qualification"
description: "Shared drafts and exact native image authority carry Nara text editing through Apply, document history and disk Reopen; complete Inspector and platform qualification remain open."
timestamp: 2026-11-03T19:23:10Z
record_id: "fc9f002f34ed414e84470bf32a812b8e"
producer_id: "codex-foundation-main"
related_plan: "docs/plans/2026-11-04-owned-inspector-and-editable-text-plan.md"
git_branch: "refactor/foundation"
git_commit: "e6fbe80"
---

# Verification

Windows 11, Rust 1.95.0, root `target`, `CARGO_BUILD_JOBS=1` and serial Cargo invocations.
The source starts at `fdf7b66`; shared segmentation and pointer-route corrections are committed as
`f7148fd` and `e6fbe80`. This record additionally covers the qualified Editor integration working
tree under the [owning unit](../../../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md).
It follows the [shared text prerequisite](2026-11-03T182954Z-bounded-editable-text-and-exact-fallback-glyph-ownership-qualify-the-shared-inspector-prerequisite-9047e27973f9491e9c6cb56b5c49e351.md).

| Gate | Result |
| --- | --- |
| Text editing and pointer interaction controls | 55/55, `4ad5645f-5e5f-4bb9-9e97-289790563d51`; includes complex-script word selection, paint-only capture migration and four fixed-generation invalidation controls |
| Editor and six shared UI/text/platform packages | 730/730, `4dbb8c3c-8c91-4ca4-9858-813707423317`; 53 existing ignored cases excluded; includes an actual generated starter build, READY/Stop and clean Editor exit |
| Final field controllers and selected native regressions | 19/19, `6aa1c551-2a87-444f-b2e8-5b99f225ae7c`; 11 CPU controls plus eight actual Windows/DX12 trajectories |
| Independent public UI workspace | 7/7, `6056e91c-0a82-463e-8feb-bc6f46efd97e` |
| Correctness Clippy | All targets of the seven affected packages pass `-D clippy::correctness`; existing warnings and inherited `EditorRunError` result-size warnings remain |
| Independent review | Two source reviewers close the actionable draft, presentation, focus and pointer-coordinate findings; a separate worker adds the four pointer invalidation controls |

The broad ordinary run precedes the last presented-text-origin and composition-button correction.
The final 19-case selection reruns all affected field controls and the native workflows after those
changes. The subsequent equivalent filter/map cleanup passes final all-target Clippy. Formatting
and documentation checks do not rerun the generated starter build.

The eight native trajectories are the owned scalar Inspector; predecessor field-draft/IME;
predecessor field reset/remove; Canvas editing and persistence; paused-idle resume; Game input,
reopen, resize and exact Step; view detach/minimize/recover/retire; and Project-field IME lifetime.
Their existing source assertions remain enabled.

Commands, executed serially:

```text
cargo +1.95.0 nextest run --locked -p nara_text --test editing -p nara_ui_kernel --test interaction --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --locked -p nara-editor -p nara_text -p nara_ui_kernel -p nara_ui_widgets -p nara_ui_render -p nara_ui -p nara_winit --features nara_ui/serde,nara_winit/accessibility --lib --tests --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --locked -p nara-editor -p nara_ui -p nara_winit --features nara_ui/serde,nara_winit/accessibility --lib --run-ignored all -E 'test(owned_inspector::) | test(field_editor::) | test(native_owned_inspector_pointer_and_ime_drafts_apply_undo_redo_save_and_reopen) | test(native_field_draft_focus_and_ime_survive_unrelated_document_revision) | test(native_field_reset_and_remove_round_trip_authored_scene_and_prefab_overrides) | test(native_canvas_widgets_pick_move_resize_and_save_reopen_real_source) | test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive) | test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) | test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts) | test(native_project_fields_use_upstream_ime_composition_and_reject_stale_owner_commit)' --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/ui-module-consumer/Cargo.toml --target-dir target --test-threads 1 --no-fail-fast
cargo +1.95.0 clippy --locked -p nara-editor -p nara_text -p nara_ui_kernel -p nara_ui_widgets -p nara_ui_render -p nara_ui -p nara_winit --features nara_ui/serde,nara_winit/accessibility --all-targets -- -D clippy::correctness
```

Native runs select `WGPU_BACKEND=dx12`, `NARA_WGPU_FORCE_FALLBACK=0` and
`DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`. The font policy uses the bundled primary and an installed
standalone SimHei TrueType face through `NARA_EDITOR_CJK_FONT`. The trajectory refuses missing-glyph
diagnostics; no system CJK font file is distributed with the repository. All windows in this group
are hidden; the tests do not move the system pointer or activate a visible window.

# Result and corrected failures

The optional `Nara properties` tab uses private Nara controls, the original field-draft store and
the ordinary Application result owner. Preedit, composition completion and focus loss do not mutate
source/history. Explicit Apply captures one draft, local queued/in-flight admission blocks later
input, and only its exact result clears or retains it. The actual workflow edits `draft 中文 café`
and `U64(42)`, performs document Undo/Redo, saves, reopens and compares original entity records and
the complete post-creation unavailable-record manifest.

The existing graphics Host renders the panel image with its original device/queue and retires it
through its existing drain/renderer/view/platform order. Input requires matching image receipt,
display occurrence and owning native window. Repeated unchanged painting reuses the same compiled
frame and retained image receipt, with no continuous follow-up request. This is not an idle-cost
measurement or a second render owner.

The corrections preserve these boundaries:

- Local queued Apply originally left the draft editable before formal admission. An immediate
  queued guard now prevents subsequent typing, Discard or duplicate Apply from being attached to
  the earlier submission result. Failed admission releases that guard and preserves the input.
- Presentation promotion originally changed epochs without migrating pointer state. The Host now
  reconciles and reduces cleanup before new routing. Lost composite authority cancels kernel and
  widget pointer state even when no text field owns focus. Native mapping changes cancel gestures.
- Comparing every layout fact treated caret/selection paint as a new pointer route. The kernel
  compares the actual transform, ordered hit regions and keyed propagation facts. Changed keyed
  ancestry, overlap order, clip and platform transform still cancel capture at fixed generations.
- Blur originally left kernel focus on the old field, preventing refocus from issuing another
  token. Focus release now also settles the old panel interaction. Same-panel button handoff keeps
  the newly accepted press. Tab hiding preserves dirty/in-flight drafts and their result links.
- Focus can scroll a long buffer before the next image appears. Pointer placement now uses the
  exact displayed Text paint origin and rejects unpresented text. Apply/Reapply/Reset stay disabled
  during preedit; pointer selection waits for composition completion or cancellation.
- The pinned Parley non-complex configuration reported missing CJK segmentation data. Its existing
  `complex-scripts` feature supplies dictionary segmentation without system font discovery. Thai
  word selection and local Undo qualify that data path. Chinese visual word stepping retains the
  pinned Parley line/word-boundary behavior; no custom language segmenter is introduced.

Two qualification setup/assertion failures were corrected without weakening product acceptance:
the shell's existing font loader refuses a TTC, so the native coexistence run uses an installed
standalone TTF; the preservation manifest includes complete source placements, so its scalar-edit
baseline is captured after the test's explicit entity creation. The original source entity records
are still compared separately after Save.

# Follow-up and limits

The [native procedure](../../../../verification/native-editor-qualification.md) and
[editing guide](../../../../guides/editable-text.md) describe the supported checkpoint. Boolean,
structured and reference fields remain read-only in the new tab. The predecessor stays until a
complete selected Inspector has equivalent behavior. Clipboard, complete keyboard navigation,
the Nara-owned clipped Scene/external-tool visual, virtualization and measured idle costs remain open.

IME messages here are constructed Winit events. Refocusing rejects a commit before the new Enabled
handshake, but these tests do not prove actual OS event provenance, candidate placement or manual
cross-window typing. They do not read back the native framebuffer or qualify text accessibility,
detached-window/DPI transfer of the new tab, Linux/macOS, full R11 or R12. Prior shared GPU-mask
evidence retains its original scope. Font-policy replacement preserves drafts in source review but
has no dedicated dirty/queued/pending replacement trajectory in this cohort.

# Pending recovery, source editing and game input review

The eighth workflow review started at `6a3d4ac` and followed the RON writer change in
`617b920` and the admitted key index in `06ce6a0` through their qualification at `f595fe7`.
Eight findings reproduced. At that revision an Editor update can leave a project in `Pending`.
Working-tree repairs passed six follow-up checks, including recovery of both types of
interrupted project. The new key index passes the original 32,768-row panel cases. The two
outstanding checks concern the file encoding and the first edit of an omitted default.
[Working-tree retest](#working-tree-retest) · [Seventh review and repairs](2026-11-20-camera-recovery-and-authoring-review.md#repair-status)

## P1 · Changing RON indentation blocks recovery of an interrupted Prefab extraction

Start a Prefab extraction, confirm it, then exit after the Pending manifest reaches disk. Run
the public recovery companion in a new process. The same eight-source consumer project produced
these results using the two actual writers. Both generations wrote v1 files.

| Producer | Recovery reader | Inspect | Resume |
| --- | --- | --- | --- |
| Four-space RON | Four-space | Review | 7 files, Ready |
| Four-space RON | Two-space | `SourceDecoding`, exit 2 | See interrupted run below |
| Two-space RON | Two-space | Review | 7 files, Ready |
| Two-space RON | Four-space | `SourceDecoding`, exit 2 | Inspect already refused |

A second producer stopped with four dirty documents and eight planned outputs. The updated
reader authenticated the same 273,187-byte package as the old reader, then rejected both Inspect
and Resume at step zero. Ordinary Open returned `project.editor.source-authoring-pending`.
The old reader published two outputs and stopped. The new reader again refused, with every
file still byte-identical to its state immediately before that attempt. The old reader then
finished all eight outputs and a new Editor opened the Ready project.

Of the seven reviewed Scene and Prefab outputs, six JSON documents re-encoded byte for byte
with either writer. `prefabs/closed.prefab.ron` decoded with both. The old writer emitted the
original 3,240 bytes and the new writer emitted 2,184. Every line matched after removing its
leading indentation. Recovery decodes an authenticated output and compares it against a fresh
serialization using the **running binary's** canonical formatter. The Editor can therefore read
every source in a transaction and still refuse to finish that transaction. Authored Schema
upgrade recovery uses the same comparison for successor sources. That second operation was
traced in code. [Writer](../../crates/nara_scene/src/format.rs#L1589) · [Output validation](../../src/project_host/authoring/project/prefab_extraction/recovery_data/validate.rs#L637) · [Schema upgrade comparison](../../src/project_host/authoring/source_upgrade_recovery.rs#L916) · [Recovery contract](../guides/prefab-extraction.md#recover-in-a-new-process)

Keep the exact output bytes and their digests as the publication commitment. Verify the decoded
working-to-output relation under the admitted Schema. If canonical spelling is also required
as evidence, record the writer version that defines it and support the existing v1 packages.
The cross-version test should finish both an all-original and a partly published project,
preserving the reviewed bytes and already published files. Godot's text resource loader reads
the declared format and parses the source independently of the current writer's layout.
[Godot format reader](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/resources/resource_format_text.cpp#L1117-L1155)

## P2 · Export rejects compatible uses of one file across two profiles

A menu importer reads `assets/controls-a.json` as raw text. Give the project a byte-identical
`controls-b.json`. Use one manifest with a base Input selection and a Desktop profile override.
Both Checks pass in each configuration, and the two delivery receipts always report the same
digest for `assets/controls-a.json`. The public `prepare_project_export` produces these results.

| Headless Input | Desktop Input | Role of A in each profile | Actual directory result |
| --- | --- | --- | --- |
| A | A | Input / Input | 9 members published, both profiles passed Check in the exported tree |
| A | B | Input / raw asset | `ContentObservationConflict` on A, before staging |
| B | A | Raw asset / Input | Same refusal, before staging |
| B | B | Raw asset / raw asset | 10 members published, both profiles passed Check in the exported tree |

The within-profile closure merges a typed use and a raw read of matching bytes. The Export
planner joins the two independently valid receipts by path but requires the complete member
records to compare equal. Reuse the closure's compatibility rule at the cross-profile join,
retain both profiles' startup observations and copy the file once. The mixed cases should
publish and survive relocation. A changed digest or incompatible typed owners should still
refuse. Godot's export traversal deduplicates dependencies by path. [Within-profile merge](../../src/project_content/closure.rs#L432) · [CLI handoff](../../src/project_tooling/export.rs#L90) · [Cross-profile join](../../src/project_workflow/export.rs#L603) · [Godot dependency traversal](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/editor_export_platform.cpp#L674-L692)

## P2 · An unrelated HUD decoration cancels a held button

In a 200 × 100 menu, press `Resume` at `(20, 20)`. At `y = 66`, change one keyed status bar's
`z_index` from 4 to 6 past another bar at 5. Across the two presented frames the hit regions,
focus order, semantic nodes and Surface mapping compare equal. The button still loses its
pressed state. Releasing at the original point returns `NoPointerCapture` and emits zero
actions. Keeping the original drawing order, or applying the same reorder to the unkeyed
decoration, emits `resume` once. Fully clipping the keyed bar also cancels the press. Partial
clipping preserves it.

The compiler sorts layouts by paint order. Default pointer continuity compares the ordered identity and
visibility of **every** keyed layout, including decorations outside the target's route. Check
the hit ordering and propagation declarations that can affect the captured interaction.
Acceptance should keep the press through both decoration changes, and still cancel when the
button disappears or its action changes. Bevy correlates release and the earlier press on an
entity. Godot routes release to the saved mouse focus. [Layout order](../../crates/nara_ui_kernel/src/compiler.rs#L355) · [Continuity comparison](../../crates/nara_ui_kernel/src/interaction/continuity.rs#L17) · [Capture cancellation](../../crates/nara_ui_kernel/src/interaction/state.rs#L316) · [Bevy pointer events](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_picking/src/events.rs#L1109) · [Godot viewport](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/main/viewport.cpp#L2058)

## P2 · Tab is lost between the Windows game window and Runtime UI

In a hidden HWND with `Resume` focused and `Settings` next, post Tab down and up, then Enter.
The input timeline receives no Tab transition and the action journal records `resume`.
Right then Enter in the same window records `settings`. Supplying the Runtime input driver
with `KeyCode::Character('\t')` then Enter also records `settings`.

Winit's physical key converter has no Tab case. Its caller returns before publishing either
transition. The UI already has the sequential focus action, and the Reference Game tests enter
downstream of conversion. Give Tab an agreed input representation from the native converter
through UI navigation. Verify down, up, one focus advance, activation, and wraparound with a
second Tab. Bevy maps Winit's Tab explicitly and Godot binds it to `ui_focus_next`.
[Window ingress](../../crates/nara_winit/src/lib.rs#L1776) · [Key converter](../../crates/nara_winit/src/lib.rs#L2474) · [Runtime navigation](../../crates/nara_ui/src/interaction/timeline.rs#L256) · [Reference Game test](../../reference-game/tests/support/ui_product_fixture/interaction.rs#L55) · [Bevy key conversion](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_winit/src/converters.rs#L170) · [Godot action](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/input/input_map.cpp#L484-L485)

## P2 architecture · A valid RON source becomes read-only when its JSON spelling is too large

A keyed inventory has two `u64` fields per row and a separate scalar `amount = 7`. With 39,000
rows, the RON Scene accepts a last-row edit and an amount change. Commit, Undo and Redo succeed
for both. At 39,500 rows the canonical RON still opens, the last row and `amount` still read,
and a separately authored `amount = 8` round-trips in exactly the original RON byte count.
The Workspace rejects either edit with `scene.authoring-field-edit-format-invalid`. The
revision and original values survive the refusal.

| 39,500-row source | Original RON | RON after changing 7 to 8 | Canonical JSON |
| --- | ---: | ---: | --- |
| Scene | 16,529,315 bytes | 16,529,315 bytes | Exceeds 16,777,216 bytes |
| Prefab | 16,529,316 bytes | 16,529,316 bytes | Exceeds 16,777,216 bytes |

The current editing contract validates both encoders on every publication. The recent
two-space RON change lets the seventh review's fixture fit. This example reaches the reverse
boundary, where **JSON** blocks an edit to a RON file. For a file-bound source, validate the
candidate's shared structure and the destination's actual encoding. Apply a second encoder's
limit when requesting that format. If universal convertibility is a deliberate product
requirement, surface the blocking format at admission so the Editor can explain which changes
are available. Godot selects a saver using the resource and destination path. [Scene publication](../../crates/nara_scene/src/authoring_decode.rs#L2808) · [Prefab publication](../../crates/nara_scene/src/authoring_decode.rs#L3077) · [Two encoders](../../crates/nara_scene/src/format.rs#L1476) · [Godot saver selection](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/io/resource_saver.cpp#L111-L130)

## P2 architecture · The first edit of an omitted default still depends on every sibling row

Use the same optional Schema field and the same 86-row default in two documents. Store the
list explicitly in one and omit it in the other. Both return `86` for the final row's value.
The stored row accepts `86 → 85`, Undo and Redo. The omitted row refuses preparation with
`scene.authoring-field-patch-unavailable`. Reducing the default to 85 rows allows the edit and
Undo restores `Defaulted(85)`. The separate `amount` field remains editable in all cases.
Both Scene and Prefab reproduced this boundary. The omitted 86-row Scene is only 943 bytes
as JSON, and the Prefab is 944 bytes.

During the first descendant edit the source owner materializes the default and lowers the
change to `SetField`. Its payload costs `1 + 3 × rows` value nodes. The 256-node limit admits
85 rows and rejects 86. This is the documented omitted-root history design. The new index
fixes lookup and the explicit-root edit, leaving the choice of storage visible to a designer
changing one number. Represent materialization from the exact admitted default in history,
with an inverse that reinstates absence. Check the final source size independently. Acceptance
should exercise old and new values, original absence after Undo and Save/Reopen for both
source kinds. [Root initialization](../../crates/nara_scene/src/authoring/property.rs#L194) · [Root patch](../../crates/nara_scene/src/authoring/property.rs#L231) · [Edit limit](../../crates/nara_scene/src/authoring_decode.rs#L647) · [Existing design](../architecture/authoring-read-projection-design.md#selective-edits-in-authored-collections)

## P2 in the Two-Level Game · An unloaded pickup still awards points and plays its sound

From the Meadow spawn, walk right for 14 fixed steps. Repeat after a game plugin calls
`retire_and_despawn_scene_entity` on `gem-near`. Before moving, the second run reports
`Tombstoned(Despawned)` and one visible pickup instead of two. Both runs reach
`x = −75.20004`, award 10 points, record one collected pickup and finish the same pickup
sound. Its WASAPI callback peak is `0.24655151` in both cases.

The contact test uses positions cached at level preparation. It commits progress before
looking up the current entity. An absent entity skips sprite hiding and still publishes the
sound. Resolve the live pickup and its fixed-step position before committing collection.
Use that committed result for score, visibility and audio. Test a retired pickup and one
moved away from its authored position. Bevy's Breakout example tests live entities before
emitting the event consumed by audio. [Position cache](../../samples/two-level-game/src/gameplay/content.rs#L159) · [Pickup commit](../../samples/two-level-game/src/gameplay/mod.rs#L558) · [Retirement API](../../crates/nara_scene/src/spawn.rs#L1720) · [Bevy collision and audio](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/examples/showcase/breakout.rs#L339-L400)

## P2 in the Two-Level Game · A full diagnostic history ends a healthy desktop session

Stand still in Meadow with callback PCM capture disabled. Repeatedly request Retry and wait
for the device to apply the new Scene instance. After 4,096 retries the device reported zero
faults and zero accepted effects. Retry 4,097 returned `audio scope record capacity exhausted`.
The original interactive loop exited and closed its window and device. In a separate run that
kept the Runtime alive, travel to Cavern succeeded but its sound had zero accepts and starts.

Draining the device queue still appends each `ScopeApplied` to a lifetime-limited game journal.
Once full, its observation error suppresses later playback and reaches the interactive exit
path. Keep the current device scope independently of retained diagnostic history. A bounded
history can report overwritten records to a verifier. After 4,097 retries the interactive window
should stay open, the next travel cue should start once and switching scenes should still
cancel old voices. Godot reclaims finished playbacks and applies its polyphony limit to live
voices. [History bound](../../samples/two-level-game/src/audio.rs#L49) · [Record handling](../../samples/two-level-game/src/audio.rs#L240) · [Playback suppression](../../samples/two-level-game/src/audio.rs#L384) · [Desktop exit](../../samples/two-level-game/src/bin/audio_desktop/workflow.rs#L367) · [Godot voice retirement](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/audio/audio_stream_player_internal.cpp#L79-L99)

## Baseline repair order and verification

Fix recovery first. Keep a package produced by the earlier writer as a cross-version fixture,
including a RON output already published to disk. Verify exact output bytes with the independent
companion and confirm that ordinary Open succeeds at Ready. Exercise the mixed-profile source
Export through directory publication and qualify UI fixes by the action the game receives.
For authoring, test both explicit and defaulted roots and use the document's
actual on-disk format as the comparison. The two sample corrections can be verified independently.

- Windows x64, Rust 1.95.0. The serial release nextest selection passed **46/46** in run
  `bfa9d840-0c71-4391-81fc-1cc920152381`. It includes authoring queries, property children,
  canonical formats and retained key positions. The two independently locked registered-panel
  tests also passed from their release binary. They exercise 32,768 explicit rows, key changes,
  duplicate refusal, Undo/Redo, Save and Reopen. The new default and format probes used public
  `EditorWorkspace` and the actual Scene and Prefab encoders and decoders.
- Recovery used separate producer and reader processes, actual Pending manifests and project
  files. The old and new companions used the same public CLI source linked to the respective
  engine libraries. The seven-source interruption and the eight-source consumer project supplied
  independent controls. [Producer interruption](../../src/project_host/runtime/editor/prefab_extraction/tests/process_recovery.rs#L219) · [Public recovery companion](../../tests/fixtures/prefab-extraction-recovery/src/main.rs) · [Panel regressions](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages.rs)
- The Export probe used a real external text importer, actual profile Checks, captured builds
  of two small fixture binaries and the production planner and directory publisher. Its product
  agreement observations were supplied by the fixture. Launching the stock Desktop product from
  the published tree was outside that probe. Four existing controls covered shared raw Input,
  a raw sidecar dependency, foreign Products and changed bytes across profiles. All ten external
  Cook selection regressions also passed. [Shared-input Player control](../../src/project_tooling/project_cook/content/tests.rs#L414) · [Different-byte Export control](../../src/project_workflow/export/source_tests.rs#L1058)
- The 72 UI interaction controls and the previous Vulkan pixel-edge test passed. The decoration
  case used normalized Runtime input and two CPU-presented frames. Tab used posted native key
  messages and an actual Winit event loop. The audio tests used semantic game commands in a
  hidden desktop window and the WASAPI callback. The existing sound verifier accepted 5 and 7
  effects on its two normal paths, with zero maximum PCM error in both. Physical keystrokes,
  foreground mouse delivery for the new decoration case, other platforms and the full workspace
  suite were outside this review.

## Working-tree retest

Changes to the affected systems arrived during the review on November 20–21. The findings
above describe the named baseline. These checks reran the scenarios against the revised
implementations in the shared checkout.

- **Prefab extraction recovery passed.** The updated companion inspected and finished a
  four-space v1 project with all outputs pending and a second with two already published.
  All eight output files in each project matched the original Review bytes exactly. The two
  already published files kept their file identities and modification times. Guards kept
  their original bytes. The revised reader admits stored bytes and checks their commitments.
  The producer retains its canonical-output check. The Schema upgrade path has a similar
  change and still needs its own cross-version process case. [Producer and reader](../../src/project_host/authoring/project/prefab_extraction/recovery_data.rs#L554) · [Source admission](../../src/project_host/authoring/project/prefab_extraction/recovery_data/validate.rs#L623)
- **Both menu cases passed.** The same status-bar reorder and the same complete clip now
  preserve `pressed` and emit `resume` exactly once on release. On the hidden HWND, native
  Tab publishes both `Pressed` and `Released` and moves focus to `Settings`. The following
  Enter emits `settings` once. These checks rebuilt the working-tree UI and Winit modules
  against the existing dependencies and reran the same input probe. [Route comparison](../../crates/nara_ui_kernel/src/interaction/continuity.rs#L19) · [Native mapping](../../crates/nara_winit/src/lib.rs#L2479)
- **All four Export combinations passed.** With the revised `merge_usage` join, the two mixed
  selections each published a ten-member directory. Both profiles passed Check in each exported
  directory, preserving its selected Input and the menu's original A bytes. A/A still published
  nine members, and B/B still published ten. [Receipt merge](../../src/project_content/closure.rs#L90) · [Export planner](../../src/project_workflow/export.rs#L603)
- **Pickup scoring passed.** The updated game passed both Headless cases. Walking over the
  retired pickup left score and collection at zero. A reparented pickup kept its visible
  sprite and scored once at its new fixed world position, then stayed collected on the next
  tick. The repaired pickup's sound was outside this Headless run. [Live pickup check](../../samples/two-level-game/src/gameplay/mod.rs#L546)
- **The audio rollover passed on WASAPI.** Starting from Meadow, a hidden desktop session
  applied 4,097 consecutive retries with zero accepted cues, zero faults and playback enabled.
  Traveling to Cavern then accepted and started exactly one cue. The witness reported two
  evicted history records and the session retired normally. [Journal eviction](../../samples/two-level-game/src/audio.rs#L240)

Two authoring cases still needed verification at the 00:04 CST cutoff on November 21. The
working tree was gaining both the source encoding on Scene/Prefab candidates and a history
operation for materializing an omitted default. During the attempted rebuild, Scene callers
were still being updated for the changed candidate and history interfaces. The last measured
results for the 39,500-row RON edit and the 86-row omitted default are the baseline results
above. Qualify each through preparation, commit, Undo, Redo and Save/Reopen against the
integrated implementation.

## Repair status

| Workflow | Repair and regression |
|---|---|
| Prefab extraction and Schema upgrade | Recovery parses committed v1 bytes and verifies the stored transition under the admitted Schema. It republishes exact committed bytes and preserves the identity of files already published. Preparation still checks the current encoder's output. The process cases cover four-space RON, including a mixed publication, and a Schema upgrade case rejects a structurally altered successor with recomputed commitments. [Extraction process cases](../../src/project_host/runtime/editor/prefab_extraction/tests/process_recovery.rs) · [Schema upgrade process case](../../src/project_host/authoring/project/source_upgrade_resume/tests.rs) |
| Bound RON authoring | Scene and Prefab candidates retain their binding's encoding through classification, edits, history and persistence. Explicit publication checks the selected destination writer and reader limits. The registered panel edits an independent scalar and the final keyed row in both 39,500-row sources. Explicit JSON output still rejects the same documents at its 16 MiB limit. [Panel workflow](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages/round_eight.rs) |
| Omitted defaults | The first nested edit records the selected value and an exact witness of the default in Session history. Undo verifies the reconstructed value before restoring omission. The 86-row Scene and Prefab workflows save and reopen both omitted and explicit states. [History contract](../architecture/adr/0026-editor-command-patch-and-undo-model.md) · [Panel workflow](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages/round_eight.rs) |
| Cross-profile source Export | The planner joins compatible uses for one path and digest while retaining each profile's original observations. The four profile pairings publish and run Check again in their new directory. [Export workflow](../../src/project_workflow/export/source_tests.rs) |
| Menu capture and Tab | The interaction owner tests continuity of the held action and its route across decorations. Winit forwards Tab through its focused window. [Route continuity](../../crates/nara_ui_kernel/src/interaction/continuity.rs) · [Windows ingress](../../crates/nara_winit/src/lib.rs) |
| Two-Level Game | Pickup scoring observes the live Scene occurrence and completed fixed-step position. The audio witness retains a bounded chronological suffix and exposes eviction to the source and cooked verification workflows. [Pickup workflow](../../samples/two-level-game/tests/pickup_gameplay.rs) · [Audio callback cases](../../samples/two-level-game/src/audio/device_tests.rs) |

Windows release input qualification passed 72 UI kernel, 131 Runtime UI, and 82 Winit
library cases, plus two selected hidden-window native cases. The run identifiers are
`b71d9d54-f1b1-48f2-922e-1dca898096a5`, `65cf3ac2-0b8f-4985-8a7b-aae783669f2c`,
`e44cef3d-7b38-4281-af7d-6f4e98cef261`, and `74550e95-e0f3-48c1-89d7-5444146455e1`.

The independently locked Editor panel release run passed 24 cases, with one unrelated case
skipped (`782b1812-a575-4d25-a896-be3556c4f8c6`). It includes the four original authoring
reproductions, both adjacent 32,768-row keyed workflows, and the existing panel's compact
forward/inverse inspection. The saved RON files match the expected bytes after Undo, Redo and
Reopen. Reopening the saved JSON controls reads `Defaulted` after omission is restored and
`Present` after the selected write is published again.

The project and Scene release selection passed 386 of 387 cases
(`7978aebc-f1e4-4c05-afc9-5bbba4d914e8`), including all 209 project cases. The sole failure
asked the already-decoded transition checker to count serialized bytes. The corrected control
exercises actual JSON and RON decoding for both source kinds and retains the transition's
decoded-value and scratch-release assertions. The eight targeted Scene recovery cases then
passed under the debug test profile (`24ab98db-4359-449e-8ee3-ab6ac122a773`).

The project cases include `four_space_v1_schema_upgrade_resumes_in_a_fresh_process`. The fixture
constructs the earlier four-space RON layout and publishes the Prefab before invoking the
current reader in a fresh process. The test checks exact output bytes and the already published
file's identity. The independently locked recovery companion also passed the four-space
extraction process regression with both publication states.

The Two-Level Game release selection passed 144 cases (`56ac5871-6b92-41a0-a129-b62183374e85`).
It includes retired and moved pickups, both PCM verifiers' incomplete-history refusal, and a
deterministic device callback that starts and retires a Travel cue after scope history eviction.
Five selected native cases passed on Windows (`db40ea2e-ec10-49da-b7e2-ddd4d1061779` and
`44644800-5af9-4336-800e-f5242978ca48`). They cover the source desktop, the relocated cooked
Player, development audio enablement, edited and rebound WAVs, and a cue that finishes before
Retry. The desktop cases use DX12 and the original WASAPI callback. This repair's sustained
rollover gate is deterministic. The earlier 4,097-retry WASAPI observation remains recorded in
the working-tree retest above.

The root and sample formatting checks, the independent panel package formatting check, and
Clippy correctness for the affected engine libraries and sample targets passed. These results
cover the selected Windows x64 workflows on Rust 1.95.0.

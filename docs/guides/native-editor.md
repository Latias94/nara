# Edit Night Relay in the native editor

Place the Nara and Open-GPUI v2 checkouts next to each other, with the Open-GPUI directory named
`open-gpui`. From Nara, build and open the reference project with Rust 1.95.

```powershell
$env:CARGO_BUILD_JOBS = '1'
cargo +1.95.0 run --locked --manifest-path crates/nara_editor/Cargo.toml --target-dir target -- --project reference-game
```

Start without `--project` to open Project Manager. The project folder contains `nara.toml`. The
optional `.nara.ron` selects an entry scene relative to the project's asset directory.

## Edit the scene

1. Select **Trust Project** for a project whose Cargo configuration and Rust code you intend to
   run. Then choose **Refresh Schema** to discover its custom components. Select **Round rules**
   in Hierarchy and edit **Shield charges** in Inspector. Press Enter to commit the draft.
2. Select **Courier** in Hierarchy or Scene View. Use Shift to include multiple rows in Hierarchy.
   The Inspector marks fields whose values differ. Edit a shared field to apply it to all selected
   entities as one Undo action.
   A color field offers an sRGB picker and exact linear-channel inputs. Apply a picked color to
   commit it in one Undo action.
3. In Scene View, **Q**, **W**, **E**, **R**, and **B** select the tools. Drag a Gizmo handle to
   transform the selection around its last selected entity. A child follows a selected ancestor.
   Hold Shift to snap translation, and press Escape to cancel the current gesture. Middle-button
   drag pans, the wheel zooms at the pointer, and **F** frames the selection. Choose a PNG in Assets
   and use the Tile Brush for grid placement, or drag a PNG onto the scene to place one sprite.
4. Use **Undo**, **Redo**, or **Save** on the Scene View toolbar. The saved RON contains typed
   component values and stable entity UUIDs. Unknown components remain available for inspection
   and survive saves. **Remove** deletes a component from the selected entities that have it, and
   Undo restores its value. Saved layouts live in `.nara/editor-layout.json` beside the Schema cache.
   The editor accepts layout files up to 1 MiB and restores a floating window only when it contains
   an editor pane.

Select **Power cell B** to inspect a prefab instance. Edits to inherited fields become overrides.
Use **Revert** to inherit the prefab's current value again. **Apply to Prefab…** asks for
confirmation before saving the value to the shared prefab and clearing the instance override.
Save the scene to retain the cleared override. Scene Undo can restore the local override.

Keep assets and their `.meta` files together when renaming or moving them. Refresh **Assets** to
update the index. On the next scene save, the asset aliases keep their UUIDs and gain current paths.

Drag a panel tab to split or group panes. Drag outside the editor to float a pane where the native
platform supports peer windows. Closing a newly floated window docks its panes back. For a floating
window restored from a saved layout, drag its tabs into the main editor. The Inspector's name field
accepts the platform's IME. Closing the main window with unsaved edits prompts you to save,
discard, or cancel. Unapplied Inspector drafts prompt you to review or discard them first. For an
untitled scene, choose a new filename ending in `.scene.ron` inside the asset directory.

## Play the current edit

**Play** builds the project and opens the game in a separate native window. An ordinary project
runs with its usual launch arguments and saved data. **Stop** terminates and reaps either kind of
game process tree. Cargo and game output appear in Console.

To play a temporary snapshot of the current edits, add the following to `nara.toml` and implement
the editor protocol in the selected game binary. The editor keeps the source scene and its Undo
history throughout Play.

```toml
[editor]
play = "snapshot"
```

The game reads `--editor-play PATH` and prints `NARA_EDITOR ready` after its first frame. When the
editor sends `pause`, `resume`, or `step` on stdin, the game replies `NARA_EDITOR paused`,
`NARA_EDITOR running`, or `NARA_EDITOR stepped` after applying it. Step advances one fixed tick.
Night Relay implements this protocol in its development desktop build.

## Verify the native workflow

On a native desktop with WGPU, run the built editor with `--project reference-game --trusted
--verify`. It uses a temporary copy for scene saves, checks the shared viewport before and after
resize, verifies retained presentation and one idle refresh, floats the Inspector, and round-trips
the layout. Night Relay reports the edited shield count and acknowledges Pause, Step, and Resume.
The editor waits for Stop to reap the game process tree and checks that the source and edited
documents still match their pre-Play values. Success prints `Night Relay editor verification passed`.

The Open-GPUI dockspace editor also provides `--smoke-native` and `--smoke-recovery`. Both run to a
result on a Windows desktop without pointer or keyboard interaction. The separate mixed-DPI native
placement test requires two displays with distinct scaling.

For a GPU capture, launch the built editor with `--project reference-game --capture-frames 180`.
It opens one window, warms the shared viewport, presents the requested frames, prints the texture
and buffer creation counts, and exits. PIX on Windows can launch the DX12 editor directly in this
mode. Allow enough frames for the capture tool to reach the warmed-up portion of the run.

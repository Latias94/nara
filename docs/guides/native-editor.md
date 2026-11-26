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
3. In Scene View, **Q**, **W**, **E**, **R**, and **B** select the tools. Drag a Gizmo handle, hold
   Shift to snap translation, and press Escape to cancel the current gesture. Middle-button drag
   pans, the wheel zooms at the pointer, and **F** frames the selection. Choose a PNG in Assets and
   use the Tile Brush for grid placement, or drag a PNG onto the scene to place one sprite.
4. Use **Undo**, **Redo**, or **Save** on the Scene View toolbar. The saved RON contains typed
   component values and stable entity UUIDs. Unknown components remain available for inspection
   and survive saves. **Remove** deletes a component from the selected entities that have it, and
   Undo restores its value. Saved layouts live in `.nara/editor-layout.json` beside the Schema cache.

Drag a panel tab to split or group panes. Drag outside the editor to float a pane where the native
platform supports peer windows. The Inspector's name field accepts the platform's IME.

## Play the current edit

**Play** builds the project and opens the game in a separate native window using a temporary copy
of the current scene. **Pause**, **Step**, and **Resume** become available when the running game
confirms its state. Step advances one fixed tick. **Stop** terminates and reaps the game process
tree. The editor keeps the original scene and its Undo history throughout Play. Cargo and game
output appear in Console.

The default `game` binary must implement the editor protocol to support these controls. It reads
`--editor-play PATH`, signals `NARA_EDITOR ready` on stdout after its first frame, and reads
`pause`, `resume`, and `step` lines on stdin. It replies `NARA_EDITOR paused`, `NARA_EDITOR running`,
or `NARA_EDITOR stepped`, respectively, after the operation completes. Night Relay is the reference
implementation. Its game runner has no dependency on the editor or Open-GPUI.

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

# Nara Editor

`nara-editor` is Nara's standalone first-party authoring application. It opens a project from its
data contract (`nara.toml`, the authoring schema bundle, Scenes, Prefabs, and referenced assets)
without compiling or linking that project's Rust code.

## Build

From the repository root:

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo build -p nara-editor
```

The executable can then be reused without rebuilding it for each project.

## Open the Project Manager

```powershell
.\target\debug\nara-editor.exe
```

## Open a project directly

```powershell
.\target\debug\nara-editor.exe .\reference-game --profile desktop
```

The project must declare a valid authoring schema bundle. Admission failures remain inside the
running Editor and are shown through Recovery, where the project can be repaired and retried.

The ordinary authoring lane currently supports the admitted Scene and Prefab workspace, safe scalar
editing, persistence, Scene preview, UI Canvas preview, and explicit PNG image Import/Reimport.
Records without an installed exact authoring adapter remain visible but read-only. Supported saves
preserve their exact owner, version, value, placement, and reference facts in canonical project
source. Ordinary Play/Run is intentionally unavailable until Nara has a separate external
execution Host; the Editor never falls back to compiling or executing project Rust in-process.

## Import a PNG image

Open the Project sidebar, select a PNG under the project's configured assets root, choose `sRGB`
or `Linear`, and press `Import`. The Editor creates or updates the adjacent canonical `.meta` file
and publishes the accepted image into the current authoring session. A UI Prefab can then select
that logical image path from its brush field and show it in the native UI Canvas.

`Reimport` applies a changed color-space recipe through the same explicit transaction. Saving a
Prefab records only the logical image reference. Reopening the same project hydrates referenced
images from the admitted authoring closure; it does not compile project Rust. If decoding or
publication fails, the current last-good image and Canvas remain available and the failure is
reported through bounded Editor diagnostics.

The first workflow is deliberately narrow: PNG only, one explicit image transaction at a time,
and manual Import/Reimport. It does not provide file watching, batch import, an import cache, other
image formats, or implicit project-code execution.

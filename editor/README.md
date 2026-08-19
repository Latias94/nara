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
editing, persistence, Scene preview, and UI Canvas preview. Records without an installed exact
authoring adapter remain visible but read-only. Supported saves preserve their exact owner,
version, value, placement, and reference facts in canonical project source. Ordinary Play/Run is
intentionally unavailable until Nara has a separate external execution Host; the Editor never
falls back to compiling or executing project Rust in-process.

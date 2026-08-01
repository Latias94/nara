# Nara Reference Game

This small 2D game validates Nara's public headless and desktop product paths. It exercises project
manifest loading, versioned scene and prefab data, image import, semantic gameplay commands, fixed
ticks, sprites, input, HUD rendering, retry, runtime shutdown, and stable headless output.

It is a development and product-regression fixture, not a finished commercial game.

## Rust Composition

The checked-in launch entries use the same pure Rust `ProductRecipe` as the reference game's
headless, desktop, and editor product paths. `wave_recipe()` binds the gameplay plugin and its
schema provider once; `desktop_wave_recipe()` adds only the desktop presentation plugin. Project
loading, runtime admission, driving, and bounded shutdown remain owned by Nara's product Hosts.
The fixture does not assemble plugin definitions, slot edits, or parallel schema-provider lists in
its normal launch path.

## Run From Source

From the repository root, run the deterministic headless wave:

```text
cargo run --manifest-path reference-game/Cargo.toml --locked --bin headless
```

The command writes exactly one terminal JSON object to standard output. `--max-ticks` accepts a
value from 1 through 256 and returns a non-zero exit status if the wave does not finish.

Run the desktop game:

```text
cargo run --manifest-path reference-game/Cargo.toml --locked --features desktop --bin desktop
```

See `CONTROLS.md` for the input map.

## Project Data

`nara.toml` is the project settings authority. The startup content closure includes:

- `scenes/startup.scene.json`
- `prefabs/enemy.prefab.json`
- the referenced image assets and `.meta` records under `assets/`
- the component schema fixtures under `schema/`

The packaged binaries do not search the current working directory or user home for these files.

## Standalone Candidate Layout

The release-candidate tooling creates one fixed ZIP with this shape:

```text
nara-reference-game/
  manifest.json
  README.md
  CONTROLS.md
  LICENSE-MIT
  LICENSE-APACHE
  bin/
    headless[.exe]
    desktop[.exe]
  tools/
    desktop-render-probe[.exe]
  project/
    nara.toml
    assets/
    prefabs/
    scenes/
    schema/
```

Packaged executables resolve only the sibling `project/` directory. Candidate consumers execute the
formal `bin/desktop[.exe] --candidate-smoke` entry, which runs the normal desktop product recipe and
exits only after a bounded submitted product frame. The desktop render probe remains a
measurement-only utility; it cannot substitute for the formal desktop smoke. Each consumer run
creates an isolated extraction, home, working-directory, and temporary root beneath the supplied
safe work parent. The archive consumer verifies the fixed entry table, regular-file modes, byte
budgets, manifest, and every file digest before extraction.

No public candidate has been published yet. A local archive is preparation evidence only until the
documented hosted Windows and Linux candidate and no-checkout consumer jobs pass.

## Asset Attribution

The Tiny Dungeon art is provided by Kenney under CC0. The bundled attribution record is
`assets/kenney-tiny-dungeon.LICENSE.txt`.

# Test fixtures

This directory contains test data and small consumer projects. The consumer projects model
external callers of Nara's public APIs; they are not engine modules or user-facing sample games.

| Project | Purpose |
|---|---|
| [Module consumers](module-consumer/README.md) | Direct Scene, schema, filesystem, material and Editor panel use, plus the public project-workflow facade and compiled source asset domains. |
| [UI module consumer](ui-module-consumer/README.md) | Text, UI compilation and widgets without the engine's ECS, Scene, window or renderer graph. |
| [Product slices](product-slices/) | Focused root-facade feature combinations. |

Consumer workspaces own their manifests and lockfiles, with explicit local dependencies and no
root workspace inheritance. Keeping the UI consumer separate also prevents the broader consumer
graph from supplying undeclared dependencies or unified features.

Run the documented commands from the repository root. Set `CARGO_BUILD_JOBS=1` and
`CARGO_TARGET_DIR` to the repository's absolute `target` path to reuse build artifacts, and run
Cargo commands serially. The root workspace excludes these independent consumers, so ordinary
root tests alone do not execute them. CI invokes each supported consumer explicitly.

The full module workspace includes Windows-specific filesystem/project-workflow checks. CI runs
the supported scene, material and panel consumers separately on Linux. Passing these fixtures
does not establish native platform, released-package or arbitrary module-combination support.

## Earlier paths

The two consumer directories previously lived at the repository root. Historical plans and
immutable engineering records retain those original paths and their tested source revisions.
Use `tests/fixtures/module-consumer/` and `tests/fixtures/ui-module-consumer/` for current source.
For the original text, use `git show 73c6857:module-consumer/<path>` or the corresponding
`ui-module-consumer` path. Relocation does not extend earlier verification claims.

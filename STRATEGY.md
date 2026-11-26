# Nara strategy

Nara is an open-source game engine for teams building complete games in Rust. The first production
proof is a 2D desktop game that authors can edit, play, debug, and export using one integrated
workflow. Rust is the complete first-party authoring language. Data and visual workflows serve artists
and designers working in the same project.

## Product direction

- Grow the engine from complete games. Use `reference-game` to prove the public 2D runtime, its
  headless simulation, and its desktop presentation.
- Deliver a standalone editor with multi-window docking, a live scene viewport, schema-driven
  authoring, and supervised Play for an external Rust project.
- Keep shipping games small and independent of editor infrastructure. Use one lean desktop runner
  for games and let the editor own its own windowing and user interface.
- Design for growth to the breadth of Bevy and Godot. Keep the core independent of scene dimension,
  and let real projects drive adoption of 3D, networking, scripting, and additional targets.
- Develop for Windows, Linux, and macOS. Ground each release claim in evidence from the platform.

## Measures

- **Edit-to-result latency** measures the median and 95th percentile for data saves, logic edits, and
  structural Rust changes in the reference game. Record the reload or rebuild path with each result.
- **Public API coverage** measures how much of the reference game and editor workflow uses public APIs.
- **Authoring and delivery** exercise editing, Undo, persistence, supervised Play, and a clean-machine
  launch of an exported game.
- **Runtime cost** tracks 99th-percentile frame time, memory, and steady-state GPU allocations on
  identified hardware.
- **Build cost** tracks cold and warm CI time, workspace package count, and maintained source size.

The first production proof focuses on 2D desktop. A second official authoring language, native
dynamic ABI, universal scripting host, package registry, and marketplace can wait for a concrete
consumer. Data reload and incremental Rust rebuild with process restart cover the initial iteration
workflow.

The [architecture overview](docs/architecture/overview.md) owns product boundaries. The
[v2 master plan](docs/plans/v2-refactor-master-plan.md) owns delivery order and phase verification.

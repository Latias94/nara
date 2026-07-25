---
name: Nara
last_updated: 2026-07-12
---

# Nara Strategy

## Target problem

Game developers and studios that want to build in Rust face a tradeoff: mature
engines provide integrated production workflows but do not treat Rust as a
first-class authoring language, while Rust engines often leave editor workflows,
iteration, debugging, packaging, and delivery integration to each game team. The
resulting infrastructure work competes directly with finishing the game.

## Our approach

Develop Nara from complete games outward as a Rust-first integrated product built
from explicit, composable modules. Public Rust APIs must support the whole game
production path, while the default module combination prioritizes a coherent
experience. Stable extension seams allow modules, backends, editor tools, and
optional scripting adapters without requiring a second official author language.
Generalize only when a complete game or focused external evidence creates pressure.

## Who it's for

**Primary during Trial:** Rust game developers and studios taking a complete PC
game from prototype to release. They are hiring Nara for Rust control together
with an integrated editor, runtime, tooling, and delivery path. A team may include
artists, designers, and other contributors who work through visual and data
workflows; Nara is not limited to teams in which every contributor is a programmer.

## Key metrics

- **Rust edit-to-result latency** - P50 and P95 for representative data, function
  body, and structural Rust changes in the reference game, reported separately by
  the iteration path used.
- **Public production coverage** - Percentage of the reference game built through
  public Rust, project, editor, and tooling APIs without engine-private hooks.
- **Release reproducibility** - Clean-machine Windows/Linux export and headless
  replay acceptance rate.
- **Module integration quality** - Success rate and completion time for adding,
  replacing, or independently consuming a supported module through documented
  contracts.
- **Runtime tail budget** - Frame-time P99, memory, and service-boundary overhead
  on target hardware.

## Tracks

### Reference-game proof

Build and release one open-source Brotato-like game as Nara's cross-cutting
production fixture and source of measured evidence. Steam publication is not part
of this proof.

_Why it serves the approach:_ It prevents architecture completeness from becoming
a substitute for a finished game.

### Rust production experience

Make Rust gameplay, project data, editing, iteration, debugging, and export one
coherent production path. Treat compile and reload latency as measured product
work rather than an unavoidable implementation detail.

_Why it serves the approach:_ This is where runtime quality becomes usable game
production rather than engine infrastructure.

### Integrated modular platform

Preserve a robust Rust and `bevy_ecs` substrate with explicit ownership, service,
performance, and headless boundaries. Keep modules independently understandable,
replaceable where their contracts allow it, with explicit source/data migration when contracts
differ, and usable without importing the entire product, while making the first-party default
combination work as one engine.

_Why it serves the approach:_ Product coherence and ecosystem extensibility share
the same deliberate boundaries instead of competing architecture goals.

### Delivery and ecosystem

Make public APIs, documentation, standalone export, Cargo/Git/local package use,
and later external-developer validation part of the production path.

_Why it serves the approach:_ A game that only works inside the engine repository
does not prove a usable engine.

## Not working on

- A mandatory second author language, a universal language-neutral behavior host,
  a universal Wasm boundary, or a stable native dynamic ABI before evidence
  requires them. A scripting adapter such as Luau may be an optional package.
- A custom package registry, marketplace, signing system, or ecosystem governance
  layer before Cargo, Git, and local packages expose a concrete product gap.
- A promise that every Rust edit can hot-reload in place. Nara will measure and
  combine data reload, compatible code patching, incremental rebuild, isolated
  runtime restart, and explicit state restoration.
- High-end 3D, networking/rollback, browser, mobile, or console support in the
  first production proof.
- Horizontal crate and ADR expansion without a direct reference-game, safety, or
  measured platform requirement.

## Marketing

**One-liner:** Nara is an open-source game engine written in Rust.

**Key message:** Nara is in early development. APIs and project formats are
expected to change. Technical direction belongs in feature and architecture
documentation rather than in the identity statement.

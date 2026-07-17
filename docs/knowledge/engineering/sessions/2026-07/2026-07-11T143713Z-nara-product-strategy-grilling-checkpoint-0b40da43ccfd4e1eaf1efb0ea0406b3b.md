---
type: "Session Handoff"
title: "Nara product strategy and architecture checkpoint"
description: "Current Rust-first product direction, modularity priorities, reference-game proof, iteration layers, and optional scripting boundaries."
timestamp: 2026-07-11T14:37:13Z
record_id: "0b40da43ccfd4e1eaf1efb0ea0406b3b"
tags: ["nara", "strategy", "rust", "modularity", "iteration", "scripting", "ecs"]
status: "in-progress"
producer_id: "codex-root"
run_id: "session-019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
git_branch: "refactor/engine-foundation-contracts"
---

# Summary

Nara is an open-source game engine written in Rust. The current strategy is to
prove a complete Rust-first production path through a real game, while building
the engine from explicit modules that compose into one coherent default product.
Nara does not need a technically unique market category or a second official
language to justify its existence; it needs to help developers and studios finish
games and express their ideas.

The plain public identity, product strategy, feature documentation, and
architecture contracts remain separate. Terms such as ECS, schema, hot-patching,
and optional scripting describe implementation or features rather than Nara's
identity.

# Current Decisions

## Product and audience

1. **Rust is the complete official authoring language.** Public Rust APIs must
   cover gameplay, project types, assets, editor integration, diagnostics,
   headless tests, packaging, and release. Rust is not limited to engine internals
   or measured performance hotspots.
2. **Nara is not limited to programmer-led teams.** Rust developers and studios
   are the primary technical adopters, while artists, designers, and other team
   members should work through editor, scene, asset, and data workflows without
   needing to author engine code.
3. **The default product experience wins conflicts with maximum module
   independence.** First-party modules must form a coherent engine. Their
   ownership and dependency boundaries still support documented independent use,
   replacement, and third-party extension, but Nara does not promise arbitrary
   cross-engine drop-in compatibility.
4. **Product quality is the differentiation.** Overlap with Bevy, Fyrox, Godot,
   Unity, or Unreal is acceptable. Architecture novelty is not a goal; productive
   authoring, iteration, debugging, modular integration, and delivery are.

## Reference-game proof

1. The first proof is one open-source Brotato-like 2D arena-survival game.
2. It may and should implement gameplay in Rust through public Nara APIs.
3. It must exercise representative typed gameplay, project data, dense entities,
   runtime UI, audio, saves, diagnostics, Play Mode, headless tests, replay, and
   Windows/Linux standalone delivery.
4. It will be available through source control and release artifacts, but it will
   not be published on Steam.
5. It proves one production path, not high-end 3D, networking, every platform,
   scripting quality, external learnability, or Unity-level productivity.

## Runtime and ownership

`bevy_ecs` remains the simulation substrate. ECS is a runtime mechanism and an
advanced public Rust capability, not the authority for every engine concern.

```text
EngineHost
  ProjectContext
    RuntimeInstance
      App
        bevy_ecs::World
          Tick / Frame
```

- `EngineHost` owns process-level and thread-affine facilities.
- `ProjectContext` owns project catalogs, documents, asset-database state, and
  editor-facing project services.
- `RuntimeInstance` owns one play, preview, server, or test execution.
- `World` owns current ECS simulation state.
- GPU, audio, filesystem, task, editor, and optional VM hosts own their native
  state and expose bounded runtime-facing contracts rather than turning `World`
  into a universal service locator.
- Edit documents and the isolated Play runtime have distinct authorities. Stop
  discards runtime changes by default; Apply Changes emits validated patches.

## Schema and project data

Stable type/field identity, versioning, migrations, and runtime-independent
persistent catalogs remain valuable for scenes, prefabs, saves, inspector data,
patches, and runtime reconstruction. They are architecture contracts, not public
positioning and not a requirement to create a universal non-Rust project type
system.

Rust declarations are the primary source for typed gameplay components and data.
Optional generators or scripting adapters may contribute adapter-specific schema
metadata when a real workflow requires it. Dynamic non-Rust ECS components and a
universal language-neutral Behavior Host are not baseline requirements.

## Modules, packages, and languages

1. Trusted Rust modules initially use Cargo source integration and static linking.
2. Local paths, Git dependencies, and Cargo registries are the near-term package
   transports. A custom registry, marketplace, signing system, and governance
   model wait for concrete product gaps.
3. Scripting runtimes are optional packages. A first-party experimental
   `nara_luau` package is a valid way to offer separately reloadable gameplay, but
   Luau is not a required core dependency or coequal official language.
4. C#, Rhai, Wasm, and other runtimes remain possible concrete adapters. Wasm is
   considered when sandboxing, portability, mods, or server extensions create a
   real need; it is not the universal plugin ABI.
5. Shared scripting-host contracts are extracted from actual adapters and domain
   consumers, not designed in advance as a universal language IR.

## Iteration and hot reload

Nara treats hot iteration as four distinct paths:

1. Assets, scenes, prefabs, and project data use direct generation-stamped reload
   with last-good values.
2. Compatible Rust function-body changes may use an optional Subsecond-like
   development plugin at a complete tick/schedule quiescent boundary.
3. Type layout, signature, component registration, plugin graph, static
   initialization, Cargo dependency, and unknown changes rebuild and start a
   fresh isolated runtime. State restoration is explicit and validated.
4. An optional script adapter such as Luau owns module reload and VM-private state
   semantics for projects that choose it.

Subsecond remains experimental: it still compiles and links Rust, cannot migrate
arbitrary layouts, has toolchain/platform coupling, and does not provide a
verified post-commit rollback transaction. The reliable baseline is incremental
build, last-good executable, runtime restart, and document/checkpoint restoration.

# Open Threads

1. Build the thinnest playable reference-game slice and establish edit/build/run
   baselines before expanding architecture breadth.
2. Define and test which modules can be independently consumed or replaced while
   preserving a coherent default product.
3. Prototype Subsecond only after the ordinary rebuild/restart loop is observable;
   compare Windows/Linux P50/P95 latency, debugger quality, memory growth, and
   `bevy_ecs` quiescence behavior.
4. Decide whether demand justifies a `nara_luau` package through one real gameplay
   workflow; do not build it merely to prove language extensibility.
5. Grow editor workflows from reference-game tasks, including scene/prefab
   authoring, data tuning, Play Mode, diagnostics, and export for mixed-discipline
   teams.
6. Revisit custom package discovery and marketplace UX only after Cargo/Git/local
   packages create measured installation or governance problems.

# Next Action

Keep the active engine-foundation work aligned with `STRATEGY.md`, then start the
smallest reference-game vertical slice that can measure Rust edit-to-result
latency and public API coverage. Architecture expansion should cite a concrete
reference-game, safety, or platform requirement.

# Citations

- [Nara strategy](../../../../../STRATEGY.md)
- [Rust-first extension decision](../../decisions/2026-07/2026-07-11T163345Z-defer-extension-technology-selection-behind-a-unified-package-experience-7f435154e74e45359c661b98d145d693.md)
- [Reference-game production proof](../../decisions/2026-07/2026-07-12T003130Z-use-the-brotato-like-reference-game-as-a-production-proof-1e9cf73b7f144e05bed780c0f84bf7a1.md)
- [ADR 0021](../../../../architecture/adr/0021-scripting-and-wasm-boundary.md)
- [Dioxus/Subsecond research](../../subagents/2026-07/2026-07-12-dioxus-subsecond-rust-hot-patching-research.md)
- [Official positioning research](../../subagents/2026-07/2026-07-12-nara-positioning-official-language-research.md)
- [Leaving Rust gamedev](https://loglog.games/blog/leaving-rust-gamedev/)
- [Bevy 0.19](https://bevy.org/news/bevy-0-19/)
- [Fyrox 1.0](https://fyrox.rs/blog/post/fyrox-game-engine-1-0-0/)

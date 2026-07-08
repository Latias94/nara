# AGENTS.md

This file provides repo-local guidance for agents working on nara.

## Project Direction

- nara is a Rust-native, code-first, data-driven game engine.
- The runtime is ECS-first. Components are data; systems own behavior.
- `nara_ecs` uses `bevy_ecs` as the ECS substrate. Do not reintroduce a custom ECS unless an ADR explicitly replaces ADR 0002.
- `nara_app` owns nara's product-facing `App`, `Plugin`, stage, runner, and lifecycle boundary. Do not adopt `bevy_app`.
- Keep backend crates behind adapters. Core gameplay-facing crates must not directly depend on `wgpu`, `winit`, egui, or dear-imgui.
- `nara_winit` owns all `winit` imports and desktop event-loop integration.
- `nara_render_wgpu` owns all `wgpu` imports and GPU surface/device lifecycle.
- The root `nara` facade must keep `winit` and `wgpu` optional; default features stay backend-free.
- `repo-ref/` contains reference source trees. Treat it as read-only reference material and keep it out of git.

## Architecture Rules

- Record durable architecture decisions under `docs/architecture/adr/`.
- Keep `docs/architecture/nara-foundation.md` aligned with implemented crate boundaries.
- Use `docs/knowledge/engineering/` for session memory, subagent findings, verification, and handoff state.
- Prefer fearless refactoring before compatibility layers. This project is pre-1.0; remove obsolete scaffolding instead of preserving it.
- Keep scene/prefab/save data independent from runtime `bevy_ecs::Entity` values and backend-native handles.
- Runtime UI is expected to be nara-owned long term. egui/dear-imgui are acceptable for debug/editor tooling, not as the runtime UI foundation.

## Rust Workflow

- Use Rust 2024 and the workspace MSRV in `Cargo.toml`.
- Format with `cargo fmt --all`.
- Prefer `cargo nextest run --workspace` for tests.
- Run `cargo check --workspace` before considering implementation work complete.
- Check optional backend examples with `cargo check -p nara --features winit,wgpu --example windowed_clear` when touching platform or render backend code.
- Use dependency boundary searches when touching backend crates: `rg -n "winit::|winit =" crates src Cargo.toml` and `rg -n "wgpu::|wgpu =" crates src Cargo.toml`.
- Use precise commits with Conventional Commit messages.
- Do not discard or rewrite user changes. Never use `git reset --hard`, `git checkout --`, `git restore`, `git clean`, or stash to remove work unless the user explicitly asks.

## Subagent Guidance

- For architecture research, prefer read-only subagents with an explicit instruction not to spawn nested subagents.
- Subagents may inspect `repo-ref/`, docs, and source files, but the orchestrating agent owns edits, staging, commits, and final verification unless the user says otherwise.

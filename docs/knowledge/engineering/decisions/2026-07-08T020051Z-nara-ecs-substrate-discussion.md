---
type: "Decision"
title: "Nara ECS substrate discussion"
description: "Decision for Nara ECS substrate discussion."
timestamp: 2026-07-08T02:00:51Z
tags: ["nara", "ecs", "architecture", "discussion"]
---

# Decision

Proposed, not yet accepted: use `bevy_ecs` as nara's ECS substrate while keeping `nara_app`, scene/prefab, asset, rendering backend seams, tooling, and AI-facing schemas owned by nara.

# Context

nara targets a Rust-native, code-first, strict ECS game engine. The user raised that Rust's type system makes ECS difficult and asked whether to use Bevy ECS, self-build, or seek an EnTT-like mature experience.

Current working direction from the discussion:

- Avoid adopting `bevy_app` wholesale; nara should own app lifecycle, plugin stages, runner, and product boundaries.
- Strongly consider adopting or wrapping/re-exporting `bevy_ecs` because query borrowing, system params, schedules, commands, resources, change detection, and parallel execution are high-risk to self-build early.
- Preserve EnTT-like ergonomics as a product goal: registry/world as the central entry point, typed component data, comfortable views/queries, snapshots, metadata, and resource/asset integration.
- Reflection, pluginization, asset/scene serialization, and editor/tooling seams need explicit design before deeper implementation.

# Alternatives

1. Use `bevy_ecs` directly/re-exported through `nara_ecs`.
2. Use a smaller ECS crate such as `hecs` or `shipyard` and build nara's own app/schedule/reflection stack.
3. Bind to a mature C/C++ ECS such as Flecs-style APIs.
4. Self-build an ECS from first principles.

# Consequences

If accepted, Phase 1 should pivot the current placeholder `nara_ecs` toward a `bevy_ecs` substrate. The key ADR should define what is committed:

- Whether `nara::prelude::*` exposes Bevy ECS names (`Query`, `Res`, `Commands`) directly.
- Whether `nara_ecs` is a thin re-export or a compatibility facade.
- How reflection and serialization metadata relate to Bevy's `Reflect` ecosystem.
- Which APIs nara reserves as product-owned and not Bevy-owned.

# Citations

- Current architecture draft: [../../architecture/nara-foundation.md](../../architecture/nara-foundation.md)
- Workspace boundary ADR draft: [../../architecture/adr/0001-runtime-workspace-boundaries.md](../../architecture/adr/0001-runtime-workspace-boundaries.md)
- Local references consulted during the session: `repo-ref/bevy`, `repo-ref/godot`, `repo-ref/wgpu`, `repo-ref/dear-imgui-rs`

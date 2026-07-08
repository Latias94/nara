---
type: "Implementation Log"
title: "Runtime foundation switched to bevy_ecs substrate"
description: "Initial nara runtime foundation implementation aligned with accepted ECS, App, reflection, diagnostics, and transform ADRs."
tags: ["ecs", "runtime", "app", "reflection", "diagnostics", "transform"]
timestamp: 2026-07-08T04:41:33Z
status: "active"
---

# Runtime Foundation Implementation

Implemented the first architecture slice after accepting fearless refactoring:

- `nara_ecs` is now a thin `bevy_ecs` substrate boundary.
- `nara_app` owns nara-specific `App`, `Plugin`, startup stages, core stages, and schedule execution.
- `nara_transform` owns `Transform2d` and `GlobalTransform2d` ECS components.
- `nara_reflect` owns stable component schema IDs and a Bevy-reflect-backed registry.
- `nara_diagnostic` owns structured diagnostics and diagnostic reports.
- Domain crates now derive real Bevy ECS `Component`/`Resource` types where appropriate.
- `examples/hello_world.rs` demonstrates `Commands` and `Query` authoring through nara's facade.

Verification:

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo nextest run --workspace`
- `cargo run -q`
- `cargo run -q --example hello_world`

---
type: "Subagent Finding"
title: "Runtime driver transactions must be configured before execution"
description: "Subagent Finding for Runtime driver transactions must be configured before execution."
timestamp: 2026-10-15T11:19:06Z
record_id: "c8b70859ecb1443bb070b7c1bb4cb01b"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
subagent_id: "/root/u5_execution_owner"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
---

# Finding

An overridable World operation on __RuntimeDriverPort let a runtime caller define a temporary
Resource type and receive the managed World without ever configuring that operation. doc(hidden)
does not restrict that behavior. Ordinary resource-local ports should remain resource-local.

# Evidence

The focused pre-fix test missing_driver_resource_cannot_run_a_trait_selected_world_operation
expected MissingResource but received Ok and ran its supplied operation (Nextest
e9ca2c82-fcaf-4d7a-8520-24850ebd4c56). The outer fault bridge could detect protected replacement
afterward, but could not restore dropped physical responsibility.

Existing Editor image admission/publication and Game View completion coordinate multiple resources.
Game View completion must remain synchronous and available while stopping/faulted to retire exact
presentation ownership. Moving every operation to a frame queue would break that real closure path.

# Recommendation

Remove the trait World override. App may configure a resource-typed operation before freezing;
a private RegisteredDriverPort<R> retains its function pointer. Runtime callers supply only the
typed input. They cannot construct the private slot, register an operation, or substitute its callback.
Unregistered ports mutate only their existing resource. Keep input ownership on refusal, state gates,
and fault bridge checks. This is trusted native configuration, not a plugin sandbox.

Use a private typed function pointer instead of registered ECS systems here: run_system_with consumes
move-only input on lookup/validation failure, requiring an unnecessary recovery mechanism. No general
operation registry, TypeId permission model, task framework, or second execution owner is added.

# Disposition

Implemented in nara_app and migrating its consumers. Four focused regressions cover missing resource,
unconfigured local-only behavior, unique/frozen configuration, and exact owned-input return. The full
nara_app/nara_tasks set passes 138 tests, Nextest ce9d32df-7d3e-4e33-89ee-32be92fa82f5. Whole-consumer
U5 verification is still required before this unit is claimed complete.

# Citations

- crates/nara_app/src/runtime.rs: __RuntimeDriverPort and RegisteredDriverPort
- crates/nara_app/src/runtime.rs: App::__register_driver_port
- src/project_host/runtime/editor/authoring_runtime.rs
- src/project_host/runtime/editor/game_view.rs
- repo-ref/bevy/crates/bevy_ecs/src/system/system_registry.rs: run_system_with
- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U5 / KTD7

---
status: active
date: 2026-07-10
audience: engine contributors and early project authors
supported_baseline: unreleased source tree at the start of the engine-foundation refactor
---

# July 2026 Engine Foundation Migration Guide

This guide records deliberate breaking replacements made by the engine-foundation completion plan. nara is unreleased: the goal is one correct canonical contract, not compatibility with prototype APIs or draft file shapes.

## Policy

- Remove obsolete code, exports, fixtures, and documentation in the same implementation unit.
- Do not add deprecated aliases, compatibility wrappers, parallel `V1`/`V2` Rust types, or a second loading path.
- Give the corrected Rust API the canonical unsuffixed name.
- A superseded pre-release persistent shape is deleted and replaced by the correct canonical `format_version = 1` after every in-repository source/fixture is updated.
- Runtime readers never silently rewrite project source files. A source rewrite is explicit and performed before or outside runtime load.
- Generated caches may be rebuilt, quarantined, or deleted; they are not compatibility authorities.
- Preserve an old reader or migration chain only when an ADR names the compatibility window, owner, removal trigger, and fixtures.
- Back up external experimental projects before applying a source rewrite.

## Migration Summary

Every implementation unit that changes a public API, persistent shape, cache contract, or observable behavior adds a row. A unit with no external migration records `No external migration` in its verification evidence rather than inventing an entry.

| Migration ID | Unit | Commit | Kind | Affected contract | Required action |
|---|---|---|---|---|---|
| U2-1 | U2 | `feat(app): contain plugin lifecycle failures` | `rust-api` | `App` mutation/update, `Plugin`, cleanup, group, and runner signatures | Propagate setup/update errors, use narrow cleanup/group contexts, and let runners borrow the app. |
| U2-2 | U2 | `feat(app): contain plugin lifecycle failures` | `rust-api` | Built-in `register_*_components` helpers | Handle `ComponentRegistryError`; plugin installation now reports contextual registration failure. |

## Entry Contract

Each migration entry contains:

1. **Removed contract**: the exact public symbol, serialized shape, cache identity, or behavior removed.
2. **Canonical replacement or deletion rationale**: the unsuffixed replacement and why no compatibility path remains.
3. **Before/after**: required for Rust API changes; use short compilable examples.
4. **Affected examples and fixtures**: every in-repository caller/source updated by the unit.
5. **User action**: source edit, configuration edit, no action, or explicit regeneration command.
6. **Source action**: `none`, `manual-rewrite`, or an explicitly named offline tool. Runtime auto-rewrite is forbidden.
7. **Cache action**: `keep`, `rebuild`, `quarantine`, or `delete`.
8. **Compatibility window**: normally `none (unreleased canonical replacement)`; otherwise link the authorizing ADR.
9. **Rollback**: how to recover source data or revert the code commit without relying on a compatibility shim.
10. **Verification anchors**: tests, fixtures, examples, and stale-symbol searches proving the replacement is complete.

## U2-1: Terminal Plugin Lifecycle and Fallible App Mutation

**Removed contract**:

- `App::try_update()` plus panic-based `App::update() -> AppFrameOutcome`.
- Infallible mutable entry points such as `world_mut`, `insert_resource`, `init_resource`,
  `add_startup_systems`, `add_systems`, `configure_sets`, and `set_runner`.
- `Plugin::cleanup(&mut App) -> ()` and unrestricted `PluginGroup::build(&mut App)`.
- `RunnerFn = FnOnce(App)`, which hid cleanup failures inside runner-owned `Drop`.
- The implicit `plugins_finished: bool` behavior that allowed committed failures to be retried.

**Canonical replacement or deletion rationale**: `App` exposes one explicit terminal plugin state
machine. Mutable entry points and `update` return `Result`; `Plugin::preflight` is the only
pre-mutation retry boundary; cleanup uses `PluginCleanupContext` and `PluginCleanupError`; groups use
`PluginGroupBuilder`; runners borrow `&mut App`; failure details use `PluginFailureReport`. The old
paths are deleted because they could run a partially initialized app or lose cleanup ownership.

**Before**:

```rust
app.insert_resource(GameState::default())
    .add_systems(CoreStage::Update, update_game);
app.update();

fn cleanup(&self, app: &mut App) {
    app.world_mut().remove_resource::<Backend>();
}
```

**After**:

```rust
app.insert_resource(GameState::default())?;
app.add_systems(CoreStage::Update, update_game)?;
app.update()?;

fn cleanup(
    &self,
    context: &mut PluginCleanupContext<'_>,
) -> Result<(), PluginError> {
    context.world_mut().remove_resource::<Backend>();
    Ok(())
}
```

**Affected examples and fixtures**: root facade plugin groups, all repository examples, plugin crate
tests, window/wgpu examples, asset import examples, and headless/server examples now use only the
fallible canonical API.

**User action**: update downstream plugin/app code to propagate or explicitly handle every returned
error; replace group access to `App` with `PluginGroupBuilder`; change runners to borrow `&mut App`.

**Source action**: `none`.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the U2 commit and its callers together. Do not restore the boolean lifecycle or
add deprecated wrappers.

**Verification anchors**: `nara_app` lifecycle tests; focused plugin-crate nextest runs; all-target
facade check; searches for `try_update`, ignored update results, old cleanup/group signatures, and
unhandled mutable app calls.

## U2-2: Fallible Built-In Component Registration

**Removed contract**: built-in `register_*_components(&mut ComponentRegistry) -> ()` helpers that
called `expect` when a component ID or Rust type was already registered.

**Canonical replacement or deletion rationale**: registration helpers return
`Result<(), ComponentRegistryError>`. Built-in plugins use read-only
`ComponentRegistry::validate_component_registration` during preflight and map commit-time failures
to `PluginError::ComponentRegistrationFailed` with stable plugin and component context. A duplicate
is project/setup data, not a reason to panic the process.

**Before**:

```rust
register_transform_components(&mut registry);
```

**After**:

```rust
register_transform_components(&mut registry)?;
```

**Affected examples and fixtures**: scene/prefab/schema examples and transform, render, sprite,
hierarchy, tilemap, and runtime UI codec tests.

**User action**: handle or propagate the returned `ComponentRegistryError` when calling registration
helpers directly. Plugin users receive a contextual `PluginError` automatically.

**Source action**: `none`.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the U2 registration commit and callers together; do not restore `expect`.

**Verification anchors**: component registry read-only validation test, built-in duplicate conflict
tests, and a stale search for registration `expect` calls.

## Persistent Format Matrix

U9 and later format-owning units populate this table before writing a new shape. Rows describe only formats intentionally supported after the refactor; deleted draft formats do not remain as pseudo-legacy rows.

| Kind | Canonical written version | Readable versions | Retained migration chain | Engine minimum | Source action | Cache action |
|---|---:|---|---|---|---|---|
| _Added by the format owner_ | 1 | 1 | none | _set by owner_ | _set by owner_ | _set by owner_ |

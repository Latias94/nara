---
type: Subagent Finding
title: codebase foundation audit
tags: nara,audit,foundation,subagent,architecture
timestamp: 2026-07-09
status: summarized
---

# Finding

Read-only audit agents reviewed the current Rust engine foundation for data
integrity, error handling, security, architecture, and persistence risk. The
valid returned audits found no critical security issue, and confirmed that the
current `wgpu`, `winit`, and `egui` crate boundaries are intact.

The highest-priority risks are foundation contract issues that could cause
silent data loss or expensive future migration work:

1. `export_scene` can skip a component after encode failure and still return an
   exported document. The current diagnostic is a warning, which can allow save
   paths to persist a scene missing serializable component data.
2. `SceneSpawner` is not fully transactional after preflight. If a custom
   `PreparedComponent::apply` fails, the target `World` can already contain
   new entities, partial components, or committed `AssetServer` changes.
3. Scene/prefab deserialization is lenient toward unknown fields. This is
   useful for compatibility, but dangerous for AI-generated or hand-authored
   data because misspelled fields can be ignored before validation sees intent.
4. `ScenePatchDocument` and prefab override patches lack their own format
   version and component schema version context for field paths, making future
   field rename migrations painful.
5. `ComponentRegistry` can register the same Rust `TypeId` under multiple
   `ComponentTypeId` values, which can leave stale schemas/codecs and ambiguous
   export behavior.
6. `nara_reflect` is now a large mixed-responsibility module containing values,
   paths, schemas, codecs, migrations, and registry logic. It should be split
   before derive/codegen, schema export, or editor field controls expand.
7. The `RenderBackend` trait is not the actual backend integration contract.
   Real integration currently happens through plugins, systems, and resources,
   which will be ambiguous for future 3D, runtime UI, postprocessing, or second
   backend work.
8. Plugin setup still has panic-based helper paths and an infallible
   `Plugin::build`, which conflicts with the mature diagnostics-aware plugin
   lifecycle direction.

# Evidence

- `crates/nara_scene/src/export.rs:80` emits `scene.export-component-failed` as a warning.
- `crates/nara_scene/src/spawn.rs:173`, `:180`, and `:200` commit asset/server and world state
  before all component apply closures are known to have succeeded.
- `crates/nara_scene/src/document.rs:60`, `:122`, `crates/nara_scene/src/prefab.rs:7`,
  `:138`, and `crates/nara_scene/src/format.rs:33` derive serde without an explicit strict
  unknown-field policy.
- `crates/nara_scene/src/patch.rs:17`, `:141`, `crates/nara_scene/src/prefab.rs:143`,
  and `:380` show patch/prefab override documents without patch format version or field-path
  schema version.
- `crates/nara_reflect/src/lib.rs:1539`, `:1559`, and `:1481` allow Rust type-id mapping to be
  overwritten while existing schema/codec entries remain.
- `crates/nara_reflect/src/lib.rs:1215` begins a broad `ComponentRegistry` implementation in a
  1700+ line file.
- `crates/nara_render/src/lib.rs:269` defines `RenderBackend`; `crates/nara_render_wgpu/src/lib.rs:68`
  uses a resource/plugin integration path instead.
- `crates/nara_render_wgpu/src/lib.rs:483`, `crates/nara_winit/src/lib.rs:281`, and
  `crates/nara_sprite_render/src/lib.rs:45` use panic-based prerequisite plugin helpers.

# Recommendation

Recommended next implementation order:

1. Make export/save strict by default: component encode failures should be errors on save paths, or
   strict export should be the default editor/API path.
2. Make scene spawn atomic after preflight. Either make `PreparedComponent` apply infallible after
   preflight, or apply into a transaction/scratch world slice and commit only on success.
3. Decide strict scene/prefab parse policy. Default editor/AI import should reject unknown fields;
   compatibility should use an explicit lenient/migration path.
4. Version patch/prefab override documents and carry component schema version context for
   field-path operations.
5. Harden `ComponentRegistry`: reject duplicate Rust `TypeId` registration unless an explicit alias
   or compatibility API is designed; validate serializable schema field coverage and default value
   kinds.
6. Split `nara_reflect` into narrow modules before adding derive/codegen or more editor field
   features.
7. Clarify the render backend contract: either remove the unused trait or evolve it into a real
   backend/render-graph integration seam.
8. Move plugin dependency installation away from panic helpers toward fallible install reports or a
   first-class plugin group/dependency model.

# Disposition

Use this audit as input for the next `ce-plan`. The most valuable next slice is a foundation
hardening pass focused on strict export/import and transactional spawn before adding larger feature
surface area.

# Verification Context

The main thread also ran:

- `cargo check --workspace --features serde`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`

The worktree was clean after committing the previously untracked Play Mode plan.

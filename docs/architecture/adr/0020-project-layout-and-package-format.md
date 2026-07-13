# ADR 0020: Project Source Layout

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0035: Project Manifest and Runtime Settings Authority

## Context

nara needs a project shape that works for code-first games, AI-generated projects, asset import, scene/prefab discovery, hot reload, and future editor tooling.

## Decision

nara projects use a simple manifest plus conventional directories.

This ADR defines the authoring/source-tree convention only. It does not define cooked products,
runtime packages, package catalogs, mount precedence, patches, DLC, signing, or deployment.

Recommended layout:

```text
Cargo.toml
Cargo.lock
nara.toml
src/
assets/
scenes/
prefabs/
scripts/
.nara/
  import-cache/
```

Rules:

- `Cargo.toml` (or an owning workspace manifest) and the application `Cargo.lock` are the Rust
  package graph, target, dependency, and feature-resolution authority.
- `nara.toml` is the project manifest.
- `assets/` holds source assets.
- `scenes/` and `prefabs/` hold data documents.
- `.nara/import-cache/` holds generated import artifacts and records and should not be hand-authored.
- AI-generated projects should be able to produce this layout without editor involvement.
- The layout is conventional, not a hard requirement for embedded/library use.
- `nara.toml` does not duplicate Cargo dependencies, version resolution, source lists, or lock
  state.

## Alternatives Considered

### Option A: No project convention

**Pros**: Maximum flexibility.

**Cons**: Weak tooling, hot reload, editor, and AI story.

**Decision**: Rejected.

### Option B: Editor-owned project database

**Pros**: Powerful editor workflows.

**Cons**: Conflicts with code-first and AI-friendly file generation.

**Decision**: Rejected as the primary model.

### Option C: Manifest plus conventional directories (Chosen)

**Pros**: Simple, code-first, AI-friendly, and editor-compatible.

**Cons**: Some conventions may evolve.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| AI project generation | Minimal project can be generated as files | Future smoke test |
| Hot reload readiness | Asset/scenes directories are discoverable | Future test |
| Editor compatibility | Editor can open project from manifest | Future integration test |
| Code-first compatibility | Runtime can run without editor database | Design review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Layout becomes too rigid | Medium | Medium | Treat as convention with configurable roots |
| Cache checked into source accidentally | Low | Medium | Add `.nara/import-cache/` guidance to gitignore/templates |
| Manifest grows too large | Medium | Medium | Keep manifest project-level; scene/asset data stays in files |

## Consequences

- File-backed projects remain code-first and source-control friendly; embedded use may provide the
  same settings and content through explicit host configuration.
- `.nara/import-cache/` is generated importer output, not a distributable runtime package.
- Target cooking, package publication, and runtime catalogs require their own decision and must not
  silently accrete into this source layout contract.

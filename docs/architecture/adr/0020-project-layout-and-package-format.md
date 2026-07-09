# ADR 0020: Project Layout and Package Format

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0035: Project Manifest and Runtime Settings Authority

## Context

nara needs a project shape that works for code-first games, AI-generated projects, asset import, scene/prefab discovery, hot reload, and future editor tooling.

## Decision

nara projects use a simple manifest plus conventional directories.

Recommended layout:

```text
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

- `nara.toml` is the project manifest.
- `assets/` holds source assets.
- `scenes/` and `prefabs/` hold data documents.
- `.nara/import-cache/` holds generated import artifacts and records and should not be hand-authored.
- AI-generated projects should be able to produce this layout without editor involvement.
- The layout is conventional, not a hard requirement for embedded/library use.

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

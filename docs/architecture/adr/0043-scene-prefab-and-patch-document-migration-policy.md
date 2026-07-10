# ADR 0043: Scene, Prefab, and Patch Document Migration Policy

**Status**: Accepted
**Date**: 2026-07-09
**Amended**: 2026-07-10 for the unreleased canonical-version reset policy
**Refines**: ADR 0006, ADR 0011, ADR 0026, ADR 0038
**Refined By**: ADR 0049: Untrusted Project Input and Parse Budget Policy; ADR 0051: Persistent
File Envelope, Migration, and Golden Fixtures

## Context

`SceneDocument`, `PrefabDocument`, and `ScenePatchDocument` now have format versions and validation
rules. Component-level migrations exist through `nara_reflect`, but document-level migrations are
still a separate problem.

If nara only migrates component values, it cannot safely rename document fields, change prefab
instance shape, change patch operation encoding, alter provenance records, or split/merge document
sections. A mature released engine needs explicit document migrations, but nara has not shipped a
persistent compatibility promise yet. Preserving prototype shapes now would turn known design debt
into a permanent reader and fixture burden.

## Decision

nara distinguishes document-format migration from component-value migration and distinguishes an
unreleased canonical reset from a real compatibility window.

```mermaid
flowchart TD
    File[JSON / RON document] --> Parse[Parse raw document]
    Parse --> Version{Declared supported version?}
    Version -->|canonical v1| CompMig[Component value migrations]
    Version -->|ADR-retained older version| DocMig[Pure document migration chain]
    Version -->|prototype or unknown| Reject[Structured rejection]
    DocMig --> CompMig
    CompMig --> Validate[Schema-aware validation]
    Validate --> Preflight[Scene/prefab preflight]
    Preflight --> World[World spawn / patch apply]
```

Rules:

- The corrected scene, prefab, and patch shapes become canonical `format_version = 1`. Superseded
  prototype structs, readers, and fixtures are deleted; corrected Rust types use canonical
  unsuffixed names.
- Each document kind publishes a strict compatibility matrix. Until an ADR grants a compatibility
  window, the only readable version is canonical version 1.
- A document migration chain is added only when an ADR names the retained version, owner, fixtures,
  support window, and removal trigger.
- Document migrations are pure transformations from one known document version to the next. They do
  not mutate the live `World`, asset server, or source file in place.
- Component migrations run after the document shape has been migrated into a version whose component
  payload locations and patch operation payloads are known.
- Unsupported future document versions fail with structured diagnostics.
- Prototype, unknown, and missing migration steps fail with structured diagnostics. nara must not
  best-effort guess unknown document shapes.
- Patch documents migrate before validation/apply. Undo/redo stacks should store current-version
  inverse patches once they are produced by the active editor session.
- Migrations must preserve authoring identity and provenance. They may rewrite IDs only through an
  explicit documented migration that also updates references and overrides.
- AI/editor tooling emits canonical version 1. Loading another version is supported only when the
  compatibility matrix links to its authorizing ADR.
- Automatic write-back of migrated files is an explicit tool/editor action. Runtime loading should
  not rewrite source files silently.
- In-repository prototype source files are updated in the breaking implementation unit. External
  experimental files require the documented manual/offline source action; runtime does not carry a
  hidden prototype reader.

## Alternatives Considered

### Option A: Reject all non-current document versions forever

**Pros**: Simple validation and no migration bugs.

**Cons**: Cannot support real released projects when a future compatibility promise exists.

**Decision**: Rejected as the permanent policy.

### Option B: Handle migrations ad hoc inside each loader

**Pros**: Easy to add one-off fixes near parsing code.

**Cons**: Hard to audit, hard to test, and likely to diverge between scene, prefab, patch, JSON, and
RON loaders.

**Decision**: Rejected.

### Option C: Preserve every prototype version through formal migration chains

**Pros**: Avoids breaking any experimental file.

**Cons**: Freezes known prototype mistakes and creates compatibility code before there is a shipped
contract or user population.

**Decision**: Rejected.

### Option D: Canonical reset before release, explicit migration windows afterward

**Pros**: Ships one correct version-1 contract while preserving a rigorous migration mechanism for
future compatibility promises.

**Cons**: Experimental projects must perform an explicit source rewrite during foundation work.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Canonical loading | Corrected version-1 scene/prefab/patch fixtures load and reserialize canonically | Golden-file tests |
| Prototype rejection | Removed draft shapes fail instead of entering a hidden compatibility path | Negative fixture tests |
| Retained compatibility | Every non-v1 readable version has an authorizing ADR and pure chain | Ledger and golden tests |
| Future version safety | Future versions fail with structured diagnostics | Unit tests |
| Component migration order | Component migrations run after document migration | Migration tests |
| Source safety | Runtime load does not rewrite source files silently | Loader tests/review |
| Provenance preservation | Prefab overrides and source IDs remain valid after migration | Scene/prefab tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Migration bugs corrupt authored data | High | Medium | Keep migrations pure, test golden inputs/outputs, and require explicit save action. |
| Prototype files are reused after the reset | Medium | Medium | Reject removed shapes strictly and document the required source rewrite. |
| Too many retained versions become maintenance burden | Medium | Medium | Add a chain only for an ADR-backed compatibility window with a removal trigger. |
| Patch migration changes undo semantics | Medium | Medium | Store active session undo entries in current-version inverse patch format. |
| Component and document migrations conflict | High | Low | Run document migration first and component migration only through registry-known payload paths. |

## Consequences

- `nara_scene` should expose migration registries only when the compatibility matrix contains an
  ADR-retained version; it must not retain prototype readers speculatively.
- Current hard rejection of prototype, unknown, and future versions remains valid.
- Component migration ADR 0011 remains valid but does not cover document shape evolution.
- The breaking unit updates all repository-owned source documents and records the source action in
  the migration guide instead of adding `V1`/`V2` APIs.

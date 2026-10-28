# ADR 0035: Project Manifest and Runtime Settings Authority

**Status**: Accepted
**Date**: 2026-07-09
**Last Revised**: 2026-10-28
**Refines**: ADR 0020: Project Source Layout
**Refined By**: ADR 0039: Main Loop, Time Domains, Pause, and Runtime State; ADR 0041: Input Routing,
Actions, Text Input, UI Focus, and Accessibility; ADR 0046: Plugin Metadata and Default Plugin
Groups; ADR 0056: Headless Runtime and Dedicated Server Readiness; ADR 0070: Capability-Oriented
Filesystem Substrate; ADR 0079: Root Product Capabilities and Placeholder Domain Retirement; ADR
0095: Plugin-Owned Specialized Domains and Project Configuration;
[ADR 0110](0110-first-party-project-workflow-and-source-carrying-export.md): First-Party Project
Workflow and Source-Carrying Export

## ADR 0095 Refinement

Settings authority is not code-selection authority. `nara.toml` cannot install a Rust crate or
choose a physics, audio, text, UI, or other plugin implementation. Cargo and Rust product
composition select implementations. Plugin settings stay in typed Rust until a concrete file-backed
consumer separately admits either a plugin-owned versioned asset or a namespaced manifest
extension; this ADR does not create a generic plugin-settings registry.

## Context

ADR 0020 established `nara.toml` plus conventional project directories, but it intentionally left
the concrete manifest authority open. That is now becoming a cross-cutting risk. Asset roots,
import cache paths, default scenes, runtime profiles, task-pool sizing, window defaults, and input
maps should not each grow a private configuration format.

nara also needs to stay code-first. A library user should be able to construct an `App` without a
project file, while a file-backed game, editor session, or AI-generated project needs one stable
manifest authority.

## Decision

`nara.toml` is the project-level settings authority for file-backed projects. Runtime embedding may
override it through explicit `App` resources, but no engine subsystem should invent a second
persistent project configuration source for the same setting.

The integrated Editor's concrete local finite-tool lifecycle record is
`.nara/source-tools/selection.json`. It alone records the selected installed artifact and desired
enabled/disabled state for the current single tool slot. This is a local installation lifecycle
record, not a second manifest setting, source dependency specification or portable package lock.
`nara.toml` does not duplicate these fields or select executable tool code. The record binds a
project-relative installation, nonzero installation ID, exact receipt/manifest digests and package
identity. Native file IDs, runtime activation identities, launcher configuration and executable
approval are never restored from it. Missing is unselected; invalid or inaccessible data does not
become an empty record or permission to overwrite. Ordinary Open observes this data without a
probe. Actual implementation and platform evidence remain in the local lifecycle plan and ledger.

The manifest contract is:

- `schema_version` is required and gates future manifest migrations.
- `[project]` owns project name, a required non-nil `source_universe`, a required
  `native_document_identity` policy, and optional descriptive stable ID/version fields.
- `[paths]` owns logical project roots such as assets, scenes, prefabs, scripts, and generated
  `.nara/import-cache`.
- `[startup]` owns the default startup scene or entry point when a project is launched from files.
- `[runtime]` owns pause, time scale, maximum frame delta, fixed timestep, per-frame fixed work,
  bounded debt, catch-up policy, and a narrow runtime preset.
- Project capability settings request an additive subset of the host's compiled product
  capabilities. Preset implications and explicit requests normalize to one inspectable set.
- `[tasks.io]`, `[tasks.compute]`, and `[tasks.async_compute]` own non-zero worker/pending limits;
  `[tasks.shutdown]` owns bounded drain/cancel/join timeouts. Production settings are threaded.
  Programmatic `TaskPlugin` configuration remains valid for embedded apps, while the inline driver
  remains test-only.
- `[window]` owns default primary-window settings for file-backed desktop launches.
- `[input]` owns named input action map files or inline action-map settings once ADRs define the
  input-action model.
- `[profiles.<name>]` provides platform/build/profile overrides, including future `headless`,
  `server`, `editor`, `dev`, and `release` profiles. Overrides patch manifest values; they do not
  replace the manifest authority.

Canonical version 1 declares source authority separately from product selection:

```toml
schema_version = 1

[project]
name = "Example"
source_universe = "72011426-37e2-48ba-bc06-78841468889a"
native_document_identity = "stable_metadata"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d", "render-wgpu"]
```

The runtime preset is one of `minimal`, `local-headless`, or `server`. Capability IDs are the
coarse ADR 0079 feature names. A profile capability patch replaces the requested set rather than
merging it, so one manifest plus profile has one auditable request.

### Authored Source Authority

`source_universe` is an authored UUID namespace for named and project-location sources. It is not
the optional descriptive `stable_id`, a physical project-root identity, an opened-document
incarnation, or a hash of project bytes. Its small value type belongs in ECS-independent
`nara_core`; `nara_project` owns the required manifest carrier and policy, not a source registry.

`native_document_identity` is project-wide, including native documents opened outside configured
scan roots. `stable_metadata` requires the source owner's valid authored stable metadata;
`project_location` selects the universe plus full project-relative location. Missing or corrupt
metadata under the stable policy refuses admission; it never selects the location policy or mints
an identity during loading. Stable asset/product identity does not inherit the receiving project's
universe. These fields cannot be profile overlays. A deliberate policy or identity change requires
source readmission rather than transparent identity continuity.

Ordinary integrated project creation selects `stable_metadata`. Explicit project creation prepares
fresh source identities once and retains their exact manifest/metadata bytes in the creation plan.
Parsing, profile resolution, publication retry, reopening, export and repair must not regenerate
them. The unreleased manifest schema remains 1; the current shape replaces its previous shape
without a compatibility parser or inferred fields. Internal refactors do not create a released
format lineage. Normalize other displaced unreleased formats at their owning refactor boundaries,
not by retaining obsolete readers merely because a previous implementation used another number.
The optional `stable_id` remains a descriptive application coordinate and is not substituted for
`source_universe`.

This establishes source configuration authority, not the complete Scene/Prefab source-binding or
logical-occurrence API. Their owning replacement and qualification are tracked in the
[source/occurrence design](../scene-source-and-occurrence-identity-design.md) and implementation
ledger. A carrier-only implementation must not be reported as loader or UI identity qualification.

Host/composition code opens and bounds `nara.toml` through a host-issued `nara_fs` capability, then
passes an immutable byte or UTF-8 candidate into `nara_project`. The project crate owns parsing,
validation diagnostics, profile overlays, and lowering; it exposes no ambient `File::open` or
authorization-checked raw-path API.

`EffectiveProjectSettings` contains validated domain values. The root product host first publishes
an immutable `ProjectSettingsCandidate` only after the runtime preset and additive request fit the
compiled product ceiling. RGF-U4 then proves the resolved plan's required product capabilities fit
that request and closes plugin service requirements/conflicts and group membership before touching
`App`. Candidate ingest returns `ProjectCandidateError`. Root product closure returns
`CompositionError`, and plugin group/slot/dependency closure returns `PluginPlanError`, before
mutation. App-level plugin hooks return `PluginError`. Only a valid resolved plan may create a
private prepared transfer and enter closed plugin commit. `nara_project` itself remains
side-effect-free.

RGF-U12 consumes the candidate's path/startup settings and the matching resolved plan's frozen
schema-validation input through the same project-root identity. It publishes a bounded immutable
`ProjectContentSnapshot` carrying that lineage and schema fingerprint without creating an `App`,
opening ambient paths, or storing source authority in the snapshot.

```mermaid
flowchart TD
    Host[Host composition] --> FS[nara_fs capability read and byte bound]
    FS --> Candidate[Immutable nara.toml candidate]
    Candidate --> Validate[nara_project parse, validate, profile, and lower]
    Validate --> Product[Resolve compiled/requested/required product subsets]
    Product --> Services[Close plugin service requirements/conflicts and groups]
    Services --> Gate{Both closures valid?}
    Gate -->|no| Reject[CompositionError / PluginPlanError; App unchanged]
    Gate -->|yes| Apply[Prepare definitions, then apply settings and commit plugins]
    Apply --> Paths[Project paths / AssetSourceRoot / import cache]
    Apply --> Startup[Startup scene / entry settings]
    Apply --> Runtime[FixedTime / pause / background policy]
    Apply --> Tasks[TaskPoolConfig]
    Apply --> Window[Window defaults]
    Apply --> Input[Input action map sources]
    Paths --> Asset[nara_asset]
    Startup --> Scene[nara_scene]
    Runtime --> App[nara_app]
    Tasks --> TaskPlugin[nara_tasks]
    Window --> Winit[nara_winit / nara_window]
    Product --> Profile[plugin groups / capability policy]
```

## Rules

- `nara.toml` values are project data and should be serializable, reviewable, and AI-generatable.
- File-backed hosts obtain manifest bytes through `nara_fs`; `nara_project` accepts immutable
  candidates and never acquires ambient filesystem authority.
- Code-first callers may construct equivalent resources manually. This is an override path, not a
  competing persistent format.
- Generated/cache directories are configurable only through the manifest or explicit embedding
  resources.
- Profile overlays must be deterministic: a project plus profile name resolves to one effective
  settings document.
- Domain crates may define their own settings structs, but the persistent owner is the project
  manifest layer.

## Alternatives Considered

### Option A: Each subsystem owns local config files

**Pros**: Local autonomy and quick implementation.

**Cons**: Asset roots, input maps, task pools, and startup scenes would drift. AI-generated projects
would need to understand many authorities.

**Decision**: Rejected.

### Option B: Editor-owned project database

**Pros**: Strong editor workflow and central indexing.

**Cons**: Conflicts with code-first authoring and makes the editor a source of truth for runtime
configuration.

**Decision**: Rejected as the primary model.

### Option C: `nara.toml` as authority with code overrides

**Pros**: Simple for projects and AI agents, compatible with editor tooling, still works for
library-style embedded apps.

**Cons**: Requires careful schema versioning and validation diagnostics.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Single project authority | Asset roots, startup scene, task defaults, window defaults, and input-map sources resolve from one manifest for file-backed apps | Manifest tests |
| Code-first compatibility | Minimal examples can still configure `App` without `nara.toml` | Example check |
| Deterministic profiles | Same manifest plus profile name yields same effective settings | Unit test |
| Diagnostic quality | Invalid required fields report structured diagnostics | Unit test |
| Applied task policy | Manifest worker/queue/shutdown values equal installed `TaskPools::config()` | Facade integration test |
| Capability-bound ingest | `nara_project` contains no ambient manifest open and bounded host bytes parse identically | Boundary and parser tests |
| Atomic composition | A missing compiled capability, unrequested plan product capability, missing plugin service, or conflict leaves `App` unchanged and retryable | Facade integration test |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Manifest becomes a dumping ground | Medium | Medium | Keep manifest project-level; scene, prefab, asset, and material data stay in their own files. |
| Embedded users feel forced into files | Medium | Low | Keep resource/plugin configuration as an explicit override path. |
| Profile overlays become order-dependent | Medium | Medium | Require deterministic overlay resolution and tests. |
| Early schema overfits desktop | Medium | Medium | Keep profile fields platform-neutral and let adapters lower settings. |
| Validation and installation closures drift | Critical | Medium | Install from the same inspectable resolved plan that preflight validated and compare installed membership in tests. |

## Consequences

- ADR 0020 remains the layout decision; this ADR defines the settings authority inside that layout.
- Future `nara_project` or manifest code should be a pure validation/lowering layer, not a hidden
  runtime service.
- File-backed launch composition owns manifest capabilities and bytes; project data cannot grant or
  reconstruct filesystem authority.
- Runtime presets and additive capabilities replace the mutually exclusive product plugin-plan
  shape; product subset gates and the separate plugin service closure reject before any `App`
  mutation.
- Invalid duration quantization/overflow, zero/non-finite values, per-kind limits, aggregate task
  limits, and shutdown timeout limits are rejected before lowering.

## Open Questions

- Should profile overlays live inline in `nara.toml` only, or may they be split into profile files
  later while preserving one effective manifest authority?

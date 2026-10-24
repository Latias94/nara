---
type: "Verification Evidence"
title: "Project source authority carrier and creation identity qualification"
description: "Verification Evidence for Project source authority carrier and creation identity qualification."
timestamp: 2026-10-24T06:54:24Z
record_id: "6cdfe4a580b14b8da78e659010497632"
producer_id: "codex"
run_id: "session-01a1629b-source-authority"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
verified_by: "serial nextest; workspace checks; independent module-consumer check; scoped review lenses"
---

# Verification

This checkpoint replaces the unreleased project manifest shape in place. It keeps schema version
1 but requires a non-nil authored `SourceUniverseId` and a project-wide
`NativeDocumentIdentity`; there is no compatibility reader or inferred fallback.

# Result

`SourceUniverseId` is an ECS-independent `nara_core` value with UUID parsing, equality, display,
serde validation and nil rejection. `nara_project` carries the required source universe and native
policy through canonical TOML and `EffectiveProjectSettings`; profiles cannot override them.

Explicit desktop project creation consumes one 32-byte entropy read after dependency preparation,
retains the generated universe and startup Scene stable identity in the plan, emits the exact
manifest and `scenes/startup.scene.json.meta` bytes, validates them against the plan before staging,
and never regenerates them during publication or admission. The existing publication custody keeps
the directory on post-publication admission failure.

This does not qualify native Scene/Prefab loader admission, canonical Prefab provenance, logical
occurrence claims, Runtime/Canvas source keys, or shared UI conformance. Those remain the next
production owner unit.

# Evidence

- The first source tests failed before implementation (`c02fa11a`): current source fields could
  not parse, and the old shape still succeeded. The first green (`0e434444`, four cases) used an
  intermediate schema 2. The user then explicitly requested unreleased formats normalize to 1;
  final schema-1 focused Core/Project evidence passes four cases (`d29ad643`) while missing
  identity, nil/invalid UUID and profile override still reject.
- The original public creation test failed (`754c3b72`) because its starter emitted the obsolete
  shape. `cargo nextest run -p nara --features project-workflow --test project_workflow_creation
  --offline` then passed 11 cases (`cad0775a`).
- `cargo nextest run -p nara --all-features --lib --test project_workflow_creation --test
  project_workflow_cli --locked -j 1 -E 'test(project_workflow::) |
  binary(project_workflow_creation) | (binary(project_workflow_cli) & test(new_command))'`:
  51 passed, 354 skipped (`18ff0bbc`). This includes real Local/Git CLI creation and published
  metadata bytes, not a complete compiled Check/Export trajectory.
- `cargo nextest run -p nara_core -p nara_project -p nara --all-features --lib --locked -j 2`:
  442 passed, 3 skipped. The final root-only all-feature rerun after adding the metadata
  regression and narrowing the error payload passes 384, 3 skipped (`404c0932`).
- `cargo check --workspace --all-features --all-targets --locked`: the first run exposed an
  unhandled `ProjectMember::StartupSceneMetadata` in the Editor Application test fixture. After
  adding that concrete match arm, the complete command passed with existing warnings.
- Independent `tests/fixtures/module-consumer`: `cargo check --workspace --all-features --all-targets --offline` passed and updated its checked-in lockfile for the new `nara_core -> uuid` edge.
- Nine other tracked standalone lockfiles were resolved through serial `cargo metadata --offline
  --format-version 1 --manifest-path ...`, adding the real local dependency edges without package
  version upgrades. A standalone metadata resolution is not a build/test qualification.
- Focused publication revalidation after the review gap fix: metadata tamper and manifest digest
  regressions both passed (`fb527a53`). The first retains a published directory and refuses another
  valid metadata identity with exact `PublishedMember(StartupSceneMetadata)` classification.
- `cargo clippy -p nara_core -p nara_project -p nara --all-features --all-targets --locked --
  -D clippy::correctness` passes with existing warnings. This is not warning-clean evidence.
- Seven scoped review lenses returned no correctness, API, reliability, maintainability or
  adversarial blocking defect. Standards review required this ledger update; testing/reliability/API
  reviews requested the metadata tamper regression now present. The full review receipt and
  bounded post-snapshot follow-up are recorded separately when complete.

No native Editor trajectories, runtime performance or non-Windows execution were run for this
unit. Serial Cargo used one build job. The independent consumer check inadvertently used its
default nested target; further consumer commands must set `CARGO_TARGET_DIR` to the root target.
No generated/user/vendor content was deleted to recover that build storage.

# Follow-up

Integrate one shared native-source admission owner into selected Scene, Prefab and Browser paths,
retain metadata-present/absent observations through closure and revalidation, then replace
requested Prefab coordinates with canonical source bindings before occurrence and UI projection
adoption. Do not add a per-file policy registry or load-time metadata minting.
Current export source closure does not yet include selected Scene metadata; the existing
source-carrying export evidence therefore does not qualify preservation of this new sidecar.

# Citations

- [Source value](../../../../../crates/nara_core/src/source.rs)
- [Project sections/effective settings](../../../../../crates/nara_project/src/sections.rs)
- [Creation owner](../../../../../src/project_workflow/creation.rs)
- [Starter contract](../../../../../src/project_workflow/starter.rs)
- [Publication metadata custody](../../../../../src/project_workflow/publication.rs)
- [ADR 0035](../../../../architecture/adr/0035-project-manifest-and-runtime-settings-authority.md)
- [Source/occurrence design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)

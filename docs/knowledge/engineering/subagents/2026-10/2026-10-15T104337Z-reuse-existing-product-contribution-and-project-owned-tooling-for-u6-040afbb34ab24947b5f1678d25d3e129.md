---
type: "Subagent Finding"
title: "Reuse existing product contribution and project-owned tooling for U6"
description: "The existing typed contribution and runtime plan already provide composition authority; the remaining obstacle is the empty-recipe CLI bootstrap."
timestamp: 2026-10-15T10:43:37Z
record_id: "040afbb34ab24947b5f1678d25d3e129"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
subagent_id: "/root/mods_plugins"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Finding

ProductRecipe, SchemaContribution<P>, and RuntimePlan already provide the composition authority
needed by U6. Add no second GameDefinition, provider registration list, migration builder, or plugin
framework. The actual remaining obstacle is bootstrap discovery and the location of strict tooling.

# Evidence

SchemaContribution owns typed P, its PluginDefinition, and provider definitions. Direct and
file-backed paths install schema authority from that contribution, and plugin_composition tests
already prove one callback registration. The Reference Game provider callback registers its
persistent components and real Weapon migration; ReferenceGamePlugin still redundantly preflights
and registers the same provider. Migrate its bare-plugin test/tracer consumers before deleting that
redundancy, exposing the existing reference_game_contribution function if needed.

CLI check::execute and export::execute use ProductRecipe::new for strict checking before building
the project's binary. A custom schema cannot satisfy that empty-definition precheck. The existing
headless executable must branch into explicit tool mode before normal argument parsing/gameplay,
select its recipe, and own strict checking and publication in that process.

# Recommendation

First finish contribution migration. Then extract export Cargo and acceptance adapters plus native
host filesystem admission from CLI-only modules into one reusable native-tool module, retaining
the path-free project_workflow transaction interfaces. Parent CLI performs bounded manifest/Cargo
discovery and bootstraps headless, then receives bounded summaries; it never reconstructs closure
or publication authority from returned digests. Missing protocol/mode/build is an explicit error.

Reuse load_checked_candidate's resolved candidate/RuntimePlan pair rather than parse a second
definition. Keep source/schema/product agreement, before/after build-input revalidation, exact
artifact selection, and publication revalidation. Bootstrap compilation is not export agreement.
Generic Editor continues data-only bundle recovery without loading project Rust.

The Cargo export adapter currently builds --bins and rejects additional same-package binaries.
Select only --bin headless --bin desktop so Reference Game probes/tools do not require a larger
artifact protocol. Existing CargoMetadataFacts requires desktop target/feature, and desktop_build
fixes dev/desktop: reuse bounded parsing and package/path validation, but give bootstrap Check an
explicit fixed headless role instead of claiming the old type is generic.

The current export build-input inventory has five fixed files (Cargo.toml/Cargo.lock/src/lib.rs
and the two entries). Preserve and state that boundary; do not implement Rust module resolution to
claim arbitrary source equivalence. Reference Game also lacks the starter's full selected-product
observation/first-present export protocol. Use a freshly generated custom-component starter for
the first complete Check/Run/Export trajectory, then migrate the Reference Game observation adapter.

# Disposition

Read-only U6 implementation map from /root/mods_plugins, captured while U5 verification runs. U6
implementation has not started. The renamed-root real compiled fixture should contain a persisted
custom component and migration; the pure project-workflow module consumer must stay Cargo-free.

# Citations

- src/product.rs: SchemaContribution and ProductRecipe::add_contribution
- src/project_host/check.rs: load_checked_candidate
- src/bin/nara/check.rs: execute
- src/bin/nara/export.rs: execute
- src/bin/nara/export/cargo.rs: build_with_target_directory and ArtifactSelector
- src/bin/nara/export/acceptance.rs: observe_built_products and accept
- src/project_workflow/export/build.rs: BUILD_INPUT_PATHS and build preparation
- reference-game/src/lib.rs: ReferenceGamePlugin and reference_game_contribution
- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U6

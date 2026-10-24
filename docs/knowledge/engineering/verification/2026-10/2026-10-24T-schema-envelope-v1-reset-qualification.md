---
type: "Verification Evidence"
title: "Canonical v1 schema envelope reset qualification"
description: "Component schema catalogs and authoring schema bundles now use canonical persistent envelope version 1 while preserving semantic catalog lineage and fingerprint version 3."
timestamp: 2026-10-24T08:06:39Z
record_id: "0f9cd07fa3534b2a8cc0d5e56cab476b"
resource: "docs/architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md"
tags: ["architecture", "format", "reflection", "verification"]
status: "qualified"
producer_id: "codex"
run_id: "session-01a1629b-schema-v1-reset"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "6932ade"
verified_by: "serial nextest; workspace check; cargo fmt check"
---

# Verification

# Result

The unreleased `component_schema_catalog` and `authoring_schema_bundle` envelopes now write and
read canonical `format_version = 1`. The recursive semantic payload, catalog generation,
predecessor lineage, component schema versions, and semantic fingerprint domain remain unchanged.
Envelope versions other than 1 are rejected; no compatibility reader or converter was added.
Repository-owned generated JSON bundles and catalog generations were rewritten to the same
canonical outer version. The existing migration guide remains; no new release migration guide was
created.

# Evidence

- Updated the catalog and bundle format owners, focused format tests, and canonical v1 JSON/RON
  fixtures.
- Updated tracked generated bundles/catalogs under `reference-game`, `samples`, and test
  fixtures. Names such as `component-schema-v1` through `component-schema-v5` remain semantic
  catalog lineage, not envelope versions.
- Updated ADR 0051's compatibility matrix and ADR 0081 to separate file envelope v1 from semantic
  fingerprint v3. The implementation ledger records the reset and keeps source/product/occurrence
  identity outside this unit.
- `cargo fmt --all -- --check`: passed.
- `cargo nextest run -p nara_reflect --all-features --locked -j 1`: 253 passed, 0 skipped.
- `cargo check --workspace --all-features --all-targets --locked` with `CARGO_BUILD_JOBS=1`:
  passed with existing warnings.
- An initial full Reflect run exposed one test fixture still constructing an obsolete version-3
  envelope; the test was corrected to use v1 and the focused cohort plus full 253-test cohort were
  rerun successfully.

# Follow-up

Continue the version-axis inventory at the owning boundaries. Do not rewrite runtime epochs,
stale-result generations, package semver, Cargo lock format, artifact format versions, component
schema versions, or example-game save revisions merely because their values are greater than 1.
Only remove a component migration or lineage edge when a concrete consumer no longer needs it.

# Citations

- [Component catalog format owner](../../../../../crates/nara_reflect/src/format.rs)
- [Authoring bundle format owner](../../../../../crates/nara_reflect/src/authoring_bundle.rs)
- [Canonical file envelope policy](../../../../architecture/adr/0051-persistent-file-envelope-migration-and-golden-fixtures.md)
- [Schema catalog identity policy](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)

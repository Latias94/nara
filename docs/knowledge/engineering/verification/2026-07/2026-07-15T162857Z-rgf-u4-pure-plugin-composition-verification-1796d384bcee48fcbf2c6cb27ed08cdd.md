---
type: "Verification Evidence"
title: "RGF-U4 pure plugin composition verification"
description: "Verified pure profile/plugin resolution, stable construction identity, schema-provider closure, sealed App commit, and independent reference-game consumption."
timestamp: 2026-07-15T16:28:57Z
record_id: "1796d384bcee48fcbf2c6cb27ed08cdd"
tags: ["rgf-u4", "verification", "plugins", "composition", "schema", "lifecycle"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
pre_commit_head: "1f635e6084917adcaf02fd16c017866ab8519d4b"
verified_by: "cargo-nextest,cargo-check,cargo-fmt,cargo-clippy,independent-review"
---

# Verification

RGF-U4 replaces App-mutating profile expansion with a pure, repeatable plugin plan. Static plugin
declarations, stable construction-policy/configuration identity, data-only groups and slots,
type-directed edits, product capability and service closure, schema-provider selection, private
preparation, preflight, and sealed App commit now form one inspectable path. The independent
reference game consumes that path through the public root package.

# Result

- `PluginPlan` resolution is pure and repeatable. Plan and group fingerprints use framed canonical
  inputs and include declarations, definitions, slots, provenance, nested group structure, and
  intrinsic edits.
- Repeated top-level composition preserves repeatable definition witnesses. Opaque direct plugin
  instances remain intentionally non-repeatable, and overlapping product groups accept atomic
  type-directed configure, disable, insert-before, and insert-after edits.
- Declaration, group, factory, schema-provider, preflight, build, finish, shutdown-error, and
  shutdown-panic failures retain typed phase evidence. Shutdown is reverse-order and once-only.
- Generic schedule execution seals before running a custom schedule and rejects built-in startup
  and core stages; a missing custom schedule leaves the App configurable.
- Built-in schema owners validate component registration conflicts during preflight. Provider
  bindings use stable semantic IDs rather than function addresses, and provider panics become
  retryable typed composition errors.
- Manifest-derived configuration remains private: project name and window-title canaries do not
  appear in candidate, lineage, `PluginDefinition`, `ProjectRuntimePlugins`, or `RuntimePlan`
  `Debug` output.
- The large implementation files were split without public-path changes: plugin definition, group,
  resolution, and fingerprint internals; root product bundles; and project ingest/composition now
  have separate focused modules.

# Evidence

## Automated gates

- The exact seven-crate U4 gate passed 164/164 (nextest run
  `8ed22423-9590-4985-aa70-0105007bce32`).
- The root default-feature U4 gate passed 19/19, and the all-feature overlapping-group/privacy gate
  passed 16/16 (run `22e34beb-bb0e-444d-b331-a1724ef40ed9`).
- The final post-Clippy `nara_app` composition run passed 23/23 (run
  `9d0f6679-55b8-4fa0-b5e4-9a1b1a8e7147`). The explicit dependency tuple test compiles and
  executes the migration document's replacement for hidden prerequisite installation.
- The focused reference-game composition test passed, and its final independent full run passed
  17/17 (run `e73d9b15-c3ab-41ef-a3c3-355aec9f92d2`).
- The final root workspace run passed 700/700 with three declared conditional skips (run
  `89cdde84-fb4b-46b6-9aba-dee68653919b`). `cargo check --workspace --locked` passed.
- Root no-default, default, every coarse single-feature ceiling, and
  `--all-features --all-targets` checks passed. The independent reference-game all-target check
  passed.
- The `windowed_clear`, `windowed_sprites`, and `runtime_ui_panel` examples passed under the
  combined runtime-2d/runtime-ui/desktop-winit/render-wgpu ceiling. Backend dependency searches
  found native imports only in the root wiring and owning adapter crates.
- Root and reference-game formatting checks, architecture-document tests (5/5), and staged plus
  unstaged `git diff --check` passed.

## Clippy scope

- `nara_app --all-targets` passed with all warnings denied.
- The U4-owned root lib, plugin-composition, and product-capability targets passed with all warnings
  denied after allowing only the pre-existing `result_large_err` contract in
  `ProjectCandidateError`.
- The U4-owned reference-game lib and plugin-composition target passed under the same single
  baseline allowance.
- A strict all-workspace run remains blocked by pre-existing lints outside the U4 diff:
  `nara_asset` has `double_must_use`, `too_many_arguments`, and `derivable_impls`; root and
  reference-game image safety fixtures have `needless_return` and `too_many_arguments`. These are
  not treated as U4 failures or silently changed during this unit.

## Independent review

- API-contract review findings for built-in schedule bypass, overlapping edits, incremental
  definition identity, explicit construction policy, canonical fingerprints, and the migration
  example were fixed and covered by focused tests.
- Security review found no production disclosure bug; its remaining window-title canary gap was
  added and passed.
- Agent-native review found no U4 public-action gap. Product startup, managed runtime control, and
  Editor commands remain correctly assigned to later units.
- Three independent mechanical-split reviews preserved public paths and behavior; their focused
  checks and the combined workspace gates passed after integration.

# Non-Claims And Follow-Up

- U4 does not own a product start attempt. Retaining a partially committed failed App and its
  obligation ledger through pre-publication failure remains RGF-U24 work.
- U4 does not load asset roots, startup-scene closure, or an immutable startup-content snapshot;
  RGF-U12 owns that boundary.
- U4 does not introduce `RuntimeInstance`, gameplay fault propagation, or bounded runtime close;
  RGF-U5 owns those contracts.
- External package contribution, exclusive Host/runner replacement, and cross-plugin version
  negotiation remain evidence-triggered follow-up rather than implied U4 support.
- The next dependency-ready units are RGF-U12 and RGF-U5. U26 follows U12, and U24 joins the
  content and runtime owners only after both paths are proven.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`
- `docs/architecture/runtime-composition-interface-design.md`
- `docs/architecture/adr/0003-own-app-plugin-and-schedule-lifecycle.md`
- `docs/architecture/adr/0010-plugin-lifecycle-dependencies-and-failure.md`
- `docs/architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md`
- `docs/architecture/adr/0079-root-product-capabilities-and-placeholder-domain-retirement.md`
- `docs/migrations/2026-07-engine-foundation.md`

---
type: "Verification Evidence"
title: "PCC-U3 ProductRecipe ordinary Host authority verification"
description: "Verifies one ProductRecipe ordinary Host composition input, explicit one-definition advanced probes, schema-authority closure, and Headless/Desktop/Editor parity."
timestamp: 2026-08-13T04:50:32Z
record_id: "02de9ad341db4dcbb1519e0f5e8843f1"
tags: ["product-composition", "product-recipe", "project-host", "pcc-u3"]
status: "completed"
producer_id: "codex-root"
run_id: "product-composition-contribution-correction"
related_plan: "docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "864c151"
verified_by: "cargo-nextest-and-static-review"
---

# Verification

PCC-U3 only. This record verifies commit
`864c151620aac9524d99a02ab01e301cf1d26165` on
`refactor/engine-foundation-contracts`. It does not publish the Product Composition Conformance
Baseline, resume RUP-U3, or claim Runtime UI product validation.

All Cargo commands ran serially with `CARGO_BUILD_JOBS=1`, reused the workspace `target`
directory, and used one nextest test thread.

# Result

- Ordinary `HeadlessRunIntent`, `DesktopRunIntent`, and `EditorProjectIntent` expose one
  `ProductRecipe` composition input plus execution policy. Their raw configure, disable, relative
  ordering, and independent schema-provider channels were removed without compatibility wrappers.
- `project_host::advanced::RuntimeProbe` accepts exactly one non-replayable plugin definition.
  It cannot carry profile edits or a schema-provider list, and a schema-owning definition is
  rejected during pure plan resolution before an `App` exists.
- Profile-owned Image, Tilemap, window, WGPU, and presentation contribution choices remain
  project/profile facts. A recipe can add or configure only entries it owns.
- The Editor runtime bridge still lowers through a private Host contribution. Direct `App`
  composition remains a separate explicit API.
- First-party Reference Game helpers now reuse official recipes and add at most one advanced probe
  instead of reconstructing a parallel product closure.

# Evidence

- `cargo nextest run --locked --features runtime-2d,serde,tooling --test
  project_host_boundary --test project_runtime_boot --test workspace_play_runtime --test
  plugin_composition --test-threads=1 --no-tests fail` — **61/61 passed**.
- `cargo nextest run --locked --manifest-path reference-game/Cargo.toml --features
  host-parity,desktop-accessibility-probe --test plugin_composition --test runtime_drive --test
  first_wave --test-threads=1 --no-tests fail` — **29/29 passed**.
- `cargo nextest run --locked --manifest-path reference-game/Cargo.toml --features
  host-parity,desktop-accessibility-probe --test host_parity --test-threads=1 --no-tests fail` —
  **2/2 passed**.
- `cargo nextest run --locked --features runtime-2d,serde --test runtime_runner_contract
  --test-threads=1 --no-tests fail` — **5/5 passed**, including the independently locked renamed-root
  external consumer.
- `cargo fmt --all -- --check` and the scoped `git diff --check` passed.
- Three independent simplification reviews produced six applied clarity/reuse improvements and no
  actionable hot-path change for this startup-only composition seam.

# Follow-up

PCC-U4 is the sole active convergence unit. It must run the complete serial composition gate,
publish a distinct Product Composition Conformance Baseline, update the migration and architecture
authority sources, and explicitly rebase RUP-U3 without replacing its UI Foundation
`baseline_record`.

# Citations

- `docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md`
- `docs/architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md`
- `docs/architecture/adr/0106-content-and-presentation-product-composition.md`
- `864c151 refactor(host): make ProductRecipe the ordinary authority`

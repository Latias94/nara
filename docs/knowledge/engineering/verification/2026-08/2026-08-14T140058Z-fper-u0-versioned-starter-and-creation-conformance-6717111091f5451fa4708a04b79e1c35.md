---
type: "Verification Evidence"
title: "FPER-U0 versioned starter and creation conformance"
description: "Verifies the closed desktop-2d/v1 starter, typed dependency authority, atomic publication, ordinary admission, thin CLI and independent public consumer."
timestamp: 2026-08-14T14:00:58Z
record_id: "6717111091f5451fa4708a04b79e1c35"
resource: "nara-first-party-project-workflow"
tags: ["project-workflow", "fresh-project", "fper-u0", "starter", "creation", "conformance"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-14-002-feat-fresh-project-source-carrying-export-and-repair-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0b99ad4"
verified_by: "focused-nextest-generated-product-consumer-clippy-static-review"
---

# Verification

Commit `0b99ad4` closes FPER-U0 over the reviewed activation baseline `44a1157`. The
implementation lineage is `796ad88`, `919bfe9`, `68d06bd`, `18020f4`, `f38c73a`, `aacbcb3` and
`0b99ad4`.

The root `project_workflow` Module now owns one embedded `desktop-2d/v1` starter, a closed typed
dependency-source family, immutable project planning, finite-tree staging, exact no-replace
publication and ordinary post-publication admission. `src/project_workflow/starter/product_contract.rs`
is a private deep Module: it renders and validates the generated Cargo workspace, project profiles,
shared `ProductRecipe`, Rust entry-point relationship and startup Scene before any staging begins.
The generated project is then reopened through the ordinary manifest, product-composition and
project-content authorities.

The thin `nara new` Adapter derives local Cargo paths only from an authorized checkout-to-parent
relation. Native path APIs establish component boundaries before `nara_fs::RelativePath` assembles
the logical path, so Windows separators are not reinterpreted as data. Immutable Git sources use a
credential-free HTTPS authority/path grammar and an exact 40/64-character ASCII hexadecimal
revision; FPER-U0 does not claim network fetch or dependency availability.

The external `module-consumer/project-workflow-consumer` remains compile-only and imports the public
root `project_workflow` Module without CLI, desktop, Cargo-process or private-crate authority. One
real generated-project canary additionally runs Cargo metadata and an offline
`cargo check --all-targets --all-features`, proving the independent workspace, library, headless and
desktop targets rather than relying only on source markers.

# Result

- `nara new desktop-2d/v1` creates and admits one normal project on the hosted Windows local-volume
  tier. The hard generated-product canary cannot silently downgrade a creation failure to success.
- Unsupported or unproven filesystem tiers remain explicit pre-staging rejection outcomes; they do
  not create a destination or hidden staging residue.
- Local dependency relations, immutable Git coordinates, project slug, starter inventory, generated
  Cargo/product agreement and published member digests are validated before a `Created` receipt can
  be returned.
- The generated project is isolated from an ancestor Cargo workspace and compiles its library,
  presentation-free headless target and desktop-feature target against the selected local checkout.
- No template registry, generic generator SPI, importer framework, export abstraction, repair
  framework or Reference Game identity was introduced.
- FPER-U0 is complete. Check/run, startup-content closure, source-carrying export and explicit repair
  remain owned by FPER-U1 through FPER-U3 and are not claimed by this record.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo nextest run --locked --package nara_fs --test path_contract --test-threads=1`
  - 5 passed; run ID `560d0662-f7a8-464d-84ef-11a7fb1c3378`.
- `CARGO_BUILD_JOBS=1 cargo nextest run --locked --no-default-features --features project-workflow --test project_workflow_creation --test project_workflow_cli --test-threads=1`
  - 12 passed on the final implementation commit; run ID
    `23459e95-6336-49cc-94e1-00058f42d34d`.
- `CARGO_BUILD_JOBS=1 cargo nextest run --locked --no-default-features --features project-workflow --lib --test-threads=1`
  - 53 passed; run ID `08c6ce3f-c788-4208-8181-bb8fa3040371`.
- `CARGO_BUILD_JOBS=1 cargo check --manifest-path module-consumer/Cargo.toml --locked --workspace --all-targets`
  - passed after the exact consumer lockfile was refreshed without dependency upgrades.
- `CARGO_BUILD_JOBS=1 cargo nextest run --locked --test module_consumer_boundary --test ci_policy --test-threads=1`
  - 13 passed; run ID `4586f01d-ca29-4810-a0f8-7c9057d7c052`.
- `CARGO_BUILD_JOBS=1 cargo check --locked --tests` - passed, including the default-feature build in
  which the feature-gated CLI test is absent.
- `CARGO_BUILD_JOBS=1 cargo clippy --locked --package nara_fs --all-targets -- -D warnings` - passed.
- Scoped root Clippy over the project-workflow library, CLI and two integration tests passed with
  only the repository's explicitly enumerated pre-existing dead-code and large-error lint classes
  allowed; no changed-file Clippy warning remained.
- `cargo fmt --all -- --check` and scoped `git diff --check` - passed.
- Independent correctness, testing, maintainability and project-standards reviews found no unresolved
  P0, P1 or P2. Review corrections rejected credential-bearing Git coordinates and signed ports,
  added structural product-contract validation, fixed Windows native path conversion, gated the CLI
  test by its feature, and promoted the generated project from metadata-only inspection to a real
  Cargo compilation.

# Follow-up

Activate FPER-U1 as the sole active unit. It must add the public project check seam, one
project-content-owned `StartupContentClosureReceipt`, presentation-free headless execution and a
bounded real desktop run without changing the completed creation/publication Interface.

# Citations

- `src/project_workflow/`
- `src/bin/nara/`
- `starters/desktop-2d/v1/`
- `module-consumer/project-workflow-consumer/`
- `crates/nara_fs/src/path.rs`
- `tests/project_workflow_creation.rs`
- `tests/project_workflow_cli.rs`
- `tests/module_consumer_boundary.rs`
- `docs/architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md`
- `docs/plans/2026-08-14-002-feat-fresh-project-source-carrying-export-and-repair-plan.md`

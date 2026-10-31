---
type: "Work Progress"
title: "Cooked Player content assembly and package binding integration in progress"
description: "Work Progress for Cooked Player content assembly and package binding integration in progress."
timestamp: 2026-10-31T04:31:15Z
record_id: "43a245bed70541e6bf6656113c7297c9"
producer_id: "codex-foundation-main"
run_id: "cooked-player-delivery-2026-10-31"
---

# Summary

The complete foundation Goal remains active. `main` and `refactor/foundation` started this unit at
`60aa0319c78ee2d82d12dde72adde4718386d493`; predecessor explicit Cargo/source/native qualification
is complete and is not being repeated without an affected-path reason. Cooked Player delivery is
in development under its existing plan, with no package/native qualification or commit yet.

# Details

- Cicero implemented the finite `project_workflow::cooked_content` owner: actual Scene cooking,
  retained verified groups, concrete Input/TileSet encoders, deterministic member paths, original
  captured-base admission and complete deep-validation coverage. Surplus members reject rather
  than escape decoder validation. A new crate-private `capture_members` reuses the original
  `AdmittedCatalog`; it is not a public bypass.
- The parent implemented generated independent Rust constants, exact captured-Cargo input and
  full-selection binding, streamed expected-pack hashing, three-file staging/verification and
  existing whole-directory publication. Control panics become finite failures; published receipts,
  clean rejections, orphan custody and unknown begin outcomes stay distinct. Manifest encoding
  now uses the canonical version-one `player_package` envelope; it is a delivery description, not
  Runtime expectation authority.
- Native executable-relative opening is owned by `project_host` with `runtime-2d + serde`, not
  project-tooling. Player execution need not enable Cargo or cooking just to open its original pack.
- Carson's six design findings are incorporated in ADR0007/0103/0110 and Proposed0088: independent
  expectation trust, exact provenance/selection binding, Host-owned content freshness envelope,
  every delivered payload decoded before publication, original cleanup/receipt custody, aggregate
  live-allocation accounting, and complete Product versus demand semantics. Actual source reviews
  by Carson (package) and Ptolemy (assembler) remain pending at this capture.
- Heisenberg owns the independent actual cooked Host/Player consumer in
  `tests/fixtures/cargo-build-consumer`; its source is still being written. It must demonstrate real
  Image/compound cooking, global physical Input, shared content, Deferred travel, relocation,
  unrelated CWD and source/cache/toolchain absence. Existing seven processed Input cases remain.
- The supplied ZIP already has a complete current-HEAD reconciliation and byte-identical imported
  studies. No missing archive document requires a roadmap reset. Authored evolution, joint
  motion/execution, finite render graph/output completion and editable Nara-drawn Editor remain open.

Actual verification so far:

- `cargo +1.95.0 check -p nara --no-default-features --features project-tooling --lib --locked`
  passed in 7.66 seconds before the latest manifest/test changes (existing cfg warnings).
- Focused cooked-content owner nextest: **15 passed**, run
  `09d7d76e-dd97-4b37-bf03-e7c9a6a3190d`, build 1m16s, execution 1.397s. A first attempt exposed
  three existing tests importing optional tooling persistence without its feature. Only those
  exact functions now carry `cfg(feature = "tooling")`; ordinary module tests remain enabled.
- Initial package nextest: **3 passed / 3 failed**, run
  `07f28b4c-7dd0-40e0-af11-e1dc440255cb`. All failures used a test-only Runtime allowance of32MiB
  while retaining the Scene decoder's default256MiB work promise. The fixture now explicitly
  selects a1MiB Scene promise, without relaxing production admission. This correction is unrun
  at this capture. New package source changed after the preceding core build; do not overstate
  that build's coverage.

# Next Action

Finish source reviews and owning corrections, then freeze writers and inspect diffs before
formatting. Run focused owner tests serially, the independent actual cooked Player workflow,
affected Runtime/source/native regressions and feature/Clippy checks. Do not commit or locally
fast-forward this WIP before complete consumer and independent review qualify it. Preserve vendor,
refs and concurrent edits; Cargo one job, one process, root target; no push/PR.

# Citations

- [Cooked Player plan](../../../../plans/2026-10-31-cooked-player-delivery-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Research reconciliation](../../../../research/2026-10-30-engine-direction-reconciliation.md)
- [Predecessor qualification](../../verification/2026-10/2026-10-31T035034Z-explicit-cargo-builds-captured-executables-and-supervised-protocol-qualification-4cfff5128fa64423b6f234ea7b312c63.md)

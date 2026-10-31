---
type: "Work Progress"
title: "Explicit Cargo contracts independently reviewed with actual native and foreign consumers"
description: "Work Progress for Explicit Cargo contracts independently reviewed with actual native and foreign consumers."
timestamp: 2026-10-31T02:00:11Z
record_id: "ca2e1780a12140d6acee32ad39c35481"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-explicit-cargo-build-selection-plan.md"
git_branch: "refactor/foundation"
git_commit: "0c36715"
---

# Summary

The explicit Cargo selection unit remains uncommitted on `refactor/foundation` at `0c36715`.
The complete foundation Goal stays active. Core, native owner, Export input binding and Editor
generation changes have independent source review; actual custom native/foreign consumers pass.
Original source Export and selected native Editor qualification are still pending.

# Details

Review corrected three pure Cargo assumptions: workspace metadata features are not the selected
compiler unit's features; dependency-qualified `required-features` eligibility belongs to Cargo;
and built-in test/bench profiles use debug/release output directories. Actual artifact features
now feed captured provenance and Editor generation, while the invocation digest labels its separate
metadata observation honestly. No feature resolver or compatibility alias remains.

Export now retains captured declared input path/digest facts and checks each role against the exact
preparation inventory, preventing old A code from being bound to new B inputs despite identical
startup agreement. Staging reads captured bytes. Independent review approved this scoped binding.
The native owner keeps all nested target-directory leases with unconfirmed child custody. Bootstrap
uses the same private bounded status/output helper and transparently preserves process ownership;
discovery errors are no longer erased as strings. The overflow-plus-failed-reap regression retains
the child through bootstrap error conversion and successful retry.

Actual verification so far:

- Root/Editor focused run `b2df86ec-5cab-46ac-ac52-8856f302b8e5`: 137 passed, 1288 skipped.
- Final root owner/custody run `84bfcaf3-b873-4931-a648-12e013fe5f33`: 51 passed, 1084 skipped.
- Public actual build run `76e414eb-e1fb-4976-ab63-220642724a49`: 2 passed in 18.966s. The
  generated workspace proves metadata-only activation through a decoy dependency, actual narrower
  artifact features and dependency-only missing-required-feature rejection. It captures and runs
  custom native Host/Player bytes, exercises all seven source-free processed Input cases, and builds
  a foreign WASI artifact without executing it. Independent review approved the corrected oracle.

Earlier run `d3f5bbf6-a918-421a-963c-243516609eba` passed foreign but failed native on an
unsupported cross-build executable byte-equality assertion. Equivalent selections now require
the same actual features and observable behavior, distinct requested/provenance facts and each
capture's own exact digest. Captured A immutability checks remain. No reproducible Rust executable
claim is selected. The stronger dependency-only negative oracle and metadata assertion pass in the
final public run above.

No Cargo runs overlap. The original renamed-root processed process test is now compiling/running
as exec session `16179`; its shared fixture extraction still needs this original-consumer evidence.
Root target is reused. Vendor and reference checkouts remain untouched. No commit/push/PR yet.

# Next Action

Finish the original processed process, real source CLI Check/Run/Repair/Export and selected native
supervised trajectories. Run affected minimal/Clippy/fmt and document checks, record exact final
qualification and only then commit and locally fast-forward main. The new build guide documents
logical budget, declared-input and cleanup limits. Complete target-content/package delivery is
still the next R7 outcome; a read-only source study is refining its consumer while tests run.

# Citations

- [Unit plan](../../../../plans/2026-10-31-explicit-cargo-build-selection-plan.md)
- [Build guide](../../../../guides/cargo-builds.md)
- [Accepted contract](../../../../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md#explicit-cargo-selection-and-captured-build-results)
- [Independent consumer](../../../../../tests/fixtures/cargo-build-consumer/README.md)
- [Predecessor progress](2026-10-31T002716Z-explicit-cargo-selection-and-captured-executable-ownership-replacement-in-progress-29b2a117d6af41679ca9fef5d1250965.md)

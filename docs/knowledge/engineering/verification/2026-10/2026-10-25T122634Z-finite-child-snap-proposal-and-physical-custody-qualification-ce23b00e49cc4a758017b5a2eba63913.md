---
type: "Verification Evidence"
title: "Finite child Snap proposal and physical custody qualification"
description: "Qualifies the opt-in real child proposal through the existing source owner after error-fact regressions, complete review and controlled Reopen proof correction; broader R1 and R1b remain open."
timestamp: 2026-10-25T12:26:34Z
record_id: "ce23b00e49cc4a758017b5a2eba63913"
producer_id: "codex-root"
run_id: "foundation-child-snap-20261025"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "a0abec0780965c5303386a2da9bc087abd4bbf7b"
---

# Verification

This covers only the default-off `snap-child-experiment` in the independently compiled panel
consumer on `a0abec0` plus the [nine-file normalized source manifest](../../../../verification/data/2026-10-25-finite-snap-child-source.json).
Manifest SHA-256: `c4bfc4a0b7dd1703dd849735d3afaf6a98abaf860fa73e9ff865ed96ddaea5d7`.
No engine owner API, Accepted ADR, Play supervisor, document store or history implementation was
changed. This is an R1 subunit, not full R1/R1b or foundation Goal completion.

# Result

- Parent-side `SnapInput` captures two coherent bounded source observations without computing
  the answer. The selected helper invokes the shared numeric source in a real process and returns
  only coordinates or bounded refusal codes. The original target/basis remains in the parent.
- One 40-byte request, one 32-byte response plus EOF and at most 4096 stderr bytes use fixed
  buffers with one-byte overflow probes. Malformed/version/identity/value/exit disagreements and
  output overflow reject; raw helper stderr is not projected as a public diagnostic.
- One lane owns spawn, direct child, concurrent stdin/stdout/stderr workers, exact request
  affinity and terminal observation. Cancel/timeout withdraw publication interest; the physical
  slot stays occupied until child reap and all worker joins. Observed failure survives later
  cancellation; worker panic survives helper nonzero exit. Cleanup progress is independent.
- The actual child proposal drives inactive-document Source preparation, exact forward/inverse
  preview, one Undo/Redo and attempt-correlated Save/Reopen without changing active navigation.
  Controlled edit/Reopen barriers return a real changing proposal with its original basis;
  validation/preparation reject it without changing the reopened document or history.
- The original six local/panel proof tests remain. Only the shared numeric implementation and
  fixture helpers moved; the local planner now composes capture, calculation and attachment.
  `SnapInput::proposal` bounds values but does not certify an arbitrary helper's mathematics.

# Evidence

## Executed Failures And Review

Initial focused nextest `6554f316-c1c3-45a1-9a05-9f51ee6cbc34` passes11. Adding a held I/O-return
barrier gives `6077879d-62ec-4c02-a8d6-5a8d33f4bad2`, passing12. The barrier proves child exit
cannot release a lane while a worker remains unjoined; the 500 ms drain observation reports
incomplete cleanup and refuses reuse until actual return.

An independent custody pass found two P2 error-fact defects. Before correction, nextest
`d683b543-475f-43b6-a52f-7e52fea9021d` runs the two strengthened tests and fails both exactly:
known Protocol became Cancelled, and controlled stdin worker panic became NonzeroExit. The
fix retains failed outcomes, prioritizes joined worker panic and still joins every admitted I/O
worker before selecting a result. A direct execution of the newly compiled child test binary
then passes7; that was intermediate evidence, not a successful full Cargo command.

Complete CE review `20261025-115745-6488d55a` runs nine native Codex lenses, merge, one fresh
validator and report, with every handle collected/closed. Its original receipt remains
`complete / Ready with fixes`, with one confirmed P2: the Reopen half discarded a NoChange
completion. The correction restores off-grid input, holds a fresh helper across exact Reopen,
then requires the returned Change/original basis and actual preparation refusal. It preserves
the edit-staleness proof and all six prior Snap proofs. An independent source correction review
confirms #1 is addressed and finds no new actionable issue; it also rechecks the two earlier
error-fact fixes. This follow-up remains separate from the original review receipt.

Five simplification lenses ran. Applied changes share the existing calculator/fixture, derive probe
sizes from wire limits, keep one cancel state and remove a non-contract private module header.
No proof assertion, finite bound, source admission or physical cleanup obligation was weakened.
External programming models were not used; same-serving-model agreement is not cross-model proof.

## Final Serial Qualification

All Cargo uses Rust 1.95.0, explicit `RUSTUP_TOOLCHAIN=1.95.0`, `CARGO_BUILD_JOBS=1` and root `target`.

| Check | Actual result |
| --- | --- |
| Focused local + real-child nextest | `568c84c7-2349-41f8-8fa5-0e14e73a88ea`: 13 passed, 0 skipped |
| Package all-target feature-on Clippy | Passed `-D clippy::correctness -D clippy::suspicious` |
| Package all-target default-off check | Passed with `--no-default-features` |
| Root and independent-consumer format checks | Both passed |

Memory render/check and validation pass with existing historical metadata warnings. A focused
document check resolves803 local file links across six changed documents; it is not a general
anchor validator. `git diff --check` passes and all nine qualified text hashes remain unchanged.

The [guide](../../../../guides/editor-panels.md#explicit-finite-child-experiment) gives the exact
nextest command. Final logs use the local temporary names `nara-r1-child-final-focused.log`,
`nara-r1-child-final-clippy.log` and `nara-r1-child-final-default-off-check.log`.
`nara-r1-child-review.txt` locates full review, correction and pre/post source artifacts.

Final output contains 24 per-child timing observations across success, refusal and deliberately
held/cancelled cases: spawn observation11.286–29.656 ms, reap22.823–255.576 ms, all I/O joined
22.902–561.953 ms. The held reader deliberately exceeds500 ms. These are one warm Windows debug
test execution, not a latency SLO, P99, cold-build benchmark or cross-platform comparison.

## Build Resource Failures

Two intermediate Cargo attempts fail linking unrelated native canaries: LNK1318 on Text import,
then LNK1140 on the panel canary, with F: nearly full. Neither is a behavior-test red or a passing
Cargo qualification. Four specifically identified current-turn PDB files (failed outputs and their
earlier root copies) were moved, not discarded, into the C: review run's `pdb-link-failure` folder
after absolute-path/time/ownership checks. No source/vendor/other cache was removed or moved,
no new target was allocated and no Cargo clean ran. The subsequent final nextest command passes.

# Follow-up

- This is explicitly selected trusted **direct-child** execution; the helper creates no descendants.
  Blocking std I/O and Drop do not guarantee bounded arbitrary-native/process-tree cleanup. A
  five-second deadline can revoke interest during a late spawn; it cannot interrupt OS spawn.
  The controlled test delays observation after a real spawn, not the kernel's process creation.
- No new native Editor window/GPU, Linux/macOS execution, install/update/disable/remove or inline
  Inspector qualification is claimed. R1b still needs a runner-owned physical lane surviving UI
  retirement and a measured placement/dependency comparison. Window-free execution here does not
  prove a GUI-free package dependency graph; the existing consumer still compiles native canaries.
- The R4a sidecar identified callback-order versus dependency-completion pressure and prepared a
  disposable public probe outside the repo. It is uncompiled/unexecuted; neither a typed composite
  nor a sealed dependency plan has been selected. Continue dependency-ordered waves under the
  master plan; do not mark the Goal complete from this experiment.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Finite child design and alternatives](../../../../research/2026-10-25-finite-snap-child-proposal.md)
- [Actual child/public workflow](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/snap_child_proposal.rs)
- [Private custody owner](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/snap_child_process.rs)
- [Previous source-built qualification](2026-10-25T112059Z-source-local-snap-public-consumer-and-installer-qualification-f89f21479f7d4c06a81ab5bc63b5ff87.md)

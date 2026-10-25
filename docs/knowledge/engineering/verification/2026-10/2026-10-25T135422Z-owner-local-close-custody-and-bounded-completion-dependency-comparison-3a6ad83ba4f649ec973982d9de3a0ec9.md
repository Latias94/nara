---
type: "Verification Evidence"
title: "Owner-local close custody and bounded completion dependency comparison"
description: "Qualifies local Rust close owners, exact registration custody and typed dependent release through public Runtime consumers; actual native graphics integration remains open."
timestamp: 2026-10-25T13:54:22Z
record_id: "3a6ad83ba4f649ec973982d9de3a0ec9"
producer_id: "codex-root"
run_id: "foundation-native-close-20261025"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "2c13c6715035ba69d1a0133cd9a6593f97490cda"
---

# Verification

R4a local Rust custody and bounded alternative comparison, on the working source descended from
`2c13c67`. This is a scoped prerequisite in the full foundation plan. The implementation changes
the concrete participant/ledger transport boundary; it does not migrate Editor graphics teardown.

# Result

`RuntimeCloseParticipant` admits `!Send` local controllers. Concrete ledgers are owner-local;
transportable definitions remain separate. Existing Runtime owner, failure, retirement and
quarantine paths keep local state on its constructing thread. No new close driver, crate, public
graph, compatibility layer or migration guide was added.

The public incumbent characterization demonstrates independent reverse callbacks without a
completion barrier. The two-node comparison exercises a typed composite and a private sealed
plan with the same provider-reading controller. Both gate release for Pending, retryable and
terminal incomplete results and retain completed-error facts. The fixed aggregate selects typed
composition; missing/cyclic plans and declaration-order independence remain bounded test evidence,
not a general graph qualification. ADRs 0003/0010/0042/0082/0084 and the current design/navigation
are reconciled with this decision.

# Evidence

All Cargo commands use `+1.95.0`, `RUSTUP_TOOLCHAIN=1.95.0`, `CARGO_BUILD_JOBS=1` and the existing
`F:/SourceCodes/Rust/nara/target`, serially. Independent fixtures also use that root target.

| Check | Actual result |
| --- | --- |
| Public incumbent characterization | 3 passed; one ignored child helper is actually launched by its parent; nextest `49e41687-c4a0-4d4a-918f-2e30e1901e18` |
| Local registration before boundary correction | Compile failed with E0277 for `Rc<Probe>: !Send`, after correcting an unrelated fixture `unwrap_err` Debug constraint |
| Local registration after correction | 2 passed; nextest `e14e8310-815b-4b3a-98d3-4b4d4effc36e` |
| App/Tasks library regression | 121 passed, 0 skipped; nextest `f776029e-f4ea-4246-ad53-4cf54d511612` |
| App complete test cohort | Initial 147 passed, one child-helper skip as above (`43b88915-0aa9-40fa-8f35-c5fadcce41a4`); final proof-corrected source 147 passed, one helper skip, nextest `76249f40-262d-4180-a00d-daa5b5b59200` |
| Reviewed proof corrections | Final affected comparison/registration cohort 9 passed, 0 skipped; nextest `be20da2e-6257-4e96-88a2-dcedb9dde9a1` |
| Independent renamed-root consumer | 1 passed, 0 skipped; nextest `56d120bd-136d-44e9-9b21-f25044929093` |
| App doctest | 1 compile-fail test passed, proving the concrete ledger cannot be sent |
| App all-target correctness/suspicious Clippy | Passed; affected comparison/registration targets rechecked after proof corrections; unrelated existing large-error/type-complexity/style warnings remain |
| Workspace all-feature all-target locked check | Passed in 5m57s, including Editor and downstream consumers; existing unused/dead-code warnings remain |
| Root and renamed-root fixture format checks | Passed on final Rust source |
| Changed document links, diff whitespace and engineering memory validation | Passed; historical memory warnings retained |

The independent fixture's minimal runtime feature union emits existing unused/dead-code warnings
from unedited root owners. No warning-free claim follows. Source models run on Windows; there
was no native window, GPU or SDK execution in this unit.

## Independent Review And Corrections

- Source/architecture review `01a17342-245c-7551-850a-2d9427088b19` grounded fixed typed ownership
  against current registration consumers, Bevy render return/pump and Godot finish barriers. It
  warned that an Editor Host aggregate must not be moved into a Play Runtime to reuse the ledger.
- API review `01a1734b-b431-7d13-8983-34c73dad7090` found two P3 document contradictions: stale
  Drop/unwind claims and stale architecture navigation. Both were corrected; the API review found
  no P1/P2 implementation regression in its inspected scope.
- Adversarial review `01a17352-722b-71b1-af67-5ea8c8fdcfaa` found two P2 proof gaps (unwind could
  invoke callbacks unnoticed; duplicate owners used indistinguishable probes) and one P3 gap
  (terminal repeated-callback evidence covered only the typed candidate). Tests now snapshot
  pre-drain zero callbacks, distinguish both owner identities and check both terminal traces.
  The final affected nine-test run above passes. These are retained review findings, not a
  zero-finding review or reproduced product bug. No full nine-persona CE receipt is claimed.
  Both reviewers independently checked their corrections afterward: all original findings are
  closed in their scoped source follow-ups, with no new issue found. The final parent-executed
  App cohort above supplies runtime evidence separately from those read-only reviews.
- Native follow-up researcher `01a17355-a7db-77c0-8156-04bec88829f1` checked the captured outcome
  inventory and narrowed error retention to facts actually exposed by the SDK, distinguished
  authority acknowledgment from verification, and restored pre/postflight and both Host exit
  entry points. The source inventory incorporates those clarifications; its proposed native
  implementation and tests remain unexecuted future work.

## Artifact Cleanup

The user had cleaned and rebuilt the checkout. Current F: free space permitted reuse of `target`.
Four exact PDB artifacts recorded by the preceding handoff were resolved under the named
`20261025-115745-6488d55a/pdb-link-failure` temporary review directory and removed, releasing
1,979,398,581 bytes. The review's source findings and logs were retained. D: candidate build
directories were for other projects and were untouched. No source/vendor or global cache cleanup.

# Follow-up

- Actual `NativeImgui` SDK shutdown/result classification, retained Host retiring owner and native
  qualification remain open. The model's consumer destructor revokes a synchronous capability;
  it is not evidence that an arbitrary SDK destructor ends asynchronous physical work.
- The terminal incumbent child deliberately exits its disposable process with the retained
  retirement alive. Its watchdog, hidden Windows launch and kill/wait guard bound observation;
  this is not clean retirement or arbitrary process-tree shutdown evidence.
- Local quarantine cannot retain an unowned external event loop or authority. The Host must keep
  them available and explicitly drive close; unfinished custody at thread exit still aborts.
- R4b activity/facility policy, R4c phase comparison, dynamic module code custody and all other
  master-plan waves remain open. No total rebaseline completion or progress-percentage increase
  is inferred from the test count.

# Citations

- [Owner-local close design](../../../../architecture/native-close-ownership-design.md)
- [Incumbent source study](../../../../research/2026-10-25-native-close-dependency-ordering.md)
- [Native graphics outcome inventory](../../../../research/2026-10-25-native-graphics-retirement-outcomes.md)
- [Master plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)

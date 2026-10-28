---
type: "Work Progress"
title: "Durable installed tool selection and paired attempt integration in progress"
description: "Records the current single-slot selection replacement, independent review corrections and live qualification boundary."
timestamp: 2026-10-28T01:07:18Z
record_id: "9621f4a6d44249569e458a0e4006fa4a"
producer_id: "codex-root"
run_id: "durable-source-tool-selection-2026-10-28"
related_plan: "docs/plans/2026-10-28-local-source-tool-release-plan.md"
git_branch: "feat/durable-source-tool-selection"
supersedes: "d9a0d6253fc3473ab82093b446048d9c"
---

# Summary

Continue the complete active foundation Goal from `main = b77c445`, on
`feat/durable-source-tool-selection`. All source changes in this unit are uncommitted and not
merged. The original untracked `vendor/` remains untouched. No push or PR, no concurrent Cargo;
reuse the root target with one build job. This record is an integration checkpoint, not a
qualification or completion claim for the local lifecycle plan or the foundation rebaseline.

# Details

The Session now observes `.nara/source-tools/selection.json` as bounded format-1 desired state,
separate from artifact receipts and executable approval. Missing is unselected; invalid records
block blind overwrite. Fresh Open and explicit reconciliation do no describe probe. Managed
approval retains the actual inspected installation directory, installation/receipt/manifest facts
and captured images. `DirectoryCapability::same_object` compares proven facts from two live
directory handles without equating capability sessions or changing rights/trust.

Ready candidates publish selection before the sole route-promotion helper. Post-publication
Session, original selection, predecessor, attempt and Schema checks may truthfully leave disk
successor / active predecessor. Disable immediately withdraws execution and review; its durable
write and physical child/file retirement are separate. File Hosts use the existing HostCustody
and shared publisher, not a new store or cleanup owner. Application and native consumers now use
project-local installation selection, retain the DLL, and test fresh Open without execution.

Independent review found and drove four concrete corrections:

1. A single result field lost one of two co-terminal publication/deferred-Disable outcomes. The
   public view now has two named `EditorSourceToolSelectionAttempt` observations: `operation`
   for Observe/Enable/Reconcile and `disable` for Disable, each with optional terminal outcome.
   Each survives until an explicitly accepted operation replaces its own lane. Pending Disable
   refuses another Disable. No arbitrary history ledger or compatibility alias remains.
2. Unified Disable/Close changed phase before cancelling a durable probe, losing its original
   terminal result. Cancellation now completes the original attempt as `ProbeCancelled` regardless
   of that phase change. Probe failure is `ProbeFailed`, distinct from missing Host approval.
3. Matching leaf identity/digest did not prove a publisher's current parent. `admits_publication`
   checks the current fixed parent as well as the retained leaf and digest. Refusals retain the
   actual Host; uncertainty retains publication facts and requires explicit reconciliation.
4. Durable Enable captured its predecessor before calling an entry point that implicitly advanced
   another returned probe. It could accept a stale basis and necessarily fail later promotion.
   Owner advancement now precedes original-basis validation, and shared private probe admission
   never advances the owner. A real returned-but-unconsumed probe regression covers this cut.

The parent also found the Application Close-decision filter omitted `DisableInstalled`; it now
admits that stop action, and the actual C# Application test includes a dirty-source Close decision
and cancellation. That new Application path has not yet executed at this checkpoint. The store's
path-boundary failure was a fixture arithmetic mistake: the alleged oversized path was exactly
512 bytes. Production again accepts <=512; explicit 512/513-byte assertions replace the mistake.

## Actual Evidence At This Checkpoint

- Final focused root selection/store/conformance: 28 passed, 660 excluded;
  `b2cfca40-b4d9-41f0-93d9-d27b8f24faa4`, 18.501 s. This includes real code-free installation,
  BeforeWork/AfterWork gates, paired outcomes, same-leaf/replaced-parent refusal and cleanup
  retained across incomplete Close, abnormal Drop and runtime quarantine. Internal Ready tests
  enter after probing; they do not fabricate or qualify managed executable approval.
- Filesystem full all-feature suite: 87 passed, 3 skipped;
  `48b50fa7-7cc0-49a5-bde5-7ef15db993e9`, 2.486 s. This includes the four live-directory comparison
  regressions with independent sessions, rights/trust and renamed objects.
- Tooling full all-feature suite: 86 passed, 1 skipped;
  `c2d930dd-d4b2-467e-95e0-b38fd2fb4b58`, 2.129 s.
- Root/Editor all-feature production library check passed before the final private-probe admission
  extraction; the final focused root build covers that extraction. Root and independent-fixture
  formatting and normal Git diff checks passed. A temporary `core.autocrlf=false` diff check
  misclassified CRLF as whitespace; the normal repository-configured check passes, with no config
  or source rewrite performed in response.
- The actual installed V1/invalid/V2/Disable/Close/Open/re-enable test passed before paired
  observations. A broader manual language invocation then passed 18/24, with six setup failures
  because that hand command omitted runner-only module/probe environment variables. Use the
  formal runner for full qualification, not the incomplete command. This is not product failure
  evidence. Single-run debug timings from that pre-paired invocation are not a performance claim.

One focused test build initially failed because the added admission regression imported a private
fixture timeout. It now owns its explicit deadline; the passing 28-case run covers that fix.
A subsequent cleanup-only `let _` consumes the must-use quarantine status without changing its
behavior. The actual installed consumer additionally tests public durable probe cancellation;
that last fixture edit is formatted but has not executed yet.

# Next Action

At capture, exact exec session `56619` owns the live root all-feature library + public
`editor_persistence` nextest run `d7529984-f52a-465e-977a-950d686b20fa`: 702 selected tests,
6 skipped, log `$TEMP/nara-selection-root-persistence-full.log`. Its outcome is still unknown.
Poll that same handle; do not start another Cargo command before authoritative terminal evidence.

Then run `python tests/fixtures/behavior-languages/run.py --native-editor`, the affected Editor
non-native suite, selected native critical trajectories, no-default checks and correctness Clippy
serially. The runner sets all required .NET/module/probe environment variables and the root target.
Fix actual failures, recheck the corrected source, and collect the second review's final disposition.
Update the implementation ledger and final immutable verification, render/validate memory, then
make a precise Conventional Commit and local ff `main`. Do not push. Full dependency-safe removal,
metadata adoption/evolved lineage, modified/adopted content and Package UI remain later work.

Only the read-only second reviewer `01a17ffc-d602-7c31-8027-bed178b2da75` remains open, rechecking
the admission correction. All writing agents are closed and their files are frozen. All current
dirty tracked/new paths belong to this authorized unit; inspect again before committing.
An automatic policy rejection prevented removing the accidental duplicate fixture `target`;
it remains. Do not bypass the rejection or confuse that directory with the preserved root target.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Local lifecycle plan](../../../../plans/2026-10-28-local-source-tool-release-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [Finite source-tool guide](../../../../guides/finite-source-tools.md)
- [Predecessor evidence](../../verification/2026-10/2026-10-27T233313Z-continuous-file-publication-and-editor-source-custody-qualification-d9a0d6253fc3473ab82093b446048d9c.md)

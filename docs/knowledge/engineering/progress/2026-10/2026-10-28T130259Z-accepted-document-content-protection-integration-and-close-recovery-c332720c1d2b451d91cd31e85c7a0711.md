---
type: "Work Progress"
title: "Accepted document content protection integration and close recovery"
description: "Records the known-versus-published protection correction, independent close/recovery findings and intermediate verification before final qualification."
timestamp: 2026-10-28T13:02:59Z
record_id: "c332720c1d2b451d91cd31e85c7a0711"
producer_id: "codex-root"
run_id: "document-edit-content-protection-2026-10-28"
related_plan: "docs/plans/2026-10-28-protected-source-tool-content-plan.md"
git_branch: "feat/document-edit-content-protection"
---

# Summary

Continue the full active foundation Goal from local `main = 2f032e7`. Generic exact-object unlink
already exists and remains independent of safe installed-content reclamation. This unit attaches
sticky protection to actual accepted document changes; it does not expose reclamation.
All implementation is uncommitted at this checkpoint. Preserve the original untracked vendor
directory. Windows/PowerShell, Rust 1.95.0, serial Cargo with one job/root target; no push or PR.

# Details

Five existing Session acceptance points capture the bound project-relative source and source-owned
content identity before consuming an operation. Changed content marks the matching member immediately;
no-op, refusal and navigation do not. Existing per-document persistence lanes retain a private
accepted-touch flag across initial reads and new registration. No new document/history owner exists.

Confirmed management/adoption/observation now enters the known ledger before IO. Exact pending bytes
stay immutable; failures retain confirmed protection and later edits form an automatic successor.
Automatic publication does not overwrite the explicit operation slot. Reads retain their original
protection basis. Actual weaker disk needs explicit repair; an older read missing only later
accepted document facts can queue a successor. Accepted async publication, not edit acceptance,
establishes disk evidence; no crash-window or Windows parent-directory durability guarantee follows.

Independent reviews found and corrected:

- Retired content Hosts were mistaken for borrowed Hosts and reblocked physical Close. The first
  focused run `f7012049-f26a-4d07-b2d0-7187aeb9620c` reproduced repeated Close failures and was
  cancelled. RetainedHost now exposes its actual retired fact internally; the protection preparation
  gate applies before irreversible Close only.
- A failed first management publication stranded locally confirmed entries absent from disk.
  Bounded unpublished-installation provenance now permits explicit repair of those entries,
  while deletion of previously admitted installations remains a conflict.
- Initial Unavailable management data could lose an edit/clean-Undo touch on Close. Edited
  documents now require successful reconciliation before Save/Reopen/release; whole Close leaves
  the Session repairable. Unchanged documents do not create that obligation.
- Application authoring access was unavailable during retirement/replacement. The narrow content
  view and exact repair now reach the retained published Session; ordinary authoring remains gated.

The corrected focused content run `eab86452-f433-450a-a88c-66569fef95f7` passed 36 tests, build
1 m 15 s/run 16.041 s. This predates the final recovery tests and unavailable-read guard.
Integrated root/Editor library check also passed before those final changes.

The first broad all-feature library run `fabe5e45-4d7d-4fc3-b5b0-ec6ad8460c02` executed 1,142
cases: 1,113 passed, 29 failed, 38 explicit skips; build 5 m 26 s/run 363.392 s. Most failures
were ordinary Save admission rejecting a completed but unconsumed initial content read. Persistence
admission/polling now consumes that dependency; unknown pending classification can retain the
original owned checkpoint/request while actual IO waits. The new held-task test also incorrectly
assumed task cancellation had already occurred; it now explicitly cancels the real independent
handle before challenging late-result disposal. Neither fix removes the original assertions.

Actual text-document, Application failure/repair, held real Tasks, older read/publication and an
independent C# release/sample consumer are implemented. Their final qualification is still pending.
One integration regression run is currently active, exec session `91041`, log
`nara-content-edit-regressions.log` under the local temporary directory.

# Next Action

Finish the current focused regression run, repair actual failures, then qualify the final broad
libraries, independent managed/native consumer, minimal features and correctness Clippy serially.
Complete the final independent review, record exact results, update the implementation ledger
and plan evidence, commit precisely and fast-forward local main. Keep the full foundation Goal
active. Safe all-stream reclamation/recovery, gameplay publication/migration, resources/delivery,
rendering and Nara-owned custom-drawn Editor UI remain separate work.

# Citations

- [Protected-content plan](../../../../plans/2026-10-28-protected-source-tool-content-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Qualified unlink predecessor](../../verification/2026-10/2026-10-28T114302Z-exact-object-file-unlink-and-durable-installed-content-protection-995675b08109422ab0f90b15d6004546.md)
- [Content owner](../../../../../src/project_host/runtime/editor/source_tools/content.rs)
- [Application boundary](../../../../../editor/src/app/source_tools.rs)

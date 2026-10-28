---
type: "Verification Evidence"
title: "Owned source history and whole-workspace schema preparation"
description: "Records bounded frozen input handoff and exact original publication checks without claiming complete Session adoption."
timestamp: 2026-10-28T06:16:11Z
record_id: "a4021ad50ca24035b6530ae233b9dff3"
producer_id: "codex-root"
run_id: "owned-schema-preparation-2026-10-28"
related_plan: "docs/plans/2026-10-28-authoring-owner-adoption-plan.md"
git_branch: "feat/owned-schema-preparation"
verified_by: "01a18121-5f8d-7d61-8587-1b67f18bd887"
---

# Verification

Continue the complete active foundation Goal from local `main = 58dfb8f`. This unit supplies
bounded owned source/history preparation and complete Workspace input handoff, not installed
Session publication/adoption. Windows 11, Rust `+1.95.0`, serial Cargo with one build job and
root `target`; no push/PR and original `vendor/` untouched. The containing commits bind the
qualified source. The corrected source-history/current-composition policy remains in force;
no all-history combination compiler or second history interpreter is introduced.

# Result

Scene and Prefab retain nominal, move-only finite preparation requests. Original document and
classification allocations are shared; exact private history entries, from/to content identities,
costs and conversion scopes are copied only after owning work/retention admission. This is a
bounded input copy, not a new Session, SourceId, content identity or publication capability.
The retained incumbent ticket is rechecked for its exact work allowance, source kind, revision
and basis at capture and execution. Requests cannot skip charges by changing the allowance.

Direct and owned paths use the same classification helpers and existing two-way history kernel.
Prepared output now retains the original document Arc so finite Host work can borrow its exact
source under the prepared classification without consulting a live Session. Session/Workspace
publication still requires the original live incarnation/revision/basis and complete set. Old
prepared-source tests intentionally observe one additional pinned Arc handle, not copied bytes.

The Workspace first admits all live incumbents, then captures every source/history request under
one allowance. Request execution owns no live Workspace or executor. It returns one complete
prepared set, frozen Scene/Prefab views and cumulative modeled usage. Direct and owned paths share
the complete input admission; all-source exclusive guards remain the publication cut. `nara_tasks`
is a dev-only Tooling dependency for the actual finite Compute consumer; production Tooling does
not acquire execution authority. Cargo.lock changes only that already existing package edge.
Capture still performs bounded incumbent retention scans and exact history copying on its caller;
this does not establish a wall-time deadline or measured Editor responsiveness. Measure that owner
occupancy in the actual adoption workflow before changing ordinary history storage or adding caches.

# Evidence

- Source/Workspace all-feature library check passes, 4.10 s; root all-feature library check passes,
  9.68 s. An initial source check found an Arc/state borrow mismatch; input capture now uses the
  existing private `published_shared` handle, and shared classification helpers borrow the state.
- Independent adversarial source/Workspace review found no confirmed P1/P2 in before-copy charging,
  original identity/scope, common history interpretation, frozen views or publication fencing.
  This was read-only: no Cargo, allocator instrumentation or complete Session qualification.
- Initial Workspace request cohort: **16 passed**, 71 excluded;
  `6e8a51c3-f757-4604-be04-1d96a87d983a`, 0.427 s after an 18.91 s build. Includes actual finite
  Compute Tasks, held handoff while the live Workspace changes, frozen original Arc views,
  stale-return refusal, physical worker shutdown and original Workspace Drop/foreign publication.
- Combined source/Workspace cohort: **47 passed**, 481 excluded;
  `c1b39a7d-330a-4298-8131-6d7a8471bb0c`, 2.351 s after a 31.64 s build. Six new nominal
  Scene/Prefab cases exercise owned `Send + 'static` worker transfer, frozen Undo/Redo, later
  edit/reopen/basis changes, wrong tickets/allowance, actual calibrated copy/proof limits and
  a frozen historical dangling reference that still refuses after live Reopen clears its history.
  The first source-test build used Arc operations on a state borrow at eleven fixture sites;
  tests now use the existing private shared-authority accessor, preserving pointer assertions.
- The existing actual complete-Prefab-conversion test now runs both direct and owned preparation
  for Scene/Prefab and both retained branches. Actual conversion-created scopes, exact source
  restoration and subsequent Undo/Redo are retained; no fixture-created footprint is accepted.
- Complete Scene/Tooling all-feature nextest: **834 passed, 4 skipped**, 41 binaries;
  `2c47ce63-c23f-4c0a-bad1-a397cd17156d`, 37.637 s after a 2 m 53 s build. Existing decision-local
  allocation/query/preparation observations remain explicitly skipped; none is a new exemption.
- Related root Authoring/file-publication/schema nextest: **126 passed**, 589 excluded;
  `2dfa0859-e32c-4122-ab68-4469553286a1`, 20.055 s after a 1 m 25 s build. This rechecks the actual
  metadata/project/capture/history counterexamples against prepared tokens retaining source Arcs.
- Minimal Scene `serde` and root `runtime-2d,serde,tooling` checks pass, 2.77 s and 6.55 s.
- Scene/Tooling/root all-feature/all-target Clippy with `-D clippy::correctness` passes, 1 m 21 s.
  Existing non-correctness warnings and the same large report error-type warnings at new API
  uses remain; no unrelated lint/reporting rewrite. Formatting, ordinary Git whitespace checks,
  changed Markdown links and engineering-memory render/validation pass at final integration.

# Follow-up

The next concrete owner is `EditorProjectSession`, not a new task manager or metadata Runtime.
Read-only source review by `01a1811a-ed2b-70d1-ab1f-569132e7fb2c` identifies the following actual
integration seams; these are implementation inputs, not completed behavior:

- The project owner must lend a narrow owned input containing its actual incumbent schema Arc,
  nominal basis and scene limits. An empty Workspace still has project authority; do not infer it
  from the first open document or readmit a forked startup. Prepare saved startup and every current
  Scene from the same successor, using all prepared dirty Prefab snapshots and real persistence
  observations. Current captures need cumulative owner-budget validation like the saved project.
- The Session operation needs concrete Busy/quiescing observations for persistence Save/Reopen,
  pending Open/workspace intent, image/source import and cleanup, conversion preparation/validation,
  tool probe/invocation/retiring work, and selection pending/ready/deferred Disable. Tool `jobs()`
  omits selection IO. `hydration::has_pending_work` excludes cancelled pending work and cannot prove
  physical return. Statically accepted reviews, results and immutable presentation may remain.
- Autonomous Scene capture/index admission must stop while the finite predecessor work returns.
  `cancel_source_index()` is a permanent Close operation, not a reusable pause. Prefer existing
  observation-only polling plus Session admission gating when sufficient; add no generic pause
  service. Rendering/GPU idleness is not a condition for metadata adoption. `authoring_retirements`
  must physically complete; a healthy current authoring Runtime itself is not Busy.
- Reuse `authoring_runtime.persistence_tasks()` and `HostCustody::retain_publication`. The selection
  IO latch pattern keeps actual publication results/accepted leaf in Session/worker shared custody,
  with a `TaskHandle<()>`, return observer and nonblocking polling. A cancelled typed task result
  must not own the only published-leaf proof. Register the publisher before any write; Close retains
  cleanup and late Published/Uncertain facts while revoking live adoption authority immediately.
- Review retains installation inspection, original source selections, prepared metadata, complete
  Workspace, saved project and current captures. Adopt rechecks the exact live set/source receipts,
  then verifies original installation/metadata/source/lookup observations before publishing. Attach
  current captures before consuming the metadata owner for the project. Complete fallible
  receipt-to-Session conversion before binding every source guard and infallibly swapping both
  project and Workspace in one owner turn. Known PublishedWithoutAdoption and uncertainty require
  explicit reconciliation, preserve dirty sources/history and never trigger automatic rollback.
- Gate conflicting mutation/construction/gesture, Save/Open/import/conversion/tool/selection
  admissions explicitly. Keep ordinary read-only query/property/declaration observations and
  Cancel/Acknowledge available; do not overload lifecycle `accepts_new_work` so Busy looks Closing.
  Same-turn invalidation must cover Scene capture, authoring preview content, UI authoring frame/
  projection cache, UI preview and old published-frame authority because source revision is unchanged.
  Old immutable renderer packets may remain visible under existing stale-input rules.

Then qualify actual missing-owner installed C# review/adoption, separate execution approval,
inactive dirty Scene/Prefab/history/checkpoints, late Close/publication, explicit reconciliation,
Save/physical Close/fresh Open and selected native interaction. Remaining installed dependencies,
content removal and later foundation waves remain open. Full Goal stays active.

# Citations

- [Owner-adoption plan](../../../../plans/2026-10-28-authoring-owner-adoption-plan.md)
- [Saved-project predecessor](2026-10-28T054209Z-same-basis-saved-project-receipt-before-whole-session-schema-adoption-a796d5f92d5e4fb4b1ae7cbdca4d2741.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)

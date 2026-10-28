---
type: "Verification Evidence"
title: "Whole-workspace schema preparation and shared immutable published sources"
description: "Qualifies exact whole-set source classification and immutable source sharing without claiming project or Session adoption."
timestamp: 2026-10-28T05:09:25Z
record_id: "dc568b9954884122a24a9f8a25491451"
producer_id: "codex-root"
run_id: "whole-workspace-schema-adoption-2026-10-28"
related_plan: "docs/plans/2026-10-28-authoring-owner-adoption-plan.md"
git_branch: "feat/whole-workspace-schema-adoption"
verified_by: "01a180d8-010b-7700-926d-25fd87f94243,01a180e8-b78b-7aa3-ae47-092a130f55f5"
---

# Verification

Continue the complete active foundation Goal from local `main = 565f108`. This unit closes
whole-Workspace source classification preparation/publication and removes actual Published fork
deep copies. It does not complete project receipt assembly, effective historical composition or
installed-owner Session adoption. Windows 11, Rust `+1.95.0`, serial Cargo, one build job and root
`target`, including independent fixture workspaces. No remote publication; original `vendor/`
remains untouched. The containing commits identify the final verified source.

# Result

`PreparedEditorWorkspaceSchemaReclassification` is one move-only candidate for its original
Workspace's complete ordered open Scene/Prefab set. Preparation checks exact predecessor admission
and unsupported/native owners before work, charges candidate/ticket/commit storage, then retains
every incumbent source/classification/history before preparing the first successor. A source-owned
linear incumbent token binds its preparation allowance identity, revision/source incarnation,
previous nominal basis and source kind; a foreign allowance or source cannot skip its charge.
The original single-source API uses the same split and retains early successor rejection.

Validated source borrows expose original dirty document Arcs and the prepared nominal successor
to Host capture. Complete pre-write validation and exclusive binding require original Workspace,
open set, source binding, incarnation/revision and basis. All source guards bind before any
classification changes; dropping a guard set publishes nothing. Commit only swaps classifications
and marks Scene projection dirty. It does not replace source/history/selection, content-state,
saved checkpoint/digest, external-reload state or persistence authority. Old queries and prepared
edits reject their old basis even though source revision remains unchanged.

Published Scene and Prefab now own private `Arc<Document>` and `Arc<AuthoringDocumentState>`.
Candidates remain owned and publication wraps once. Fork clones immutable handles; `into_parts`
transfers them directly into Session construction. Each Session still creates independent source
identity, content-state ownership and history. Later edit/history/reclassification replaces only
that Session's handles. Public arbitrary document/classification pairing, compatibility aliases
and hidden `unwrap_or_clone` copies are absent. Arc-dependent getters are ordinary functions,
an intentional unreleased source-API change rather than preserved const compatibility.

ADRs 0047/0118 adopt the Workspace boundary. ADR 0114 and the adoption plan clarify that a retained
startup is not automatically the latest Save. A fresh bounded saved-source read using the same
successor basis is allowed; relabeling old startup bytes with a new digest, admitting another basis
or dropping dirty participant revisions is not. This avoids inventing a second long-lived saved
snapshot store before the actual Host workflow requires one.

# Evidence

- Initial Scene reclassification cohort: 24 passed, one failed because splitting incumbent admission
  moved invalid-successor rejection after budget charging. The original exact zero-usage assertion
  remains; production now checks successor first. Final source cohort **25 passed**, 407 excluded;
  `6c30aa82-e21c-4144-829a-15da3d4d3841`, 1.674 s after a 9.50 s build.
- Final focused Scene/Tooling cohort: **42 passed**, 478 excluded;
  `115958ee-67dc-4642-8317-0b7dcb6c2b41`, 2.479 s after a 1 m 39 s build. Fourteen new Workspace
  tests cover mixed inactive dirty sources, saved checkpoints and real Undo/Redo, stale final
  source, foreign Workspace, open/close/replace/reopen, changed bindings, old query/candidate
  rejection, unsupported owners and cumulative budgets. The ordering counterexample combines
  an invalid first successor with a large final incumbent: incumbent retention refuses first,
  calibrated through actual source-owner usage rather than a duplicate cost formula.
- Complete Scene/Tooling all-feature nextest: **826 passed, 4 skipped**, 41 binaries;
  `2116d9ab-4ec9-4453-8ff3-87d7552aa6ac`, 38.392 s after a 2 m 56 s build. Skips are existing
  decision-local query/preparation/allocation observations, not new behavior exemptions.
- Root Authoring/file-publication/schema cohort: **117 passed**, 589 excluded;
  `3ff73283-d098-44cf-9ce1-b5d2b0ece7df`, 18.219 s after a 1 m 37 s build.
- Independent module consumer property-authoring/data/Prefab-conversion/Editor-conversion cohort:
  **15 passed**, four binaries; `27c07fb8-d394-4368-ba38-04b9a8496fdb`, 1.908 s after a 2 m 35 s
  build. Uses `--manifest-path tests/fixtures/module-consumer/Cargo.toml` and explicit root target.
- Independent Editor panel prepared-authoring consumer: **9 passed, 1 skipped**;
  `fb654dba-f091-49a9-a407-95d80c1da41b`, 3.701 s after a 2 m 50 s build. The skip is its existing
  explicit preparation-cost observation. Actual non-active query/read/prepare/retain/commit,
  history and persistence paths pass through the compiled external panel consumer.
- Minimal Scene `--no-default-features --features serde --lib` and root
  `--no-default-features --features runtime-2d,serde,tooling --lib` checks pass, 4.37 s and 9.47 s.
  Scene/Tooling/root all-feature/all-target Clippy with `-D clippy::correctness` passes, 1 m 30 s.
  Non-correctness warnings remain, including the existing large report error type at new API
  uses; this unit does not rewrite unrelated reporting or lint policy.
- `cargo +1.95.0 fmt --all`, ordinary Git whitespace checks and local links in seven changed
  Markdown documents pass. Engineering-memory render/validation checks pass at final integration.

Two independent read-only reviews found no confirmed remaining P1/P2 in their scoped production
changes: exact whole-set/budget/guard publication and immutable Published sharing, respectively.
Reviewers did not execute Cargo or instrument allocator/RSS. Three new shared-publication tests
exercise both nominal source kinds: exact allocation/Arc transfer, independent history/source
identity, separate edit/Undo/Redo and successor classification preserving predecessor/sibling.

Pinned source checks use Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. Bevy's `TypeRegistryArc` shares a mutable locked
registry; it is not proof of an immutable admission successor. Godot's Editor Undo manager keeps
distinct history IDs and checks action/history consistency; it does not prove Nara's whole-set
classification publication. These are concept/source comparisons, not executed reference-engine
qualification or grounds for copying their mutation authority.

# Follow-up

Next implement the same-basis complete project receipt, using real saved-startup observation and
separate dirty current captures. Populate actual selected source bases/assets/database/lookups,
retain final cumulative operation usage and create a fresh bundle-bound source index. Convert the
receipt into its Session snapshot before the first Workspace swap. Preserve actual accepted bundle
leaf/evidence on published-but-unadopted or attachment failure.

Historical selected compilation remains required before metadata publication. Stream exact states
through the existing history kernel, retaining bounded file/lookup evidence and cumulative work,
not a snapshot per edge or a second patch interpreter. A historical Prefab must be evaluated in its
real containing/outer context: preserve dormant malformed payloads lawfully discarded or repaired,
while refusing invalid surviving values. Current capture `Ok`/Degraded is not an adoption policy.

The actual Project Session still needs finite task requests, concrete persistence/import/tool/
selection/retirement blockers, original-observation fences, publication/reconciliation outcomes,
same-turn project/Workspace adoption and immediate preview invalidation. Then qualify the genuinely
missing-owner installed C# consumer through Application, Save/physical Close/fresh Open and selected
native interaction. No native adoption is claimed here. Installed dependencies/content removal,
gameplay executable owner replacement, resources/delivery/render/UI and the full Goal remain open.

# Citations

- [Owner-adoption plan](../../../../plans/2026-10-28-authoring-owner-adoption-plan.md)
- [Metadata/current-source predecessor](2026-10-28T041431Z-snapshot-preserving-bundle-successors-and-prepared-current-source-capture-74dd681a584541178fb81b346b53e878.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0114](../../../../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)

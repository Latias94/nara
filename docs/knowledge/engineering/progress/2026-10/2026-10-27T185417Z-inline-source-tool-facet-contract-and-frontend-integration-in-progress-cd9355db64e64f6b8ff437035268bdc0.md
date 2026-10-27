---
type: "Work Progress"
title: "Inline source tool facet contract and frontend integration in progress"
description: "Records the shared Tooling replacement, explicit activation routing and active disjoint implementation lanes before complete production qualification."
timestamp: 2026-10-27T18:54:17Z
record_id: "cd9355db64e64f6b8ff437035268bdc0"
producer_id: "codex-root"
run_id: "inline-source-tool-facets-2026-10-28"
related_plan: "docs/plans/2026-10-28-inline-source-tool-facet-plan.md"
git_branch: "refactor/inline-source-tool-facets"
supersedes: "fc2363973364434096d350ae4a8d762c"
---

# Current Scope

Continue the full active foundation Goal after `6acc21f`, already fast-forwarded to local main.
The new branch is `refactor/inline-source-tool-facets`. At entry only the original untracked
`vendor/` remained. Current other changes belong to the authorized parent/worker implementation.
The unit plan is `docs/plans/2026-10-28-inline-source-tool-facet-plan.md`; it implements the real
EnemyStats health property plus small panel consumer selected in the foundation plan. Nara's
own custom-drawn UI remains the destination; the current frontend supplies interaction evidence.

# Implemented Contract And Limits

The parent added `crates/nara_tooling/src/source_tools.rs`, its private admission tests, exports,
full `EditorFrameModel::source_tool()` / `with_source_tool`, and `EditorAction::SourceTool` with
typed outcome. The old pending-only frame API is removed. This crate now directly uses the
workspace's existing `thiserror` dependency; root Cargo.lock's package edge was updated offline.
Independent workspace lockfiles may need the corresponding Cargo-generated edge update before
their locked checks; no dependency version was selected or downloaded.

Pure tool IDs, active metadata, finite property declaration, view, review summary/borrow, errors,
commit and actions move out of root Host. Only command/definition and physical process/Tasks
execution remain root-owned. IDs are fresh opaque nominal identities; only those retained by the
actual Session are admitted. A full frame carries immutable active owner/component metadata and
the optional one-root integer proposal facet. Remove `enabled` / `review_ready`: use
`view.active.is_some()` and `view.review.is_some()`. Summary includes original basis and intent,
request, shared title, operation count and fixed created ID. Actual prepared patch/inverse stay
in one Session candidate, accessible through borrowed `EditorSourceToolReview`.

Direct requests now require `(&activation_id, &basis, intent, parameters)`. Disable requires the
original activation and returns a result. A separate `cancel_source_tool_attempt(&attempt)` only
cancels that exact pending probe and preserves an active predecessor. The corresponding actions
are Request, IntegerProperty, Accept, Disable and CancelAttempt. Every deferred request/disable
retains its original execution basis. Stale activation must reject before revoking the current
request/review. IntegerProperty uses the actual active declaration's parameter key, with exact
owner/component/field matching and normal source preparation. No compatibility alias, generic
UI tree, callback ABI, second result ledger or document executor is selected.

The optional child description is `integer_property: {field, parameter, label, initial}`. Shared
admission bounds metadata and requires the selected editable i64 root. Labels are display text,
never widget IDs. Root field IDs use `ComponentFieldId::try_new`, not its crate-private validate.
Negative initial parameter values remain legal: this is a proposal parameter, not a source value.

ADRs 0047/0118 have the owning amendments. Proposed 0120 remains independent. The shared Tooling
check passes; focused new admission tests pass 2/2 under run
`a180ca5c-b48f-4c10-bbbe-909e6dbbb322`. **No current full root/Editor/C#/native qualification has
run.** Transitional files are not a passing production slice. Cargo currently has no running
parent handle. Do not compile active worker write sets or run global fmt until frozen.

# Parent Integration

Parent owns Tooling types/frame, Editor Application direct delegates and old persistence-test
migration, frontend `mod.rs`, `workspace.rs`, `submission.rs`, `authoring.rs`, `field_editor.rs`,
native maintenance and panel registration. Root source-tool actions reuse the existing ordered
batch and panel incarnation/result ledger. There is one new Recipient::SourceTool for the small
built-in panel; inline actions use Recipient::Authoring, forwarded to both field and tool state
by exact submission ID. The built-in `nara.source-tool` panel is registered, default closed.

Each panel owns its separate SourceToolWidgets. Parent wires begin/end paint, original document
basis, shared borrowed review and text-owner observation. Both ordinary Inspector and public
`edit_field` use the same facet painter; unmatched/read-only fields keep generic rendering.
FieldEditors now removes matching default drafts when a facet takes ownership, even if a hidden
field was not painted, and reports cancellation to the tool controller. A retired custom control
must not resurrect its prior input when the default renderer returns.

# Active Lanes

These are live coordination facts, not file reservations. Workers share this checkout, never
run Cargo/fmt/commits and must freeze before parent verification:

- Newton `01a17e5a-3cf8-7a90-a497-f7b021bcc776`: all relevant root `src/project_host` exports,
  runtime/editor owner, actions/model and `source_tools.rs` / tests. Removes moved duplicates,
  parses/adopts actual facet, keeps process custody, migrates root tests and adds stale actions.
- Euler `01a17e4e-6441-7070-b0f6-aa33726f7df1`: only `tests/fixtures/behavior-languages/**` and
  `editor/src/app/source_tools/csharp_tests.rs`. Updates actual C# declaration/consumers and adds
  explicit runner `--native-editor` selection, defaulting to ordinary consumers without GPU.
- Singer `01a17ea3-30da-7400-bbce-26bfbd6d69f2`: only new
  `editor/src/frontend/imgui/source_tool.rs` and descendants. Implements SourceToolWidgets;
  see method signatures already wired by parent. Required begin_paint, end_paint,
  show_integer_property -> (customized, optional action), show_panel -> finite action Vec,
  admitted, action_finished, text_owner and default_draft_retired. Parent requests at most one
  queued operation per paint and exact pending-ID matching. A local route serial must retire
  Dear active input; nominal ID Debug output is not a unique widget key.
- Noether `01a17eac-4ec2-7d82-adf5-259e4a0aa856`: only new
  `editor/src/platform/imgui/authoring_workflow_tests/source_tool.rs`. Parent added its mod.
  Reuses existing hidden `run_native_workflow` and AuthoringWorkflow helpers for actual C#
  inline/panel shared review, history, persistence and default fallback. Exact ignored name:
  `native_source_tool_inline_property_and_panel_share_review_history_and_persistence`.
- Carver `01a17e5a-6e39-78e3-b5be-dedc65d8f1c0`: read-only contract challenge of this unit.
  No current review findings have returned at this checkpoint.

# Next Integration

Collect source-ready reports; compile owner slices only after their writer freezes. Resolve real
API/borrow/exhaustiveness errors, inspect diffs, then global fmt. Run focused serial nextest,
actual language runner (including the explicit native flag when prerequisites apply), affected
root/Editor/independent consumers, no-default and correctness Clippy. The new actual widget path
needs its own native evidence. Preserve the prior unit's physical gate/Close regression checks.
Independently review, capture actual results and commit precisely before local fast-forward main.
No push, PR, separate target directory or vendor change. The full foundation Goal stays active.
